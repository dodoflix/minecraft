import java.util.*;
import net.minecraft.core.BlockPos;
import net.minecraft.core.component.DataComponents;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.inventory.AbstractContainerMenu;
import net.minecraft.world.inventory.BrewingStandMenu;
import net.minecraft.world.inventory.ContainerInput;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.alchemy.PotionContents;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.phys.AABB;

/**
 * Real-vanilla trace of the brewing stand: a survival player with a random inventory opens the menu of a brewing stand
 * (real server, real {@code BrewingStandBlockEntity}), then a seeded script of menu clicks / quick-craft drags / server
 * ticks runs; the state after every op is printed.
 * Usage: scripts/entity-oracle.sh crates/mc-game/tests/oracle/BrewingMenuOracle.java -o crates/mc-game/tests/fixtures/brewing/menu_trace.txt [scenarios [ops]]
 * Output: crates/mc-game/tests/fixtures/brewing/menu_trace.txt
 *
 * <pre>
 * SCENARIO n
 * SETUP slot stack      player inventory container slot (0-8 hotbar, 9-35 main) and stand slot s0..s4 as `STAND i stack`
 * CLICK slot button input(0-6)     TICK n
 * S i=stack ... (non-empty menu slots 0..40) | D brew fuel totalBrew totalFuel | C carried | B block state | E dropped items
 * </pre>
 * Stack syntax: {@code id#count#potion} (id without namespace, potion = registry path or {@code -}).
 */
public final class BrewingMenuOracle {
   static final String[] PALETTE = {
      "nether_wart", "blaze_powder", "redstone", "glowstone_dust", "gunpowder", "fermented_spider_eye", "sugar", "stone",
      "glass_bottle", "stick", "dragon_breath", "blaze_rod", "diamond", "water_bucket", "potion:water", "potion:awkward",
      "potion:mundane", "potion:thick", "potion:healing", "potion:swiftness", "potion:strong_healing", "splash_potion:awkward",
      "splash_potion:water", "lingering_potion:water", "potion", "milk_bucket", "spider_eye", "ghast_tear", "magma_cream"
   };

   static String st(ItemStack s) {
      if (s.isEmpty()) {
         return "-";
      }
      String id = net.minecraft.core.registries.BuiltInRegistries.ITEM.getKey(s.getItem()).getPath();
      PotionContents pc = s.get(DataComponents.POTION_CONTENTS);
      String potion = "-";
      if (pc != null && pc.potion().isPresent()) {
         potion = pc.potion().get().unwrapKey().get().identifier().getPath();
      }
      return id + "#" + s.getCount() + "#" + potion;
   }

   static String command(String stack) {
      String[] p = stack.split(":");
      return p.length == 1 ? "minecraft:" + p[0] : "minecraft:" + p[0] + "[potion_contents={potion:\"minecraft:" + p[1] + "\"}]";
   }

   public static void main(String[] args) throws Exception {
      int scenarios = args.length > 0 ? Integer.parseInt(args[0]) : 8;
      int ops = args.length > 1 ? Integer.parseInt(args[1]) : 220;
      VanillaLevel.Options o = new VanillaLevel.Options();
      o.levelType = "flat";
      VanillaLevel v = VanillaLevel.boot(o);
      var out = VanillaLevel.OUT;
      VanillaLevel.FakePlayer fp = v.connect("Brewer", GameType.SURVIVAL, 0.5, -60, 2.5, 180, 0, false);
      BlockPos pos = new BlockPos(0, -60, 0);
      for (int sc = 0; sc < scenarios; sc++) {
         Random r = new Random(1000 + sc);
         out.println("SCENARIO " + sc);
         v.command("setblock 0 -60 0 minecraft:air");
         v.command("setblock 0 -60 0 minecraft:brewing_stand");
         v.command("kill @e[type=item]");
         v.tick(1);
         v.command("clear Brewer");
         for (int slot = 0; slot < 36; slot++) {
            if (r.nextInt(100) < 45) {
               String item = PALETTE[r.nextInt(PALETTE.length)];
               int count = item.contains("potion") || item.contains("bucket") ? 1 : 1 + r.nextInt(8);
               v.command("item replace entity Brewer container." + slot + " with " + command(item) + " " + count);
               String[] tok = item.split(":");
               out.println("SETUP " + slot + " " + tok[0] + "#" + count + "#" + (tok.length > 1 ? tok[1] : "-"));
            }
         }
         int fuel = r.nextInt(3) == 0 ? 0 : 1 + r.nextInt(25);
         StringBuilder items = new StringBuilder();
         String[] bottles = {"water", "awkward", "mundane", "thick", "healing", "swiftness"};
         String[] reagents = {"nether_wart", "redstone", "glowstone_dust", "gunpowder", "fermented_spider_eye", "sugar", "blaze_powder", "stone"};
         if (r.nextInt(100) < 70) {
            for (int b = 0; b < 3; b++) {
               if (r.nextInt(100) < 70) {
                  String kind = r.nextInt(4) == 0 ? "splash_potion" : "potion";
                  items.append("{Slot:").append(b).append("b,id:\"minecraft:").append(kind).append("\",count:1,components:{\"minecraft:potion_contents\":{potion:\"minecraft:").append(bottles[r.nextInt(bottles.length)]).append("\"}}},");
               }
            }
            items.append("{Slot:3b,id:\"minecraft:").append(reagents[r.nextInt(reagents.length)]).append("\",count:").append(1 + r.nextInt(3)).append("},");
            if (r.nextBoolean()) {
               items.append("{Slot:4b,id:\"minecraft:blaze_powder\",count:").append(1 + r.nextInt(3)).append("},");
            }
         }
         v.command("data merge block 0 -60 0 {Fuel:" + fuel + ",Items:[" + items + "]}");
         out.println("FUEL " + fuel);
         BlockEntity be = v.level.getBlockEntity(pos);
         for (int s = 0; s < 5; s++) {
            ItemStack standItem = ((net.minecraft.world.Container) be).getItem(s);
            if (!standItem.isEmpty()) {
               out.println("STAND " + s + " " + st(standItem));
            }
         }
         fp.player.openMenu((net.minecraft.world.MenuProvider) be);
         AbstractContainerMenu menu = fp.player.containerMenu;
         for (int i = 0; i < ops; i++) {
            int kind = r.nextInt(100);
            if (kind < 72) {
               int slot = r.nextInt(100) < 55 ? r.nextInt(5) : r.nextInt(100) < 5 ? -999 : r.nextInt(41);
               int type = r.nextInt(7);
               if (slot == -999 && type != 0) {
                  slot = r.nextInt(41);
               }
               int button = switch (type) {
                  case 0, 1, 4 -> r.nextInt(2);
                  case 2 -> r.nextBoolean() ? r.nextInt(9) : 40;
                  case 3 -> 2;
                  default -> 0;
               };
               out.println("CLICK " + slot + " " + button + " " + type);
               menu.clicked(slot, button, ContainerInput.values()[type], fp.player);
               menu.broadcastChanges();
            } else if (kind < 84) {
               int type = r.nextInt(2);
               out.println("CLICK -999 " + (0 | (type << 2)) + " 5");
               menu.clicked(-999, 0 | (type << 2), ContainerInput.QUICK_CRAFT, fp.player);
               int n = 1 + r.nextInt(4);
               for (int k = 0; k < n; k++) {
                  int slot = r.nextInt(100) < 60 ? r.nextInt(5) : 5 + r.nextInt(36);
                  out.println("CLICK " + slot + " " + (1 | (type << 2)) + " 5");
                  menu.clicked(slot, 1 | (type << 2), ContainerInput.QUICK_CRAFT, fp.player);
               }
               int button = 2 | (type << 2);
               out.println("CLICK -999 " + button + " 5");
               menu.clicked(-999, button, ContainerInput.QUICK_CRAFT, fp.player);
               menu.broadcastChanges();
            } else {
               int n = switch (r.nextInt(4)) { case 0 -> 1; case 1 -> 7; case 2 -> 60; default -> 401; };
               out.println("TICK " + n);
               v.tick(n);
               if (fp.player.containerMenu != menu) {
                  menu = fp.player.containerMenu;
                  out.println("# menu closed");
                  if (menu == fp.player.inventoryMenu) {
                     fp.player.openMenu((net.minecraft.world.MenuProvider) v.level.getBlockEntity(pos));
                     menu = fp.player.containerMenu;
                  }
               }
            }
            StringBuilder sb = new StringBuilder("S");
            for (int s = 0; s < menu.slots.size(); s++) {
               ItemStack it = menu.getSlot(s).getItem();
               if (!it.isEmpty()) {
                  sb.append(' ').append(s).append('=').append(st(it));
               }
            }
            BrewingStandMenu bm = (BrewingStandMenu) menu;
            sb.append(" | D ").append(bm.getBrewingTicks()).append(' ').append(bm.getFuel()).append(' ').append(bm.getTotalBrewingTicks()).append(' ').append(bm.getTotalFuel());
            sb.append(" | C ").append(st(menu.getCarried()));
            BlockState bs = v.level.getBlockState(pos);
            sb.append(" | B ").append(bs.getValue(net.minecraft.world.level.block.BrewingStandBlock.HAS_BOTTLE[0])).append(bs.getValue(net.minecraft.world.level.block.BrewingStandBlock.HAS_BOTTLE[1])).append(bs.getValue(net.minecraft.world.level.block.BrewingStandBlock.HAS_BOTTLE[2]));
            TreeMap<String, Integer> dropped = new TreeMap<>();
            for (ItemEntity ie : v.level.getEntitiesOfClass(ItemEntity.class, new AABB(-30, -70, -30, 30, -40, 30))) {
               String key = st(ie.getItem().copyWithCount(1));
               dropped.merge(key, ie.getItem().getCount(), Integer::sum);
            }
            sb.append(" | E ").append(dropped);
            out.println(sb);
         }
      }
      out.flush();
      System.exit(0);
   }
}
