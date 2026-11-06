// Golden generator for the `inventory` play packets (containers, recipe book/displays, maps, trading, books/signs).
// Usage: crates/mc-protocol/tests/fixtures/play/gen.sh GenInventory.java
// Output: crates/mc-protocol/tests/fixtures/play/inventory.golden
import io.netty.buffer.ByteBuf;
import io.netty.buffer.Unpooled;
import java.nio.charset.StandardCharsets;
import java.util.*;
import it.unimi.dsi.fastutil.ints.Int2ObjectLinkedOpenHashMap;
import net.minecraft.core.Holder;
import net.minecraft.core.HolderSet;
import net.minecraft.core.component.DataComponentExactPredicate;
import net.minecraft.core.component.DataComponentPatch;
import net.minecraft.core.component.DataComponentType;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.registries.Registries;
import net.minecraft.network.HashedPatchMap;
import net.minecraft.network.HashedStack;
import net.minecraft.network.ProtocolInfo;
import net.minecraft.resources.ResourceKey;
import net.minecraft.tags.ItemTags;
import net.minecraft.world.inventory.ContainerInput;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.ItemStackTemplate;
import net.minecraft.world.item.Items;
import net.minecraft.world.item.crafting.Ingredient;
import net.minecraft.world.item.crafting.RecipePropertySet;
import net.minecraft.world.item.crafting.SelectableRecipe;
import net.minecraft.world.item.crafting.display.*;
import net.minecraft.world.item.equipment.trim.TrimPattern;
import net.minecraft.world.item.equipment.trim.TrimPatterns;
import net.minecraft.world.item.trading.ItemCost;
import net.minecraft.world.item.trading.MerchantOffer;
import net.minecraft.world.item.trading.MerchantOffers;
import net.minecraft.world.item.crafting.RecipeBookCategories;

import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.network.codec.StreamCodec;
import net.minecraft.network.protocol.Packet;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.chat.Component;
import net.minecraft.ChatFormatting;
import net.minecraft.network.protocol.game.*;
import net.minecraft.resources.Identifier;
import net.minecraft.stats.RecipeBookSettings;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.inventory.MenuType;
import net.minecraft.world.inventory.RecipeBookType;
import net.minecraft.world.item.crafting.display.RecipeDisplayId;
import net.minecraft.world.level.block.entity.SignTextSlot;
import net.minecraft.world.level.saveddata.maps.*;

public class GenInventory {
   static Identifier id(String s) { return Identifier.parse(s); }

   static void varint(ByteBuf b, int v) { net.minecraft.network.VarInt.write(b, v); }

   /** Packet id (as a VarInt) of a packet type, taken from encoding a sample packet. */
   static byte[] idBytes(ProtocolInfo<?> info, Packet<?> sample) {
      ByteBuf b = Unpooled.buffer();
      ((StreamCodec) info.codec()).encode(b, sample);
      ByteBuf id = Unpooled.buffer();
      varint(id, net.minecraft.network.VarInt.read(b));
      byte[] r = new byte[id.readableBytes()];
      id.getBytes(0, r);
      return r;
   }

   /** Writes `bad_<label>` (the bytes), and `err_<label>` = the message of the exception vanilla's codec throws on them (or `ok`). */
   static void bad(String label, ProtocolInfo<?> info, byte[] idBytes, java.util.function.Consumer<ByteBuf> body) {
      ByteBuf b = Unpooled.buffer();
      b.writeBytes(idBytes);
      body.accept(b);
      byte[] bytes = new byte[b.readableBytes()];
      b.getBytes(0, bytes);
      PlayHarness.raw("bad_" + label, w -> w.writeBytes(bytes));
      String msg;
      try {
         ((StreamCodec) info.codec()).decode(Unpooled.wrappedBuffer(bytes));
         msg = "<no error>";
      } catch (Throwable t) {
         // IdDispatchCodec wraps decode failures in 'Failed to decode packet ...'; the cause is the codec's own error
         msg = (t.getCause() != null ? t.getCause() : t).getMessage();
      }
      String m = msg;
      PlayHarness.raw("err_" + label, w -> w.writeBytes(m.getBytes(StandardCharsets.UTF_8)));
   }

   /** Writes `bad_<label>` and `norm_<label>` = the bytes vanilla re-encodes after leniently decoding them. */
   static void lenient(String label, ProtocolInfo<?> info, byte[] idBytes, java.util.function.Consumer<ByteBuf> body) {
      ByteBuf b = Unpooled.buffer();
      b.writeBytes(idBytes);
      body.accept(b);
      byte[] bytes = new byte[b.readableBytes()];
      b.getBytes(0, bytes);
      PlayHarness.raw("bad_" + label, w -> w.writeBytes(bytes));
      ByteBuf out = Unpooled.buffer();
      Object p = ((StreamCodec) info.codec()).decode(Unpooled.wrappedBuffer(bytes));
      ((StreamCodec) info.codec()).encode(out, p);
      byte[] re = new byte[out.readableBytes()];
      out.getBytes(0, re);
      PlayHarness.raw("norm_" + label, w -> w.writeBytes(re));
   }

   public static void main(String[] args) throws Exception {
      PlayHarness.init(args[0]);
      simple();
      maps();
      limits();
      items();
      recipes();
      merchant();
      itemLimits();
      PlayHarness.close();
   }

   static void simple() {
      for (int c : new int[]{0, 1, 127, 128, 255, 300, Integer.MAX_VALUE, -1, Integer.MIN_VALUE}) {
         PlayHarness.cb("cb_container_close_" + c, new ClientboundContainerClosePacket(c));
         PlayHarness.sb("sb_container_close_" + c, new ServerboundContainerClosePacket(c));
      }
      PlayHarness.cb("cb_set_data_basic", new ClientboundContainerSetDataPacket(3, 0, 200));
      PlayHarness.cb("cb_set_data_negative", new ClientboundContainerSetDataPacket(-1, -32768, 32767));
      PlayHarness.cb("cb_set_data_big_container", new ClientboundContainerSetDataPacket(1000, 2, -1));
      PlayHarness.cb("cb_cooldown_basic", new ClientboundCooldownPacket(id("minecraft:ender_pearl"), 20));
      PlayHarness.cb("cb_cooldown_zero", new ClientboundCooldownPacket(id("minecraft:shield"), 0));
      PlayHarness.cb("cb_cooldown_custom_ns", new ClientboundCooldownPacket(id("example:some/group"), Integer.MAX_VALUE));
      PlayHarness.cb("cb_cooldown_neg", new ClientboundCooldownPacket(id("minecraft:x"), -5));
      PlayHarness.cb("cb_open_book_main", new ClientboundOpenBookPacket(InteractionHand.MAIN_HAND));
      PlayHarness.cb("cb_open_book_off", new ClientboundOpenBookPacket(InteractionHand.OFF_HAND));
      for (MenuType<?> m : BuiltInRegistries.MENU) {
         String n = BuiltInRegistries.MENU.getKey(m).getPath();
         PlayHarness.cb("cb_open_screen_" + n, new ClientboundOpenScreenPacket(7, m, Component.translatable("container." + n)));
      }
      PlayHarness.cb("cb_open_screen_literal", new ClientboundOpenScreenPacket(1, MenuType.GENERIC_9x3, Component.literal("My \uD83D\uDE00 chest")));
      PlayHarness.cb("cb_open_screen_styled", new ClientboundOpenScreenPacket(255, MenuType.ANVIL,
         Component.literal("Repair").withStyle(ChatFormatting.RED, ChatFormatting.BOLD).append(Component.literal(" & Name").withColor(0x12ab34))));
      PlayHarness.cb("cb_open_screen_empty", new ClientboundOpenScreenPacket(0, MenuType.HOPPER, Component.empty()));
      PlayHarness.cb("cb_open_screen_translatable_args", new ClientboundOpenScreenPacket(2, MenuType.MERCHANT,
         Component.translatable("merchant.title", "Villager", "Master", 5, Component.literal("x").withStyle(ChatFormatting.RED))));
      for (SignTextSlot s : SignTextSlot.values()) {
         PlayHarness.cb("cb_open_sign_" + s, new ClientboundOpenSignEditorPacket(new BlockPos(100, 64, -200), s));
      }
      PlayHarness.cb("cb_open_sign_extreme", new ClientboundOpenSignEditorPacket(new BlockPos(-33554432, -2048, 33554431), SignTextSlot.FRONT));
      for (int s : new int[]{0, 1, 8, 40, 127, 128, -1}) {
         PlayHarness.cb("cb_set_held_slot_" + s, new ClientboundSetHeldSlotPacket(s));
      }
      for (RecipeBookType t : RecipeBookType.values()) {
         PlayHarness.sb("sb_recipe_book_change_" + t + "_00", new ServerboundRecipeBookChangeSettingsPacket(t, false, false));
         PlayHarness.sb("sb_recipe_book_change_" + t + "_10", new ServerboundRecipeBookChangeSettingsPacket(t, true, false));
         PlayHarness.sb("sb_recipe_book_change_" + t + "_01", new ServerboundRecipeBookChangeSettingsPacket(t, false, true));
         PlayHarness.sb("sb_recipe_book_change_" + t + "_11", new ServerboundRecipeBookChangeSettingsPacket(t, true, true));
      }
      RecipeBookSettings settings = new RecipeBookSettings();
      PlayHarness.cb("cb_recipe_book_settings_default", new ClientboundRecipeBookSettingsPacket(settings));
      settings.setOpen(RecipeBookType.CRAFTING, true);
      settings.setFiltering(RecipeBookType.FURNACE, true);
      settings.setOpen(RecipeBookType.BLAST_FURNACE, true);
      settings.setFiltering(RecipeBookType.BLAST_FURNACE, true);
      settings.setFiltering(RecipeBookType.SMOKER, true);
      PlayHarness.cb("cb_recipe_book_settings_mixed", new ClientboundRecipeBookSettingsPacket(settings));
      for (RecipeBookType t : RecipeBookType.values()) {
         settings.setOpen(t, true);
         settings.setFiltering(t, true);
      }
      PlayHarness.cb("cb_recipe_book_settings_all", new ClientboundRecipeBookSettingsPacket(settings));
      PlayHarness.cb("cb_recipe_book_remove_empty", new ClientboundRecipeBookRemovePacket(List.of()));
      PlayHarness.cb("cb_recipe_book_remove_one", new ClientboundRecipeBookRemovePacket(List.of(new RecipeDisplayId(0))));
      PlayHarness.cb("cb_recipe_book_remove_many", new ClientboundRecipeBookRemovePacket(
         List.of(new RecipeDisplayId(1), new RecipeDisplayId(127), new RecipeDisplayId(128), new RecipeDisplayId(16384), new RecipeDisplayId(-1), new RecipeDisplayId(Integer.MAX_VALUE))));
      List<RecipeDisplayId> big = new ArrayList<>();
      for (int i = 0; i < 300; i++) big.add(new RecipeDisplayId(i * 7));
      PlayHarness.cb("cb_recipe_book_remove_300", new ClientboundRecipeBookRemovePacket(big));

      // serverbound
      PlayHarness.sb("sb_bundle_selected_basic", new ServerboundSelectBundleItemPacket(36, 2));
      PlayHarness.sb("sb_bundle_selected_none", new ServerboundSelectBundleItemPacket(0, -1));
      PlayHarness.sb("sb_bundle_selected_big", new ServerboundSelectBundleItemPacket(-1, Integer.MAX_VALUE));
      PlayHarness.sb("sb_button_click", new ServerboundContainerButtonClickPacket(3, 1));
      PlayHarness.sb("sb_button_click_neg", new ServerboundContainerButtonClickPacket(200, -1));
      PlayHarness.sb("sb_slot_state_true", new ServerboundContainerSlotStateChangedPacket(5, 2, true));
      PlayHarness.sb("sb_slot_state_false", new ServerboundContainerSlotStateChangedPacket(-1, 300, false));
      PlayHarness.sb("sb_place_recipe_max", new ServerboundPlaceRecipePacket(1, new RecipeDisplayId(42), true));
      PlayHarness.sb("sb_place_recipe_one", new ServerboundPlaceRecipePacket(0, new RecipeDisplayId(0), false));
      PlayHarness.sb("sb_place_recipe_big", new ServerboundPlaceRecipePacket(255, new RecipeDisplayId(100000), true));
      PlayHarness.sb("sb_seen_recipe_0", new ServerboundRecipeBookSeenRecipePacket(new RecipeDisplayId(0)));
      PlayHarness.sb("sb_seen_recipe_big", new ServerboundRecipeBookSeenRecipePacket(new RecipeDisplayId(123456)));
      PlayHarness.sb("sb_rename_empty", new ServerboundRenameItemPacket(""));
      PlayHarness.sb("sb_rename_basic", new ServerboundRenameItemPacket("Excalibur"));
      PlayHarness.sb("sb_rename_non_bmp", new ServerboundRenameItemPacket("Sw\u00f6rd \uD83D\uDDE1\uFE0F \u4e2d\u6587 \uD83D\uDE00"));
      PlayHarness.sb("sb_rename_long", new ServerboundRenameItemPacket("x".repeat(50)));
      PlayHarness.sb("sb_select_trade_0", new ServerboundSelectTradePacket(0));
      PlayHarness.sb("sb_select_trade_7", new ServerboundSelectTradePacket(7));
      PlayHarness.sb("sb_select_trade_neg", new ServerboundSelectTradePacket(-1));
      for (int s : new int[]{0, 8, 255, 32767, -1, -32768}) {
         PlayHarness.sb("sb_set_carried_" + s, new ServerboundSetCarriedItemPacket(s));
      }
      PlayHarness.sb("sb_edit_book_no_pages", new ServerboundEditBookPacket(0, List.of(), Optional.empty()));
      PlayHarness.sb("sb_edit_book_one_page", new ServerboundEditBookPacket(36, List.of("Hello"), Optional.empty()));
      PlayHarness.sb("sb_edit_book_sign", new ServerboundEditBookPacket(40, List.of("Page 1", "Page \uD83D\uDE00 2", ""), Optional.of("My Title")));
      PlayHarness.sb("sb_edit_book_max_title", new ServerboundEditBookPacket(-1, List.of("a"), Optional.of("t".repeat(32))));
      PlayHarness.sb("sb_edit_book_max_page", new ServerboundEditBookPacket(1, List.of("p".repeat(1024)), Optional.of("")));
      List<String> hundred = new ArrayList<>();
      for (int i = 0; i < 100; i++) hundred.add("page " + i);
      PlayHarness.sb("sb_edit_book_100_pages", new ServerboundEditBookPacket(2, hundred, Optional.of("\uD83D\uDE00".repeat(16))));
   }

   static byte[] cbId(Packet<?> p) { return idBytes(PlayHarness.clientbound, p); }
   static byte[] sbId(Packet<?> p) { return idBytes(PlayHarness.serverbound, p); }

   static void limits() {
      var sb = PlayHarness.serverbound;
      var cb = PlayHarness.clientbound;
      byte[] edit = sbId(new ServerboundEditBookPacket(0, List.of(), Optional.empty()));
      bad("edit_book_101_pages", sb, edit, b -> { varint(b, 0); varint(b, 101); });
      bad("edit_book_page_1025", sb, edit, b -> { varint(b, 0); varint(b, 1); b.writeBytes(utf("p".repeat(1025))); });
      bad("edit_book_page_1024_utf16_over", sb, edit, b -> { varint(b, 0); varint(b, 1); b.writeBytes(utf("\uD83D\uDE00".repeat(513))); });
      bad("edit_book_title_33", sb, edit, b -> { varint(b, 0); varint(b, 0); b.writeBoolean(true); b.writeBytes(utf("t".repeat(33))); });
      bad("edit_book_neg_pages", sb, edit, b -> { varint(b, 0); varint(b, -1); });
      lenient("edit_book_max", sb, edit, b -> { varint(b, 0); varint(b, 1); b.writeBytes(utf("\uD83D\uDE00".repeat(512))); b.writeBoolean(true); b.writeBytes(utf("\uD83D\uDE00".repeat(16))); });
      byte[] rename = sbId(new ServerboundRenameItemPacket(""));
      bad("rename_32768", sb, rename, b -> b.writeBytes(utf("x".repeat(32768))));
      lenient("rename_32767", sb, rename, b -> b.writeBytes(utf("x".repeat(32767))));
      byte[] bundle = sbId(new ServerboundSelectBundleItemPacket(0, 0));
      bad("bundle_selected_minus2", sb, bundle, b -> { varint(b, 1); varint(b, -2); });
      bad("bundle_selected_min", sb, bundle, b -> { varint(b, 1); varint(b, Integer.MIN_VALUE); });
      byte[] rbs = sbId(new ServerboundRecipeBookChangeSettingsPacket(RecipeBookType.CRAFTING, false, false));
      for (int v : new int[]{4, 5, 100, -1, Integer.MIN_VALUE})
         bad("recipe_book_type_" + v, sb, rbs, b -> { varint(b, v); b.writeBoolean(true); b.writeBoolean(true); });
      byte[] os = cbId(new ClientboundOpenScreenPacket(0, MenuType.ANVIL, Component.empty()));
      int menus = BuiltInRegistries.MENU.size();
      for (int v : new int[]{menus, menus + 1, 1000, -1})
         bad("open_screen_menu_" + v, cb, os, b -> { varint(b, 1); varint(b, v); b.writeByte(8); b.writeShort(0); });
      byte[] sign = cbId(new ClientboundOpenSignEditorPacket(BlockPos.ZERO, SignTextSlot.FRONT));
      for (int v : new int[]{2, 3, -1, 1000})
         lenient("open_sign_slot_" + v, cb, sign, b -> { b.writeLong(BlockPos.ZERO.asLong()); varint(b, v); });
      byte[] map = cbId(new ClientboundMapItemDataPacket(new MapId(0), (byte) 0, false, (Collection<MapDecoration>) null, (MapItemSavedData.MapPatch) null));
      int decos = BuiltInRegistries.MAP_DECORATION_TYPE.size();
      bad("map_decoration_type_" + decos, cb, map, b -> { varint(b, 1); b.writeByte(0); b.writeBoolean(false); b.writeBoolean(true); varint(b, 1); varint(b, decos); b.writeBytes(new byte[]{0, 0, 0, 0}); });
      for (int rot : new int[]{16, 31, 0x7f, -1, -128})
         lenient("map_decoration_rot_" + rot, cb, map, b -> { varint(b, 1); b.writeByte(0); b.writeBoolean(false); b.writeBoolean(true); varint(b, 1); varint(b, 0); b.writeByte(3); b.writeByte(4); b.writeByte(rot); b.writeBoolean(false); b.writeByte(0); });
      lenient("map_patch_zero_width", cb, map, b -> { varint(b, 1); b.writeByte(0); b.writeBoolean(false); b.writeBoolean(false); b.writeByte(0); });
      bad("map_patch_array_too_long", cb, map, b -> { varint(b, 1); b.writeByte(0); b.writeBoolean(false); b.writeBoolean(false); b.writeByte(2); b.writeByte(2); b.writeByte(0); b.writeByte(0); varint(b, 6); b.writeBytes(new byte[4]); });
   }

   static byte[] utf(String s) {
      ByteBuf b = Unpooled.buffer();
      byte[] raw = s.getBytes(StandardCharsets.UTF_8);
      varint(b, raw.length);
      b.writeBytes(raw);
      byte[] r = new byte[b.readableBytes()];
      b.getBytes(0, r);
      return r;
   }

   static void maps() {
      PlayHarness.cb("cb_map_minimal", new ClientboundMapItemDataPacket(new MapId(0), (byte) 0, false, (Collection<MapDecoration>) null, (MapItemSavedData.MapPatch) null));
      PlayHarness.cb("cb_map_locked_scale", new ClientboundMapItemDataPacket(new MapId(1234567), (byte) 4, true, (Collection<MapDecoration>) null, (MapItemSavedData.MapPatch) null));
      PlayHarness.cb("cb_map_neg_scale", new ClientboundMapItemDataPacket(new MapId(-1), (byte) -128, true, List.of(), null));
      List<MapDecoration> decos = new ArrayList<>();
      int i = 0;
      for (var t : BuiltInRegistries.MAP_DECORATION_TYPE.listElements().toList()) {
         decos.add(new MapDecoration(t, (byte) (i * 5 - 100), (byte) (100 - i * 4), (byte) i, i % 3 == 0 ? Optional.of(Component.literal("n" + i)) : Optional.empty()));
         i++;
      }
      PlayHarness.cb("cb_map_all_decorations", new ClientboundMapItemDataPacket(new MapId(5), (byte) 2, false, decos, null));
      PlayHarness.cb("cb_map_decoration_rot_masked", new ClientboundMapItemDataPacket(new MapId(5), (byte) 2, false,
         List.of(new MapDecoration(MapDecorationTypes.PLAYER, (byte) 127, (byte) -128, (byte) 0x7f, Optional.of(Component.translatable("filled_map.id", 3).withStyle(ChatFormatting.GOLD)))), null));
      PlayHarness.cb("cb_map_decoration_neg_rot", new ClientboundMapItemDataPacket(new MapId(5), (byte) 2, false,
         List.of(new MapDecoration(MapDecorationTypes.FRAME, (byte) 0, (byte) 0, (byte) -1, Optional.empty())), null));
      byte[] colors = new byte[128 * 128];
      for (int k = 0; k < colors.length; k++) colors[k] = (byte) (k * 31);
      PlayHarness.cb("cb_map_full_patch", new ClientboundMapItemDataPacket(new MapId(9), (byte) 1, false, null,
         new MapItemSavedData.MapPatch(0, 0, 128, 128, colors)));
      PlayHarness.cb("cb_map_small_patch", new ClientboundMapItemDataPacket(new MapId(9), (byte) 1, true, null,
         new MapItemSavedData.MapPatch(17, 99, 3, 2, new byte[]{1, 2, 3, -4, -5, -6})));
      PlayHarness.cb("cb_map_patch_extreme", new ClientboundMapItemDataPacket(new MapId(9), (byte) 1, true, List.of(),
         new MapItemSavedData.MapPatch(255, 254, 255, 1, new byte[255])));
      PlayHarness.cb("cb_map_everything", new ClientboundMapItemDataPacket(new MapId(77), (byte) 3, true, decos,
         new MapItemSavedData.MapPatch(1, 2, 4, 4, new byte[16])));
   }

   static Holder<Item> h(Item i) { return i.builtInRegistryHolder(); }

   /** Only max_stack_size/max_damage/damage/unbreakable have Rust codecs so far: `named` carries `unbreakable`. */
   static ItemStack named(Item i, int count, Component name) {
      return new ItemStack(h(i), count, DataComponentPatch.builder().set(DataComponents.UNBREAKABLE, net.minecraft.util.Unit.INSTANCE).build());
   }

   /** Item stacks with and without component patches (set / removed / both). */
   static LinkedHashMap<String, ItemStack> stacks() {
      var m = new LinkedHashMap<String, ItemStack>();
      m.put("empty", ItemStack.EMPTY);
      m.put("diamond", new ItemStack(Items.DIAMOND));
      m.put("dirt64", new ItemStack(Items.DIRT, 64));
      m.put("cobble99", new ItemStack(Items.COBBLESTONE, 99));
      m.put("named", named(Items.DIAMOND_SWORD, 1, Component.literal("Excalibur \uD83D\uDDE1").withStyle(ChatFormatting.AQUA)));
      m.put("damaged", new ItemStack(h(Items.IRON_PICKAXE), 1, DataComponentPatch.builder().set(DataComponents.DAMAGE, 17).build()));
      m.put("removed", new ItemStack(h(Items.DIAMOND_PICKAXE), 1, DataComponentPatch.builder().remove(DataComponents.MAX_DAMAGE).build()));
      m.put("set_and_removed", new ItemStack(h(Items.NETHERITE_AXE), 1,
         DataComponentPatch.builder().set(DataComponents.DAMAGE, 3).remove(DataComponents.MAX_DAMAGE).set(DataComponents.UNBREAKABLE, net.minecraft.util.Unit.INSTANCE).build()));
      m.put("max_stack_override", new ItemStack(h(Items.STICK), 5, DataComponentPatch.builder().set(DataComponents.MAX_STACK_SIZE, 16).build()));
      return m;
   }

   static void items() {
      var st = stacks();
      for (var e : st.entrySet()) {
         PlayHarness.cb("cb_set_slot_" + e.getKey(), new ClientboundContainerSetSlotPacket(2, 17, 5, e.getValue()));
         PlayHarness.cb("cb_set_cursor_" + e.getKey(), new ClientboundSetCursorItemPacket(e.getValue()));
         PlayHarness.cb("cb_set_player_inventory_" + e.getKey(), new ClientboundSetPlayerInventoryPacket(40, e.getValue()));
      }
      PlayHarness.cb("cb_set_slot_extremes", new ClientboundContainerSetSlotPacket(-1, -1, -1, st.get("diamond")));
      PlayHarness.cb("cb_set_slot_big", new ClientboundContainerSetSlotPacket(300, 70000, 32767, st.get("named")));
      PlayHarness.cb("cb_set_slot_min", new ClientboundContainerSetSlotPacket(0, 0, -32768, st.get("empty")));
      PlayHarness.cb("cb_set_player_inventory_neg", new ClientboundSetPlayerInventoryPacket(-1, st.get("dirt64")));
      PlayHarness.cb("cb_set_content_empty_list", new ClientboundContainerSetContentPacket(0, 0, List.of(), ItemStack.EMPTY));
      PlayHarness.cb("cb_set_content_all", new ClientboundContainerSetContentPacket(5, 123456, new ArrayList<>(st.values()), st.get("named")));
      List<ItemStack> inv = new ArrayList<>();
      for (int i = 0; i < 46; i++) inv.add(i % 3 == 0 ? ItemStack.EMPTY : new ItemStack(Items.DIRT, 1 + i));
      PlayHarness.cb("cb_set_content_46", new ClientboundContainerSetContentPacket(0, 1, inv, new ItemStack(Items.APPLE, 3)));
      List<ItemStack> big = new ArrayList<>();
      for (int i = 0; i < 300; i++) big.add(i % 2 == 0 ? ItemStack.EMPTY : new ItemStack(Items.STONE));
      PlayHarness.cb("cb_set_content_300", new ClientboundContainerSetContentPacket(255, -5, big, ItemStack.EMPTY));
      PlayHarness.cb("cb_cooldown_unused", new ClientboundCooldownPacket(id("minecraft:diamond"), 1));

      // serverbound
      for (var e : st.entrySet()) {
         PlayHarness.sb("sb_creative_slot_" + e.getKey(), new ServerboundSetCreativeModeSlotPacket(36, e.getValue()));
      }
      PlayHarness.sb("sb_creative_slot_neg", new ServerboundSetCreativeModeSlotPacket(-1, st.get("dirt64")));
      PlayHarness.sb("sb_creative_slot_max", new ServerboundSetCreativeModeSlotPacket(32767, st.get("empty")));

      // container click
      var hs = new LinkedHashMap<String, HashedStack>();
      hs.put("empty", HashedStack.EMPTY);
      hs.put("diamond", new HashedStack.ActualItem(h(Items.DIAMOND), 1, new HashedPatchMap(new LinkedHashMap<>(), new LinkedHashSet<>())));
      hs.put("dirt64", new HashedStack.ActualItem(h(Items.DIRT), 64, new HashedPatchMap(new LinkedHashMap<>(), new LinkedHashSet<>())));
      var added = new LinkedHashMap<DataComponentType<?>, Integer>();
      added.put(DataComponents.DAMAGE, 0x12345678);
      added.put(DataComponents.CUSTOM_NAME, -1);
      var removed = new LinkedHashSet<DataComponentType<?>>();
      removed.add(DataComponents.MAX_STACK_SIZE);
      removed.add(DataComponents.MAX_DAMAGE);
      hs.put("patched", new HashedStack.ActualItem(h(Items.NETHERITE_SWORD), 99, new HashedPatchMap(added, removed)));
      hs.put("only_removed", new HashedStack.ActualItem(h(Items.STICK), 2, new HashedPatchMap(new LinkedHashMap<>(), new LinkedHashSet<>(removed))));
      hs.put("neg_count", new HashedStack.ActualItem(h(Items.STICK), -3, new HashedPatchMap(new LinkedHashMap<>(), new LinkedHashSet<>())));
      for (var in : ContainerInput.values()) {
         var slots = new Int2ObjectLinkedOpenHashMap<HashedStack>();
         PlayHarness.sb("sb_click_" + in + "_no_changes", new ServerboundContainerClickPacket(1, 2, (short) 3, (byte) 0, in, slots, HashedStack.EMPTY));
         slots.put(7, hs.get("dirt64"));
         PlayHarness.sb("sb_click_" + in + "_one_change", new ServerboundContainerClickPacket(0, 99, (short) -999, (byte) 1, in, slots, hs.get("diamond")));
         slots.put(-1, HashedStack.EMPTY);
         slots.put(32767, hs.get("patched"));
         slots.put(-32768, hs.get("only_removed"));
         PlayHarness.sb("sb_click_" + in + "_many_changes", new ServerboundContainerClickPacket(255, -1, (short) 32767, (byte) -128, in, slots, hs.get("patched")));
      }
      for (var e : hs.entrySet()) {
         var slots = new Int2ObjectLinkedOpenHashMap<HashedStack>();
         slots.put(1, e.getValue());
         PlayHarness.sb("sb_click_hashed_" + e.getKey(), new ServerboundContainerClickPacket(300, 70000, (short) 1, (byte) 127, ContainerInput.PICKUP, slots, e.getValue()));
      }
      var max = new Int2ObjectLinkedOpenHashMap<HashedStack>();
      for (int i = 0; i < 128; i++) max.put(i * 3 - 100, hs.get(i % 2 == 0 ? "diamond" : "empty"));
      PlayHarness.sb("sb_click_128_changes", new ServerboundContainerClickPacket(1, 1, (short) 0, (byte) 0, ContainerInput.QUICK_CRAFT, max, HashedStack.EMPTY));
   }

   static SlotDisplay item(Item i) { return new SlotDisplay.ItemSlotDisplay(i); }

   static Holder<TrimPattern> pattern() {
      return PlayHarness.access.lookupOrThrow(Registries.TRIM_PATTERN).getOrThrow(TrimPatterns.SENTRY);
   }

   static HolderSet<Item> tag(net.minecraft.tags.TagKey<Item> key) {
      return PlayHarness.access.lookupOrThrow(Registries.ITEM).getOrThrow(key);
   }

   static HolderSet<Item> direct(Item... items) {
      return HolderSet.direct(Arrays.stream(items).map(GenInventory::h).toList());
   }

   static LinkedHashMap<String, SlotDisplay> slotDisplays() {
      var m = new LinkedHashMap<String, SlotDisplay>();
      m.put("empty", SlotDisplay.Empty.INSTANCE);
      m.put("any_fuel", SlotDisplay.AnyFuel.INSTANCE);
      m.put("item", item(Items.DIAMOND));
      m.put("item_air", item(Items.AIR));
      m.put("item_stack", new SlotDisplay.ItemStackSlotDisplay(new ItemStackTemplate(Items.APPLE)));
      m.put("item_stack_count", new SlotDisplay.ItemStackSlotDisplay(new ItemStackTemplate(Items.STICK, 12)));
      m.put("item_stack_patch", new SlotDisplay.ItemStackSlotDisplay(new ItemStackTemplate(h(Items.DIAMOND_SWORD), 3,
         DataComponentPatch.builder().set(DataComponents.DAMAGE, 9).remove(DataComponents.MAX_DAMAGE).build())));
      m.put("tag_named", new SlotDisplay.TagSlotDisplay(tag(ItemTags.PLANKS)));
      m.put("tag_direct", new SlotDisplay.TagSlotDisplay(direct(Items.OAK_PLANKS, Items.STONE, Items.DIAMOND)));
      m.put("tag_direct_empty", new SlotDisplay.TagSlotDisplay(HolderSet.empty()));
      m.put("any_potion", new SlotDisplay.WithAnyPotion(item(Items.POTION)));
      m.put("only_with_component", new SlotDisplay.OnlyWithComponent(item(Items.APPLE), DataComponents.FOOD));
      m.put("dyed", new SlotDisplay.DyedSlotDemo(item(Items.DYE.red()), item(Items.LEATHER_CHESTPLATE)));
      m.put("smithing_trim_ref", new SlotDisplay.SmithingTrimDemoSlotDisplay(item(Items.IRON_CHESTPLATE), item(Items.GOLD_INGOT), pattern()));
      m.put("smithing_trim_direct", new SlotDisplay.SmithingTrimDemoSlotDisplay(item(Items.IRON_CHESTPLATE), item(Items.GOLD_INGOT),
         Holder.direct(new TrimPattern(id("custom:pattern"), Component.literal("Custom \uD83D\uDE00").withStyle(ChatFormatting.GREEN), true))));
      m.put("smithing_trim_direct_nodecal", new SlotDisplay.SmithingTrimDemoSlotDisplay(SlotDisplay.Empty.INSTANCE, SlotDisplay.AnyFuel.INSTANCE,
         Holder.direct(new TrimPattern(id("a:b"), Component.empty(), false))));
      m.put("with_remainder", new SlotDisplay.WithRemainder(item(Items.LAVA_BUCKET), item(Items.BUCKET)));
      m.put("composite_empty", new SlotDisplay.Composite(List.of()));
      m.put("composite", new SlotDisplay.Composite(List.of(item(Items.STONE), SlotDisplay.Empty.INSTANCE, new SlotDisplay.TagSlotDisplay(tag(ItemTags.COALS)))));
      SlotDisplay deep = item(Items.DIAMOND);
      for (int i = 0; i < 6; i++) {
         deep = switch (i % 3) {
            case 0 -> new SlotDisplay.Composite(List.of(deep, new SlotDisplay.WithRemainder(deep, SlotDisplay.Empty.INSTANCE)));
            case 1 -> new SlotDisplay.WithAnyPotion(new SlotDisplay.DyedSlotDemo(deep, SlotDisplay.AnyFuel.INSTANCE));
            default -> new SlotDisplay.OnlyWithComponent(new SlotDisplay.SmithingTrimDemoSlotDisplay(deep, item(Items.EMERALD), pattern()), DataComponents.DAMAGE);
         };
      }
      m.put("deep_nested", deep);
      return m;
   }

   static void recipes() {
      PlayHarness.raw("trim_sentry_id", b -> net.minecraft.network.VarInt.write(b, PlayHarness.access.lookupOrThrow(Registries.TRIM_PATTERN).getId(pattern().value())));
      var sd = slotDisplays();
      var all = new ArrayList<SlotDisplay>(sd.values());
      for (var e : sd.entrySet()) {
         PlayHarness.raw("slot_" + e.getKey(), b -> SlotDisplay.STREAM_CODEC.encode(b, e.getValue()));
      }
      SlotDisplay station = item(Items.CRAFTING_TABLE);
      var displays = new LinkedHashMap<String, RecipeDisplay>();
      displays.put("shapeless_empty", new ShapelessCraftingRecipeDisplay(List.of(), SlotDisplay.Empty.INSTANCE, SlotDisplay.Empty.INSTANCE));
      displays.put("shapeless", new ShapelessCraftingRecipeDisplay(List.of(item(Items.OAK_PLANKS), new SlotDisplay.TagSlotDisplay(tag(ItemTags.COALS))), item(Items.STICK), station));
      displays.put("shapeless_all_slot_displays", new ShapelessCraftingRecipeDisplay(all, new SlotDisplay.ItemStackSlotDisplay(new ItemStackTemplate(Items.STICK, 4)), station));
      displays.put("shaped_1x1", new ShapedCraftingRecipeDisplay(1, 1, List.of(item(Items.STONE)), item(Items.STONE_BRICKS), station));
      displays.put("shaped_3x3", new ShapedCraftingRecipeDisplay(3, 3, List.of(
         item(Items.STONE), item(Items.STONE), item(Items.STONE), SlotDisplay.Empty.INSTANCE, item(Items.STICK), SlotDisplay.Empty.INSTANCE,
         SlotDisplay.Empty.INSTANCE, item(Items.STICK), SlotDisplay.Empty.INSTANCE), item(Items.STONE_PICKAXE), station));
      displays.put("shaped_0x0", new ShapedCraftingRecipeDisplay(0, 0, List.of(), SlotDisplay.Empty.INSTANCE, SlotDisplay.Empty.INSTANCE));
      displays.put("shaped_2x3_nested", new ShapedCraftingRecipeDisplay(2, 3, all.subList(0, 6), sd.get("deep_nested"), sd.get("with_remainder")));
      displays.put("furnace", new FurnaceRecipeDisplay(item(Items.IRON_ORE), SlotDisplay.AnyFuel.INSTANCE, item(Items.IRON_INGOT), item(Items.FURNACE), 200, 0.7f));
      displays.put("furnace_neg", new FurnaceRecipeDisplay(sd.get("composite"), sd.get("dyed"), sd.get("any_potion"), sd.get("only_with_component"), -1, -0.5f));
      displays.put("furnace_big", new FurnaceRecipeDisplay(SlotDisplay.Empty.INSTANCE, SlotDisplay.Empty.INSTANCE, SlotDisplay.Empty.INSTANCE, SlotDisplay.Empty.INSTANCE, Integer.MAX_VALUE, 3.4028235E38f));
      displays.put("stonecutter", new StonecutterRecipeDisplay(item(Items.STONE), item(Items.STONE_SLAB), item(Items.STONECUTTER)));
      displays.put("smithing", new SmithingRecipeDisplay(item(Items.NETHERITE_UPGRADE_SMITHING_TEMPLATE), item(Items.DIAMOND_SWORD), item(Items.NETHERITE_INGOT), sd.get("item_stack_patch"), item(Items.SMITHING_TABLE)));
      displays.put("smithing_trim", new SmithingRecipeDisplay(sd.get("composite"), sd.get("smithing_trim_ref"), sd.get("smithing_trim_direct"), sd.get("with_remainder"), sd.get("empty")));
      for (var e : displays.entrySet()) {
         PlayHarness.raw("display_" + e.getKey(), b -> RecipeDisplay.STREAM_CODEC.encode(b, e.getValue()));
         PlayHarness.cb("cb_place_ghost_" + e.getKey(), new ClientboundPlaceGhostRecipePacket(3, e.getValue()));
      }

      // recipe book add
      int n = 0;
      var entries = new ArrayList<ClientboundRecipeBookAddPacket.Entry>();
      for (var cat : BuiltInRegistries.RECIPE_BOOK_CATEGORY) {
         for (var d : List.of(displays.get("shapeless"), displays.get("furnace"))) {
            OptionalInt group = n % 2 == 0 ? OptionalInt.empty() : OptionalInt.of(n * 100);
            Optional<List<Ingredient>> reqs = switch (n % 3) {
               case 0 -> Optional.empty();
               case 1 -> Optional.of(List.of());
               default -> Optional.of(List.of(Ingredient.of(Items.STONE), Ingredient.of(tag(ItemTags.PLANKS)), Ingredient.of(Items.OAK_PLANKS, Items.BIRCH_PLANKS)));
            };
            var entry = new RecipeDisplayEntry(new RecipeDisplayId(n), d, group, cat, reqs);
            PlayHarness.raw("entry_" + n, b -> RecipeDisplayEntry.STREAM_CODEC.encode(b, entry));
            entries.add(new ClientboundRecipeBookAddPacket.Entry(entry, (byte) (n % 4)));
            n++;
         }
      }
      PlayHarness.cb("cb_recipe_book_add_empty", new ClientboundRecipeBookAddPacket(List.of(), false));
      PlayHarness.cb("cb_recipe_book_add_empty_replace", new ClientboundRecipeBookAddPacket(List.of(), true));
      PlayHarness.cb("cb_recipe_book_add_one", new ClientboundRecipeBookAddPacket(entries.subList(0, 1), false));
      PlayHarness.cb("cb_recipe_book_add_all", new ClientboundRecipeBookAddPacket(entries, true));
      int k = 0;
      for (var d : displays.values()) {
         var entry = new RecipeDisplayEntry(new RecipeDisplayId(1000 + k), d, OptionalInt.of(Integer.MAX_VALUE), RecipeBookCategories.CRAFTING_MISC, Optional.empty());
         PlayHarness.cb("cb_recipe_book_add_display_" + k, new ClientboundRecipeBookAddPacket(List.of(new ClientboundRecipeBookAddPacket.Entry(entry, (byte) -1)), false));
         k++;
      }
      for (byte flags : new byte[]{0, 1, 2, 3, 4, 127, -128, -1}) {
         var entry = new RecipeDisplayEntry(new RecipeDisplayId(5), displays.get("stonecutter"), OptionalInt.of(-2), RecipeBookCategories.STONECUTTER, Optional.empty());
         PlayHarness.cb("cb_recipe_book_add_flags_" + flags, new ClientboundRecipeBookAddPacket(List.of(new ClientboundRecipeBookAddPacket.Entry(entry, flags)), false));
      }

      // update recipes
      Map<ResourceKey<RecipePropertySet>, RecipePropertySet> none = new LinkedHashMap<>();
      var empty = SelectableRecipe.SingleInputSet.<net.minecraft.world.item.crafting.StonecutterRecipe>empty();
      PlayHarness.cb("cb_update_recipes_empty", new ClientboundUpdateRecipesPacket(none, empty));
      var sets = new LinkedHashMap<ResourceKey<RecipePropertySet>, RecipePropertySet>();
      sets.put(RecipePropertySet.SMITHING_BASE, RecipePropertySet.create(List.of(Ingredient.of(Items.DIAMOND_SWORD))));
      sets.put(RecipePropertySet.FURNACE_INPUT, RecipePropertySet.create(List.of()));
      sets.put(RecipePropertySet.BREWING_REAGENTS, RecipePropertySet.create(List.of(Ingredient.of(Items.NETHER_WART))));
      sets.put(ResourceKey.create(RecipePropertySet.TYPE_KEY, id("custom:set")), RecipePropertySet.EMPTY);
      PlayHarness.cb("cb_update_recipes_sets", new ClientboundUpdateRecipesPacket(sets, empty));
      var multi = new LinkedHashMap<ResourceKey<RecipePropertySet>, RecipePropertySet>();
      multi.put(RecipePropertySet.SMOKER_INPUT, RecipePropertySet.create(List.of(Ingredient.of(Items.BEEF, Items.PORKCHOP, Items.CHICKEN, Items.COD, Items.SALMON, Items.POTATO))));
      PlayHarness.cb("cb_update_recipes_multi_item_set", new ClientboundUpdateRecipesPacket(multi, empty));
      var cutter = new SelectableRecipe.SingleInputSet<net.minecraft.world.item.crafting.StonecutterRecipe>(List.of(
         new SelectableRecipe.SingleInputEntry<>(Ingredient.of(Items.STONE), new SelectableRecipe<>(item(Items.STONE_SLAB), Optional.empty())),
         new SelectableRecipe.SingleInputEntry<>(Ingredient.of(Items.STONE), new SelectableRecipe<>(item(Items.STONE_STAIRS), Optional.empty())),
         new SelectableRecipe.SingleInputEntry<>(Ingredient.of(tag(ItemTags.PLANKS)), new SelectableRecipe<>(sd.get("deep_nested"), Optional.empty())),
         new SelectableRecipe.SingleInputEntry<>(Ingredient.of(Items.COBBLESTONE, Items.STONE), new SelectableRecipe<>(SlotDisplay.Empty.INSTANCE, Optional.empty()))));
      PlayHarness.cb("cb_update_recipes_stonecutter", new ClientboundUpdateRecipesPacket(none, cutter));
      PlayHarness.cb("cb_update_recipes_both", new ClientboundUpdateRecipesPacket(sets, cutter));
      var allSd = new ArrayList<SelectableRecipe.SingleInputEntry<net.minecraft.world.item.crafting.StonecutterRecipe>>();
      for (var d : all) allSd.add(new SelectableRecipe.SingleInputEntry<>(Ingredient.of(Items.STONE), new SelectableRecipe<>(d, Optional.empty())));
      PlayHarness.cb("cb_update_recipes_all_displays", new ClientboundUpdateRecipesPacket(none, new SelectableRecipe.SingleInputSet<>(allSd)));
   }

   static void merchant() {
      var plain = new ItemCost(Items.EMERALD, 5);
      var comp = new ItemCost(h(Items.PAPER), 2, DataComponentExactPredicate.builder().expect(DataComponents.DAMAGE, 4).expect(DataComponents.MAX_STACK_SIZE, 8).build());
      var st = stacks();
      var offers = new MerchantOffers();
      PlayHarness.cb("cb_merchant_none", new ClientboundMerchantOffersPacket(1, offers, 1, 0, true, true));
      offers.add(new MerchantOffer(plain, st.get("diamond"), 12, 2, 0.05f));
      PlayHarness.cb("cb_merchant_one", new ClientboundMerchantOffersPacket(2, offers.copy(), 2, 10, true, false));
      var o2 = new MerchantOffer(comp, Optional.of(plain), st.get("named"), 3, 16, 7, 0.2f, 11);
      o2.setSpecialPriceDiff(-4);
      offers.add(o2);
      var o3 = new MerchantOffer(new ItemCost(Items.STICK, 64), Optional.empty(), st.get("dirt64"), 0, 0, 0, 0f, -9);
      offers.add(o3);
      var o4 = new MerchantOffer(plain, Optional.of(new ItemCost(h(Items.DIAMOND), 1, DataComponentExactPredicate.EMPTY)), st.get("set_and_removed"), 7, 7, 30, -1.5f, Integer.MAX_VALUE);
      o4.setSpecialPriceDiff(Integer.MIN_VALUE);
      offers.add(o4);
      PlayHarness.cb("cb_merchant_four", new ClientboundMerchantOffersPacket(3, offers, 5, 250, false, false));
      PlayHarness.cb("cb_merchant_levels", new ClientboundMerchantOffersPacket(300, offers, 200, 100000, true, true));
      PlayHarness.cb("cb_merchant_neg", new ClientboundMerchantOffersPacket(-1, offers, -1, -1, false, true));
      var many = new MerchantOffers();
      for (int i = 0; i < 20; i++) many.add(new MerchantOffer(new ItemCost(Items.EMERALD, 1 + i), new ItemStack(Items.DIAMOND, 1 + i), i + 1, i, 0.05f));
      PlayHarness.cb("cb_merchant_20", new ClientboundMerchantOffersPacket(9, many, 3, 60, true, true));
      PlayHarness.raw("item_cost_plain", b -> ItemCost.STREAM_CODEC.encode(b, plain));
      PlayHarness.raw("item_cost_components", b -> ItemCost.STREAM_CODEC.encode(b, comp));
      PlayHarness.raw("item_cost_optional_none", b -> ItemCost.OPTIONAL_STREAM_CODEC.encode(b, Optional.empty()));
      PlayHarness.raw("item_cost_optional_some", b -> ItemCost.OPTIONAL_STREAM_CODEC.encode(b, Optional.of(comp)));
   }

   static void itemLimits() {
      var sb = PlayHarness.serverbound;
      var cb = PlayHarness.clientbound;
      byte[] click = sbId(new ServerboundContainerClickPacket(0, 0, (short) 0, (byte) 0, ContainerInput.PICKUP, new Int2ObjectLinkedOpenHashMap<>(), HashedStack.EMPTY));
      bad("click_129_changes", sb, click, b -> { varint(b, 0); varint(b, 0); b.writeShort(0); b.writeByte(0); varint(b, 0); varint(b, 129); });
      bad("click_neg_changes", sb, click, b -> { varint(b, 0); varint(b, 0); b.writeShort(0); b.writeByte(0); varint(b, 0); varint(b, -1); });
      for (int v : new int[]{7, 8, 100, -1, Integer.MIN_VALUE}) {
         lenient("click_input_" + v, sb, click, b -> { varint(b, 0); varint(b, 0); b.writeShort(0); b.writeByte(0); varint(b, v); varint(b, 0); b.writeBoolean(false); });
      }
      bad("click_hashed_unknown_item", sb, click, b -> { varint(b, 0); varint(b, 0); b.writeShort(0); b.writeByte(0); varint(b, 0); varint(b, 0); b.writeBoolean(true); varint(b, BuiltInRegistries.ITEM.size()); varint(b, 1); varint(b, 0); varint(b, 0); });
      bad("click_hashed_patch_too_many", sb, click, b -> { varint(b, 0); varint(b, 0); b.writeShort(0); b.writeByte(0); varint(b, 0); varint(b, 0); b.writeBoolean(true); varint(b, 1); varint(b, 1); varint(b, 257); });
      bad("click_hashed_removed_too_many", sb, click, b -> { varint(b, 0); varint(b, 0); b.writeShort(0); b.writeByte(0); varint(b, 0); varint(b, 0); b.writeBoolean(true); varint(b, 1); varint(b, 1); varint(b, 0); varint(b, 257); });
      bad("click_hashed_unknown_component", sb, click, b -> { varint(b, 0); varint(b, 0); b.writeShort(0); b.writeByte(0); varint(b, 0); varint(b, 0); b.writeBoolean(true); varint(b, 1); varint(b, 1); varint(b, 1); varint(b, 100000); b.writeInt(0); varint(b, 0); });
      byte[] ghost = cbId(new ClientboundPlaceGhostRecipePacket(0, new StonecutterRecipeDisplay(item(Items.STONE), item(Items.STONE), item(Items.STONE))));
      for (int v : new int[]{5, 6, 1000, -1}) {
         bad("display_type_" + v, cb, ghost, b -> { varint(b, 0); varint(b, v); });
      }
      for (int v : new int[]{11, 12, 1000, -1}) {
         bad("slot_display_type_" + v, cb, ghost, b -> { varint(b, 0); varint(b, 3); varint(b, v); });
      }
      bad("slot_display_nested_bad", cb, ghost, b -> { varint(b, 0); varint(b, 3); varint(b, 10); varint(b, 2); varint(b, 0); varint(b, 99); });
      bad("shaped_wrong_size", cb, ghost, b -> { varint(b, 0); varint(b, 1); varint(b, 2); varint(b, 2); varint(b, 3); varint(b, 0); varint(b, 0); varint(b, 0); varint(b, 0); varint(b, 0); });
      lenient("shaped_overflow_size", cb, ghost, b -> { varint(b, 0); varint(b, 1); varint(b, 65536); varint(b, 65536); varint(b, 0); varint(b, 0); varint(b, 0); });
      bad("shaped_neg_size", cb, ghost, b -> { varint(b, 0); varint(b, 1); varint(b, -1); varint(b, 1); varint(b, 0); varint(b, 0); varint(b, 0); });
      bad("slot_item_unknown", cb, ghost, b -> { varint(b, 0); varint(b, 3); varint(b, 4); varint(b, BuiltInRegistries.ITEM.size()); });
      bad("slot_item_stack_air", cb, ghost, b -> { varint(b, 0); varint(b, 3); varint(b, 5); varint(b, 0); varint(b, 1); varint(b, 0); varint(b, 0); });
      bad("slot_item_stack_zero_count", cb, ghost, b -> { varint(b, 0); varint(b, 3); varint(b, 5); varint(b, 1); varint(b, 0); varint(b, 0); varint(b, 0); });
      bad("slot_only_with_component_unknown", cb, ghost, b -> { varint(b, 0); varint(b, 3); varint(b, 3); varint(b, 0); varint(b, 100000); });
      byte[] add = cbId(new ClientboundRecipeBookAddPacket(List.of(), false));
      bad("ingredient_empty", cb, add, b -> { varint(b, 1); stonecutterEntryHead(b); b.writeBoolean(true); varint(b, 1); varint(b, 1); });
      bad("ingredient_air", cb, add, b -> { varint(b, 1); stonecutterEntryHead(b); b.writeBoolean(true); varint(b, 1); varint(b, 2); varint(b, 0); });
      bad("ingredient_unknown_item", cb, add, b -> { varint(b, 1); stonecutterEntryHead(b); b.writeBoolean(true); varint(b, 1); varint(b, 2); varint(b, BuiltInRegistries.ITEM.size()); });
      bad("ingredient_neg_count", cb, add, b -> { varint(b, 1); stonecutterEntryHead(b); b.writeBoolean(true); varint(b, 1); varint(b, -1); });
      bad("category_unknown", cb, add, b -> { varint(b, 1); varint(b, 0); varint(b, 3); varint(b, 0); varint(b, 0); varint(b, 0); varint(b, 0); varint(b, BuiltInRegistries.RECIPE_BOOK_CATEGORY.size()); });
      byte[] upd = cbId(new ClientboundUpdateRecipesPacket(new LinkedHashMap<>(), SelectableRecipe.SingleInputSet.empty()));
      bad("update_unknown_item_in_set", cb, upd, b -> { varint(b, 1); b.writeBytes(utf("minecraft:x")); varint(b, 1); varint(b, BuiltInRegistries.ITEM.size()); });
      lenient("update_dup_items_in_set", cb, upd, b -> { varint(b, 1); b.writeBytes(utf("minecraft:x")); varint(b, 3); varint(b, 5); varint(b, 5); varint(b, 5); varint(b, 0); });
      bad("update_bad_key", cb, upd, b -> { varint(b, 1); b.writeBytes(utf("Bad Key")); varint(b, 0); varint(b, 0); });
      byte[] offersId = cbId(new ClientboundMerchantOffersPacket(0, new MerchantOffers(), 0, 0, false, false));
      bad("merchant_result_empty", cb, offersId, b -> { varint(b, 0); varint(b, 1); varint(b, 1); varint(b, 1); varint(b, 0); varint(b, 0); varint(b, 0); });
   }

   static void stonecutterEntryHead(ByteBuf b) {
      varint(b, 0);       // recipe display id
      varint(b, 3);       // stonecutter display
      varint(b, 0); varint(b, 0); varint(b, 0);   // 3 empty slot displays
      varint(b, 0);       // no group
      varint(b, 0);       // category
   }
}
