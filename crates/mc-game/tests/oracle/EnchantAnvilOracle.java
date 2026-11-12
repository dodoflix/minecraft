import java.util.*;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.inventory.ContainerInput;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.block.EnchantingTableBlock;

/**
 * Golden generator for crates/mc-game/tests/golden/enchant_anvil_golden.rs: scripted sessions through the REAL vanilla
 * EnchantmentMenu / AnvilMenu / GrindstoneMenu (MenuOracleBase: real ServerPlayer on a real ServerLevel, void world).
 * Usage: scripts/enchant-anvil-golden.sh
 * Output: crates/mc-game/tests/fixtures/enchant_anvil/{enchant,anvil,grindstone}.txt.gz (the wrapper runs this program once per section and compresses)
 * Stdout: the op and OUT lines of MenuOracleBase (see its header), plus `OFFSETS` (the bookshelf offsets) and `SECTION`/`#` lines.
 * Ops used: OPEN kind xp seed creative pseed lseed shelves blockers | SET | GIVE | CLICK | BUTTON | NAME | CLOSE.
 */
public final class EnchantAnvilOracle extends MenuOracleBase {
   public static void main(String[] args) throws Exception {
      boot();
      OUT.println("OFFSETS " + EnchantingTableBlock.BOOKSHELF_OFFSETS.stream().map(b -> b.getX() + "," + b.getY() + "," + b.getZ()).reduce((a, b) -> a + ";" + b).get());
      String which = args.length > 0 ? args[0] : "all";
      if (which.equals("smoke")) {
         open("enchant", 30, 5, false, 11, 22, 8, 0);
         set(1, mk("lapis_lazulix3"));
         set(0, mk("diamond_sword"));
         button(1);
         open("anvil", 100, 1, false, 3, 7, 0, 0);
         set(0, mk("iron_pickaxe,d=100"));
         set(1, mk("iron_ingotx3"));
         name("Foo");
         click(2, 0, ContainerInput.PICKUP);
         open("grindstone", 0, 1, false, 5, 9, 0, 0);
         set(0, mk("diamond_sword,e=sharpness@5;vanishing_curse@1"));
         click(2, 0, ContainerInput.PICKUP);
      }
      if (which.equals("all") || which.equals("enchant")) enchantCases();
      if (which.equals("all") || which.equals("anvil")) anvilCases();
      if (which.equals("all") || which.equals("grindstone")) grindstoneCases();
      OUT.flush();
      System.exit(0);
   }

   // ---------------------------------------------------------------------------------------------------------- enchanting table

   static void enchantCases() throws Exception {
      OUT.println("SECTION enchant");
      List<String> enchantables = new ArrayList<>();
      for (Item i : BuiltInRegistries.ITEM) {
         if (new ItemStack(i).has(DataComponents.ENCHANTABLE)) enchantables.add(BuiltInRegistries.ITEM.getKey(i).getPath());
      }
      OUT.println("# enchantable items: " + enchantables.size());
      int[] seeds = {0, 123456789, -987654321};
      String lapis = "lapis_lazuli x3".replace(" x", "x");
      for (String name : enchantables) {
         for (int seed : seeds) {
            for (int shelves = 0; shelves <= 15; shelves++) {
               open("enchant", 30, seed, false, 11, 22, shelves, 0);
               set(1, mk(lapis));
               set(0, mk(name));
               // the full action at the cheapest and most expensive option for a few shelf counts
               if (seed == 123456789 && (shelves == 0 || shelves == 4 || shelves == 15)) {
                  for (int b = 0; b < 3; b++) {
                     open("enchant", 30, seed, false, 11, 22, shelves, 0);
                     set(1, mk(lapis));
                     set(0, mk(name));
                     button(b);
                     close();
                  }
               }
            }
         }
      }
      // shelf counting: all 32 positions, blocked transmitters, beyond-the-cap
      for (int shelves : new int[] {16, 20, 24, 32}) {
         for (int blockers : new int[] {0, 3, 9}) {
            open("enchant", 30, 777, false, 11, 22, shelves, blockers);
            set(1, mk(lapis));
            set(0, mk("diamond_pickaxe"));
            open("enchant", 30, 777, false, 11, 22, shelves, blockers);
            set(1, mk(lapis));
            set(0, mk("book"));
            button(2);
         }
      }
      // payments: xp, lapis counts, creative, level requirement, already enchanted, stacks, non-enchantables
      for (int xp : new int[] {0, 1, 2, 3, 5, 10, 30}) {
         for (int lap : new int[] {0, 1, 2, 3, 64}) {
            for (boolean creative : new boolean[] {false, true}) {
               for (int b = 0; b < 3; b++) {
                  open("enchant", xp, 424242, creative, 11, 22, 15, 0);
                  if (lap > 0) set(1, mk("lapis_lazuli" + (lap > 1 ? "x" + lap : "")));
                  set(0, mk("iron_chestplate"));
                  button(b);
                  button(b);
                  close();
               }
            }
         }
      }
      for (String spec : new String[] {"stone", "diamond_pickaxe,e=efficiency@1", "enchanted_book,s=sharpness@1", "iron_sword,d=20", "book", "book,rc=3",
         "bow", "trident", "fishing_rod", "elytra", "shears", "mace", "crossbow", "carrot_on_a_stick", "shield", "leather_helmet", "turtle_helmet",
         "golden_sword", "netherite_boots", "stick", "bookx2", "iron_swordx2", "diamond_pickaxe,n=Hello", "nautilus_shell", "brush", "flint_and_steel"}) {
         open("enchant", 30, 99, false, 11, 22, 15, 0);
         set(1, mk("lapis_lazulix3"));
         set(0, mk(spec));
         button(0);
         button(1);
         button(2);
         click(0, 0, ContainerInput.QUICK_MOVE);
         click(1, 0, ContainerInput.QUICK_MOVE);
         close();
      }
      // click handling: quick moves between table and inventory, lapis rules, stack limits
      open("enchant", 30, 5, false, 11, 22, 8, 0);
      give(9, mk("diamond_sword"));
      give(10, mk("lapis_lazulix7"));
      give(11, mk("diamond_swordx2"));
      give(12, mk("stone"));
      give(36, mk("book"));
      for (int s : new int[] {2, 3, 4, 5, 35, 2, 0, 1, 36, 37}) click(s, 0, ContainerInput.QUICK_MOVE);
      click(0, 0, ContainerInput.PICKUP);
      click(1, 0, ContainerInput.PICKUP);
      click(2, 0, ContainerInput.PICKUP);
      click(1, 1, ContainerInput.PICKUP);
      click(0, 0, ContainerInput.PICKUP);
      button(-1);
      button(3);
      button(7);
      close();
   }

   // ---------------------------------------------------------------------------------------------------------- anvil

   static final String[] ANVIL_ITEMS = {
      "iron_pickaxe", "iron_pickaxe,d=100", "iron_pickaxe,d=250", "iron_pickaxe,d=10,e=efficiency@3;unbreaking@2", "iron_pickaxe,rc=3,e=efficiency@5",
      "iron_pickaxe,d=100,rc=7", "diamond_pickaxe,e=efficiency@5;fortune@3", "diamond_pickaxe,e=silk_touch@1", "diamond_sword,e=sharpness@5;looting@3",
      "diamond_sword,e=smite@5", "diamond_sword,e=sharpness@5;mending@1", "diamond_sword,d=1000", "diamond_swordx2", "enchanted_book,s=sharpness@5",
      "enchanted_book,s=sharpness@4", "enchanted_book,s=smite@5", "enchanted_book,s=mending@1", "enchanted_book,s=unbreaking@3",
      "enchanted_book,s=efficiency@5;unbreaking@3", "enchanted_book,s=protection@4", "enchanted_book,s=fire_protection@4", "enchanted_book,s=silk_touch@1",
      "enchanted_book,s=fortune@3", "enchanted_book,s=looting@3,rc=1", "book", "bookx2", "iron_ingot", "iron_ingotx3", "iron_ingotx64", "diamond", "diamondx2",
      "leather,d=0", "iron_chestplate,d=100", "iron_chestplate,e=protection@4", "iron_chestplate,e=blast_protection@3", "iron_helmet,e=respiration@3",
      "elytra,d=200", "phantom_membrane", "phantom_membranex4", "bow,d=100,e=power@5", "crossbow,e=piercing@4", "crossbow,e=multishot@1", "crossbow,e=quick_charge@3",
      "trident,e=loyalty@3;channeling@1", "trident,e=riptide@3", "fishing_rod,e=luck_of_the_sea@3", "shears", "stone", "cobblestonex64", "oak_planks",
      "leather_helmet,d=30", "diamond_helmet,e=thorns@3;binding_curse@1", "diamond_boots,e=feather_falling@4;depth_strider@3",
      "diamond_boots,e=frost_walker@2", "netherite_ingot", "netherite_sword,e=sharpness@5;fire_aspect@2;sweeping_edge@3", "mace,e=density@5;wind_burst@3",
      "mace,e=breach@4", "turtle_helmet,d=40", "turtle_scute", "shield,d=50", "oak_planksx2", "wolf_armor,d=10", "armadillo_scute", "wolf_armor,e=protection@1",
      "iron_pickaxe,n=Mine", "iron_pickaxe,n=Mine,d=50", "name_tag", "diamond_pickaxe,e=efficiency@5;unbreaking@3;mending@1;fortune@3,rc=31",
      "diamond_pickaxe,e=efficiency@5;unbreaking@3;mending@1,rc=1048575", "stick", "carrot_on_a_stick,d=10", "flint_and_steel,d=30", "brush,d=50", "spyglass",
      "iron_axe,e=efficiency@4", "iron_axe,d=3,e=sharpness@3", "diamond_spear,e=lunge@3", "diamond_spear,d=5"
   };

   static void anvilCases() throws Exception {
      OUT.println("SECTION anvil");
      int cases = 0;
      for (int li = 0; li < ANVIL_ITEMS.length; li++) {
         for (int ri = 0; ri < ANVIL_ITEMS.length; ri++) {
            for (int variant = 0; variant < 3; variant++) {
               if (variant == 1 && (li + ri) % 3 != 0) continue;
               if (variant == 2 && (li + ri) % 7 != 0) continue;
               String nm = variant == 0 ? null : variant == 1 ? "Foo" : "";
               boolean creative = variant == 2 && (cases % 2 == 0);
               open("anvil", 100, 1, creative, 31 + cases, 7, 0, 0);
               set(0, mk(ANVIL_ITEMS[li]));
               set(1, mk(ANVIL_ITEMS[ri]));
               if (nm != null) name(nm);
               if (menu.getSlot(2).hasItem()) {
                  click(2, 0, ContainerInput.PICKUP);
                  click(2, 0, ContainerInput.PICKUP);
               }
               cases++;
            }
         }
      }
      // anvil damage chance: many takes on a fresh anvil with different player random seeds
      for (String block : new String[] {"anvil", "chipped_anvil", "damaged_anvil"}) {
         for (int seed = 0; seed < 40; seed++) {
            open(block, 100, 1, false, seed, 7, 0, 0);
            set(0, mk("iron_pickaxe,d=10"));
            set(1, mk("iron_ingot"));
            click(2, 0, ContainerInput.PICKUP);
         }
      }
      // xp requirements, too expensive, creative, rename edge cases, quick move
      for (int xp : new int[] {0, 1, 2, 5, 7, 20, 39, 40}) {
         for (boolean creative : new boolean[] {false, true}) {
            open("anvil", xp, 1, creative, 5, 7, 0, 0);
            set(0, mk("diamond_sword,e=sharpness@5;looting@3,rc=15"));
            set(1, mk("enchanted_book,s=sharpness@5;fire_aspect@2"));
            click(2, 0, ContainerInput.QUICK_MOVE);
            click(2, 0, ContainerInput.PICKUP);
            close();
         }
      }
      for (String nm : new String[] {"A", "x".repeat(50), "x".repeat(51), "\u00a7bColour", "tab\there", "\u00e9\u00e8", "\ud83d\ude00".repeat(25), "\ud83d\ude00".repeat(26), "  spaced  ", "Foo", "Foo"}) {
         for (String it : new String[] {"iron_pickaxe", "iron_pickaxe,n=Foo", "diamond_sword,d=5", "stone", "enchanted_book,s=mending@1"}) {
            open("anvil", 30, 1, false, 5, 7, 0, 0);
            set(0, mk(it));
            name(nm);
            name(nm);
            click(2, 0, ContainerInput.QUICK_MOVE);
            name("Bar");
            close();
         }
      }
      open("anvil", 30, 1, false, 5, 7, 0, 0);
      give(9, mk("iron_pickaxe,d=10"));
      give(10, mk("iron_ingotx9"));
      give(11, mk("stone"));
      for (int s : new int[] {3, 4, 5, 3, 2, 0, 1, 4, 3, 2}) click(s, 0, ContainerInput.QUICK_MOVE);
      close();
   }

   // ---------------------------------------------------------------------------------------------------------- grindstone

   static final String[] GRIND_ITEMS = {
      "iron_pickaxe", "iron_pickaxe,d=100", "iron_pickaxe,d=250", "iron_pickaxe,e=efficiency@3;unbreaking@2", "iron_pickaxe,d=10,e=efficiency@5;mending@1",
      "iron_pickaxe,e=efficiency@5,rc=7", "diamond_pickaxe,d=1000,e=efficiency@5;fortune@3", "diamond_pickaxe,e=silk_touch@1", "diamond_sword,e=sharpness@5;looting@3",
      "diamond_sword,e=sharpness@5;vanishing_curse@1", "diamond_sword,e=binding_curse@1", "diamond_helmet,e=binding_curse@1;protection@4;thorns@3",
      "diamond_helmet,d=100,e=protection@4;aqua_affinity@1", "enchanted_book,s=sharpness@5", "enchanted_book,s=sharpness@4;vanishing_curse@1",
      "enchanted_book,s=vanishing_curse@1", "enchanted_book,s=mending@1", "enchanted_book,s=unbreaking@3", "enchanted_book,s=efficiency@5;unbreaking@3",
      "enchanted_book", "book", "stone", "bow,d=100,e=power@5", "bow,d=20", "bow", "crossbow,e=piercing@4;multishot@1", "trident,d=10,e=loyalty@3;channeling@1",
      "elytra,d=50", "elytra,e=unbreaking@3", "stick,e=sharpness@1", "diamond_pickaxe,d=500,e=vanishing_curse@1", "netherite_helmet,d=10,e=protection@4;respiration@3",
      "diamond_sword,d=3", "diamond_swordx2", "iron_helmet,e=protection@1,d=0", "oak_planksx3", "stonex2", "iron_ingot", "carrot_on_a_stick,d=3", "fishing_rod,d=1,e=luck_of_the_sea@3"
   };

   static void grindstoneCases() throws Exception {
      OUT.println("SECTION grindstone");
      int cases = 0;
      for (String left : GRIND_ITEMS) {
         for (String right : GRIND_ITEMS) {
            open("grindstone", 0, 1, cases % 7 == 0, 5, 100 + cases, 0, 0);
            set(0, mk(left));
            set(1, mk(right));
            if (menu.getSlot(2).hasItem()) {
               click(2, 0, ContainerInput.PICKUP);
               click(2, 0, ContainerInput.PICKUP);
            }
            cases++;
         }
      }
      for (String left : GRIND_ITEMS) {
         open("grindstone", 0, 1, false, 5, 55, 0, 0);
         set(0, mk(left));
         click(2, 0, ContainerInput.QUICK_MOVE);
         close();
         open("grindstone", 0, 1, false, 5, 56, 0, 0);
         set(1, mk(left));
         click(2, 0, ContainerInput.QUICK_MOVE);
         close();
      }
      open("grindstone", 0, 1, false, 5, 7, 0, 0);
      give(9, mk("diamond_sword,e=sharpness@5"));
      give(10, mk("diamond_sword,e=smite@5"));
      give(11, mk("stone"));
      give(12, mk("iron_pickaxe,d=50"));
      for (int s : new int[] {3, 4, 5, 6, 2, 0, 1, 2}) click(s, 0, ContainerInput.QUICK_MOVE);
      close();
   }
}
