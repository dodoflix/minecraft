import java.util.*;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.attribute.BedRule;
import net.minecraft.world.attribute.EnvironmentAttribute;
import net.minecraft.world.clock.WorldClock;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.biome.MobSpawnSettings;
import net.minecraft.world.phys.Vec3;
import net.minecraft.util.TriState;
import org.joml.Vector3fc;
import org.joml.Vector4fc;

/**
 * Environment attribute oracle: the REAL vanilla {@code EnvironmentAttributeSystem} of the overworld, the nether and the end of an
 * in-process vanilla server (see {@link VanillaLevel}), sampled for every registered attribute at several world clock values and
 * weather levels. Run: {@code scripts/entity-oracle.sh crates/mc-game/tests/oracle/AttributeOracle.java > crates/mc-game/tests/fixtures/attribute_oracle.txt}
 *
 * <pre>
 * B dim idx x y z full=BIOME noise=BIOME     the probe positions and the biomes the biome layers see there
 * V dim ticks rain thunder attribute v0|v1|..   value per probe (a single value when all probes agree); non-positional
 *                                               attributes print the dimension value
 * </pre>
 * Value formats: floats = Float.toString, booleans, tri state / moon phase / activity names, rgb "x,y,z", argb "x,y,z,w",
 * bed rule "can_sleep/can_set_spawn/destroy_on_use/destroy_on_leave/error", mob spawn settings "categories:...;costs:...",
 * the remaining (sound/particle) types print only whether they are empty.
 */
public final class AttributeOracle {
   static final int[][] PROBES = {{0, 0}, {-90, 40}, {75, -60}, {30, 95}, {-70, -85}};
   static final long[] TICKS = {0, 71, 133, 6000, 11867, 12542, 13000, 13670, 18000, 22330, 23460, 23999, 24000, 72000, 96000, 168000, 192000, 120001};
   static final float[][] WEATHER = {{0, 0}, {1, 0}, {1, 1}, {0.5f, 0.3f}, {0.8f, 0.8f}};

   public static void main(String[] args) throws Exception {
      VanillaLevel.Options o = new VanillaLevel.Options();
      o.levelType = "normal";
      o.preloadRadiusChunks = 2;
      VanillaLevel v = VanillaLevel.boot(o);
      var out = VanillaLevel.OUT;
      @SuppressWarnings("unchecked")
      List<ResourceKey<Level>> dims = List.of(Level.OVERWORLD, Level.NETHER, Level.END);
      for (ResourceKey<Level> dim : dims) {
         ServerLevel level = v.server.getLevel(dim);
         String name = dim.identifier().toString();
         List<BlockPos> probes = new ArrayList<>();
         for (int i = 0; i < PROBES.length; i++) {
            BlockPos pos = new BlockPos(PROBES[i][0], 64, PROBES[i][1]);
            probes.add(pos);
            String full = level.getBiome(pos).unwrapKey().get().identifier().toString();
            String noise = level.getBiomeManager().getNoiseBiomeAtPosition(pos.getX() + 0.5, pos.getY() + 0.5, pos.getZ() + 0.5).unwrapKey().get().identifier().toString();
            out.println("B " + name + " " + i + " " + pos.getX() + " " + pos.getY() + " " + pos.getZ() + " full=" + full + " noise=" + noise);
         }
         Holder<WorldClock> clock = level.dimensionType().defaultClock().orElse(null);
         for (long ticks : TICKS) {
            if (clock == null) {
               if (ticks != TICKS[0]) {
                  continue;
               }
            } else {
               v.server.clockManager().setTotalTicks(clock, ticks);
            }
            for (float[] w : WEATHER) {
               level.setRainLevel(w[0]);
               level.setThunderLevel(w[1]);
               level.environmentAttributes().invalidateTickCache();
               for (var entry : BuiltInRegistries.ENVIRONMENT_ATTRIBUTE.entrySet()) {
                  EnvironmentAttribute<?> attribute = entry.getValue();
                  String id = entry.getKey().identifier().getPath();
                  List<String> values = new ArrayList<>();
                  if (attribute.isPositional()) {
                     for (BlockPos pos : probes) {
                        values.add(format(attribute, level.environmentAttributes().getValue(attribute, Vec3.atCenterOf(pos))));
                     }
                  } else {
                     values.add(format(attribute, level.environmentAttributes().getDimensionValue(attribute)));
                  }
                  boolean same = values.stream().distinct().count() == 1;
                  out.println("V " + name + " " + (clock == null ? -1 : ticks) + " " + w[0] + " " + w[1] + " " + id + " " + (same ? values.get(0) : String.join("|", values)));
               }
            }
         }
      }
      out.flush();
      System.exit(0);
   }

   static String f(float x) {
      return Float.toString(x);
   }

   @SuppressWarnings({"unchecked", "rawtypes"})
   static String format(EnvironmentAttribute attribute, Object value) {
      if (value instanceof Boolean || value instanceof Integer) {
         return value.toString();
      }
      if (value instanceof Float x) {
         return f(x);
      }
      if (value instanceof TriState t) {
         return t.getSerializedName();
      }
      if (value instanceof net.minecraft.world.level.MoonPhase m) {
         return m.getSerializedName();
      }
      if (value instanceof net.minecraft.world.entity.schedule.Activity a) {
         return BuiltInRegistries.ACTIVITY.getKey(a).toString();
      }
      if (value instanceof Vector3fc c) {
         return f(c.x()) + "," + f(c.y()) + "," + f(c.z());
      }
      if (value instanceof Vector4fc c) {
         return f(c.x()) + "," + f(c.y()) + "," + f(c.z()) + "," + f(c.w());
      }
      if (value instanceof BedRule r) {
         String error = r.errorMessage().map(m -> net.minecraft.network.chat.ComponentSerialization.CODEC.encodeStart(com.mojang.serialization.JsonOps.INSTANCE, m).getOrThrow().toString()).orElse("-");
         return r.canSleep().getSerializedName() + "/" + r.canSetSpawn().getSerializedName() + "/" + r.destroyOnUse() + "/" + r.destroyOnLeave() + "/" + error;
      }
      if (value instanceof MobSpawnSettings s) {
         StringBuilder b = new StringBuilder("categories:");
         for (var c : net.minecraft.world.entity.MobCategory.values()) {
            var list = s.getMobsInCategory(c);
            if (list != null) {
               b.append(c.getName()).append('=').append(list.unwrap().size()).append(',');
            }
         }
         b.append(";costs:").append(s.allSpawnCosts().size());
         return b.toString();
      }
      if (value instanceof List<?> l) {
         return "list" + l.size();
      }
      String json = attribute.type().valueCodec().encodeStart(com.mojang.serialization.JsonOps.INSTANCE, value).getOrThrow().toString();
      return json.equals("{}") || json.equals("[]") ? "empty" : "set";
   }
}
