import java.io.PrintStream;
import java.security.MessageDigest;
import java.util.*;
import net.minecraft.core.BlockPos;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.util.Unit;
import net.minecraft.world.entity.EntityTypes;
import net.minecraft.world.entity.npc.villager.Villager;
import net.minecraft.world.entity.npc.villager.VillagerData;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.component.MapDecorations;
import net.minecraft.world.item.trading.MerchantOffer;
import net.minecraft.world.item.trading.VillagerTrade;
import net.minecraft.world.level.saveddata.maps.MapDecoration;
import net.minecraft.world.level.saveddata.maps.MapId;
import net.minecraft.world.level.saveddata.maps.MapItemSavedData;
import net.minecraft.world.level.storage.loot.LootContext;
import net.minecraft.world.level.storage.loot.LootParams;
import net.minecraft.world.level.storage.loot.LootTable;
import net.minecraft.world.level.storage.loot.parameters.LootContextParamSets;
import net.minecraft.world.level.storage.loot.parameters.LootContextParams;
import net.minecraft.world.phys.Vec3;

/**
 * Exploration map oracle: the real vanilla {@code exploration_map} loot function in a seeded normal world with structures,
 * through the chest tables that use it and the cartographer map trades. The cases run in this order against one level (the
 * structure reference counters and the map ids they use persist from case to case), the Rust replay runs the same order.
 * Usage: scripts/entity-oracle.sh crates/mc-game/tests/oracle/ExplorationMapGolden.java -o crates/mc-game/tests/fixtures/exploration_map_golden.txt
 * Output: crates/mc-game/tests/fixtures/exploration_map_golden.txt
 *
 * <pre>
 * LOOT table x y z seed             a chest table rolled at the origin with the seed
 * TRADE trade villagertype seed     a cartographer trade offered by a villager of the type at (100.5, 64, -20.5)
 * I item count +component,..        a resulting stack (a trade: the offer's result), components of its patch
 * D key type x z rotation           an entry of the stack's map_decorations
 * M id centerX centerZ scale locked sha1(colors) type x y rot ...    the stack's map saved data and its decorations
 * NONE                              the trade offered nothing
 * </pre>
 */
public final class ExplorationMapGolden {
   static final long SEED = 4242L;
   static final String[] TABLES = {
      "chests/shipwreck_map", "chests/underwater_ruin_big", "chests/underwater_ruin_small", "chests/abandoned_camp_common_chest",
      "chests/abandoned_camp_secret_chest",
   };
   static final int[][] ORIGINS = {{0, 64, 0}, {1500, 64, -800}, {-3000, 64, 2500}};
   static final long[] SEEDS = {1L, 2L, 3L};
   static final String[] TYPES = {"desert", "jungle", "plains", "savanna", "snow", "swamp", "taiga"};

   static VanillaLevel v;
   static PrintStream out;

   public static void main(String[] args) throws Exception {
      out = VanillaLevel.OUT;
      VanillaLevel.Options o = new VanillaLevel.Options();
      o.levelType = "normal";
      o.seed = SEED;
      o.generateStructures = true;
      o.preloadRadiusChunks = 1;
      v = VanillaLevel.boot(o);
      ServerLevel level = v.level;
      for (String path : TABLES) {
         LootTable table = v.server.reloadableRegistries().getLootTable(ResourceKey.create(Registries.LOOT_TABLE, Identifier.withDefaultNamespace(path)));
         for (int[] origin : ORIGINS) {
            for (long seed : SEEDS) {
               LootParams params = new LootParams.Builder(level)
                  .withParameter(LootContextParams.ORIGIN, Vec3.atCenterOf(new BlockPos(origin[0], origin[1], origin[2]))).create(LootContextParamSets.CHEST);
               out.println("LOOT " + path + " " + origin[0] + " " + origin[1] + " " + origin[2] + " " + seed);
               for (ItemStack stack : table.getRandomItems(params, seed)) print(stack);
            }
         }
      }
      var trades = v.server.registryAccess().lookupOrThrow(Registries.VILLAGER_TRADE);
      List<ResourceKey<VillagerTrade>> keys = new ArrayList<>();
      trades.listElementIds().forEach(keys::add);
      keys.sort(Comparator.comparing(k -> k.identifier().toString()));
      for (ResourceKey<VillagerTrade> key : keys) {
         String path = key.identifier().getPath();
         if (!path.startsWith("cartographer/") || !path.contains("map")) continue;
         VillagerTrade trade = trades.getOrThrow(key).value();
         for (String type : TYPES) {
            for (long seed : new long[] {1L, 2L}) {
               Villager villager = new Villager(EntityTypes.VILLAGER, level);
               villager.setPos(100.5, 64.0, -20.5);
               villager.setVillagerData(new VillagerData(
                  BuiltInRegistries.VILLAGER_TYPE.get(Identifier.withDefaultNamespace(type)).orElseThrow(),
                  BuiltInRegistries.VILLAGER_PROFESSION.get(Identifier.withDefaultNamespace("cartographer")).orElseThrow(), 2));
               LootContext context = new LootContext.Builder(new LootParams.Builder(level)
                  .withParameter(LootContextParams.ORIGIN, villager.position())
                  .withParameter(LootContextParams.THIS_ENTITY, villager)
                  .withParameter(LootContextParams.ADDITIONAL_COST_COMPONENT_ALLOWED, Unit.INSTANCE)
                  .create(LootContextParamSets.VILLAGER_TRADE)).withOptionalRandomSeed(seed).create(Optional.empty());
               out.println("TRADE " + key.identifier() + " " + type + " " + seed);
               MerchantOffer offer = trade.getOffer(context);
               if (offer == null) {
                  out.println("NONE");
               } else {
                  print(offer.getResult());
               }
            }
         }
      }
      out.flush();
      System.exit(0);
   }

   static void print(ItemStack s) throws Exception {
      List<String> comps = new ArrayList<>();
      var split = s.getComponentsPatch().split();
      split.added().forEach(c -> comps.add("+" + BuiltInRegistries.DATA_COMPONENT_TYPE.getKey(c.type())));
      split.removed().forEach(t -> comps.add("-" + BuiltInRegistries.DATA_COMPONENT_TYPE.getKey(t)));
      Collections.sort(comps);
      out.println("I " + BuiltInRegistries.ITEM.getKey(s.getItem()) + " " + s.getCount() + " " + String.join(",", comps));
      MapDecorations decorations = s.get(DataComponents.MAP_DECORATIONS);
      if (decorations != null) {
         new TreeMap<>(decorations.decorations()).forEach((key, e) -> out.println(
            "D " + key + " " + e.type().unwrapKey().get().identifier() + " " + e.x() + " " + e.z() + " " + e.rotation()));
      }
      MapId id = s.get(DataComponents.MAP_ID);
      if (id != null) {
         MapItemSavedData data = v.level.getMapData(id);
         StringBuilder sb = new StringBuilder("M " + id.id() + " " + data.centerX + " " + data.centerZ + " " + data.scale + " " + data.locked + " "
            + sha1(data.colors));
         for (MapDecoration d : data.getDecorations()) {
            sb.append(' ').append(d.type().unwrapKey().get().identifier()).append(' ').append(d.x()).append(' ').append(d.y()).append(' ').append(d.rot());
         }
         out.println(sb);
      }
   }

   static String sha1(byte[] bytes) throws Exception {
      StringBuilder sb = new StringBuilder();
      for (byte b : MessageDigest.getInstance("SHA-1").digest(bytes)) sb.append(String.format("%02x", b));
      return sb.toString();
   }
}
