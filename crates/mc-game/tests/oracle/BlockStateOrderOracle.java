import com.mojang.serialization.Codec;
import java.util.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.NbtOps;
import net.minecraft.nbt.Tag;
import net.minecraft.util.ProblemReporter;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.storage.TagValueOutput;

/**
 * The order the real server writes the keys of an encoded block state's {@code properties} compound in (the iteration order of
 * the compound's hash map, which depends on the order the keys were inserted in): the block state codecs directly, and the saved
 * data of every entity field that holds a block state.
 * Usage: scripts/entity-oracle.sh crates/mc-game/tests/oracle/BlockStateOrderOracle.java -o crates/mc-game/tests/fixtures/block_state_order.txt
 *
 * Output: crates/mc-game/tests/fixtures/block_state_order.txt, one case per line:
 *
 * <pre>
 * CODEC block   BlockState.CODEC of the last state (the first when that is the default) of every block with two or more properties
 * FULL block    BlockState.FULL_CODEC of the same state
 * ENTITY type field state   the saved NBT of an entity that was loaded with that state in that field
 * </pre>
 * Every line ends with the property keys in written order, separated by commas.
 */
public final class BlockStateOrderOracle {
   static final String STAIRS = "{id:\"minecraft:oak_stairs\",properties:{facing:\"east\",half:\"top\",shape:\"outer_left\",waterlogged:\"true\"}}";
   static final String DOOR = "{id:\"minecraft:oak_door\",properties:{facing:\"west\",half:\"upper\",hinge:\"right\",open:\"true\",powered:\"true\"}}";
   static final String WIRE = "{id:\"minecraft:redstone_wire\",properties:{east:\"side\",north:\"up\",power:\"7\",south:\"side\",west:\"none\"}}";
   static final String[][] ENTITIES = {
      {"minecraft:falling_block", "BlockState", STAIRS},
      {"minecraft:falling_block", "BlockState", DOOR},
      {"minecraft:falling_block", "BlockState", WIRE},
      {"minecraft:block_display", "block_state", STAIRS},
      {"minecraft:block_display", "block_state", DOOR},
      {"minecraft:enderman", "carriedBlockState", STAIRS},
      {"minecraft:enderman", "carriedBlockState", WIRE},
      {"minecraft:arrow", "inBlockState", STAIRS},
      {"minecraft:arrow", "inBlockState", DOOR},
      {"minecraft:tnt", "block_state", STAIRS},
      {"minecraft:tnt", "block_state", WIRE},
      {"minecraft:minecart", "DisplayState", STAIRS},
      {"minecraft:minecart", "DisplayState", DOOR},
   };

   static String keys(Tag tag) {
      CompoundTag props = ((CompoundTag)tag).getCompound("properties").orElseThrow();
      return String.join(",", props.keySet());
   }

   static String encode(Codec<BlockState> codec, BlockState state) {
      return keys(codec.encodeStart(NbtOps.INSTANCE, state).getOrThrow());
   }

   public static void main(String[] args) throws Exception {
      try {
         run();
      } catch (Throwable t) {
         t.printStackTrace(VanillaLevel.ERR);
         Runtime.getRuntime().halt(1);
      }
   }

   static void run() throws Exception {
      EntityOracleDriver d = new EntityOracleDriver();
      d.run(EntityOracleDriver.tokens("level flat"));
      List<String> lines = new ArrayList<>();
      net.minecraft.SharedConstants.tryDetectVersion();
      net.minecraft.server.Bootstrap.bootStrap();
      for (Block block : BuiltInRegistries.BLOCK) {
         if (block.getStateDefinition().getProperties().size() < 2) {
            continue;
         }
         List<BlockState> all = block.getStateDefinition().getPossibleStates();
         BlockState state = all.getLast() == block.defaultBlockState() ? all.getFirst() : all.getLast();
         String name = BuiltInRegistries.BLOCK.getKey(block).toString();
         lines.add("CODEC " + name + " " + encode(BlockState.CODEC, state));
         lines.add("FULL " + name + " " + encode(BlockState.FULL_CODEC, state));
      }
      int n = 0;
      for (String[] c : ENTITIES) {
         String name = "e" + n++;
         d.line = "spawn " + name + " " + c[0] + " 0.5 -50 0.5 0 0 {" + c[1] + ":" + c[2] + "}";
         d.run(EntityOracleDriver.tokens(d.line));
         Entity e = d.refs.get(name);
         try (ProblemReporter.ScopedCollector rep = new ProblemReporter.ScopedCollector(e.problemPath(), org.slf4j.LoggerFactory.getLogger("oracle"))) {
            TagValueOutput o = TagValueOutput.createWithContext(rep, d.v.server.registryAccess());
            e.saveWithoutId(o);
            lines.add("ENTITY " + c[0] + " " + c[1] + " " + c[2] + " " + keys(o.buildResult().get(c[1])));
         }
      }
      for (String l : lines) {
         VanillaLevel.OUT.println(l);
      }
      VanillaLevel.OUT.flush();
      System.exit(0);
   }
}
