import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Collection;
import java.util.List;
import java.util.Locale;
import java.util.Optional;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Vec3i;
import net.minecraft.core.registries.Registries;
import net.minecraft.gametest.framework.GameTestInfo;
import net.minecraft.gametest.framework.GameTestInstance;
import net.minecraft.gametest.framework.GameTestTicker;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.NbtAccounter;
import net.minecraft.nbt.NbtIo;
import net.minecraft.network.protocol.game.ServerboundSetTestBlockPacket;
import net.minecraft.network.protocol.game.ServerboundTestInstanceBlockActionPacket;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.level.block.Rotation;
import net.minecraft.world.level.block.state.properties.TestBlockMode;

/**
 * Test block and test instance block verbs of {@link EntityOracleDriver} (called from its `default` arm), each one a serverbound packet handled by the
 * player's real {@code ServerGamePacketListenerImpl}:
 *
 * <pre>
 * set_test_block PLAYER X Y Z MODE ["MESSAGE"]   ServerboundSetTestBlockPacket (MODE: start log fail accept)
 * test_instance_action PLAYER X Y Z ACTION [test=ID] [size=X,Y,Z] [rotation=NAME] [ignore_entities=BOOL]
 *                             ServerboundTestInstanceBlockActionPacket (ACTION: init query set reset save export run; the data of the packet starts as the
 *                             client's empty one: no test, size 0 0 0, no rotation, entities kept; NAME: none clockwise_90 180 counterclockwise_90).
 *                             The status answer of `init` and `query` and the feedback of the other actions reach the player as packets: `dump packets:PLAYER`
 * dump structure_file:ID      `STRUCTFILE ID SNBT`: the generated structure `world/generated/NAMESPACE/structure/PATH.nbt` a `save` wrote (`none` without a file)
 * dump gametest               `GAMETEST ID started= done= passed= failed= tick=` per running game test of the ticker (finished tests leave it)
 * dump logs:info              `LOGINFO message` per INFO event of the test block entity and the game test runner since the last `dump logs:info`, in order
 * </pre>
 */
final class GameTestVerbs {
   private GameTestVerbs() {
   }

   static boolean run(EntityOracleDriver d, List<String> a) throws Exception {
      switch (a.get(0)) {
         case "set_test_block" -> {
            d.world();
            ServerPlayer p = d.player(a.get(1));
            String message = a.size() > 6 ? unquote(a.get(6)) : "";
            p.connection.handleSetTestBlock(new ServerboundSetTestBlockPacket(pos(a), TestBlockMode.valueOf(a.get(5).toUpperCase(Locale.ROOT)), message));
         }
         case "test_instance_action" -> {
            d.world();
            ServerPlayer p = d.player(a.get(1));
            var action = ServerboundTestInstanceBlockActionPacket.Action.valueOf(a.get(5).toUpperCase(Locale.ROOT));
            var options = d.kv(a, 6);
            Optional<ResourceKey<GameTestInstance>> test = Optional.ofNullable(options.get("test")).map(id -> ResourceKey.create(Registries.TEST_INSTANCE, EntityOracleDriver.id(id)));
            Vec3i size = Vec3i.ZERO;
            if (options.containsKey("size")) {
               String[] s = options.get("size").split(",");
               size = new Vec3i(Integer.parseInt(s[0]), Integer.parseInt(s[1]), Integer.parseInt(s[2]));
            }
            Rotation rotation = Rotation.NONE;
            if (options.containsKey("rotation")) {
               rotation = java.util.Arrays.stream(Rotation.values()).filter(r -> r.getSerializedName().equals(options.get("rotation"))).findFirst().orElseThrow();
            }
            boolean ignoreEntities = Boolean.parseBoolean(options.getOrDefault("ignore_entities", "false"));
            p.connection.handleTestInstanceBlockAction(new ServerboundTestInstanceBlockActionPacket(pos(a), action, test, size, rotation, ignoreEntities));
         }
         default -> {
            return false;
         }
      }
      return true;
   }

   static boolean dump(EntityOracleDriver d, String kind, String arg) {
      switch (kind) {
         case "structure_file" -> {
            Identifier id = EntityOracleDriver.id(arg);
            Path file = d.v.dir.resolve("world/generated").resolve(id.getNamespace()).resolve("structure").resolve(id.getPath() + ".nbt");
            if (!Files.exists(file)) {
               d.out.println("STRUCTFILE " + arg + " none");
            } else {
               CompoundTag root;
               try {
                  root = NbtIo.readCompressed(file, NbtAccounter.unlimitedHeap());
               } catch (java.io.IOException ex) {
                  throw new java.io.UncheckedIOException(ex);
               }
               d.out.println("STRUCTFILE " + arg + " " + EntityOracleDriver.canon(root));
            }
         }
         case "gametest" -> {
            @SuppressWarnings("unchecked")
            Collection<GameTestInfo> infos = (Collection<GameTestInfo>)EntityOracleDriver.priv(GameTestTicker.SINGLETON, GameTestTicker.class, "testInfos");
            for (GameTestInfo i : infos) {
               d.out.println("GAMETEST " + i.id() + " started=" + i.hasStarted() + " done=" + i.isDone() + " passed=" + i.hasSucceeded() + " failed=" + i.hasFailed() + " tick=" + i.getTick());
            }
         }
         case "logs" -> {
            if (!"info".equals(arg)) {
               return false;
            }
            List<String> logs;
            synchronized (VanillaLevel.INFO_LOGS) {
               logs = new ArrayList<>(VanillaLevel.INFO_LOGS);
               VanillaLevel.INFO_LOGS.clear();
            }
            logs.forEach(l -> d.out.println("LOGINFO " + l.replace("\\", "\\\\").replace("\n", "\\n")));
         }
         default -> {
            return false;
         }
      }
      return true;
   }

   private static BlockPos pos(List<String> a) {
      return new BlockPos(Integer.parseInt(a.get(2)), Integer.parseInt(a.get(3)), Integer.parseInt(a.get(4)));
   }

   private static String unquote(String text) {
      return text.length() > 1 && text.startsWith("\"") && text.endsWith("\"") ? text.substring(1, text.length() - 1) : text;
   }
}
