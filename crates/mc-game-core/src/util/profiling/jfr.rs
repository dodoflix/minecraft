//! `JvmProfiler` / `JfrProfiler` for `/jfr start|stop`: a flight recording of the server written as a real JFR chunk
//! (`debug/server-<timestamp>.jfr`, readable with the `jfr` tool / JMC).
//!
//! Recorded events are the Minecraft ones: `minecraft.ServerTickTime` (every second), `minecraft.PacketReceived` /
//! `minecraft.PacketSent` and `minecraft.NetworkSummary` (every 10 s), `minecraft.ChunkGeneration` (one per chunk
//! step), `minecraft.ChunkRegionRead` / `minecraft.ChunkRegionWrite`, `minecraft.StructureGeneration` and
//! `minecraft.LoadWorld`. Not produced: the JVM's own events (GC, allocation, file IO); the summary written next to
//! the recording (`SummaryReporter`) reports those JVM-only sections as empty.

use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use indexmap::IndexMap;
use parking_lot::Mutex;
use serde_json::{Map, Value, json};

use mc_core::java_collections::hash_map_ordered;
use mc_core::math::java::{java_double_to_string, java_float_to_string, java_string_hash};

use crate::util::util::get_nanos;

const TICKS_PER_SECOND: i64 = 1_000_000_000;
/// `@Period("1 s")` / `@Period("10 s")`.
const SERVER_TICK_PERIOD_NANOS: i64 = 1_000_000_000;
const NETWORK_SUMMARY_PERIOD_NANOS: i64 = 10_000_000_000;

// ---- type ids of the chunk's metadata ----
const T_LONG: i64 = 1;
const T_INT: i64 = 2;
const T_STRING: i64 = 3;
const T_BOOLEAN: i64 = 4;
const T_LABEL: i64 = 10;
const T_DESCRIPTION: i64 = 11;
const T_NAME: i64 = 12;
const T_CATEGORY: i64 = 13;
const T_CONTENT_TYPE: i64 = 14;
const T_TIMESTAMP: i64 = 15;
const T_TIMESPAN: i64 = 16;
const T_DATA_AMOUNT: i64 = 17;
const T_EVENT_SERVER_TICK: i64 = 100;
const T_EVENT_PACKET_RECEIVED: i64 = 101;
const T_EVENT_PACKET_SENT: i64 = 102;
const T_EVENT_NETWORK_SUMMARY: i64 = 103;
const T_EVENT_CHUNK_GENERATION: i64 = 104;
const T_EVENT_CHUNK_REGION_READ: i64 = 105;
const T_EVENT_CHUNK_REGION_WRITE: i64 = 106;
const T_EVENT_STRUCTURE_GENERATION: i64 = 107;
const T_EVENT_LOAD_WORLD: i64 = 108;

/// `Environment.SERVER`.
pub const ENVIRONMENT_DESCRIPTION: &str = "server";

// ---- JFR primitive encoding ----

/// Compressed (LEB128-like, at most 9 bytes) integer.
fn put_varlong(out: &mut Vec<u8>, value: i64) {
    let mut v = value as u64;
    for _ in 0..8 {
        if v < 0x80 {
            out.push(v as u8);
            return;
        }
        out.push((v & 0x7f) as u8 | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

fn varlong_len(value: i64) -> usize {
    let mut tmp = Vec::with_capacity(9);
    put_varlong(&mut tmp, value);
    tmp.len()
}

/// A string (`encoding byte`: 0 null, 1 empty, 3 UTF-8).
fn put_string(out: &mut Vec<u8>, value: &str) {
    if value.is_empty() {
        out.push(1);
    } else {
        out.push(3);
        put_varlong(out, value.len() as i64);
        out.extend_from_slice(value.as_bytes());
    }
}

/// An event: `size` (including itself), `type`, then the payload.
fn put_event(out: &mut Vec<u8>, type_id: i64, payload: &[u8]) {
    let mut body = Vec::with_capacity(payload.len() + 9);
    put_varlong(&mut body, type_id);
    body.extend_from_slice(payload);
    let mut len = 1;
    while varlong_len((body.len() + len) as i64) != len {
        len += 1;
    }
    put_varlong(out, (body.len() + len) as i64);
    out.extend_from_slice(&body);
}

// ---- metadata ----

struct Element {
    name: &'static str,
    attributes: Vec<(String, String)>,
    children: Vec<Element>,
}

fn element(name: &'static str, attributes: &[(&str, &str)], children: Vec<Element>) -> Element {
    Element {
        name,
        attributes: attributes
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect(),
        children,
    }
}

fn annotation(class: i64, value: &str) -> Element {
    element(
        "annotation",
        &[("class", &class.to_string()), ("value", value)],
        vec![],
    )
}

fn field(name: &str, class: i64, annotations: Vec<Element>) -> Element {
    element(
        "field",
        &[("name", name), ("class", &class.to_string())],
        annotations,
    )
}

fn type_class(name: &str, id: i64, children: Vec<Element>) -> Element {
    element(
        "class",
        &[("name", name), ("id", &id.to_string())],
        children,
    )
}

fn annotation_class(name: &str, id: i64, value_dimension: bool, children: Vec<Element>) -> Element {
    let mut field = element(
        "field",
        &[("name", "value"), ("class", &T_STRING.to_string())],
        vec![],
    );
    if value_dimension {
        field
            .attributes
            .push(("dimension".to_owned(), "1".to_owned()));
    }
    // `jdk.jfr.ContentType` has no `value` member
    let mut all = if name == "jdk.jfr.ContentType" {
        vec![]
    } else {
        vec![field]
    };
    all.extend(children);
    element(
        "class",
        &[
            ("name", name),
            ("superType", "java.lang.annotation.Annotation"),
            ("id", &id.to_string()),
        ],
        all,
    )
}

/// The fields every event starts with.
fn event_header_fields() -> Vec<Element> {
    vec![
        field(
            "startTime",
            T_LONG,
            vec![
                annotation(T_LABEL, "Start Time"),
                annotation(T_TIMESTAMP, "TICKS"),
            ],
        ),
        field(
            "duration",
            T_LONG,
            vec![
                annotation(T_LABEL, "Duration"),
                annotation(T_TIMESPAN, "TICKS"),
            ],
        ),
    ]
}

fn event_class(name: &str, id: i64, label: &str, category: &str, fields: Vec<Element>) -> Element {
    let mut children = event_header_fields();
    children.extend(fields);
    children.push(annotation(T_NAME, name));
    children.push(annotation(T_LABEL, label));
    children.push(element(
        "annotation",
        &[
            ("class", &T_CATEGORY.to_string()),
            ("value-0", "Minecraft"),
            ("value-1", category),
        ],
        vec![],
    ));
    element(
        "class",
        &[
            ("name", name),
            ("superType", "jdk.jfr.Event"),
            ("id", &id.to_string()),
        ],
        children,
    )
}

fn named_field(name: &str, label: &str, class: i64, extra: Vec<Element>) -> Element {
    let mut annotations = vec![annotation(T_NAME, name), annotation(T_LABEL, label)];
    annotations.extend(extra);
    field(name, class, annotations)
}

/// The fields `ChunkRegionIoEvent` declares (read and write events alike).
fn region_io_fields() -> Vec<Element> {
    vec![
        named_field("regionPosX", "Region X Position", T_INT, vec![]),
        named_field("regionPosZ", "Region Z Position", T_INT, vec![]),
        named_field("localPosX", "Local X Position", T_INT, vec![]),
        named_field("localPosZ", "Local Z Position", T_INT, vec![]),
        named_field("chunkPosX", "Chunk X Position", T_INT, vec![]),
        named_field("chunkPosZ", "Chunk Z Position", T_INT, vec![]),
        named_field("level", "Level Id", T_STRING, vec![]),
        named_field("dimension", "Dimension", T_STRING, vec![]),
        named_field("type", "Type", T_STRING, vec![]),
        named_field("compression", "Compression", T_STRING, vec![]),
        named_field("bytes", "Bytes", T_INT, vec![]),
    ]
}

fn metadata_tree() -> Element {
    let packet_fields = || {
        vec![
            named_field("protocolId", "Protocol Id", T_STRING, vec![]),
            named_field("packetDirection", "Packet Direction", T_STRING, vec![]),
            named_field("packetId", "Packet Id", T_STRING, vec![]),
            named_field("remoteAddress", "Remote Address", T_STRING, vec![]),
            named_field(
                "bytes",
                "Bytes",
                T_INT,
                vec![annotation(T_DATA_AMOUNT, "BYTES")],
            ),
        ]
    };
    let content_type = || {
        element(
            "annotation",
            &[("class", &T_CONTENT_TYPE.to_string())],
            vec![],
        )
    };
    let classes = vec![
        type_class("long", T_LONG, vec![]),
        type_class("int", T_INT, vec![]),
        type_class("boolean", T_BOOLEAN, vec![]),
        type_class("java.lang.String", T_STRING, vec![]),
        annotation_class("jdk.jfr.Label", T_LABEL, false, vec![]),
        annotation_class("jdk.jfr.Description", T_DESCRIPTION, false, vec![]),
        annotation_class("jdk.jfr.Name", T_NAME, false, vec![]),
        annotation_class("jdk.jfr.Category", T_CATEGORY, true, vec![]),
        annotation_class(
            "jdk.jfr.ContentType",
            T_CONTENT_TYPE,
            false,
            vec![
                annotation(T_LABEL, "Content Type"),
                annotation(T_DESCRIPTION, "Semantic meaning of a value"),
            ],
        ),
        annotation_class(
            "jdk.jfr.Timestamp",
            T_TIMESTAMP,
            false,
            vec![
                content_type(),
                annotation(T_LABEL, "Timestamp"),
                annotation(T_DESCRIPTION, "A point in time"),
            ],
        ),
        annotation_class(
            "jdk.jfr.Timespan",
            T_TIMESPAN,
            false,
            vec![
                content_type(),
                annotation(T_LABEL, "Timespan"),
                annotation(
                    T_DESCRIPTION,
                    "A duration, measured in nanoseconds by default",
                ),
            ],
        ),
        annotation_class(
            "jdk.jfr.DataAmount",
            T_DATA_AMOUNT,
            false,
            vec![
                content_type(),
                annotation(T_LABEL, "Data Amount"),
                annotation(T_DESCRIPTION, "Amount of data"),
            ],
        ),
        event_class(
            "minecraft.ServerTickTime",
            T_EVENT_SERVER_TICK,
            "Server Tick Time",
            "Ticking",
            vec![named_field(
                "averageTickDuration",
                "Average Server Tick Duration",
                T_LONG,
                vec![annotation(T_TIMESPAN, "NANOSECONDS")],
            )],
        ),
        event_class(
            "minecraft.PacketReceived",
            T_EVENT_PACKET_RECEIVED,
            "Network Packet Received",
            "Network",
            packet_fields(),
        ),
        event_class(
            "minecraft.PacketSent",
            T_EVENT_PACKET_SENT,
            "Network Packet Sent",
            "Network",
            packet_fields(),
        ),
        event_class(
            "minecraft.ChunkGeneration",
            T_EVENT_CHUNK_GENERATION,
            "Chunk Generation",
            "World Generation",
            vec![
                named_field("worldPosX", "First Block X World Position", T_INT, vec![]),
                named_field("worldPosZ", "First Block Z World Position", T_INT, vec![]),
                named_field("chunkPosX", "Chunk X Position", T_INT, vec![]),
                named_field("chunkPosZ", "Chunk Z Position", T_INT, vec![]),
                named_field("status", "Status", T_STRING, vec![]),
                named_field("level", "Level", T_STRING, vec![]),
            ],
        ),
        event_class(
            "minecraft.ChunkRegionRead",
            T_EVENT_CHUNK_REGION_READ,
            "Region File Read",
            "Storage",
            region_io_fields(),
        ),
        event_class(
            "minecraft.ChunkRegionWrite",
            T_EVENT_CHUNK_REGION_WRITE,
            "Region File Write",
            "Storage",
            region_io_fields(),
        ),
        event_class(
            "minecraft.StructureGeneration",
            T_EVENT_STRUCTURE_GENERATION,
            "Structure Generation",
            "World Generation",
            vec![
                named_field("chunkPosX", "Chunk X Position", T_INT, vec![]),
                named_field("chunkPosZ", "Chunk Z Position", T_INT, vec![]),
                named_field("structure", "Structure", T_STRING, vec![]),
                named_field("level", "Level", T_STRING, vec![]),
                named_field("success", "Success", T_BOOLEAN, vec![]),
            ],
        ),
        event_class(
            "minecraft.LoadWorld",
            T_EVENT_LOAD_WORLD,
            "Create/Load World",
            "World Generation",
            vec![],
        ),
        event_class(
            "minecraft.NetworkSummary",
            T_EVENT_NETWORK_SUMMARY,
            "Network Summary",
            "Network",
            vec![
                named_field("remoteAddress", "Remote Address", T_STRING, vec![]),
                named_field(
                    "sentBytes",
                    "Sent Bytes",
                    T_LONG,
                    vec![annotation(T_DATA_AMOUNT, "BYTES")],
                ),
                named_field("sentPackets", "Sent Packets", T_INT, vec![]),
                named_field(
                    "receivedBytes",
                    "Received Bytes",
                    T_LONG,
                    vec![annotation(T_DATA_AMOUNT, "BYTES")],
                ),
                named_field("receivedPackets", "Received Packets", T_INT, vec![]),
            ],
        ),
    ];
    let gmt_offset_ms = chrono::Local::now().offset().local_minus_utc() as i64 * 1000;
    element(
        "root",
        &[],
        vec![
            element("metadata", &[], classes),
            element(
                "region",
                &[
                    ("locale", "en_US"),
                    ("gmtOffset", &gmt_offset_ms.to_string()),
                    ("dst", "0"),
                ],
                vec![],
            ),
        ],
    )
}

fn intern(strings: &mut IndexMap<String, usize>, value: &str) -> usize {
    let next = strings.len();
    *strings.entry(value.to_owned()).or_insert(next)
}

fn collect_strings(e: &Element, strings: &mut IndexMap<String, usize>) {
    intern(strings, e.name);
    for (k, v) in &e.attributes {
        intern(strings, k);
        intern(strings, v);
    }
    for child in &e.children {
        collect_strings(child, strings);
    }
}

fn write_element(e: &Element, strings: &IndexMap<String, usize>, out: &mut Vec<u8>) {
    put_varlong(out, strings[e.name] as i64);
    put_varlong(out, e.attributes.len() as i64);
    for (k, v) in &e.attributes {
        put_varlong(out, strings[k.as_str()] as i64);
        put_varlong(out, strings[v.as_str()] as i64);
    }
    put_varlong(out, e.children.len() as i64);
    for child in &e.children {
        write_element(child, strings, out);
    }
}

/// The metadata event (`type 0`).
fn metadata_event(start_ticks: i64, metadata_id: i64) -> Vec<u8> {
    let root = metadata_tree();
    let mut strings = IndexMap::new();
    collect_strings(&root, &mut strings);
    let mut payload = Vec::new();
    put_varlong(&mut payload, start_ticks);
    put_varlong(&mut payload, 0);
    put_varlong(&mut payload, metadata_id);
    put_varlong(&mut payload, strings.len() as i64);
    for s in strings.keys() {
        put_string(&mut payload, s);
    }
    write_element(&root, &strings, &mut payload);
    let mut out = Vec::new();
    put_event(&mut out, 0, &payload);
    out
}

/// The (empty) checkpoint event (`type 1`) the chunk header points at.
fn checkpoint_event(start_ticks: i64) -> Vec<u8> {
    let mut payload = Vec::new();
    put_varlong(&mut payload, start_ticks);
    put_varlong(&mut payload, 0); // duration
    put_varlong(&mut payload, 0); // delta to the previous checkpoint
    payload.push(1); // checkpoint type: flush
    put_varlong(&mut payload, 0); // no constant pools
    let mut out = Vec::new();
    put_event(&mut out, 1, &payload);
    out
}

// ---- recorded events (also what the summary is computed from) ----

#[derive(Default)]
struct Aggregation {
    sent_bytes: i64,
    sent_packets: i32,
    received_bytes: i64,
    received_packets: i32,
    started_ticks: i64,
}

#[derive(Default)]
struct CountAndSize {
    count: i64,
    size: i64,
}

struct Recording {
    id: i64,
    destination: PathBuf,
    events_path: PathBuf,
    events: BufWriter<File>,
    events_len: u64,
    start_epoch_nanos: i64,
    start_ticks: i64,
    first_event_ticks: Option<i64>,
    last_event_end_ticks: i64,
    network_by_address: IndexMap<String, Aggregation>,
    next_tick_event: i64,
    next_network_summary: i64,
    current_average_tick_nanos: i64,
    tick_times: Vec<i64>,
    received: IndexMap<(String, String), CountAndSize>,
    sent: IndexMap<(String, String), CountAndSize>,
    chunks_read: IndexMap<ChunkId, CountAndSize>,
    chunks_written: IndexMap<ChunkId, CountAndSize>,
    chunk_gen: Vec<ChunkGenStat>,
    structure_gen: Vec<StructureGenStat>,
    world_creation_nanos: Option<i64>,
}

/// `ChunkIdentification`: level folder, dimension, chunk position.
type ChunkId = (String, String, i32, i32);

/// `ChunkGenStat` (`worldPos` is the chunk's first block).
struct ChunkGenStat {
    duration_nanos: i64,
    chunk_x: i32,
    chunk_z: i32,
    status: String,
    level: String,
}

/// (`StructureGenStat.from` reads `chunkPosX` for both coordinates, so the Z coordinate is never summarised.)
struct StructureGenStat {
    duration_nanos: i64,
    chunk_x: i32,
    structure: String,
    level: String,
    success: bool,
}

/// `ProfiledDuration`: an event whose start was taken when it was created; [`ProfiledDuration::finish`] commits it.
pub struct ProfiledDuration {
    start_ticks: i64,
    event: PendingEvent,
}

enum PendingEvent {
    ChunkGeneration {
        chunk_x: i32,
        chunk_z: i32,
        level: String,
        status: String,
    },
    StructureGeneration {
        chunk_x: i32,
        chunk_z: i32,
        structure: String,
        level: String,
    },
    LoadWorld,
}

impl ProfiledDuration {
    /// `finish(success)`: ends and commits the event (`success` is only a field of the structure event).
    pub fn finish(self, success: bool) {
        let end = get_nanos();
        if let Some(recording) = instance().recording.lock().as_mut() {
            recording.finish_duration(self, end, success);
        }
    }
}

impl Recording {
    fn finish_duration(&mut self, duration: ProfiledDuration, end: i64, success: bool) {
        let ProfiledDuration { start_ticks, event } = duration;
        let nanos = end - start_ticks;
        let mut fields = Vec::new();
        let type_id = match event {
            PendingEvent::ChunkGeneration {
                chunk_x,
                chunk_z,
                level,
                status,
            } => {
                put_varlong(&mut fields, i64::from(chunk_x) * 16);
                put_varlong(&mut fields, i64::from(chunk_z) * 16);
                put_varlong(&mut fields, i64::from(chunk_x));
                put_varlong(&mut fields, i64::from(chunk_z));
                put_string(&mut fields, &status);
                put_string(&mut fields, &level);
                self.chunk_gen.push(ChunkGenStat {
                    duration_nanos: nanos,
                    chunk_x,
                    chunk_z,
                    status,
                    level,
                });
                T_EVENT_CHUNK_GENERATION
            }
            PendingEvent::StructureGeneration {
                chunk_x,
                chunk_z,
                structure,
                level,
            } => {
                put_varlong(&mut fields, i64::from(chunk_x));
                put_varlong(&mut fields, i64::from(chunk_z));
                put_string(&mut fields, &structure);
                put_string(&mut fields, &level);
                fields.push(u8::from(success));
                self.structure_gen.push(StructureGenStat {
                    duration_nanos: nanos,
                    chunk_x,
                    structure,
                    level,
                    success,
                });
                T_EVENT_STRUCTURE_GENERATION
            }
            PendingEvent::LoadWorld => {
                self.world_creation_nanos = Some(nanos);
                T_EVENT_LOAD_WORLD
            }
        };
        let _ = self.commit(type_id, start_ticks, end, &fields);
    }

    /// `ChunkRegionReadEvent` / `ChunkRegionWriteEvent` (committed without a start, so a zero duration).
    fn region_io(&mut self, write: bool, io: &RegionIo<'_>) {
        let now = get_nanos();
        let mut fields = Vec::new();
        for v in [
            io.chunk_x >> 5,
            io.chunk_z >> 5,
            io.chunk_x & 31,
            io.chunk_z & 31,
            io.chunk_x,
            io.chunk_z,
        ] {
            put_varlong(&mut fields, i64::from(v));
        }
        put_string(&mut fields, io.level);
        put_string(&mut fields, io.dimension);
        put_string(&mut fields, io.storage_type);
        put_string(&mut fields, &format!("standard:{}", io.compression_id));
        put_varlong(&mut fields, i64::from(io.bytes));
        let _ = self.commit(
            if write {
                T_EVENT_CHUNK_REGION_WRITE
            } else {
                T_EVENT_CHUNK_REGION_READ
            },
            now,
            now,
            &fields,
        );
        let stats = if write {
            &mut self.chunks_written
        } else {
            &mut self.chunks_read
        };
        let entry = stats
            .entry((
                io.level.to_owned(),
                io.dimension.to_owned(),
                io.chunk_x,
                io.chunk_z,
            ))
            .or_default();
        entry.count += 1;
        entry.size += i64::from(io.bytes);
    }
}

/// What `onRegionFileRead` / `onRegionFileWrite` get: the storage (`RegionStorageInfo`), the chunk, the compression
/// (`RegionFileVersion.getId()`) and the stream length.
pub struct RegionIo<'a> {
    pub level: &'a str,
    pub dimension: &'a str,
    pub storage_type: &'a str,
    pub chunk_x: i32,
    pub chunk_z: i32,
    pub compression_id: i32,
    pub bytes: i32,
}

impl Recording {
    fn commit(
        &mut self,
        type_id: i64,
        start_ticks: i64,
        end_ticks: i64,
        fields: &[u8],
    ) -> io::Result<()> {
        let mut payload = Vec::new();
        put_varlong(&mut payload, start_ticks);
        put_varlong(&mut payload, end_ticks - start_ticks);
        payload.extend_from_slice(fields);
        let mut event = Vec::new();
        put_event(&mut event, type_id, &payload);
        self.events.write_all(&event)?;
        self.events_len += event.len() as u64;
        self.first_event_ticks = Some(
            self.first_event_ticks
                .map_or(start_ticks, |t| t.min(start_ticks)),
        );
        self.last_event_end_ticks = self.last_event_end_ticks.max(end_ticks);
        Ok(())
    }

    fn periodic(&mut self, now: i64) {
        if now >= self.next_tick_event {
            self.next_tick_event = now + SERVER_TICK_PERIOD_NANOS;
            let mut fields = Vec::new();
            put_varlong(&mut fields, self.current_average_tick_nanos);
            self.tick_times.push(self.current_average_tick_nanos);
            let _ = self.commit(T_EVENT_SERVER_TICK, now, now, &fields);
        }
        if now >= self.next_network_summary {
            self.next_network_summary = now + NETWORK_SUMMARY_PERIOD_NANOS;
            let aggregations = std::mem::take(&mut self.network_by_address);
            for (address, agg) in aggregations {
                let mut fields = Vec::new();
                put_string(&mut fields, &address);
                put_varlong(&mut fields, agg.sent_bytes);
                put_varlong(&mut fields, i64::from(agg.sent_packets));
                put_varlong(&mut fields, agg.received_bytes);
                put_varlong(&mut fields, i64::from(agg.received_packets));
                let _ = self.commit(T_EVENT_NETWORK_SUMMARY, agg.started_ticks, now, &fields);
            }
        }
    }

    fn packet(
        &mut self,
        received: bool,
        protocol: &str,
        packet_id: &str,
        remote: &str,
        bytes: i32,
    ) {
        let now = get_nanos();
        let mut fields = Vec::new();
        put_string(&mut fields, protocol);
        put_string(
            &mut fields,
            if received {
                "serverbound"
            } else {
                "clientbound"
            },
        );
        put_string(&mut fields, packet_id);
        put_string(&mut fields, remote);
        put_varlong(&mut fields, i64::from(bytes));
        let _ = self.commit(
            if received {
                T_EVENT_PACKET_RECEIVED
            } else {
                T_EVENT_PACKET_SENT
            },
            now,
            now,
            &fields,
        );
        let agg = self
            .network_by_address
            .entry(remote.to_owned())
            .or_insert_with(|| Aggregation {
                started_ticks: now,
                ..Default::default()
            });
        let stats = if received {
            &mut self.received
        } else {
            &mut self.sent
        };
        let entry = stats
            .entry((protocol.to_owned(), packet_id.to_owned()))
            .or_default();
        entry.count += 1;
        entry.size += i64::from(bytes);
        if received {
            agg.received_packets += 1;
            agg.received_bytes += i64::from(bytes);
        } else {
            agg.sent_packets += 1;
            agg.sent_bytes += i64::from(bytes);
        }
    }

    /// `recording.stop()`: the chunk (header, metadata, events, checkpoint) is written to the destination.
    fn finish(mut self) -> io::Result<(PathBuf, Summary)> {
        self.events.flush()?;
        let end_ticks = get_nanos();
        let duration_nanos = end_ticks - self.start_ticks;
        let metadata = metadata_event(self.start_ticks, self.id);
        let checkpoint = checkpoint_event(end_ticks);
        let metadata_offset = 68u64;
        let checkpoint_offset = metadata_offset + metadata.len() as u64 + self.events_len;
        let chunk_size = checkpoint_offset + checkpoint.len() as u64;
        if let Some(parent) = self.destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out = BufWriter::new(File::create(&self.destination)?);
        out.write_all(b"FLR\0")?;
        out.write_all(&2u16.to_be_bytes())?;
        out.write_all(&1u16.to_be_bytes())?;
        for v in [
            chunk_size as i64,
            checkpoint_offset as i64,
            metadata_offset as i64,
            self.start_epoch_nanos,
            duration_nanos,
            self.start_ticks,
            TICKS_PER_SECOND,
        ] {
            out.write_all(&v.to_be_bytes())?;
        }
        out.write_all(&1i32.to_be_bytes())?; // features: compressed integers
        out.write_all(&metadata)?;
        io::copy(&mut File::open(&self.events_path)?, &mut out)?;
        out.write_all(&checkpoint)?;
        out.flush()?;
        let _ = std::fs::remove_file(&self.events_path);
        let to_epoch_millis =
            |ticks: i64| (self.start_epoch_nanos + (ticks - self.start_ticks)) / 1_000_000;
        let summary = Summary {
            started_epoch_millis: self.first_event_ticks.map_or(0, to_epoch_millis),
            ended_epoch_millis: if self.first_event_ticks.is_some() {
                to_epoch_millis(self.last_event_end_ticks)
            } else {
                0
            },
            tick_times_nanos: self.tick_times,
            received: self.received,
            sent: self.sent,
            chunks_read: self.chunks_read,
            chunks_written: self.chunks_written,
            chunk_gen: self.chunk_gen,
            structure_gen: self.structure_gen,
            world_creation_nanos: self.world_creation_nanos,
        };
        Ok((self.destination, summary))
    }
}

// ---- the summary (`JfrStatsResult.asJson` of what was recorded) ----

struct Summary {
    started_epoch_millis: i64,
    ended_epoch_millis: i64,
    tick_times_nanos: Vec<i64>,
    received: IndexMap<(String, String), CountAndSize>,
    sent: IndexMap<(String, String), CountAndSize>,
    chunks_read: IndexMap<ChunkId, CountAndSize>,
    chunks_written: IndexMap<ChunkId, CountAndSize>,
    chunk_gen: Vec<ChunkGenStat>,
    structure_gen: Vec<StructureGenStat>,
    world_creation_nanos: Option<i64>,
}

/// Guava `Quantiles.scale(100).indexes(50, 75, 90, 99).compute(dataset)`, highest percentile first.
fn percentiles(values: &[f64]) -> Vec<(i32, f64)> {
    if values.is_empty() {
        return Vec::new();
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let n = sorted.len() as i64;
    [99, 90, 75, 50]
        .into_iter()
        .map(|index| {
            let numerator = i64::from(index) * (n - 1);
            let quotient = (numerator / 100) as usize;
            let remainder = numerator % 100;
            let value = if remainder == 0 {
                sorted[quotient]
            } else {
                sorted[quotient]
                    + (sorted[quotient + 1] - sorted[quotient]) * remainder as f64 / 100.0
            };
            (index, value)
        })
        .collect()
}

/// A JSON number as Gson writes it (`Double.toString`: `1.0E7`, bare `Infinity`/`NaN`), marked with control
/// characters so [`unquote`] can turn the string into a bare token.
fn raw(token: String) -> Value {
    Value::String(format!("\u{1}{token}\u{1}"))
}

fn num(value: f64) -> Value {
    raw(java_double_to_string(value))
}

/// A `float` property (`IoSummary.CountAndSize.averageSize`, `GcHeapStat.gcOverHead`): `Float.toString`.
fn float(value: f32) -> Value {
    raw(java_float_to_string(value))
}

/// Replaces every marked string of [`raw`] by its bare token (`serde_json` writes the marker as `\u0001`).
fn unquote(text: &str) -> String {
    const MARK: &str = "\\u0001";
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(&format!("\"{MARK}")) {
        out.push_str(&rest[..start]);
        let body = &rest[start + 1 + MARK.len()..];
        let end = body
            .find(&format!("{MARK}\""))
            .expect("a marked token ends with its marker");
        out.push_str(&body[..end]);
        rest = &body[end + MARK.len() + 1..];
    }
    out.push_str(rest);
    out
}

/// `Duration.toNanos()` values to the `TimedStatSummary` JSON that `chunkGen` and `structureGen` share.
struct TimedSummary {
    count: usize,
    total_nanos: i64,
    fastest: usize,
    slowest: usize,
    second_slowest: Option<usize>,
    percentiles: Vec<(i32, f64)>,
}

/// `TimedStatSummary.summary`: indices into `durations` (sorted by duration, ties keep their order).
fn timed_summary(durations: &[i64]) -> Option<TimedSummary> {
    if durations.is_empty() {
        return None;
    }
    let mut order: Vec<usize> = (0..durations.len()).collect();
    order.sort_by_key(|&i| durations[i]);
    let sorted: Vec<f64> = order.iter().map(|&i| durations[i] as f64).collect();
    Some(TimedSummary {
        count: durations.len(),
        total_nanos: durations.iter().sum(),
        fastest: order[0],
        slowest: order[order.len() - 1],
        second_slowest: (order.len() > 1).then(|| order[order.len() - 2]),
        percentiles: percentiles(&sorted),
    })
}

fn percentiles_json(summary: &TimedSummary) -> Value {
    let mut map = Map::new();
    for (p, v) in &summary.percentiles {
        map.insert(format!("p{p}"), num(*v));
    }
    Value::Object(map)
}

impl Summary {
    fn duration_seconds(&self) -> f64 {
        ((self.ended_epoch_millis - self.started_epoch_millis) / 1000) as f64
    }

    /// `IoSummary` as `JfrResultJsonSerializer.ioSummary` writes it; `identify` writes the identifier members.
    fn io_summary<K>(
        &self,
        stats: &IndexMap<K, CountAndSize>,
        identify: impl Fn(&K, &mut Map<String, Value>),
    ) -> Value {
        let total_count: i64 = stats.values().map(|s| s.count).sum();
        let total_size: i64 = stats.values().map(|s| s.size).sum();
        let seconds = self.duration_seconds();
        let mut contributors: Vec<(&K, &CountAndSize)> = stats.iter().collect();
        contributors.sort_by(|a, b| b.1.size.cmp(&a.1.size).then(b.1.count.cmp(&a.1.count)));
        let top: Vec<Value> = contributors
            .into_iter()
            .take(10)
            .map(|(id, s)| {
                let mut map = Map::new();
                identify(id, &mut map);
                map.insert("totalBytes".into(), json!(s.size));
                map.insert("count".into(), json!(s.count));
                map.insert("averageSize".into(), float(s.size as f32 / s.count as f32));
                Value::Object(map)
            })
            .collect();
        json!({"totalBytes": total_size, "count": total_count, "bytesPerSecond": num(total_size as f64 / seconds), "countPerSecond": num(total_count as f64 / seconds), "topContributors": top})
    }

    fn packet_summary(&self, stats: &IndexMap<(String, String), CountAndSize>) -> Value {
        self.io_summary(stats, |(protocol, packet), map| {
            map.insert("protocolId".into(), json!(protocol));
            map.insert("packetId".into(), json!(packet));
        })
    }

    fn chunk_summary(&self, stats: &IndexMap<ChunkId, CountAndSize>) -> Value {
        self.io_summary(stats, |(level, dimension, x, z), map| {
            map.insert("level".into(), json!(level));
            map.insert("dimension".into(), json!(dimension));
            map.insert("x".into(), json!(x));
            map.insert("z".into(), json!(z));
        })
    }

    /// `JfrResultJsonSerializer.chunkGen`: per status, the statuses with the longest total time first.
    fn chunk_gen_json(&self) -> Value {
        let mut by_status: IndexMap<&str, Vec<&ChunkGenStat>> = IndexMap::new();
        for stat in &self.chunk_gen {
            by_status.entry(&stat.status).or_default().push(stat);
        }
        let mut summaries: Vec<(&str, Vec<&ChunkGenStat>, TimedSummary)> = by_status
            .into_iter()
            .filter_map(|(status, stats)| {
                let durations: Vec<i64> = stats.iter().map(|s| s.duration_nanos).collect();
                timed_summary(&durations).map(|summary| (status, stats, summary))
            })
            .collect();
        if summaries.is_empty() {
            return json!({});
        }
        summaries.sort_by_key(|s| std::cmp::Reverse(s.2.total_nanos));
        let total: f64 = summaries.iter().map(|s| s.2.total_nanos as f64).sum();
        let stat_json = |s: &ChunkGenStat| {
            // `ChunkGenStat.from` reads `chunkPosX` for both coordinates of the chunk position
            json!({"durationNanos": s.duration_nanos, "level": s.level, "chunkPosX": s.chunk_x, "chunkPosZ": s.chunk_x, "worldPosX": s.chunk_x * 16, "worldPosZ": s.chunk_z * 16})
        };
        let statuses: Vec<Value> = summaries
            .iter()
            .map(|(status, stats, summary)| {
                json!({
                    "state": status,
                    "count": summary.count,
                    "durationNanosTotal": summary.total_nanos,
                    "durationNanosAvg": summary.total_nanos / summary.count as i64,
                    "durationNanosPercentiles": percentiles_json(summary),
                    "fastest": stat_json(stats[summary.fastest]),
                    "slowest": stat_json(stats[summary.slowest]),
                    "secondSlowest": summary.second_slowest.map_or(Value::Null, |i| stat_json(stats[i])),
                })
            })
            .collect();
        json!({"durationNanosTotal": num(total), "status": statuses})
    }

    /// `JfrResultJsonSerializer.structureGen`: the per-structure groups come in `HashMap<String, ..>` order; the
    /// fastest and slowest entries are those of all structures together.
    fn structure_gen_json(&self) -> Value {
        let durations: Vec<i64> = self
            .structure_gen
            .iter()
            .map(|s| s.duration_nanos)
            .collect();
        let Some(overall) = timed_summary(&durations) else {
            return json!({});
        };
        let stat_json = |s: &StructureGenStat| json!({"durationNanos": s.duration_nanos, "chunkPosX": s.chunk_x, "chunkPosZ": s.chunk_x, "structureName": s.structure, "level": s.level, "success": s.success});
        let mut by_name: IndexMap<&str, Vec<&StructureGenStat>> = IndexMap::new();
        for stat in &self.structure_gen {
            by_name.entry(&stat.structure).or_default().push(stat);
        }
        let groups = hash_map_ordered(
            by_name.into_iter().collect::<Vec<_>>(),
            |(name, _)| java_string_hash(name),
            16,
        );
        let structures: Vec<Value> = groups
            .into_iter()
            .map(|(name, stats)| {
                let durations: Vec<i64> = stats.iter().map(|s| s.duration_nanos).collect();
                let summary = timed_summary(&durations).expect("a group is not empty");
                json!({
                    "name": name,
                    "count": summary.count,
                    "durationNanosTotal": summary.total_nanos,
                    "durationNanosAvg": summary.total_nanos / summary.count as i64,
                    "durationNanosPercentiles": percentiles_json(&summary),
                })
            })
            .collect();
        json!({
            "structure": structures,
            "fastest": stat_json(&self.structure_gen[overall.fastest]),
            "slowest": stat_json(&self.structure_gen[overall.slowest]),
            "secondSlowest": overall.second_slowest.map_or(Value::Null, |i| stat_json(&self.structure_gen[i])),
        })
    }

    /// `JfrResultJsonSerializer.format`. The JVM-only sections (GC heap, CPU load, thread allocation, file IO) are
    /// formatted over empty statistics, which is what they are in this server.
    fn as_json(&self) -> String {
        let seconds = self.duration_seconds();
        let duration_ms = self.ended_epoch_millis - self.started_epoch_millis;
        let empty_io = || json!({"totalBytes": 0, "count": 0, "bytesPerSecond": num(0.0 / seconds), "countPerSecond": num(0.0 / seconds), "topContributors": []});
        let cpu = || json!({"min": num(f64::INFINITY), "average": num(0.0), "max": num(f64::NEG_INFINITY)});
        let mut root = Map::new();
        root.insert("startedEpoch".into(), json!(self.started_epoch_millis));
        root.insert("endedEpoch".into(), json!(self.ended_epoch_millis));
        root.insert("durationMs".into(), json!(duration_ms));
        if let Some(nanos) = self.world_creation_nanos {
            root.insert("worldGenDurationMs".into(), json!(nanos / 1_000_000));
        }
        root.insert(
            "heap".into(),
            json!({"allocationRateBytesPerSecond": num(0.0), "gcCount": 0, "gcOverHeadPercent": float(0.0_f32 / duration_ms as f32), "gcTotalDurationMs": 0}),
        );
        root.insert(
            "cpuPercent".into(),
            json!({"jvm": cpu(), "userJvm": cpu(), "system": cpu()}),
        );
        root.insert("network".into(), json!({"sent": self.packet_summary(&self.sent), "received": self.packet_summary(&self.received)}));
        root.insert("fileIO".into(), json!({"write": empty_io(), "read": empty_io(), "chunksRead": self.chunk_summary(&self.chunks_read), "chunksWritten": self.chunk_summary(&self.chunks_written)}));
        if !self.tick_times_nanos.is_empty() {
            let ms: Vec<f64> = self
                .tick_times_nanos
                .iter()
                .map(|n| *n as f64 / 1_000_000.0)
                .collect();
            let mut tick = Map::new();
            tick.insert(
                "minMs".into(),
                num(ms.iter().copied().fold(f64::INFINITY, f64::min)),
            );
            tick.insert(
                "averageMs".into(),
                num(ms.iter().sum::<f64>() / ms.len() as f64),
            );
            tick.insert(
                "maxMs".into(),
                num(ms.iter().copied().fold(f64::NEG_INFINITY, f64::max)),
            );
            for (p, v) in percentiles(&ms) {
                tick.insert(format!("p{p}"), num(v));
            }
            root.insert("serverTick".into(), Value::Object(tick));
        }
        root.insert("threadAllocation".into(), json!([]));
        root.insert("chunkGen".into(), self.chunk_gen_json());
        root.insert("structureGen".into(), self.structure_gen_json());
        unquote(&serde_json::to_string_pretty(&Value::Object(root)).expect("plain JSON values"))
    }
}

// ---- the profiler ----

/// `JfrProfiler.INSTANCE`.
pub struct JfrProfiler {
    recording: Mutex<Option<Recording>>,
    running: AtomicBool,
    next_recording_id: Mutex<i64>,
}

/// `JvmProfiler.INSTANCE`.
pub fn instance() -> &'static JfrProfiler {
    static INSTANCE: OnceLock<JfrProfiler> = OnceLock::new();
    INSTANCE.get_or_init(|| JfrProfiler {
        recording: Mutex::new(None),
        running: AtomicBool::new(false),
        next_recording_id: Mutex::new(1),
    })
}

impl JfrProfiler {
    /// `isAvailable()`: the recorder is part of this server.
    pub fn is_available(&self) -> bool {
        true
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    /// `start(environment)`: `false` when a recording is already running or it cannot be created.
    pub fn start(&self, environment: &str) -> bool {
        mc_world::region::region_io_listener::set_listener(instance());
        self.start_in(environment, Path::new("debug"))
    }

    fn start_in(&self, environment: &str, debug_dir: &Path) -> bool {
        let mut slot = self.recording.lock();
        if slot.is_some() {
            log::warn!("Profiling already in progress");
            return false;
        }
        let timestamp = chrono::Local::now().format("%Y-%m-%d-%H%M%S").to_string();
        let name = format!(
            "{environment}-{}-{timestamp}",
            crate::server::packs::GAME_VERSION
        );
        let destination = debug_dir.join(format!("{environment}-{timestamp}.jfr"));
        let events_path = debug_dir.join(format!("{environment}-{timestamp}.jfr.events"));
        let created = std::fs::create_dir_all(debug_dir).and_then(|()| File::create(&events_path));
        let events = match created {
            Ok(file) => BufWriter::new(file),
            Err(e) => {
                log::warn!("Failed to start jfr profiling\n{e}");
                return false;
            }
        };
        let now = get_nanos();
        let id = {
            let mut next = self.next_recording_id.lock();
            let id = *next;
            *next += 1;
            id
        };
        let start_epoch_nanos = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
        log::info!(
            "Started flight recorder profiling id({id}):name({name}) - will dump to {} on exit or stop command",
            destination.display()
        );
        *slot = Some(Recording {
            id,
            destination,
            events_path,
            events,
            events_len: 0,
            start_epoch_nanos,
            start_ticks: now,
            first_event_ticks: None,
            last_event_end_ticks: now,
            network_by_address: IndexMap::new(),
            next_tick_event: now,
            next_network_summary: now + NETWORK_SUMMARY_PERIOD_NANOS,
            current_average_tick_nanos: 0,
            tick_times: Vec::new(),
            received: IndexMap::new(),
            sent: IndexMap::new(),
            chunks_read: IndexMap::new(),
            chunks_written: IndexMap::new(),
            chunk_gen: Vec::new(),
            structure_gen: Vec::new(),
            world_creation_nanos: None,
        });
        self.running.store(true, Ordering::Relaxed);
        true
    }

    /// `stop()`: dumps the recording and reports its summary; the error is `IllegalStateException`'s message when no
    /// recording runs.
    pub fn stop(&self) -> Result<PathBuf, String> {
        let recording = self.recording.lock().take();
        self.running.store(false, Ordering::Relaxed);
        let Some(recording) = recording else {
            return Err("Not currently profiling".to_owned());
        };
        let (destination, summary) = recording.finish().map_err(|e| e.to_string())?;
        report_summary(&destination, &summary);
        Ok(destination)
    }

    /// `setDumpOnExit(true)`: a still running recording is dumped when the server shuts down.
    pub fn stop_on_exit(&self) {
        if self.is_running() {
            let _ = self.stop();
        }
    }

    /// `onServerTick(currentAverageTickTime)`; also runs the periodic events.
    pub fn on_server_tick(&self, current_average_tick_time_millis: f32) {
        if !self.is_running() {
            return;
        }
        if let Some(recording) = self.recording.lock().as_mut() {
            recording.current_average_tick_nanos =
                (1_000_000.0_f32 * current_average_tick_time_millis) as i64;
            recording.periodic(get_nanos());
        }
    }

    /// `onPacketReceived(protocol, packetId, remoteAddress, readableBytes)`.
    pub fn on_packet_received(
        &self,
        protocol: &str,
        packet_id: &str,
        remote_address: &str,
        bytes: i32,
    ) {
        if self.is_running()
            && let Some(recording) = self.recording.lock().as_mut()
        {
            recording.packet(true, protocol, packet_id, remote_address, bytes);
        }
    }

    /// `onPacketSent(protocol, packetId, remoteAddress, writtenBytes)`.
    pub fn on_packet_sent(
        &self,
        protocol: &str,
        packet_id: &str,
        remote_address: &str,
        bytes: i32,
    ) {
        if self.is_running()
            && let Some(recording) = self.recording.lock().as_mut()
        {
            recording.packet(false, protocol, packet_id, remote_address, bytes);
        }
    }
}

impl JfrProfiler {
    fn begin(&self, event: PendingEvent) -> Option<ProfiledDuration> {
        self.is_running().then(|| ProfiledDuration {
            start_ticks: get_nanos(),
            event,
        })
    }

    /// `onWorldLoadedStarted()`.
    pub fn on_world_loaded_started(&self) -> Option<ProfiledDuration> {
        self.begin(PendingEvent::LoadWorld)
    }

    /// `onChunkGenerate(pos, dimension, name)`: `name` is the target status, `minecraft:<status>`.
    pub fn on_chunk_generate(
        &self,
        chunk_x: i32,
        chunk_z: i32,
        dimension: &str,
        status: &str,
    ) -> Option<ProfiledDuration> {
        self.begin(PendingEvent::ChunkGeneration {
            chunk_x,
            chunk_z,
            level: dimension.to_owned(),
            status: status.to_owned(),
        })
    }

    /// `onStructureGenerate(sourceChunkPos, dimension, structure)`: `structure` is the registered name.
    pub fn on_structure_generate(
        &self,
        chunk_x: i32,
        chunk_z: i32,
        dimension: &str,
        structure: &str,
    ) -> Option<ProfiledDuration> {
        self.begin(PendingEvent::StructureGeneration {
            chunk_x,
            chunk_z,
            structure: structure.to_owned(),
            level: dimension.to_owned(),
        })
    }

    /// `onRegionFileRead(info, pos, version, readBytes)`.
    pub fn on_region_file_read(&self, io: &RegionIo<'_>) {
        if self.is_running()
            && let Some(recording) = self.recording.lock().as_mut()
        {
            recording.region_io(false, io);
        }
    }

    /// `onRegionFileWrite(info, pos, version, writtenBytes)`.
    pub fn on_region_file_write(&self, io: &RegionIo<'_>) {
        if self.is_running()
            && let Some(recording) = self.recording.lock().as_mut()
        {
            recording.region_io(true, io);
        }
    }
}

impl JfrProfiler {
    fn region_file_io(
        &self,
        write: bool,
        info: &mc_world::region::RegionStorageInfo,
        pos: &mc_core::pos::chunk_pos::ChunkPos,
        version_id: i32,
        stream_length: i32,
    ) {
        if !self.is_running() {
            return;
        }
        let dimension = info.dimension.to_string();
        let io = RegionIo {
            level: &info.level,
            dimension: &dimension,
            storage_type: &info.r#type,
            chunk_x: pos.x,
            chunk_z: pos.z,
            compression_id: version_id,
            bytes: stream_length,
        };
        if write {
            self.on_region_file_write(&io);
        } else {
            self.on_region_file_read(&io);
        }
    }
}

impl mc_world::region::region_io_listener::RegionIoListener for JfrProfiler {
    fn on_region_file_read(
        &self,
        info: &mc_world::region::RegionStorageInfo,
        pos: &mc_core::pos::chunk_pos::ChunkPos,
        version_id: i32,
        stream_length: i32,
    ) {
        self.region_file_io(false, info, pos, version_id, stream_length);
    }

    fn on_region_file_write(
        &self,
        info: &mc_world::region::RegionStorageInfo,
        pos: &mc_core::pos::chunk_pos::ChunkPos,
        version_id: i32,
        stream_length: i32,
    ) {
        self.region_file_io(true, info, pos, version_id, stream_length);
    }
}

/// `SummaryReporter.recordingStopped`.
fn report_summary(result: &Path, summary: &Summary) {
    log::info!("Dumped flight recorder profiling to {}", result.display());
    let json = summary.as_json();
    log::info!("{json}");
    let file_name = result
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stem = file_name.split(".jfr").next().unwrap_or(&file_name);
    let report = result.with_file_name(format!("jfr-report-{stem}.json"));
    match std::fs::write(&report, &json) {
        Ok(()) => log::info!("Dumped recording summary to {}", report.display()),
        Err(e) => log::warn!("Failed to output JFR report\n{e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varints_match_jfr_compressed_ints() {
        let mut v = Vec::new();
        put_varlong(&mut v, 127);
        put_varlong(&mut v, 128);
        put_varlong(&mut v, -1);
        assert_eq!(&v[..3], &[127, 0x80, 1]);
        assert_eq!(
            v.len(),
            3 + 9,
            "-1 is nine bytes: eight 0xff continuation bytes and a full last byte"
        );
    }

    #[test]
    fn percentiles_interpolate_like_guava() {
        let p = percentiles(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        assert_eq!(p[0], (99, 4.96));
        assert_eq!(p[3], (50, 3.0));
    }

    #[test]
    fn writes_a_chunk_the_jfr_tool_reads() {
        let dir = tempfile::tempdir().unwrap();
        let profiler = JfrProfiler {
            recording: Mutex::new(None),
            running: AtomicBool::new(false),
            next_recording_id: Mutex::new(1),
        };
        assert!(profiler.start_in("server", dir.path()));
        assert!(
            !profiler.start_in("server", dir.path()),
            "already in progress"
        );
        profiler.on_server_tick(12.5);
        profiler.on_packet_sent("play", "minecraft:level_event", "/127.0.0.1:1", 42);
        profiler.on_packet_received("play", "minecraft:move_player_pos", "/127.0.0.1:1", 25);
        let path = profiler.stop().unwrap();
        if let Ok(out) = std::env::var("MC_JFR_TEST_OUT") {
            std::fs::copy(&path, out).unwrap();
        }
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[..4], b"FLR\0");
        assert_eq!(
            i64::from_be_bytes(bytes[8..16].try_into().unwrap()) as usize,
            bytes.len(),
            "chunk size"
        );
        assert_eq!(profiler.stop().unwrap_err(), "Not currently profiling");
    }

    #[test]
    fn records_world_generation_storage_and_load_events() {
        let dir = tempfile::tempdir().unwrap();
        let profiler = instance();
        assert!(
            profiler
                .on_chunk_generate(0, 0, "minecraft:overworld", "minecraft:biomes")
                .is_none()
        );
        assert!(profiler.start_in("server", dir.path()));
        profiler
            .on_chunk_generate(3, -2, "minecraft:overworld", "minecraft:biomes")
            .unwrap()
            .finish(true);
        profiler
            .on_structure_generate(1, 2, "minecraft:overworld", "minecraft:village_plains")
            .unwrap()
            .finish(false);
        profiler.on_world_loaded_started().unwrap().finish(true);
        let io = RegionIo {
            level: "world",
            dimension: "minecraft:overworld",
            storage_type: "chunk",
            chunk_x: 33,
            chunk_z: -1,
            compression_id: 2,
            bytes: 1234,
        };
        profiler.on_region_file_read(&io);
        profiler.on_region_file_write(&io);
        let path = profiler.stop().unwrap();
        let bytes = std::fs::read(&path).unwrap();
        for name in [
            "minecraft.ChunkGeneration",
            "minecraft.ChunkRegionRead",
            "minecraft.ChunkRegionWrite",
            "minecraft.StructureGeneration",
            "minecraft.LoadWorld",
        ] {
            assert!(
                bytes.windows(name.len()).any(|w| w == name.as_bytes()),
                "{name} is in the metadata"
            );
        }
        let report = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("jfr-report-")
            })
            .unwrap();
        // Gson writes the non-finite rates of a recording shorter than a second as bare words
        let text = std::fs::read_to_string(report).unwrap();
        assert!(text.contains("\"bytesPerSecond\": Infinity"), "{text}");
        let text = text
            .replace("-Infinity", "null")
            .replace("Infinity", "null")
            .replace("NaN", "null");
        let json: Value = serde_json::from_str(&text).unwrap();
        assert!(json["worldGenDurationMs"].is_number());
        let chunk = &json["chunkGen"]["status"][0];
        assert_eq!(chunk["state"], "minecraft:biomes");
        assert_eq!(chunk["count"], 1);
        // vanilla reads `chunkPosX` for both coordinates of the position
        assert_eq!(chunk["fastest"]["chunkPosZ"], 3);
        assert_eq!(chunk["fastest"]["worldPosZ"], -32);
        assert_eq!(
            json["structureGen"]["structure"][0]["name"],
            "minecraft:village_plains"
        );
        assert_eq!(json["structureGen"]["slowest"]["success"], false);
        let read = &json["fileIO"]["chunksRead"];
        assert_eq!(read["totalBytes"], 1234);
        assert_eq!(read["topContributors"][0]["x"], 33);
        assert_eq!(json["fileIO"]["chunksWritten"]["count"], 1);
        assert!(chunk["durationNanosPercentiles"]["p50"].is_number());
        assert!(read["bytesPerSecond"].is_null());
    }

    #[test]
    fn doubles_are_written_like_gson() {
        let text = unquote(
            &serde_json::to_string_pretty(
                &json!({"a": num(1.5e7), "b": num(f64::NAN), "c": float(0.1)}),
            )
            .unwrap(),
        );
        assert_eq!(text, "{\n  \"a\": 1.5E7,\n  \"b\": NaN,\n  \"c\": 0.1\n}");
    }
}
