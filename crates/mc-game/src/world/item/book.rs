//! `WritableBookItem` / `WrittenBookItem`, `WrittenBookContent.resolve`, `ServerPlayer.openItemGui` and
//! `ServerGamePacketListenerImpl.handleEditBook` (`updateBookContents` / `signBook`).

use super::player_systems::with_current_menu;
use super::{InteractionHand, InteractionResult, ItemBehaviour, ItemStack};
use crate::commands::component_utils::{ResolutionContext, resolve};
use crate::commands::server_access::enter_server;
use crate::register_item_class;
use crate::server::network::game_packets::PlayCtx;
use crate::server::network::game_packets::hooks::{PLAY_HANDLERS, PlayHandlers};
use crate::server::network::text_filter::FilteredText;
use crate::world::container::Container;
use crate::world::entity::ServerPlayer;
use crate::world::entity::equipment::EquipmentSlot;
use crate::server::level::ServerLevel;
use crate::stats::Stat;
use linkme::distributed_slice;
use mc_data::{DataComponentType, Item};
use mc_protocol::components::{Filterable, WRITABLE_BOOK_CONTENT, WRITTEN_BOOK_CONTENT, WritableBookContent, WrittenBookContent};
use mc_protocol::packets::play::{ClientboundOpenBookPacket, ServerboundEditBookPacket};
use mc_text::Component;
use mc_text::util::encodes_longer_than;

/// `WritableBookItem.use` / `WrittenBookItem.use`.
struct BookItem;

impl ItemBehaviour for BookItem {
    fn use_item(&self, level: &mut ServerLevel, player: &mut ServerPlayer, hand: InteractionHand) -> InteractionResult {
        player.pending_open_book = Some(hand);
        let item = player.get_item_in_hand(hand).item();
        player.award_stat_once(level, Stat::Used(item));
        InteractionResult::SUCCESS
    }
}

register_item_class!("WritableBookItem", |_| Box::new(BookItem));
register_item_class!("WrittenBookItem", |_| Box::new(BookItem));

fn hand_stack_mut(player: &mut ServerPlayer, hand: InteractionHand) -> &mut ItemStack {
    match hand {
        InteractionHand::MainHand => player.player.inventory.get_selected_item_mut(),
        InteractionHand::OffHand => player.player.inventory.equipment_get_mut(EquipmentSlot::OffHand),
    }
}

/// `Filterable.resolve(function)`.
fn resolve_filterable<T, U>(page: &Filterable<T>, mut f: impl FnMut(&T) -> Option<U>) -> Option<Filterable<U>> {
    let raw = f(&page.raw)?;
    let filtered = match &page.filtered {
        Some(filtered) => Some(f(filtered)?),
        None => None,
    };
    Some(Filterable { raw, filtered })
}

/// `WrittenBookContent.resolvePage`: components that fail to resolve stay as they are, resolved pages that no longer
/// fit a page (`isPageTooLarge`) abort the whole resolution.
fn resolve_page(context: &ResolutionContext<'_>, page: &Filterable<Component>) -> Option<Filterable<Component>> {
    resolve_filterable(page, |component| match resolve(context, component) {
        Ok(new_component) => {
            let too_large = new_component.to_json().is_ok_and(|json| encodes_longer_than(&json, WrittenBookContent::PAGE_LENGTH));
            if too_large { None } else { Some(new_component) }
        }
        Err(_) => Some(component.clone()),
    })
}

/// `WrittenBookContent.resolve(context, registries)`: `None` when already resolved or a page cannot be resolved.
pub fn resolve_written_book(content: &WrittenBookContent, context: &ResolutionContext<'_>) -> Option<WrittenBookContent> {
    if content.resolved {
        return None;
    }
    let pages = content.pages.iter().map(|page| resolve_page(context, page)).collect::<Option<Vec<_>>>()?;
    Some(WrittenBookContent { title: content.title.clone(), author: content.author.clone(), generation: content.generation, pages, resolved: true })
}

/// `WrittenBookContent.resolveForItem(stack, context, registries)`: whether the stack content was replaced by the
/// resolved one (a failed resolution only marks the content resolved).
pub fn resolve_for_item(stack: &mut ItemStack, context: &ResolutionContext<'_>) -> bool {
    let Some(content) = stack.get(&WRITTEN_BOOK_CONTENT).cloned() else { return false };
    if content.resolved {
        return false;
    }
    match resolve_written_book(&content, context) {
        Some(resolved) => {
            stack.set(WRITTEN_BOOK_CONTENT, resolved);
            true
        }
        None => {
            stack.set(WRITTEN_BOOK_CONTENT, WrittenBookContent { resolved: true, ..content });
            false
        }
    }
}

/// `ServerPlayer.openItemGui(stack, hand)` for the hand raised by the book item: resolves the components of a
/// written book (`containerMenu.broadcastChanges()` when the pages changed) and sends `ClientboundOpenBookPacket`.
pub fn flush_open_item_gui(ctx: &mut PlayCtx) {
    let Some(hand) = ctx.player.pending_open_book.take() else { return };
    let mut stack = ctx.player.get_item_in_hand(hand);
    if !stack.has(DataComponentType::WRITTEN_BOOK_CONTENT) {
        return;
    }
    if let Some(source) = ctx.create_command_source_stack() {
        let changed = enter_server(&mut *ctx.server, || resolve_for_item(&mut stack, &ResolutionContext::create(&source)));
        *hand_stack_mut(ctx.player, hand) = stack;
        if changed {
            with_current_menu(ctx, |menu, p| menu.broadcast_changes(p));
        }
    }
    ctx.send(&ClientboundOpenBookPacket { hand: hand.to_protocol() });
}

/// `filterableFromOutgoing(text)`.
fn filterable_from_outgoing(text: &FilteredText, text_filtering_enabled: bool) -> Filterable<String> {
    if text_filtering_enabled {
        Filterable::pass_through(text.filtered_or_empty())
    } else {
        Filterable { raw: text.raw.clone(), filtered: text.is_filtered().then(|| text.filtered_or_empty()) }
    }
}

/// `updateBookContents(contents, slot)`.
fn update_book_contents(player: &mut ServerPlayer, contents: &[FilteredText], slot: usize) {
    let enabled = player.text_filtering_enabled;
    let Some(carried) = player.player.inventory.get_item_mut(slot) else { return };
    if carried.has(DataComponentType::WRITABLE_BOOK_CONTENT) {
        let pages = contents.iter().map(|c| filterable_from_outgoing(c, enabled)).collect();
        carried.set(WRITABLE_BOOK_CONTENT, WritableBookContent { pages });
    }
}

/// `signBook(title, contents, slot)`.
fn sign_book(player: &mut ServerPlayer, title: &FilteredText, contents: &[FilteredText], slot: usize) {
    let enabled = player.text_filtering_enabled;
    let carried = player.player.inventory.get_item(slot);
    if !carried.has(DataComponentType::WRITABLE_BOOK_CONTENT) {
        return;
    }
    let mut written_book = carried.transmute_copy(Item::WRITTEN_BOOK);
    written_book.remove(DataComponentType::WRITABLE_BOOK_CONTENT);
    let pages = contents
        .iter()
        .map(|page| {
            let f = filterable_from_outgoing(page, enabled);
            Filterable { raw: Component::literal(f.raw), filtered: f.filtered.map(Component::literal) }
        })
        .collect();
    let author = player.get_plain_text_name();
    written_book.set(
        WRITTEN_BOOK_CONTENT,
        WrittenBookContent { title: filterable_from_outgoing(title, enabled), author, generation: 0, pages, resolved: true },
    );
    player.player.inventory.set_item(slot, written_book);
}

/// `handleEditBook` after the hotbar/offhand slot check and the text filter (`filtered` = the filter's answer per
/// string: the title first when the packet has one, then the pages).
pub fn edit_book(player: &mut ServerPlayer, packet: &ServerboundEditBookPacket, filter: impl Fn(&str) -> FilteredText) {
    let slot = packet.slot as usize;
    let pages = packet.pages.iter().map(|p| filter(p)).collect::<Vec<_>>();
    match &packet.title {
        Some(title) => sign_book(player, &filter(title), &pages, slot),
        None => update_book_contents(player, &pages, slot),
    }
}

struct BookHandlers;

impl PlayHandlers for BookHandlers {
    /// `handleEditBook`: the dedicated server's `TextFilter` passes everything through (see `text_filter.rs`).
    fn handle_edit_book(&self, ctx: &mut PlayCtx, packet: &ServerboundEditBookPacket) {
        edit_book(ctx.player, packet, FilteredText::pass_through);
    }
}

#[distributed_slice(PLAY_HANDLERS)]
static BOOK_HANDLERS: &dyn PlayHandlers = &BookHandlers;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::component_utils::LimitBehavior;
    use std::cell::Cell;

    fn context() -> ResolutionContext<'static> {
        ResolutionContext { source: None, default_scoreboard_entity: None, depth_limit: 100, depth_limit_behavior: LimitBehavior::StopProcessingAndCopyRemaining, resolved_component_limit: 65536, resolved_component_count: Cell::new(0) }
    }

    fn book(resolved: bool, pages: Vec<Filterable<Component>>) -> ItemStack {
        let mut stack = ItemStack::new(Item::WRITTEN_BOOK);
        stack.set(WRITTEN_BOOK_CONTENT, WrittenBookContent { title: Filterable::pass_through("t".into()), author: "a".into(), generation: 0, pages, resolved });
        stack
    }

    #[test]
    fn resolve_for_item_replaces_pages_and_marks_resolved() {
        let page = Filterable { raw: Component::literal("raw"), filtered: Some(Component::literal("filtered")) };
        let mut stack = book(false, vec![page.clone()]);
        assert!(resolve_for_item(&mut stack, &context()));
        let content = stack.get(&WRITTEN_BOOK_CONTENT).unwrap();
        assert!(content.resolved);
        assert_eq!(content.pages[0].raw.get_string(), "raw");
        assert_eq!(content.pages[0].filtered.as_ref().unwrap().get_string(), "filtered");
        // already resolved: untouched
        assert!(!resolve_for_item(&mut stack, &context()));
    }

    #[test]
    fn too_large_page_only_marks_resolved() {
        let big = Filterable::pass_through(Component::literal("x".repeat(WrittenBookContent::PAGE_LENGTH + 10)));
        let mut stack = book(false, vec![big.clone()]);
        assert!(!resolve_for_item(&mut stack, &context()));
        let content = stack.get(&WRITTEN_BOOK_CONTENT).unwrap();
        assert!(content.resolved);
        assert_eq!(content.pages, vec![big]);
    }

    #[test]
    fn outgoing_filterable_follows_text_filtering() {
        let text = FilteredText::fully_filtered("secret");
        assert_eq!(filterable_from_outgoing(&text, true), Filterable::pass_through(String::new()));
        assert_eq!(filterable_from_outgoing(&text, false), Filterable { raw: "secret".into(), filtered: Some(String::new()) });
        assert_eq!(filterable_from_outgoing(&FilteredText::pass_through("ok"), false), Filterable::pass_through("ok".into()));
    }

    fn player() -> ServerPlayer {
        let profile = mc_protocol::codec::GameProfile { id: Default::default(), name: "Steve".into(), properties: Vec::new() };
        ServerPlayer::new(1, mc_core::ident::Identifier::parse("minecraft:overworld").unwrap(), profile, Default::default())
    }

    #[test]
    fn edit_book_updates_pages_in_place_and_ignores_other_stacks() {
        let mut p = player();
        p.player.inventory.set_item(2, ItemStack::new(Item::WRITABLE_BOOK));
        p.player.inventory.set_item(3, ItemStack::new(Item::STONE));
        let packet = ServerboundEditBookPacket { slot: 2, pages: vec!["a".into(), "b".into()], title: None };
        edit_book(&mut p, &packet, FilteredText::pass_through);
        let content = p.player.inventory.get_item(2).get(&WRITABLE_BOOK_CONTENT).unwrap().clone();
        assert_eq!(content.pages, vec![Filterable::pass_through("a".to_owned()), Filterable::pass_through("b".to_owned())]);
        // a slot without a writable book is left alone
        edit_book(&mut p, &ServerboundEditBookPacket { slot: 3, ..packet }, FilteredText::pass_through);
        assert_eq!(p.player.inventory.get_item(3), &ItemStack::new(Item::STONE));
    }

    #[test]
    fn signing_replaces_the_book_keeping_count_and_other_components() {
        let mut p = player();
        let mut book = ItemStack::with_count(Item::WRITABLE_BOOK, 1);
        book.set(mc_protocol::components::CUSTOM_NAME, Component::literal("mine"));
        p.player.inventory.set_item(40, book);
        let packet = ServerboundEditBookPacket { slot: 40, pages: vec!["page one".into()], title: Some("Title".into()) };
        edit_book(&mut p, &packet, FilteredText::pass_through);
        let signed = p.player.inventory.get_item(40);
        assert_eq!(signed.item(), Item::WRITTEN_BOOK);
        assert!(!signed.has(DataComponentType::WRITABLE_BOOK_CONTENT));
        assert_eq!(signed.get(&mc_protocol::components::CUSTOM_NAME).unwrap().get_string(), "mine");
        let content = signed.get(&WRITTEN_BOOK_CONTENT).unwrap();
        assert_eq!(content.title, Filterable::pass_through("Title".to_owned()));
        assert_eq!(content.author, "Steve");
        assert_eq!(content.generation, 0);
        assert!(content.resolved);
        assert_eq!(content.pages.len(), 1);
        assert_eq!(content.pages[0].raw.get_string(), "page one");
    }
}
