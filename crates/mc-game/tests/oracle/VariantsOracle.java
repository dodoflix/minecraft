import java.io.PrintStream;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.util.*;
import java.util.stream.Collectors;
import com.mojang.serialization.Codec;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Holder;
import net.minecraft.core.Registry;
import net.minecraft.core.registries.Registries;
import net.minecraft.nbt.*;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.RegistryOps;
import net.minecraft.resources.ResourceKey;
import net.minecraft.util.RandomSource;
import net.minecraft.world.attribute.EnvironmentAttributeReader;
import net.minecraft.world.attribute.EnvironmentAttributes;
import net.minecraft.world.attribute.SpatialAttributeInterpolator;
import net.minecraft.world.level.MoonPhase;
import net.minecraft.world.entity.animal.chicken.*;
import net.minecraft.world.entity.animal.cow.*;
import net.minecraft.world.entity.animal.feline.*;
import net.minecraft.world.entity.animal.frog.FrogVariant;
import net.minecraft.world.entity.animal.nautilus.ZombieNautilusVariant;
import net.minecraft.world.entity.animal.pig.*;
import net.minecraft.world.entity.animal.wolf.*;
import net.minecraft.world.entity.variant.PriorityProvider;
import net.minecraft.world.entity.variant.SpawnContext;
import net.minecraft.world.level.biome.Biome;
import net.minecraft.world.level.levelgen.structure.Structure;
import net.minecraft.world.phys.Vec3;

/**
 * Mob variant goldens (scripts/golden-variants.sh -> tests/fixtures/variants_golden.txt), run through scripts/entity-oracle.sh:
 * <pre>
 * E registry id sha1(canon(DIRECT_CODEC nbt))      every entry of the 12 variant registries
 * N registry id sha1(canon(NETWORK_CODEC nbt))     the registry_data payload
 * P registry seed id                                Registry.getRandom(RandomSource.create(seed)) (pickRandomSoundVariant)
 * C biome moonPhaseIndex structures                 a selection context (structures = ids with a piece at the context pos, or -)
 * S registry seed picked                            PriorityProvider.pick over the registry for the preceding C line
 * </pre>
 * canon: compound keys sorted, numbers with a type suffix, floats/doubles as raw bits.
 */
public final class VariantsOracle {
   @SuppressWarnings("rawtypes")
   record Reg(String name, ResourceKey<? extends Registry<?>> key, Codec direct, Codec network, boolean selects) {}

   static PrintStream out;

   public static void main(String[] args) {
      try {
         run();
      } catch (Throwable t) {
         t.printStackTrace(VanillaLevel.ERR);
         System.exit(1);
      }
   }

   @SuppressWarnings({"unchecked", "rawtypes"})
   static void run() throws Exception {
      out = VanillaLevel.OUT;
      VanillaLevel v = VanillaLevel.boot(new VanillaLevel.Options());
      var access = v.server.registryAccess();
      var ops = RegistryOps.create(NbtOps.INSTANCE, access);
      List<Reg> regs = List.of(
         new Reg("wolf_variant", Registries.WOLF_VARIANT, WolfVariant.DIRECT_CODEC, WolfVariant.NETWORK_CODEC, true),
         new Reg("wolf_sound_variant", Registries.WOLF_SOUND_VARIANT, WolfSoundVariant.DIRECT_CODEC, WolfSoundVariant.NETWORK_CODEC, false),
         new Reg("cat_variant", Registries.CAT_VARIANT, CatVariant.DIRECT_CODEC, CatVariant.NETWORK_CODEC, true),
         new Reg("cat_sound_variant", Registries.CAT_SOUND_VARIANT, CatSoundVariant.DIRECT_CODEC, CatSoundVariant.NETWORK_CODEC, false),
         new Reg("frog_variant", Registries.FROG_VARIANT, FrogVariant.DIRECT_CODEC, FrogVariant.NETWORK_CODEC, true),
         new Reg("pig_variant", Registries.PIG_VARIANT, PigVariant.DIRECT_CODEC, PigVariant.NETWORK_CODEC, true),
         new Reg("pig_sound_variant", Registries.PIG_SOUND_VARIANT, PigSoundVariant.DIRECT_CODEC, PigSoundVariant.NETWORK_CODEC, false),
         new Reg("cow_variant", Registries.COW_VARIANT, CowVariant.DIRECT_CODEC, CowVariant.NETWORK_CODEC, true),
         new Reg("cow_sound_variant", Registries.COW_SOUND_VARIANT, CowSoundVariant.DIRECT_CODEC, CowSoundVariant.NETWORK_CODEC, false),
         new Reg("chicken_variant", Registries.CHICKEN_VARIANT, ChickenVariant.DIRECT_CODEC, ChickenVariant.NETWORK_CODEC, true),
         new Reg("chicken_sound_variant", Registries.CHICKEN_SOUND_VARIANT, ChickenSoundVariant.DIRECT_CODEC, ChickenSoundVariant.NETWORK_CODEC, false),
         new Reg("zombie_nautilus_variant", Registries.ZOMBIE_NAUTILUS_VARIANT, ZombieNautilusVariant.DIRECT_CODEC, ZombieNautilusVariant.NETWORK_CODEC, true));

      for (Reg r : regs) {
         Registry reg = access.lookupOrThrow((ResourceKey) r.key);
         List<String> lines = new ArrayList<>();
         for (Object o : reg.listElements().toList()) {
            Holder.Reference h = (Holder.Reference) o;
            String id = h.key().identifier().toString();
            lines.add("E " + r.name + " " + id + " " + sha1(canon((Tag) r.direct.encodeStart(ops, h.value()).getOrThrow())));
            lines.add("N " + r.name + " " + id + " " + sha1(canon((Tag) r.network.encodeStart(ops, h.value()).getOrThrow())));
         }
         lines.forEach(out::println);
         for (long seed : new long[] {1L, 2L, 3L, 4L, 5L, 99L}) {
            Optional<Holder.Reference> pick = reg.getRandom(RandomSource.create(seed));
            out.println("P " + r.name + " " + seed + " " + pick.map(h -> h.key().identifier().toString()).orElse("-"));
         }
      }

      // a swamp hut in the flat oracle world, for the structure check
      var structures = access.lookupOrThrow(Registries.STRUCTURE);
      BlockPos huttCenter;
      {
         var holder = structures.getOrThrow(ResourceKey.create(Registries.STRUCTURE, Identifier.withDefaultNamespace("swamp_hut")));
         var gen = v.level.getChunkSource().getGenerator();
         var randomState = v.level.getChunkSource().randomState();
         var chunkPos = net.minecraft.world.level.ChunkPos.containing(new BlockPos(8, -60, 8));
         var start = holder.value().generate(holder, v.level.dimension(), access, gen, gen.getBiomeSource(),
            randomState.createClimateSampler(net.minecraft.world.level.levelgen.densityfunction.SamplerContext.EMPTY_UNCACHED), randomState,
            v.level.getStructureTemplateManager(), v.level.getSeed(), chunkPos, 0, v.level, b -> true);
         if (!start.isValid()) throw new IllegalStateException("swamp hut start invalid");
         var chunk = v.level.getChunk(chunkPos.x(), chunkPos.z());
         chunk.setStartForStructure(holder.value(), start);
         chunk.addReferenceForStructure(holder.value(), chunkPos.pack());
         huttCenter = start.getPieces().get(0).getBoundingBox().getCenter();
      }
      var biomes = access.lookupOrThrow(Registries.BIOME);
      List<BlockPos> positions = new ArrayList<>();
      BlockPos inside = huttCenter;
      if (!v.level.structureManager().getStructureWithPieceAt(inside, st -> st.is(Identifier.withDefaultNamespace("swamp_hut"))).isValid()) throw new IllegalStateException("no swamp hut piece found at " + inside);
      positions.add(inside);
      positions.add(new BlockPos(200, 64, 200));
      List<ResourceKey<Biome>> biomeKeys = biomes.listElements().map(h -> h.key()).sorted(Comparator.comparing(k -> k.identifier().toString())).toList();
      for (BlockPos pos : positions) {
         String structs = structures.listElements()
            .filter(h -> v.level.structureManager().getStructureWithPieceAt(pos, s -> s.is(h.key().identifier())).isValid())
            .map(h -> h.key().identifier().toString()).sorted().collect(Collectors.joining(","));
         if (structs.isEmpty()) structs = "-";
         for (ResourceKey<Biome> biomeKey : biomeKeys) {
            for (int phase = 0; phase < 8; phase++) {
               final int fphase = phase;
               EnvironmentAttributeReader reader = new EnvironmentAttributeReader() {
                  @Override
                  public <V> V getDimensionValue(net.minecraft.world.attribute.EnvironmentAttribute<V> attribute) {
                     return attribute.defaultValue();
                  }

                  @Override
                  @SuppressWarnings("unchecked")
                  public <V> V getValue(net.minecraft.world.attribute.EnvironmentAttribute<V> attribute, Vec3 p, SpatialAttributeInterpolator interpolator) {
                     if (attribute == EnvironmentAttributes.MOON_PHASE) return (V) MoonPhase.values()[fphase];
                     return attribute.defaultValue();
                  }
               };
               SpawnContext context = new SpawnContext(pos, v.level, reader, biomes.getOrThrow(biomeKey));
               out.println("C " + biomeKey.identifier() + " " + phase + " " + pos.getX() + "," + pos.getY() + "," + pos.getZ() + " " + structs);
               for (Reg r : regs) {
                  if (!r.selects) continue;
                  Registry reg = access.lookupOrThrow((ResourceKey) r.key);
                  for (long seed : new long[] {1L, 2L, 3L}) {
                     Optional<Holder.Reference> pick = PriorityProvider.pick(reg.listElements(), h -> (PriorityProvider) ((Holder.Reference) h).value(), RandomSource.create(seed), context);
                     out.println("S " + r.name + " " + seed + " " + pick.map(h -> h.key().identifier().toString()).orElse("-"));
                  }
               }
            }
         }
      }
      out.flush();
      System.exit(0);
   }

   static String sha1(String s) {
      try {
         StringBuilder sb = new StringBuilder();
         for (byte b : MessageDigest.getInstance("SHA-1").digest(s.getBytes(StandardCharsets.UTF_8))) sb.append(String.format("%02x", b));
         return sb.toString();
      } catch (java.security.NoSuchAlgorithmException e) {
         throw new IllegalStateException(e);
      }
   }

   static String canon(Tag tag) {
      StringBuilder sb = new StringBuilder();
      canon(tag, sb);
      return sb.toString();
   }

   static void canon(Tag tag, StringBuilder sb) {
      switch (tag) {
         case CompoundTag c -> {
            sb.append('{');
            boolean first = true;
            for (String k : new TreeSet<>(c.keySet())) {
               if (!first) sb.append(',');
               first = false;
               sb.append(k).append(':');
               canon(c.get(k), sb);
            }
            sb.append('}');
         }
         case ListTag l -> {
            sb.append('[');
            for (int i = 0; i < l.size(); i++) {
               if (i > 0) sb.append(',');
               canon(l.get(i), sb);
            }
            sb.append(']');
         }
         case ByteTag t -> sb.append(t.byteValue()).append('b');
         case ShortTag t -> sb.append(t.shortValue()).append('s');
         case IntTag t -> sb.append(t.intValue()).append('i');
         case LongTag t -> sb.append(t.longValue()).append('l');
         case FloatTag t -> sb.append(Integer.toHexString(Float.floatToIntBits(t.floatValue()))).append('f');
         case DoubleTag t -> sb.append(Long.toHexString(Double.doubleToLongBits(t.doubleValue()))).append('d');
         case StringTag t -> sb.append('"').append(t.value()).append('"');
         default -> throw new IllegalStateException(tag.toString());
      }
   }
}
