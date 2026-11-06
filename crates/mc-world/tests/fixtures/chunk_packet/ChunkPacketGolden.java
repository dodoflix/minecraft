import io.netty.buffer.*;
import java.lang.reflect.*;
import java.util.*;
import net.minecraft.core.IdMapper;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.*;
import net.minecraft.network.FriendlyByteBuf;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.network.protocol.game.*;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.entity.BlockEntityType;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.*;
import net.minecraft.world.level.levelgen.Heightmap;

/**
 * Golden generator for the chunk / light packet data encoding.
 * Usage: scripts/vanilla-java.sh -o crates/mc-world/tests/fixtures/chunk_packet/golden.txt.gz crates/mc-world/tests/fixtures/chunk_packet/ChunkPacketGolden.java crates/mc-world/tests/fixtures/chunk_packet/golden.txt.gz
 * Output: crates/mc-world/tests/fixtures/chunk_packet/golden.txt.gz
 */
public class ChunkPacketGolden {
  static String hex(byte[] b) { StringBuilder s = new StringBuilder(); for (byte x : b) s.append(String.format(java.util.Locale.ROOT, "%02x", x)); return s.toString(); }

  @SuppressWarnings({"unchecked", "rawtypes"})
  public static void main(String[] a) throws Exception {
    net.minecraft.SharedConstants.tryDetectVersion(); net.minecraft.server.Bootstrap.bootStrap();
    RegistryAccess access = RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
    Strategy<BlockState> bs = Strategy.createForBlockStates(Block.BLOCK_STATE_REGISTRY);
    int biomeCount = 64;
    IdMapper<String> map = new IdMapper<>();
    String[] names = new String[biomeCount];
    for (int i = 0; i < biomeCount; i++) { names[i] = new String("b" + i); map.add(names[i]); }
    Strategy<String> bio = Strategy.createForBiomes(map);
    Random rnd = new Random(77);
    StringBuilder out = new StringBuilder();
    Constructor<?> infoCtor = Class.forName("net.minecraft.network.protocol.game.ClientboundLevelChunkPacketData$BlockEntityInfo").getDeclaredConstructors()[0];
    infoCtor.setAccessible(true);
    Constructor<ClientboundLevelChunkPacketData> dataCtor = ClientboundLevelChunkPacketData.class.getDeclaredConstructor(Map.class, byte[].class, List.class);
    dataCtor.setAccessible(true);
    for (int c = 0; c < 4; c++) {
      out.append("CASE ").append(c).append("\n");
      int sectionCount = c == 0 ? 4 : 24;
      FriendlyByteBuf chunkBuf = new FriendlyByteBuf(Unpooled.buffer());
      for (int s = 0; s < sectionCount; s++) {
        PalettedContainer<BlockState> states = new PalettedContainer<>(Block.BLOCK_STATE_REGISTRY.byId(0), bs);
        int defaultBiome = rnd.nextInt(3);
        PalettedContainer<String> biomes = new PalettedContainer<>(names[defaultBiome], bio);
        out.append("SEC ").append(defaultBiome).append("\n");
        int kind = (s + c) % 5;
        int n = kind == 0 ? 0 : kind == 1 ? 1 : kind == 2 ? 6 : kind == 3 ? 40 : (c * 700 + 300);
        for (int i = 0; i < n; i++) {
          int x = rnd.nextInt(16), y = rnd.nextInt(16), z = rnd.nextInt(16);
          int id = kind == 1 ? 1 : rnd.nextInt(kind == 3 ? 40 : Block.BLOCK_STATE_REGISTRY.size());
          states.set(x, y, z, Block.BLOCK_STATE_REGISTRY.byId(id));
          out.append("B ").append(x).append(' ').append(y).append(' ').append(z).append(' ').append(id).append("\n");
        }
        int bn = s % 3 == 0 ? 0 : 5 + s;
        for (int i = 0; i < bn; i++) {
          int x = rnd.nextInt(4), y = rnd.nextInt(4), z = rnd.nextInt(4);
          int id = rnd.nextInt(biomeCount);
          biomes.set(x, y, z, names[id]);
          out.append("M ").append(x).append(' ').append(y).append(' ').append(z).append(' ').append(id).append("\n");
        }
        out.append("ENDSEC\n");
        LevelChunkSection section = new LevelChunkSection(states, (PalettedContainerRO) biomes);
        section.write(chunkBuf);
      }
      byte[] chunkBytes = new byte[chunkBuf.readableBytes()]; chunkBuf.readBytes(chunkBytes);
      // heightmaps: client types with random longs
      Map<Heightmap.Types, long[]> hm = new EnumMap<>(Heightmap.Types.class);
      for (Heightmap.Types t : Heightmap.Types.values()) {
        if (!t.sendToClient()) continue;
        if (c == 3 && t == Heightmap.Types.WORLD_SURFACE) continue;
        long[] l = new long[37 + c]; for (int i = 0; i < l.length; i++) l[i] = rnd.nextLong();
        hm.put(t, l);
        out.append("HM ").append(t.ordinal()).append(' ');
        for (long v : l) out.append(v).append(',');
        out.append("\n");
      }
      // also a non-client heightmap which the constructor must drop
      out.append("HM ").append(Heightmap.Types.OCEAN_FLOOR.ordinal()).append(" 1,2,3,\n");
      List<Object> infos = new ArrayList<>();
      int bes = c;
      for (int i = 0; i < bes * 2; i++) {
        int x = rnd.nextInt(16), z = rnd.nextInt(16), y = rnd.nextInt(400) - 64;
        int typeId = rnd.nextInt(BuiltInRegistries.BLOCK_ENTITY_TYPE.size());
        CompoundTag tag = i % 3 == 0 ? new CompoundTag() : new CompoundTag();
        if (i % 3 != 0) { tag.putString("CustomName", "n" + i); tag.putInt("v", i * 1000); }
        int xz = x << 4 | z;
        BlockEntityType<?> type = BuiltInRegistries.BLOCK_ENTITY_TYPE.byId(typeId);
        infos.add(infoCtor.newInstance((byte) xz, (short) y, type, tag.isEmpty() ? Optional.empty() : Optional.of(tag)));
        out.append("BE ").append(x).append(' ').append(y).append(' ').append(z).append(' ').append(typeId).append(' ').append(tag.isEmpty() ? "-" : tag.toString()).append("\n");
      }
      ClientboundLevelChunkPacketData data = dataCtor.newInstance(hm, chunkBytes, infos);

      // light
      int lightSections = sectionCount + 2;
      BitSet skyMask = new BitSet(), blockMask = new BitSet(), emptySky = new BitSet(), emptyBlock = new BitSet();
      List<byte[]> skyUpd = new ArrayList<>(), blockUpd = new ArrayList<>();
      for (int i = 0; i < lightSections; i++) {
        for (int layer = 0; layer < 2; layer++) {
          int k = rnd.nextInt(3);
          String tag = layer == 0 ? "SKY" : "BLK";
          BitSet mask = layer == 0 ? skyMask : blockMask; BitSet em = layer == 0 ? emptySky : emptyBlock; List<byte[]> upd = layer == 0 ? skyUpd : blockUpd;
          if (k == 0) { out.append(tag).append(' ').append(i).append(" -\n"); }
          else if (k == 1) { out.append(tag).append(' ').append(i).append(" E\n"); em.set(i); }
          else {
            byte[] bytes = new byte[2048]; rnd.nextBytes(bytes); bytes[0] |= 1;
            DataLayer dl = new DataLayer(bytes);
            out.append(tag).append(' ').append(i).append(" H").append(hex(bytes)).append("\n");
            mask.set(i); upd.add(dl.copy().getData());
          }
        }
      }
      ClientboundLightUpdatePacketData light = new ClientboundLightUpdatePacketData(skyMask, blockMask, emptySky, emptyBlock, skyUpd, blockUpd);
      int cx = rnd.nextInt(2000) - 1000, cz = rnd.nextInt(2000) - 1000;
      ClientboundLevelChunkWithLightPacket packet = new ClientboundLevelChunkWithLightPacket(cx, cz, data, light);
      RegistryFriendlyByteBuf buf = new RegistryFriendlyByteBuf(Unpooled.buffer(), access);
      ClientboundLevelChunkWithLightPacket.STREAM_CODEC.encode(buf, packet);
      byte[] bytes = new byte[buf.readableBytes()]; buf.readBytes(bytes);
      out.append("POS ").append(cx).append(' ').append(cz).append("\n");
      out.append("HEX ").append(hex(bytes)).append("\n");
      out.append("ENDCASE\n");
    }
    java.nio.file.Files.writeString(java.nio.file.Path.of(a[0]), out.toString());
  }
}
