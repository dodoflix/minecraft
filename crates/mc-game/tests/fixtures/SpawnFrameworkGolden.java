// Generates tests/fixtures/spawn_framework_golden.txt:
//   scripts/vanilla-java.sh crates/mc-game/tests/fixtures/SpawnFrameworkGolden.java ref/src/data/minecraft/worldgen/biome | grep "\[STDOUT\]: " | sed -e 's/^\[[0-9:]*\] \[main\/INFO\]: \[STDOUT\]: //' > crates/mc-game/tests/fixtures/spawn_framework_golden.txt
// Sections: MobCategory constants, the SpawnPlacements table (placement type + heightmap per entity type), PotentialCalculator
// charge sums over scripted positions, MobSpawnSettings decoded from every vanilla biome.
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import com.mojang.serialization.JsonOps;
import java.io.File;
import java.io.FileReader;
import java.util.Arrays;
import java.util.Random;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.MobCategory;
import net.minecraft.world.entity.SpawnPlacements;
import net.minecraft.world.level.PotentialCalculator;
import net.minecraft.world.level.biome.MobSpawnSettings;

public class SpawnFrameworkGolden {
   public static void main(String[] args) throws Exception {
      net.minecraft.SharedConstants.tryDetectVersion();
      net.minecraft.server.Bootstrap.bootStrap();

      for (MobCategory c : MobCategory.values()) {
         System.out.println("category " + c.getName() + " " + c.getMaxInstancesPerChunk() + " " + c.isFriendly() + " " + c.isPersistent() + " "
            + c.getDespawnDistance() + " " + c.getNoDespawnDistance());
      }

      for (EntityType<?> type : BuiltInRegistries.ENTITY_TYPE) {
         String key = BuiltInRegistries.ENTITY_TYPE.getKey(type).toString();
         System.out.println("placement " + key + " " + (SpawnPlacements.getPlacementType(type) == net.minecraft.world.entity.SpawnPlacementTypes.ON_GROUND ? "ON_GROUND"
            : SpawnPlacements.getPlacementType(type) == net.minecraft.world.entity.SpawnPlacementTypes.IN_WATER ? "IN_WATER"
            : SpawnPlacements.getPlacementType(type) == net.minecraft.world.entity.SpawnPlacementTypes.IN_LAVA ? "IN_LAVA" : "NO_RESTRICTIONS")
            + " " + SpawnPlacements.getHeightmapType(type));
      }

      // PotentialCalculator: seeded charges, probes after each batch (bits of the double results)
      Random r = new Random(1234);
      PotentialCalculator calculator = new PotentialCalculator();
      for (int batch = 0; batch < 6; batch++) {
         for (int i = 0; i < 5; i++) {
            BlockPos pos = new BlockPos(r.nextInt(40) - 20, 60 + r.nextInt(8), r.nextInt(40) - 20);
            double charge = r.nextInt(4) == 0 ? 0.0 : r.nextInt(100) / 100.0 - 0.2;
            calculator.addCharge(pos, charge);
            System.out.println("charge " + pos.getX() + " " + pos.getY() + " " + pos.getZ() + " " + Double.doubleToLongBits(charge));
         }
         for (int i = 0; i < 6; i++) {
            BlockPos pos = new BlockPos(r.nextInt(40) - 20, 60 + r.nextInt(8), r.nextInt(40) - 20);
            double charge = r.nextInt(5) == 0 ? 0.0 : r.nextInt(100) / 100.0;
            System.out.println("probe " + pos.getX() + " " + pos.getY() + " " + pos.getZ() + " " + Double.doubleToLongBits(charge) + " "
               + Double.doubleToLongBits(calculator.getPotentialEnergyChange(pos, charge)));
         }
      }

      // MobSpawnSettings of every vanilla biome
      File[] files = new File(args[0]).listFiles((d, n) -> n.endsWith(".json"));
      Arrays.sort(files);
      for (File f : files) {
         JsonObject root = JsonParser.parseReader(new FileReader(f)).getAsJsonObject();
         String name = f.getName().replace(".json", "");
         JsonObject attributes = root.has("attributes") ? root.getAsJsonObject("attributes") : new JsonObject();
         JsonElement entry = attributes.get("minecraft:gameplay/natural_mob_spawns");
         if (entry == null) {
            System.out.println("biome " + name + " none");
         } else {
            JsonElement argument = entry.getAsJsonObject().get("argument");
            MobSpawnSettings settings = MobSpawnSettings.CODEC.parse(JsonOps.INSTANCE, argument).getOrThrow();
            System.out.println("biome " + name + " " + entry.getAsJsonObject().get("modifier").getAsString());
            for (MobCategory c : MobCategory.values()) {
               var list = settings.getMobsInCategory(c);
               if (list == null) continue;
               System.out.println("  category " + c.getName() + " " + list.unwrap().size());
               list.unwrap().forEach(w -> System.out.println("    " + BuiltInRegistries.ENTITY_TYPE.getKey(w.value().type()) + " " + w.value().count().minInclusive() + " "
                  + w.value().count().maxInclusive() + " " + w.weight()));
            }
            settings.allSpawnCosts().entrySet().stream().sorted((a, b) -> BuiltInRegistries.ENTITY_TYPE.getKey(a.getKey()).compareTo(BuiltInRegistries.ENTITY_TYPE.getKey(b.getKey()))).forEach(e ->
               System.out.println("  cost " + BuiltInRegistries.ENTITY_TYPE.getKey(e.getKey()) + " " + Double.doubleToLongBits(e.getValue().energyBudget()) + " " + Double.doubleToLongBits(e.getValue().charge())));
         }
         JsonElement prob = attributes.get("minecraft:gameplay/creature_world_gen_spawn_probability");
         System.out.println("  creature_probability " + (prob == null ? "default" : Float.floatToIntBits(prob.getAsFloat())));
      }
   }
}
