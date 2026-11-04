import java.nio.file.*;
import net.minecraft.nbt.*;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.chunk.storage.*;
import net.minecraft.world.level.Level;

/** usage: DumpChunk <mca> <chunkX> <chunkZ> : prints the chunk's NBT as SNBT (pretty). */
public class DumpChunk {
  public static void main(String[] a) throws Exception {
    net.minecraft.SharedConstants.tryDetectVersion(); net.minecraft.server.Bootstrap.bootStrap();
    Path p = Paths.get(a[0]);
    try (RegionFile rf = new RegionFile(new RegionStorageInfo("x", Level.OVERWORLD, "chunk"), p, p.getParent(), false)) {
      var in = rf.getChunkDataInputStream(new ChunkPos(Integer.parseInt(a[1]), Integer.parseInt(a[2])));
      CompoundTag t = NbtIo.read(in);
      System.out.println(t.toString());
    }
  }
}
