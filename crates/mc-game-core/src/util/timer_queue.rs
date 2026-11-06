//! `net.minecraft.world.level.timers`: `TimerQueue` (the `/schedule` queue, saved as
//! `data/minecraft/scheduled_events.dat`), `TimerCallback(s)`, `FunctionCallback`, `FunctionTagCallback`.
//!
//! Re-entrancy: callbacks receive the queue so they can reschedule. The owner must therefore take the queue out of its
//! storage (or otherwise not alias it) while calling [`TimerQueue::tick`], exactly the discipline vanilla relies on.

use std::collections::{BTreeMap, HashMap};

use mc_core::ident::Identifier;
use mc_nbt::{CompoundTag, Tag};

use crate::util::codec::{self, DataResult, DecodeResult};
use crate::world::level::saveddata::{SavedData, SavedDataType};

/// `TimerCallback<T>`: what runs when an event fires. `encode` writes the map-codec fields (without `type`).
pub trait TimerCallback<T>: Send {
    fn handle(&self, context: &mut T, queue: &mut TimerQueue<T>, time: i64);
    /// The id this callback type is registered under in [`TimerCallbacks`] (vanilla: `codec()` dispatch key).
    fn type_id(&self) -> Identifier;
    fn encode(&self) -> CompoundTag;
}

type CallbackDecoder<C> = fn(&CompoundTag) -> DataResult<Box<dyn TimerCallback<C>>>;

/// `TimerCallbacks<C>`: id -> decoder registry (`LateBoundIdMapper`).
pub struct TimerCallbacks<C> {
    decoders: HashMap<Identifier, CallbackDecoder<C>>,
}

impl<C> Default for TimerCallbacks<C> {
    fn default() -> Self {
        TimerCallbacks {
            decoders: HashMap::new(),
        }
    }
}

impl<C> TimerCallbacks<C> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(mut self, id: Identifier, decoder: CallbackDecoder<C>) -> Self {
        self.decoders.insert(id, decoder);
        self
    }

    /// `codec().decode`: `{type: <id>, ...fields}`.
    pub fn decode(&self, tag: &Tag) -> DataResult<Box<dyn TimerCallback<C>>> {
        let m = codec::compound(tag)?;
        let id = codec::identifier(codec::field(m, "type")?)?;
        let decoder = self
            .decoders
            .get(&id)
            .ok_or_else(|| format!("Unknown element name: {id}"))?;
        decoder(m)
    }

    /// `codec().encode`.
    pub fn encode(&self, callback: &dyn TimerCallback<C>) -> DataResult<Tag> {
        let id = callback.type_id();
        if !self.decoders.contains_key(&id) {
            return Err(format!("Unknown element id: {id}"));
        }
        let mut m = callback.encode();
        m.put("type", Tag::String(id.to_string()));
        Ok(Tag::Compound(m))
    }
}

/// The server-side hooks the two built-in callbacks need (`ServerFunctionManager`).
pub trait FunctionHost {
    /// `functionManager.get(id).ifPresent(f -> execute(f, gameLoopSender))`.
    fn execute_function(&mut self, id: &Identifier);
    /// `for (f : functionManager.getTag(id)) execute(f, gameLoopSender)`.
    fn execute_function_tag(&mut self, id: &Identifier);
}

/// `FunctionCallback`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionCallback {
    pub function_id: Identifier,
}

/// `FunctionTagCallback`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionTagCallback {
    pub tag_id: Identifier,
}

fn decode_id(map: &CompoundTag) -> DataResult<Identifier> {
    codec::identifier(codec::field(map, "id")?)
}

impl<C: FunctionHost> TimerCallback<C> for FunctionCallback {
    fn handle(&self, context: &mut C, _queue: &mut TimerQueue<C>, _time: i64) {
        context.execute_function(&self.function_id);
    }
    fn type_id(&self) -> Identifier {
        Identifier::parse("minecraft:function").expect("static id")
    }
    fn encode(&self) -> CompoundTag {
        CompoundTag::new().with("id", Tag::String(self.function_id.to_string()))
    }
}

impl<C: FunctionHost> TimerCallback<C> for FunctionTagCallback {
    fn handle(&self, context: &mut C, _queue: &mut TimerQueue<C>, _time: i64) {
        context.execute_function_tag(&self.tag_id);
    }
    fn type_id(&self) -> Identifier {
        Identifier::parse("minecraft:function_tag").expect("static id")
    }
    fn encode(&self) -> CompoundTag {
        CompoundTag::new().with("id", Tag::String(self.tag_id.to_string()))
    }
}

impl<C: FunctionHost + 'static> TimerCallbacks<C> {
    /// `TimerCallbacks.SERVER_CALLBACKS`.
    pub fn server_callbacks() -> Self {
        TimerCallbacks::new()
            .register(
                Identifier::parse("minecraft:function").expect("static id"),
                |m| {
                    Ok(Box::new(FunctionCallback {
                        function_id: decode_id(m)?,
                    }))
                },
            )
            .register(
                Identifier::parse("minecraft:function_tag").expect("static id"),
                |m| {
                    Ok(Box::new(FunctionTagCallback {
                        tag_id: decode_id(m)?,
                    }))
                },
            )
    }
}

struct Event<T> {
    id: String,
    callback: Box<dyn TimerCallback<T>>,
}

/// `TimerQueue<T>`.
pub struct TimerQueue<T> {
    dirty: bool,
    /// `PriorityQueue` ordered by (`triggerTime`, unsigned `sequentialId`).
    queue: BTreeMap<(i64, u64), Event<T>>,
    sequential_id: u64,
    /// The `events` table: id -> trigger time -> sequential id. Rows are dropped when empty (Guava `HashBasedTable`).
    events: HashMap<String, HashMap<i64, u64>>,
}

impl<T: 'static> SavedData for TimerQueue<T> {
    fn is_dirty(&self) -> bool {
        self.dirty
    }
    fn set_dirty_to(&mut self, dirty: bool) {
        self.dirty = dirty;
    }
}

impl<T> Default for TimerQueue<T> {
    fn default() -> Self {
        TimerQueue {
            dirty: false,
            queue: BTreeMap::new(),
            sequential_id: 0,
            events: HashMap::new(),
        }
    }
}

impl<T: 'static> TimerQueue<T> {
    pub fn new() -> Self {
        Self::default()
    }

    /// `tick`: runs every event due at or before `current_tick`, oldest first.
    pub fn tick(&mut self, context: &mut T, current_tick: i64) {
        while let Some(callback) = self.pop_due(current_tick) {
            callback.handle(context, self, current_tick);
        }
    }

    /// One iteration of `tick`: removes the oldest event due at or before `current_tick` and returns its callback, for owners
    /// whose context contains the queue itself (they cannot lend it to `handle`).
    pub fn pop_due(&mut self, current_tick: i64) -> Option<Box<dyn TimerCallback<T>>> {
        let (&key, _) = self.queue.first_key_value()?;
        if key.0 > current_tick {
            return None;
        }
        let event = self.queue.remove(&key).expect("first key present");
        // Vanilla removes `(id, currentTick)` — not the event's own trigger time — so a late event leaves its
        // table cell behind. Kept.
        if let Some(row) = self.events.get_mut(&event.id) {
            row.remove(&current_tick);
            if row.is_empty() {
                self.events.remove(&event.id);
            }
        }
        self.set_dirty();
        Some(event.callback)
    }

    /// `schedule`: ignored when `id` already has an event at `time`.
    pub fn schedule(&mut self, id: &str, time: i64, callback: Box<dyn TimerCallback<T>>) {
        if self
            .events
            .get(id)
            .is_some_and(|row| row.contains_key(&time))
        {
            return;
        }
        self.sequential_id = self.sequential_id.wrapping_add(1);
        self.events
            .entry(id.to_string())
            .or_default()
            .insert(time, self.sequential_id);
        self.queue.insert(
            (time, self.sequential_id),
            Event {
                id: id.to_string(),
                callback,
            },
        );
        self.set_dirty();
    }

    /// `remove(id)`: removes every event with this id, returns how many table cells it had.
    pub fn remove(&mut self, id: &str) -> usize {
        let size = match self.events.remove(id) {
            Some(row) => {
                for (time, seq) in &row {
                    self.queue.remove(&(*time, *seq));
                }
                row.len()
            }
            None => 0,
        };
        self.set_dirty();
        size
    }

    pub fn get_events_ids(&self) -> impl Iterator<Item = &str> {
        self.events.keys().map(String::as_str)
    }

    /// `pack`: `(trigger_time, id, callback)` in queue order.
    fn pack(&self, callbacks: &TimerCallbacks<T>) -> DataResult<Tag> {
        let mut events = mc_nbt::ListTag::new();
        for ((time, _), event) in &self.queue {
            let mut m = CompoundTag::new();
            m.put_long("trigger_time", *time);
            m.put_string("id", event.id.clone());
            m.put("callback", callbacks.encode(&*event.callback)?);
            events.push(Tag::Compound(m));
        }
        Ok(Tag::Compound(CompoundTag::new().with("events", events)))
    }

    /// `TimerQueue.codec(callbacks).decode`.
    pub fn decode(tag: &Tag, callbacks: &TimerCallbacks<T>) -> DecodeResult<TimerQueue<T>> {
        let m = codec::compound(tag)?;
        let mut queue = TimerQueue::new();
        for e in codec::list(codec::field(m, "events")?)? {
            let em = codec::compound(&e)?;
            let time = codec::long(codec::field(em, "trigger_time")?)?;
            let id = codec::string(codec::field(em, "id")?)?.to_string();
            let callback = callbacks.decode(codec::field(em, "callback")?)?;
            queue.schedule(&id, time, callback);
        }
        Ok(queue)
    }
}

impl<C: FunctionHost + 'static> TimerQueue<C> {
    /// `TimerQueue.TYPE` (`data/minecraft/scheduled_events.dat`).
    pub fn type_() -> SavedDataType<TimerQueue<C>> {
        SavedDataType::new(
            Identifier::parse("minecraft:scheduled_events").expect("static id"),
            TimerQueue::new,
            |tag| TimerQueue::decode(tag, &TimerCallbacks::server_callbacks()),
            |q| q.pack(&TimerCallbacks::server_callbacks()),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Ctx {
        log: Vec<(String, i64)>,
    }
    impl FunctionHost for Ctx {
        fn execute_function(&mut self, id: &Identifier) {
            self.log.push((format!("f:{id}"), 0));
        }
        fn execute_function_tag(&mut self, id: &Identifier) {
            self.log.push((format!("t:{id}"), 0));
        }
    }
    fn f(name: &str) -> Box<dyn TimerCallback<Ctx>> {
        Box::new(FunctionCallback {
            function_id: Identifier::parse(name).unwrap(),
        })
    }

    #[test]
    fn orders_by_time_then_insertion_and_ignores_duplicates() {
        let mut q = TimerQueue::<Ctx>::new();
        q.schedule("a", 10, f("n:a1"));
        q.schedule("b", 5, f("n:b"));
        q.schedule("c", 10, f("n:c"));
        q.schedule("a", 10, f("n:dup")); // same (id, time): ignored
        q.schedule("a", 20, f("n:a2"));
        let mut ctx = Ctx::default();
        q.tick(&mut ctx, 4);
        assert!(ctx.log.is_empty());
        q.tick(&mut ctx, 10);
        let names: Vec<_> = ctx.log.iter().map(|e| e.0.as_str()).collect();
        assert_eq!(names, ["f:n:b", "f:n:a1", "f:n:c"]);
        let mut ids: Vec<_> = q.get_events_ids().collect();
        ids.sort();
        // "b" fired late (5 < 10): its table cell at 5 is stale (vanilla removes the cell at the current tick)
        assert_eq!(ids, ["a", "b"]);
        assert_eq!(q.remove("a"), 1);
        assert_eq!(q.remove("a"), 0);
        assert_eq!(q.remove("b"), 1);
        assert_eq!(q.get_events_ids().count(), 0);
    }

    #[test]
    fn late_event_keeps_stale_table_cell() {
        // Vanilla removes the cell at `currentTick`, so an event that fires late blocks rescheduling the same time.
        let mut q = TimerQueue::<Ctx>::new();
        q.schedule("a", 3, f("n:x"));
        let mut ctx = Ctx::default();
        q.tick(&mut ctx, 7);
        assert_eq!(ctx.log.len(), 1);
        q.schedule("a", 3, f("n:y"));
        q.tick(&mut ctx, 8);
        assert_eq!(ctx.log.len(), 1);
    }

    #[test]
    fn callback_can_reschedule_from_handle() {
        struct Again;
        impl TimerCallback<Ctx> for Again {
            fn handle(&self, context: &mut Ctx, queue: &mut TimerQueue<Ctx>, time: i64) {
                context.log.push(("again".into(), time));
                if time < 12 {
                    queue.schedule("r", time + 1, Box::new(Again));
                }
            }
            fn type_id(&self) -> Identifier {
                Identifier::parse("minecraft:function").unwrap()
            }
            fn encode(&self) -> CompoundTag {
                CompoundTag::new()
            }
        }
        let mut q = TimerQueue::<Ctx>::new();
        q.schedule("r", 10, Box::new(Again));
        let mut ctx = Ctx::default();
        q.tick(&mut ctx, 11);
        // fired at 11 (late: stale cell at 10 stays), reschedules 12 which is not yet due
        assert_eq!(ctx.log, [("again".to_string(), 11)]);
        q.tick(&mut ctx, 12);
        assert_eq!(ctx.log.len(), 2);
    }

    #[test]
    fn codec_round_trip() {
        let mut q = TimerQueue::<Ctx>::new();
        q.schedule("x", 7, f("a:b"));
        q.schedule(
            "y",
            3,
            Box::new(FunctionTagCallback {
                tag_id: Identifier::parse("t:u").unwrap(),
            }),
        );
        let cb = TimerCallbacks::<Ctx>::server_callbacks();
        let tag = q.pack(&cb).unwrap();
        let expected = mc_nbt::snbt::parse(
            "{events:[{trigger_time:3L,id:\"y\",callback:{type:\"minecraft:function_tag\",id:\"t:u\"}},{trigger_time:7L,id:\"x\",callback:{type:\"minecraft:function\",id:\"a:b\"}}]}",
        )
        .unwrap();
        assert_eq!(tag, expected);
        let back = TimerQueue::decode(&tag, &cb).map_err(|e| e.error).unwrap();
        assert_eq!(back.pack(&cb).unwrap(), tag);
        assert!(
            TimerQueue::decode(
                &mc_nbt::snbt::parse(
                    "{events:[{trigger_time:1L,id:\"x\",callback:{type:\"minecraft:nope\"}}]}"
                )
                .unwrap(),
                &cb
            )
            .is_err()
        );
    }
}
