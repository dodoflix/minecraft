import com.mojang.serialization.*;
import java.util.*;
import net.minecraft.core.IdMapper;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.*;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.*;

/**
 * Decode-side golden for the block state container codec: valid, partial and broken inputs.
 * Usage: scripts/vanilla-java.sh -o crates/mc-world/tests/fixtures/palette/decode_golden.txt crates/mc-world/tests/fixtures/palette/PaletteDecodeGolden.java crates/mc-world/tests/fixtures/palette/decode_golden.txt
 * Output: crates/mc-world/tests/fixtures/palette/decode_golden.txt
 */
public class PaletteDecodeGolden {
  static StringBuilder out = new StringBuilder();
  static Strategy<BlockState> bs;
  static BlockState air;
  static Codec<PalettedContainer<BlockState>> codec;
  static Strategy<String> bio;
  static String[] names;

  static long[] packed(int bits, int[] ids) {
    int per = 64 / bits; long[] l = new long[(4096 + per - 1) / per];
    for (int i = 0; i < 4096; i++) l[i / per] |= ((long) ids[i]) << ((i % per) * bits);
    return l;
  }

  static ListTag palette(Object... entries) {
    ListTag l = new ListTag();
    for (Object e : entries) l.add(e instanceof Tag t ? t : StringTag.valueOf((String) e));
    return l;
  }

  static CompoundTag state(String id, String... props) {
    CompoundTag c = new CompoundTag(); c.putString("id", id);
    CompoundTag p = new CompoundTag();
    for (int i = 0; i < props.length; i += 2) p.putString(props[i], props[i + 1]);
    c.put("properties", p);
    return c;
  }

  static void run(String label, CompoundTag tag) {
    out.append("IN ").append(label).append(" ").append(tag.toString()).append("\n");
    List<String> problems = new ArrayList<>();
    try {
      PalettedContainer<BlockState> c = codec.parse(NbtOps.INSTANCE, tag).promotePartial(problems::add).getOrThrow(IllegalStateException::new);
      new LevelChunkSection(c, new PalettedContainer<>(Biomes_dummy(), bioStrategy()));
      long h = 1125899906842597L;
      for (int i = 0; i < 4096; i++) h = 31 * h + Block.BLOCK_STATE_REGISTRY.getId(c.get(i & 15, (i >> 8) & 15, (i >> 4) & 15));
      Tag re = codec.encodeStart(NbtOps.INSTANCE, c).getOrThrow();
      out.append("OK ").append(h).append(' ').append(problems.isEmpty() ? 0 : 1).append(' ').append(re).append("\n");
    } catch (Throwable t) {
      out.append("ERR\n");
    }
  }

  static Object Biomes_dummy() { return "b0"; }
  @SuppressWarnings("unchecked") static Strategy bioStrategy() { return bio; }

  @SuppressWarnings({"unchecked", "rawtypes"})
  public static void main(String[] a) throws Exception {
    net.minecraft.SharedConstants.tryDetectVersion(); net.minecraft.server.Bootstrap.bootStrap();
    bs = Strategy.createForBlockStates(Block.BLOCK_STATE_REGISTRY);
    air = Blocks.AIR.defaultBlockState();
    codec = PalettedContainer.codecRW(BlockState.CODEC, bs, air);
    IdMapper<String> map = new IdMapper<>();
    names = new String[4]; for (int i = 0; i < 4; i++) { names[i] = new String("b" + i); map.add(names[i]); }
    bio = Strategy.createForBiomes(map);
    Random r = new Random(5);
    String S = "minecraft:stone", D = "minecraft:dirt";
    CompoundTag t;

    t = new CompoundTag(); run("missing-palette", t);
    t = new CompoundTag(); t.putString("palette", "x"); run("palette-not-list", t);
    t = new CompoundTag(); t.put("palette", palette()); run("empty-palette", t);
    t = new CompoundTag(); t.put("palette", palette(S)); run("single", t);
    t = new CompoundTag(); t.put("palette", palette(S)); t.put("data", new LongArrayTag(new long[]{1, 2, 3})); run("single-with-data", t);
    t = new CompoundTag(); t.put("palette", palette("minecraft:foo")); run("unknown-single", t);
    t = new CompoundTag(); t.put("palette", palette(IntTag.valueOf(3))); run("int-entry", t);
    t = new CompoundTag(); t.put("palette", palette(state("minecraft:oak_stairs", "facing", "bogus", "half", "top", "extra", "1"))); run("props-single", t);
    t = new CompoundTag(); t.put("palette", palette("minecraft:oak_stairs")); run("bare-stairs", t);
    t = new CompoundTag(); CompoundTag bad = new CompoundTag(); bad.putString("id", "minecraft:oak_stairs"); bad.putInt("properties", 5); t.put("palette", palette(bad)); run("props-not-compound", t);
    t = new CompoundTag(); CompoundTag noid = new CompoundTag(); noid.putString("Name", "minecraft:stone"); t.put("palette", palette(noid)); run("legacy-name-key", t);

    int[] ids = new int[4096];
    for (int i = 0; i < 4096; i++) ids[i] = r.nextInt(2);
    t = new CompoundTag(); t.put("palette", palette(S, D)); t.put("data", new LongArrayTag(packed(4, ids))); run("two", t);
    t = new CompoundTag(); t.put("palette", palette(S, D)); run("two-no-data", t);
    t = new CompoundTag(); t.put("palette", palette(S, D)); t.put("data", new LongArrayTag(new long[]{1, 2, 3})); run("two-short-data", t);
    t = new CompoundTag(); t.put("palette", palette(S, D)); t.putString("data", "nope"); run("two-bad-data-type", t);
    ListTag lst = new ListTag(); for (long l : packed(4, ids)) lst.add(LongTag.valueOf(l));
    t = new CompoundTag(); t.put("palette", palette(S, D)); t.put("data", lst); run("two-list-data", t);
    ListTag ints = new ListTag(); for (long l : packed(4, ids)) ints.add(IntTag.valueOf((int) l));
    t = new CompoundTag(); t.put("palette", palette(S, D)); t.put("data", ints); run("two-int-list-data", t);
    t = new CompoundTag(); t.put("palette", palette(S, "minecraft:foo", D)); int[] ids3 = new int[4096]; for (int i = 0; i < 4096; i++) ids3[i] = r.nextInt(3);
    t.put("data", new LongArrayTag(packed(4, ids3))); run("three-one-unknown", t);
    int[] oob = new int[4096]; for (int i = 0; i < 4096; i++) oob[i] = r.nextInt(3); oob[100] = 7;
    t = new CompoundTag(); t.put("palette", palette(S, D, "minecraft:gravel")); t.put("data", new LongArrayTag(packed(4, oob))); run("id-outside-palette", t);
    int[] dup = new int[4096]; for (int i = 0; i < 4096; i++) dup[i] = r.nextInt(3);
    t = new CompoundTag(); t.put("palette", palette(S, S, D)); t.put("data", new LongArrayTag(packed(4, dup))); run("duplicate-entries", t);
    for (int n : new int[]{16, 17, 32, 33, 64, 65, 128, 129, 256, 257, 300}) {
      int bits = Math.max(4, 32 - Integer.numberOfLeadingZeros(n - 1));
      List<Object> entries = new ArrayList<>();
      Set<Integer> used = new HashSet<>();
      while (entries.size() < n) {
        int id = r.nextInt(Block.BLOCK_STATE_REGISTRY.size());
        if (!used.add(id)) continue;
        BlockState s = Block.BLOCK_STATE_REGISTRY.byId(id);
        entries.add(BlockState.CODEC.encodeStart(NbtOps.INSTANCE, s).getOrThrow());
      }
      int[] v = new int[4096]; for (int i = 0; i < 4096; i++) v[i] = r.nextInt(n);
      t = new CompoundTag(); t.put("palette", palette(entries.toArray())); t.put("data", new LongArrayTag(packed(bits, v))); run("n" + n, t);
      if (n == 300) { // storage declared with more bits than needed is read as-is by vanilla (extra high ids -> errors) - use 9 bits
      }
    }
    java.nio.file.Files.writeString(java.nio.file.Path.of(a[0]), out.toString());
  }
}
