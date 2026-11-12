import it.unimi.dsi.fastutil.ints.Int2ObjectOpenHashMap;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Optional;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.network.HashedPatchMap;
import net.minecraft.network.HashedStack;
import net.minecraft.network.protocol.game.*;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.util.HashOps;
import net.minecraft.world.effect.MobEffect;
import net.minecraft.world.inventory.AbstractContainerMenu;
import net.minecraft.world.inventory.ContainerInput;
import net.minecraft.world.inventory.DataSlot;
import net.minecraft.world.inventory.RecipeBookType;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.crafting.Recipe;
import net.minecraft.world.item.crafting.display.RecipeDisplayId;

/**
 * Menu verbs of {@link EntityOracleDriver} (called from its `default` arm), each one a serverbound packet handled by the
 * player's real {@code ServerGamePacketListenerImpl}:
 *
 * <pre>
 * click PLAYER SLOT BUTTON INPUT [cid=N] [state=N|stale] [changed=SLOT:STACK,..] [carried=STACK|-]
 *                             ServerboundContainerClickPacket for the open menu (INPUT = ContainerInput name). Defaults: `cid` the open menu's container id,
 *                             `state` its state id (`stale` = the previous one), `changed` none, `carried` the stack on the menu's cursor before the click
 *                             (what the client believes). STACK = ITEM[components][*COUNT] (`-` = empty); a wrong hash makes the server resend the slot.
 * creative_slot PLAYER SLOT STACK [count]   ServerboundSetCreativeModeSlotPacket (a negative SLOT drops the stack)
 * rename PLAYER TEXT          ServerboundRenameItemPacket                     select_trade PLAYER N   ServerboundSelectTradePacket (= trade_select)
 * set_beacon PLAYER EFFECT1|- EFFECT2|-   ServerboundSetBeaconPacket
 * slot_state PLAYER SLOT true|false [cid=N]   ServerboundContainerSlotStateChangedPacket
 * place_recipe PLAYER RECIPE_ID [all]   ServerboundPlaceRecipePacket for the first display of the recipe (the recipe must be known to the player: `cmd recipe give`)
 * recipe_seen PLAYER RECIPE_ID          ServerboundRecipeBookSeenRecipePacket for the first display of the recipe
 * recipe_settings PLAYER TYPE OPEN FILTERING   ServerboundRecipeBookChangeSettingsPacket (TYPE: crafting furnace blast_furnace smoker)
 * dump recipes:PLAYER        `RECIPES PLAYER crafting=OF furnace=OF blast_furnace=OF smoker=OF known=ID,.. highlight=ID,..` (O = open, F = filtering as 0/1; ids sorted)
 * dump menu:PLAYER            `MENU PLAYER cid=N type=KEY|inventory state=N carried=STACK`, `MS i STACK` per non-empty slot, `MD i value` per data slot
 * </pre>
 */
final class MenuVerbs {
   private MenuVerbs() {
   }

   /** Runs a menu verb; {@code false} when {@code a[0]} is not one. */
   static boolean run(EntityOracleDriver d, List<String> a) throws Exception {
      switch (a.get(0)) {
         case "click" -> click(d, a);
         case "creative_slot" -> {
            d.world();
            ServerPlayer p = d.player(a.get(1));
            ItemStack stack = stack(d, a.get(3));
            if (a.size() > 4 && !stack.isEmpty()) {
               stack.setCount(Integer.parseInt(a.get(4)));
            }
            p.connection.handleSetCreativeModeSlot(new ServerboundSetCreativeModeSlotPacket(Short.parseShort(a.get(2)), stack));
         }
         case "rename" -> {
            d.world();
            String text = a.get(2);
            d.player(a.get(1)).connection.handleRenameItem(new ServerboundRenameItemPacket(text.length() > 1 && text.startsWith("\"") && text.endsWith("\"") ? text.substring(1, text.length() - 1) : text));
         }
         case "select_trade" -> {
            d.world();
            d.player(a.get(1)).connection.handleSelectTrade(new ServerboundSelectTradePacket(Integer.parseInt(a.get(2))));
         }
         case "set_beacon" -> {
            d.world();
            d.player(a.get(1)).connection.handleSetBeaconPacket(new ServerboundSetBeaconPacket(effect(d, a.get(2)), effect(d, a.get(3))));
         }
         case "slot_state" -> {
            d.world();
            ServerPlayer p = d.player(a.get(1));
            int cid = d.kv(a, 4).containsKey("cid") ? Integer.parseInt(d.kv(a, 4).get("cid")) : p.containerMenu.containerId;
            p.connection.handleContainerSlotStateChanged(new ServerboundContainerSlotStateChangedPacket(Integer.parseInt(a.get(2)), cid, Boolean.parseBoolean(a.get(3))));
         }
         case "place_recipe" -> {
            d.world();
            ServerPlayer p = d.player(a.get(1));
            p.connection.handlePlaceRecipe(new ServerboundPlaceRecipePacket(p.containerMenu.containerId, display(d, a.get(2)), a.size() > 3 && a.get(3).equals("all")));
         }
         case "recipe_seen" -> {
            d.world();
            d.player(a.get(1)).connection.handleRecipeBookSeenRecipePacket(new ServerboundRecipeBookSeenRecipePacket(display(d, a.get(2))));
         }
         case "recipe_settings" -> {
            d.world();
            d.player(a.get(1)).connection.handleRecipeBookChangeSettingsPacket(new ServerboundRecipeBookChangeSettingsPacket(
               RecipeBookType.valueOf(a.get(2).toUpperCase(Locale.ROOT)), Boolean.parseBoolean(a.get(3)), Boolean.parseBoolean(a.get(4))));
         }
         default -> {
            return false;
         }
      }
      return true;
   }

   static HashedPatchMap.HashGenerator hasher(ServerPlayer p) {
      return c -> c.encodeValue(p.registryAccess().createSerializationContext(HashOps.CRC32C_INSTANCE)).getOrThrow().asInt();
   }

   static void click(EntityOracleDriver d, List<String> a) throws Exception {
      d.world();
      ServerPlayer p = d.player(a.get(1));
      AbstractContainerMenu m = p.containerMenu;
      HashedPatchMap.HashGenerator h = hasher(p);
      int slot = Integer.parseInt(a.get(2));
      int button = Integer.parseInt(a.get(3));
      ContainerInput input = ContainerInput.valueOf(a.get(4));
      int cid = m.containerId;
      int state = m.getStateId();
      Int2ObjectOpenHashMap<HashedStack> changed = new Int2ObjectOpenHashMap<>();
      HashedStack carried = HashedStack.create(m.getCarried(), h);
      for (int i = 5; i < a.size(); i++) {
         String t = a.get(i);
         int eq = t.indexOf('=');
         String key = t.substring(0, eq);
         String value = t.substring(eq + 1);
         switch (key) {
            case "cid" -> cid = Integer.parseInt(value);
            case "state" -> state = value.equals("stale") ? m.getStateId() - 1 & 32767 : Integer.parseInt(value);
            case "changed" -> {
               for (String entry : split(value)) {
                  int colon = entry.indexOf(':');
                  changed.put(Integer.parseInt(entry.substring(0, colon)), HashedStack.create(stack(d, entry.substring(colon + 1)), h));
               }
            }
            case "carried" -> carried = HashedStack.create(stack(d, value), h);
            default -> throw new IllegalArgumentException("click option " + t);
         }
      }
      p.connection.handleContainerClick(new ServerboundContainerClickPacket(cid, state, (short)slot, (byte)button, input, changed, carried));
   }

   /** `ITEM[components][*COUNT]`, `-` = empty. */
   static ItemStack stack(EntityOracleDriver d, String spec) throws Exception {
      if (spec.equals("-")) {
         return ItemStack.EMPTY;
      }
      int star = spec.lastIndexOf('*');
      if (star > spec.lastIndexOf(']')) {
         return d.stack(spec.substring(0, star), Integer.parseInt(spec.substring(star + 1)));
      }
      return d.stack(spec, 1);
   }

   /** Splits at the commas outside brackets, braces and quotes. */
   static List<String> split(String s) {
      List<String> r = new ArrayList<>();
      StringBuilder cur = new StringBuilder();
      int depth = 0;
      boolean quote = false;
      for (char c : s.toCharArray()) {
         if (c == '"') {
            quote = !quote;
         }
         if (!quote) {
            if (c == '[' || c == '{') {
               depth++;
            } else if (c == ']' || c == '}') {
               depth--;
            }
         }
         if (c == ',' && depth == 0 && !quote) {
            r.add(cur.toString());
            cur.setLength(0);
         } else {
            cur.append(c);
         }
      }
      r.add(cur.toString());
      return r;
   }

   static Optional<Holder<MobEffect>> effect(EntityOracleDriver d, String name) {
      if (name.equals("-")) {
         return Optional.empty();
      }
      return Optional.of(d.v.server.registryAccess().lookupOrThrow(Registries.MOB_EFFECT).getOrThrow(ResourceKey.create(Registries.MOB_EFFECT, EntityOracleDriver.id(name))));
   }

   /** The first display id of a recipe (`RecipeManager.listDisplaysForRecipe`). */
   static RecipeDisplayId display(EntityOracleDriver d, String recipe) {
      List<RecipeDisplayId> ids = new ArrayList<>();
      d.v.server.getRecipeManager().listDisplaysForRecipe(ResourceKey.<Recipe<?>>create(Registries.RECIPE, EntityOracleDriver.id(recipe)), e -> ids.add(e.id()));
      return ids.get(0);
   }

   /** `RECIPES PLAYER crafting=OF furnace=OF blast_furnace=OF smoker=OF known=ID,.. highlight=ID,..` (O = open, F = filtering, ids sorted, `-` = none). */
   static void dumpRecipes(EntityOracleDriver d, String name) {
      var packed = ((ServerPlayer)d.ref(name)).getRecipeBook().pack();
      StringBuilder b = new StringBuilder("RECIPES ").append(name);
      for (RecipeBookType t : RecipeBookType.values()) {
         b.append(' ').append(t.name().toLowerCase(Locale.ROOT)).append('=').append(packed.settings().isOpen(t) ? 1 : 0).append(packed.settings().isFiltering(t) ? 1 : 0);
      }
      for (var list : List.of(Map.entry("known", packed.known()), Map.entry("highlight", packed.highlight()))) {
         List<String> ids = new ArrayList<>(list.getValue().stream().map(k -> k.identifier().toString()).toList());
         java.util.Collections.sort(ids);
         b.append(' ').append(list.getKey()).append('=').append(ids.isEmpty() ? "-" : String.join(",", ids));
      }
      d.out.println(b);
   }

   static void dump(EntityOracleDriver d, String name) {
      ServerPlayer p = (ServerPlayer)d.ref(name);
      AbstractContainerMenu m = p.containerMenu;
      String type = m == p.inventoryMenu ? "inventory" : BuiltInRegistries.MENU.getKey(m.getType()).toString();
      d.out.println("MENU " + name + " cid=" + m.containerId + " type=" + type + " state=" + m.getStateId() + " carried=" + d.itemFull(m.getCarried()));
      for (int i = 0; i < m.slots.size(); i++) {
         ItemStack s = m.getSlot(i).getItem();
         if (!s.isEmpty()) {
            d.out.println("MS " + i + " " + d.itemFull(s));
         }
      }
      List<?> data = (List<?>)EntityOracleDriver.priv(m, AbstractContainerMenu.class, "dataSlots");
      for (int i = 0; i < data.size(); i++) {
         d.out.println("MD " + i + " " + ((DataSlot)data.get(i)).get());
      }
   }
}
