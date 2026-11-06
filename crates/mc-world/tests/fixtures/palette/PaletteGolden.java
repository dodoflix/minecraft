import com.mojang.serialization.*;
import io.netty.buffer.*;
import java.util.*;
import net.minecraft.core.IdMapper;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.*;
import net.minecraft.network.FriendlyByteBuf;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.*;

/**
 * Golden generator for PalettedContainer: replayable script of sets with checkpoints.
 * Usage: scripts/vanilla-java.sh -o crates/mc-world/tests/fixtures/palette/golden.txt crates/mc-world/tests/fixtures/palette/PaletteGolden.java crates/mc-world/tests/fixtures/palette/golden.txt
 * Output: crates/mc-world/tests/fixtures/palette/golden.txt
 */
public class PaletteGolden {
  static StringBuilder out = new StringBuilder();

  static String hex(byte[] b) { StringBuilder s = new StringBuilder(); for (byte x : b) s.append(String.format(java.util.Locale.ROOT, "%02x", x)); return s.toString(); }

  static <T> void check(PalettedContainer<T> c, Strategy<T> strategy, Codec<PalettedContainer<T>> codec, java.util.function.ToIntFunction<T> id) {
    out.append("CHECK\n");
    FriendlyByteBuf buf = new FriendlyByteBuf(Unpooled.buffer());
    c.write(buf);
    byte[] b = new byte[buf.readableBytes()]; buf.readBytes(b);
    out.append("NET ").append(hex(b)).append("\n");
    out.append("BITS ").append(c.bitsPerEntry()).append("\n");
    out.append("SIZE ").append(c.getSerializedSize()).append("\n");
    PalettedContainerRO.PackedData<T> p = c.pack(strategy);
    StringBuilder e = new StringBuilder();
    for (T t : p.paletteEntries()) e.append(id.applyAsInt(t)).append(',');
    out.append("PACK ").append(e).append(" | ");
    if (p.storage().isPresent()) { p.storage().get().forEach(l -> out.append(l).append(',')); } else out.append("-");
    out.append(" | ").append(p.bitsPerEntry()).append("\n");
    TreeMap<Integer,Integer> counts = new TreeMap<>();
    c.count((t, n) -> counts.merge(id.applyAsInt(t), n, Integer::sum));
    out.append("COUNT ");
    counts.forEach((k, v) -> out.append(k).append(':').append(v).append(','));
    out.append("\n");
    Tag nbt = codec.encodeStart(NbtOps.INSTANCE, c).getOrThrow();
    out.append("NBT ").append(nbt.toString()).append("\n");
  }

  public static void main(String[] a) throws Exception {
    net.minecraft.SharedConstants.tryDetectVersion(); net.minecraft.server.Bootstrap.bootStrap();
    Random rnd = new Random(1234);
    // ---------- block states
    Strategy<BlockState> bs = Strategy.createForBlockStates(Block.BLOCK_STATE_REGISTRY);
    BlockState air = Blocks.AIR.defaultBlockState();
    Codec<PalettedContainer<BlockState>> bcodec = PalettedContainer.codecRW(BlockState.CODEC, bs, air);
    int[] distinctSteps = {1, 2, 3, 5, 9, 17, 18, 33, 65, 129, 257, 300, 513, 1000};
    for (int mode = 0; mode < 3; mode++) {
      out.append("CASE blocks mode").append(mode).append("\n");
      PalettedContainer<BlockState> c = new PalettedContainer<>(air, bs);
      check(c, bs, bcodec, s -> Block.BLOCK_STATE_REGISTRY.getId(s));
      int distinct = 1;
      int stepIdx = 0;
      List<BlockState> used = new ArrayList<>(); used.add(air);
      int pos = 0;
      while (stepIdx < distinctSteps.length) {
        int target = distinctSteps[stepIdx];
        while (distinct < target) {
          BlockState s;
          do { s = Block.BLOCK_STATE_REGISTRY.byId(rnd.nextInt(Block.BLOCK_STATE_REGISTRY.size())); } while (used.contains(s));
          used.add(s); distinct++;
          int x, y, z;
          if (mode == 0) { int i = pos++ % 4096; x = i & 15; z = (i >> 4) & 15; y = i >> 8; }
          else { x = rnd.nextInt(16); y = rnd.nextInt(16); z = rnd.nextInt(16); }
          int id = Block.BLOCK_STATE_REGISTRY.getId(s);
          out.append("SET ").append(x).append(' ').append(y).append(' ').append(z).append(' ').append(id).append("\n");
          c.set(x, y, z, s);
        }
        // overwrite a few with repeats (mode 2: overwrite so that some values disappear)
        if (mode == 2) {
          for (int k = 0; k < 200; k++) {
            int x = rnd.nextInt(16), y = rnd.nextInt(16), z = rnd.nextInt(16);
            BlockState s = used.get(rnd.nextInt(used.size()));
            out.append("SET ").append(x).append(' ').append(y).append(' ').append(z).append(' ').append(Block.BLOCK_STATE_REGISTRY.getId(s)).append("\n");
            c.set(x, y, z, s);
          }
        }
        check(c, bs, bcodec, s -> Block.BLOCK_STATE_REGISTRY.getId(s));
        stepIdx++;
      }
      out.append("ENDCASE\n");
    }
    // ---------- biomes with a synthetic 70-entry id map
    for (int size : new int[]{3, 70, 1500}) {
      IdMapper<String> map = new IdMapper<>();
      String[] names = new String[size];
      for (int i = 0; i < size; i++) { names[i] = new String("b" + i); map.add(names[i]); }
      Strategy<String> st = Strategy.createForBiomes(map);
      Codec<PalettedContainer<String>> code = PalettedContainer.codecRW(Codec.STRING.xmap(s -> names[Integer.parseInt(s.substring(1))], s -> s), st, names[0]);
      out.append("CASE biomes ").append(size).append("\n");
      PalettedContainer<String> c = new PalettedContainer<>(names[0], st);
      java.util.function.ToIntFunction<String> idf = s -> map.getId(s);
      check(c, st, code, idf);
      int distinct = 1;
      List<Integer> used = new ArrayList<>(List.of(0));
      for (int target : new int[]{2, 3, 4, 5, 9, 17, 33}) {
        while (distinct < target && distinct < size) {
          int v; do { v = rnd.nextInt(size); } while (used.contains(v));
          used.add(v); distinct++;
          int x = rnd.nextInt(4), y = rnd.nextInt(4), z = rnd.nextInt(4);
          out.append("SET ").append(x).append(' ').append(y).append(' ').append(z).append(' ').append(v).append("\n");
          c.set(x, y, z, names[v]);
        }
        check(c, st, code, idf);
      }
      out.append("ENDCASE\n");
    }
    java.nio.file.Files.writeString(java.nio.file.Path.of(a[0]), out.toString());
  }
}
