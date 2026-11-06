import com.mojang.authlib.services.MinecraftServicesDiscoveryService;
import com.mojang.brigadier.StringReader;
import com.mojang.serialization.Lifecycle;
import java.io.PrintStream;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.net.Proxy;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.commands.Commands;
import net.minecraft.commands.arguments.blocks.BlockStateParser;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.Registry;
import net.minecraft.core.registries.Registries;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.Eula;
import net.minecraft.server.Services;
import net.minecraft.server.WorldLoader;
import net.minecraft.server.WorldStem;
import net.minecraft.server.dedicated.DedicatedPlayerList;
import net.minecraft.server.dedicated.DedicatedServerProperties;
import net.minecraft.server.dedicated.DedicatedServerSettings;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.notifications.NotificationManager;
import net.minecraft.server.packs.repository.PackRepository;
import net.minecraft.server.packs.repository.ServerPacksSource;
import net.minecraft.util.BlockUtil;
import net.minecraft.util.Util;
import net.minecraft.util.datafix.DataFixers;
import net.minecraft.world.entity.EntityDimensions;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.LevelSettings;
import net.minecraft.world.level.WorldDataConfiguration;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.dimension.LevelStem;
import net.minecraft.world.level.levelgen.RandomSupport;
import net.minecraft.world.level.levelgen.WorldDimensions;
import net.minecraft.world.level.levelgen.WorldGenSettings;
import net.minecraft.world.level.portal.PortalForcer;
import net.minecraft.world.level.portal.PortalShape;
import net.minecraft.world.level.storage.LevelDataAndDimensions;
import net.minecraft.world.level.storage.LevelStorageSource;
import net.minecraft.world.level.storage.PrimaryLevelData;
import net.minecraft.world.phys.Vec3;

/**
 * Portal golden generator (PortalShape / PortalForcer / BlockUtil / BaseFireBlock.onPlace) over a real vanilla server whose
 * overworld AND nether are void flat worlds (custom world preset in a temp data pack). Output (stdout) is a script with its
 * results: {@code > command} lines are the inputs (the Rust replay in {@code tests/golden/portal_golden.rs} executes them in order),
 * {@code < result} lines the vanilla results it must reproduce.
 * Usage: scripts/entity-oracle.sh crates/mc-game/tests/oracle/PortalOracle.java -o crates/mc-game/tests/fixtures/portal/MODE.txt MODE [count]   (MODE = shapes|forcer|misc)
 * Output: crates/mc-game/tests/fixtures/portal/MODE.txt
 * Needs MC_ACCEPT_EULA=1 (the oracle starts a vanilla server).
 *
 * <pre>
 * > dim overworld|nether
 * > border CX CZ SIZE | border reset
 * > fill X1 Y1 Z1 X2 Y2 Z2 STATE      quiet setBlock (clients|known_shape|skip_on_place)
 * > set X Y Z STATE                    setBlockAndUpdate (flags 3)
 * > set2 X Y Z STATE                   quiet setBlock
 * > any X Y Z AXIS                     < A axis=.. num=.. w=.. h=.. bl=x,y,z valid=.. complete=..
 * > empty X Y Z AXIS                   < E none | E axis=.. num=.. w=.. h=.. bl=..   (findEmptyPortalShape)
 * > emptyplace X Y Z AXIS              findEmptyPortalShape + createPortalBlocks
 * > digest X1 Y1 Z1 X2 Y2 Z2           < D n=.. hash=..  then < O x y z state for obsidian/portal/fire blocks
 * > find X Y Z TONETHER                < F x y z | F none     (PortalForcer.findClosestPortalPosition)
 * > create X Y Z AXIS                  < C x y z w h | C none (PortalForcer.createPortal)
 * > rect X Y Z AXIS                    < R x y z w h          (BlockUtil.getLargestRectangleAround over equal states, limits 21/21)
 * > maxrect C0 C1 ...                  < M min max height     (BlockUtil.getMaxRectangleLocation)
 * > relpos MINX MINY MINZ W H AXIS PX PY PZ EW EH   < V x y z (PortalShape.getRelativePosition)
 * </pre>
 */
public final class PortalOracle {
   static final PrintStream OUT = VanillaLevel.OUT;
   static VanillaLevel.Server server;
   static ServerLevel overworld;
   static ServerLevel nether;
   static ServerLevel cur;
   static final int QUIET = 2 | 16 | 512;

   public static void main(String[] args) throws Exception {
      boot();
      String mode = args.length > 0 ? args[0] : "shapes";
      switch (mode) {
         case "shapes" -> shapes(Integer.parseInt(args.length > 1 ? args[1] : "400"));
         case "forcer" -> forcer(Integer.parseInt(args.length > 1 ? args[1] : "60"));
         case "misc" -> misc();
         default -> throw new IllegalArgumentException(mode);
      }
      OUT.flush();
      System.exit(0);
   }

   // ------------------------------------------------------------------------------------------------ boot

   static void boot() throws Exception {
      SharedConstants.tryDetectVersion();
      Bootstrap.bootStrap();
      Bootstrap.validate();
      Util.startTimerHackThread();
      Path dir = Files.createTempDirectory("portal-oracle-");
      Runtime.getRuntime().addShutdownHook(new Thread(() -> {
         try (var s = Files.walk(dir)) {
            s.sorted(Comparator.reverseOrder()).forEach(f -> f.toFile().delete());
         } catch (Exception ignored) {
         }
      }));
      String flat = "{\"type\":\"minecraft:flat\",\"settings\":{\"biome\":\"%s\",\"features\":false,\"lakes\":false,\"layers\":[],\"structure_overrides\":[]}}";
      Path pack = dir.resolve("world/datapacks/voidworld");
      Files.createDirectories(pack.resolve("data/portal/worldgen/world_preset"));
      Files.writeString(pack.resolve("pack.mcmeta"), "{\"pack\":{\"description\":\"void\",\"min_format\":121,\"max_format\":121}}");
      Files.writeString(pack.resolve("data/portal/worldgen/world_preset/void.json"),
         "{\"dimensions\":{\"minecraft:overworld\":{\"type\":\"minecraft:overworld\",\"generator\":" + String.format(java.util.Locale.ROOT, flat, "minecraft:plains")
            + "},\"minecraft:the_nether\":{\"type\":\"minecraft:the_nether\",\"generator\":" + String.format(java.util.Locale.ROOT, flat, "minecraft:nether_wastes")
            + "},\"minecraft:the_end\":{\"type\":\"minecraft:the_end\",\"generator\":" + String.format(java.util.Locale.ROOT, flat, "minecraft:the_end") + "}}}");
      Files.writeString(dir.resolve("server.properties"),
         "level-type=portal:void\nlevel-seed=4242\nonline-mode=false\nenforce-secure-profile=false\nspawn-protection=0\ndifficulty=normal\n"
            + "generate-structures=false\nview-distance=4\nsimulation-distance=4\nspawn-monsters=false\nspawn-animals=false\nspawn-npcs=false\n"
            + "initial-enabled-packs=vanilla,file/voidworld\npause-when-empty-seconds=0\nmax-tick-time=-1\nenable-jmx-monitoring=false\nlevel-name=world\n");
      // a vanilla server refuses to start without an accepted EULA: accepting it is the user's decision, never ours
      if (!"1".equals(System.getenv("MC_ACCEPT_EULA"))) throw new IllegalStateException("starting a vanilla server accepts the Minecraft EULA (https://aka.ms/MinecraftEULA) in your name: set MC_ACCEPT_EULA=1 if you accept it");
      Files.writeString(dir.resolve("eula.txt"), "eula=true\n");
      DedicatedServerSettings settings = new DedicatedServerSettings(dir.resolve("server.properties"));
      new Eula(dir.resolve("eula.txt"));
      Services services = Services.create(MinecraftServicesDiscoveryService.create(Proxy.NO_PROXY), dir.toFile());
      LevelStorageSource source = LevelStorageSource.createDefault(dir);
      LevelStorageSource.LevelStorageAccess access = source.validateAndCreateAccess("world");
      PackRepository packs = ServerPacksSource.createPackRepository(access);
      DedicatedServerProperties props = settings.getProperties();
      WorldDataConfiguration dataConfig = new WorldDataConfiguration(props.initialDataPackConfiguration, FeatureFlags.DEFAULT_FLAGS);
      WorldLoader.InitConfig init = new WorldLoader.InitConfig(new WorldLoader.PackConfig(packs, dataConfig, false, true), Commands.CommandSelection.DEDICATED, props.functionPermissions);
      WorldStem stem = Util.<WorldStem>blockUntilDone(executor -> WorldLoader.load(init, context -> {
         Registry<LevelStem> dims = context.datapackDimensions().lookupOrThrow(Registries.LEVEL_STEM);
         LevelSettings ls = new LevelSettings(props.levelName, props.gameMode.get(), new LevelSettings.DifficultySettings(props.difficulty.get(), props.hardcore, false), false, context.dataConfiguration());
         WorldDimensions dimensions = props.createDimensions(context.datapackWorldRegistries());
         WorldDimensions.Complete finalDims = dimensions.bake(dims);
         Lifecycle lifecycle = finalDims.lifecycle().add(context.datapackWorldRegistries().allRegistriesLifecycle());
         PrimaryLevelData data = new PrimaryLevelData(ls, finalDims.specialWorldProperty(), lifecycle);
         return new WorldLoader.DataLoadOutput<>(new LevelDataAndDimensions.WorldDataAndGenSettings(data, new WorldGenSettings(props.worldOptions, dimensions)), finalDims.dimensionsRegistryAccess());
      }, WorldStem::new, Util.backgroundExecutor(), executor)).get();
      access.saveDataTag(stem.worldDataAndGenSettings().data());
      RandomSupport.reseedUniquifier(8682522807148012L);
      server = new VanillaLevel.Server(Thread.currentThread(), access, packs, stem, settings, services, new NotificationManager());
      server.start();
      overworld = server.overworld();
      nether = server.getLevel(Level.NETHER);
      cur = overworld;
   }

   // ------------------------------------------------------------------------------------------------ script execution

   static BlockState state(String spec) throws Exception {
      return BlockStateParser.parseForBlock(server.registryAccess().lookupOrThrow(Registries.BLOCK), new StringReader(spec), false).blockState();
   }

   static Object field(Object o, String name) throws Exception {
      Field f = o.getClass().getDeclaredField(name);
      f.setAccessible(true);
      return f.get(o);
   }

   static String shape(PortalShape s) throws Exception {
      BlockPos bl = (BlockPos) field(s, "bottomLeft");
      return "axis=" + field(s, "axis") + " num=" + field(s, "numPortalBlocks") + " w=" + field(s, "width") + " h=" + field(s, "height") + " bl=" + bl.getX() + "," + bl.getY() + "," + bl.getZ();
   }

   static Direction.Axis axis(String s) {
      return Direction.Axis.byName(s);
   }

   static void exec(String line) throws Exception {
      OUT.println("> " + line);
      String[] t = line.split(" ");
      switch (t[0]) {
         case "dim" -> cur = t[1].equals("nether") ? nether : overworld;
         case "border" -> {
            var b = cur.getWorldBorder();
            if (t[1].equals("reset")) {
               b.setCenter(0, 0);
               b.setSize(5.999997E7);
            } else {
               b.setCenter(Double.parseDouble(t[1]), Double.parseDouble(t[2]));
               b.setSize(Double.parseDouble(t[3]));
            }
         }
         case "fill" -> {
            int[] c = new int[6];
            for (int i = 0; i < 6; i++) c[i] = Integer.parseInt(t[1 + i]);
            BlockState st = state(t[7]);
            for (int x = Math.min(c[0], c[3]); x <= Math.max(c[0], c[3]); x++)
               for (int y = Math.min(c[1], c[4]); y <= Math.max(c[1], c[4]); y++)
                  for (int z = Math.min(c[2], c[5]); z <= Math.max(c[2], c[5]); z++)
                     cur.setBlock(new BlockPos(x, y, z), st, QUIET);
         }
         case "set2" -> cur.setBlock(new BlockPos(Integer.parseInt(t[1]), Integer.parseInt(t[2]), Integer.parseInt(t[3])), state(t[4]), QUIET);
         case "set" -> cur.setBlockAndUpdate(new BlockPos(Integer.parseInt(t[1]), Integer.parseInt(t[2]), Integer.parseInt(t[3])), state(t[4]));
         case "any" -> {
            BlockPos p = new BlockPos(Integer.parseInt(t[1]), Integer.parseInt(t[2]), Integer.parseInt(t[3]));
            PortalShape s = PortalShape.findAnyShape(cur, p, axis(t[4]));
            OUT.println("< A " + shape(s) + " valid=" + s.isValid() + " complete=" + s.isComplete());
         }
         case "empty", "emptyplace" -> {
            BlockPos p = new BlockPos(Integer.parseInt(t[1]), Integer.parseInt(t[2]), Integer.parseInt(t[3]));
            Optional<PortalShape> s = PortalShape.findEmptyPortalShape(cur, p, axis(t[4]));
            OUT.println("< E " + (s.isPresent() ? shape(s.get()) : "none"));
            if (t[0].equals("emptyplace") && s.isPresent()) s.get().createPortalBlocks(cur);
         }
         case "digest" -> digest(t);
         case "find" -> {
            BlockPos p = new BlockPos(Integer.parseInt(t[1]), Integer.parseInt(t[2]), Integer.parseInt(t[3]));
            Optional<BlockPos> r = cur.getPortalForcer().findClosestPortalPosition(p, Boolean.parseBoolean(t[4]), cur.getWorldBorder());
            OUT.println("< F " + (r.isPresent() ? r.get().getX() + " " + r.get().getY() + " " + r.get().getZ() : "none"));
         }
         case "create" -> {
            BlockPos p = new BlockPos(Integer.parseInt(t[1]), Integer.parseInt(t[2]), Integer.parseInt(t[3]));
            Optional<BlockUtil.FoundRectangle> r = cur.getPortalForcer().createPortal(p, axis(t[4]));
            OUT.println("< C " + (r.isPresent() ? r.get().minCorner.getX() + " " + r.get().minCorner.getY() + " " + r.get().minCorner.getZ() + " " + r.get().axis1Size + " " + r.get().axis2Size : "none"));
         }
         case "rect" -> {
            BlockPos p = new BlockPos(Integer.parseInt(t[1]), Integer.parseInt(t[2]), Integer.parseInt(t[3]));
            BlockState st = cur.getBlockState(p);
            BlockUtil.FoundRectangle r = BlockUtil.getLargestRectangleAround(p, axis(t[4]), 21, Direction.Axis.Y, 21, q -> cur.getBlockState(q) == st);
            OUT.println("< R " + r.minCorner.getX() + " " + r.minCorner.getY() + " " + r.minCorner.getZ() + " " + r.axis1Size + " " + r.axis2Size);
         }
         case "maxrect" -> {
            int[] cols = new int[t.length - 1];
            for (int i = 0; i < cols.length; i++) cols[i] = Integer.parseInt(t[i + 1]);
            Method m = BlockUtil.class.getDeclaredMethod("getMaxRectangleLocation", int[].class);
            m.setAccessible(true);
            @SuppressWarnings("unchecked")
            com.mojang.datafixers.util.Pair<BlockUtil.IntBounds, Integer> r = (com.mojang.datafixers.util.Pair<BlockUtil.IntBounds, Integer>) m.invoke(null, (Object) cols);
            OUT.println("< M " + r.getFirst().min + " " + r.getFirst().max + " " + r.getSecond());
         }
         case "relpos" -> {
            BlockUtil.FoundRectangle fr = new BlockUtil.FoundRectangle(new BlockPos(Integer.parseInt(t[1]), Integer.parseInt(t[2]), Integer.parseInt(t[3])), Integer.parseInt(t[4]), Integer.parseInt(t[5]));
            Vec3 v = PortalShape.getRelativePosition(fr, axis(t[6]), new Vec3(Double.parseDouble(t[7]), Double.parseDouble(t[8]), Double.parseDouble(t[9])),
               EntityDimensions.scalable(Float.parseFloat(t[10]), Float.parseFloat(t[11])));
            OUT.println("< V " + v.x + " " + v.y + " " + v.z);
         }
         default -> throw new IllegalArgumentException(line);
      }
   }

   static void digest(String[] t) throws Exception {
      int[] c = new int[6];
      for (int i = 0; i < 6; i++) c[i] = Integer.parseInt(t[1 + i]);
      long hash = 0xcbf29ce484222325L;
      int n = 0;
      List<String> special = new ArrayList<>();
      for (int x = c[0]; x <= c[3]; x++)
         for (int y = c[1]; y <= c[4]; y++)
            for (int z = c[2]; z <= c[5]; z++) {
               BlockState st = cur.getBlockState(new BlockPos(x, y, z));
               if (st.isAir()) continue;
               n++;
               String s = x + " " + y + " " + z + " " + BlockStateParser.serialize(st);
               for (byte b : s.getBytes()) {
                  hash ^= b & 0xFF;
                  hash *= 0x100000001b3L;
               }
               String name = net.minecraft.core.registries.BuiltInRegistries.BLOCK.getKey(st.getBlock()).getPath();
               if (name.equals("obsidian") || name.equals("nether_portal") || name.equals("fire") || name.equals("soul_fire") || name.equals("crying_obsidian")) special.add("< O " + s);
            }
      OUT.println("< D n=" + n + " hash=" + Long.toHexString(hash));
      for (String s : special) OUT.println(s);
   }

   // ------------------------------------------------------------------------------------------------ generators

   /** A frame whose interior starts at (x0, y0, z0) (lowest, lowest-coordinate cell) and runs w cells along {@code axis}. */
   static void frame(Random r, Direction.Axis axis, int x0, int y0, int z0, int w, int h, String block, boolean corners) throws Exception {
      int dx = axis == Direction.Axis.X ? 1 : 0;
      int dz = 1 - dx;
      for (int i = 0; i < w; i++) {
         exec("set2 " + (x0 + dx * i) + " " + (y0 - 1) + " " + (z0 + dz * i) + " " + block);
         exec("set2 " + (x0 + dx * i) + " " + (y0 + h) + " " + (z0 + dz * i) + " " + block);
      }
      for (int j = 0; j < h; j++) {
         exec("set2 " + (x0 - dx) + " " + (y0 + j) + " " + (z0 - dz) + " " + block);
         exec("set2 " + (x0 + dx * w) + " " + (y0 + j) + " " + (z0 + dz * w) + " " + block);
      }
      if (corners) {
         for (int sx : new int[] {-1, w}) for (int sy : new int[] {-1, h}) exec("set2 " + (x0 + dx * sx) + " " + (y0 + sy) + " " + (z0 + dz * sx) + " " + block);
      }
   }

   static final String[] OTHERS = {"minecraft:dirt", "minecraft:cobblestone", "minecraft:glowstone", "minecraft:stone", "minecraft:water", "minecraft:lava", "minecraft:oak_trapdoor", "minecraft:torch", "minecraft:snow"};

   static void shapes(int cases) throws Exception {
      Random r = new Random(20260101L);
      PrintStream real = OUT;
      for (int i = 0; i < cases; i++) {
         real.println("# case " + i);
         boolean deep = r.nextInt(10) == 0;
         int y0 = deep ? -63 + r.nextInt(4) : 60 + r.nextInt(20);
         int x0 = r.nextInt(5), z0 = r.nextInt(5);
         int w = r.nextInt(3) == 0 ? 2 + r.nextInt(20) : 2 + r.nextInt(4);
         int h = r.nextInt(3) == 0 ? 3 + r.nextInt(19) : 3 + r.nextInt(4);
         if (r.nextInt(25) == 0) w = 21 + r.nextInt(2);
         if (r.nextInt(25) == 0) h = 21 + r.nextInt(2);
         if (r.nextInt(30) == 0) w = 1;
         if (r.nextInt(30) == 0) h = 2;
         Direction.Axis axis = r.nextBoolean() ? Direction.Axis.X : Direction.Axis.Z;
         int bx1 = x0 - 3, bx2 = x0 + 28, bz1 = z0 - 3, bz2 = z0 + 28;
         exec("fill " + bx1 + " " + Math.max(-64, y0 - 4) + " " + bz1 + " " + bx2 + " " + (y0 + 26) + " " + bz2 + " minecraft:air");
         String block = r.nextInt(4) == 0 ? "minecraft:crying_obsidian" : "minecraft:obsidian";
         frame(r, axis, x0, y0, z0, w, h, block, r.nextBoolean());
         int dx = axis == Direction.Axis.X ? 1 : 0, dz = 1 - dx;
         int muts = r.nextInt(4);
         for (int m = 0; m < muts; m++) {
            int kind = r.nextInt(9);
            int a = r.nextInt(w), b = r.nextInt(h);
            int ix = x0 + dx * a, iy = y0 + b, iz = z0 + dz * a;
            switch (kind) {
               case 0 -> { // break a frame block
                  int side = r.nextInt(4);
                  int fx = side == 0 ? x0 - dx : side == 1 ? x0 + dx * w : x0 + dx * a;
                  int fz = side == 0 ? z0 - dz : side == 1 ? z0 + dz * w : z0 + dz * a;
                  int fy = side == 2 ? y0 - 1 : side == 3 ? y0 + h : y0 + b;
                  exec("set2 " + fx + " " + fy + " " + fz + " " + (r.nextBoolean() ? "minecraft:air" : OTHERS[r.nextInt(OTHERS.length)]));
               }
               case 1 -> exec("set2 " + ix + " " + iy + " " + iz + " minecraft:fire");
               case 2 -> exec("set2 " + ix + " " + iy + " " + iz + " minecraft:soul_fire");
               case 3 -> exec("set2 " + ix + " " + iy + " " + iz + " " + OTHERS[r.nextInt(OTHERS.length)]);
               case 4 -> { // partial / full portal fill, right or wrong axis
                  String ax = r.nextInt(4) == 0 ? (axis == Direction.Axis.X ? "z" : "x") : axis.getName();
                  int rows = r.nextBoolean() ? h : 1 + r.nextInt(h);
                  for (int j = 0; j < rows; j++)
                     for (int k = 0; k < w; k++)
                        if (r.nextInt(8) != 0) exec("set2 " + (x0 + dx * k) + " " + (y0 + j) + " " + (z0 + dz * k) + " minecraft:nether_portal[axis=" + ax + "]");
               }
               case 5 -> exec("fill " + (x0 + dx * a) + " " + (y0 + b) + " " + (z0 + dz * a) + " " + (x0 + dx * Math.min(w - 1, a + 2)) + " " + (y0 + Math.min(h - 1, b + 1)) + " " + (z0 + dz * Math.min(w - 1, a + 2)) + " minecraft:fire");
               case 6 -> exec("set2 " + (x0 - dx * 2) + " " + y0 + " " + (z0 - dz * 2) + " minecraft:obsidian"); // stray block outside
               case 7 -> exec("set2 " + ix + " " + (y0 - 1) + " " + iz + " " + OTHERS[r.nextInt(OTHERS.length)]); // bottom frame gap
               case 8 -> exec("set2 " + (x0 + dx * w) + " " + (y0 + b) + " " + (z0 + dz * w) + " minecraft:crying_obsidian");
            }
         }
         int queries = 1 + r.nextInt(4);
         for (int q = 0; q < queries; q++) {
            int a = r.nextInt(w + 4) - 2, b = r.nextInt(h + 5) - 2;
            int qx = x0 + dx * a + (r.nextInt(6) == 0 ? dz : 0), qy = y0 + b, qz = z0 + dz * a + (r.nextInt(6) == 0 ? dx : 0);
            String ax = r.nextBoolean() ? "x" : "z";
            exec("any " + qx + " " + qy + " " + qz + " " + ax);
            exec("empty " + qx + " " + qy + " " + qz + " " + (ax.equals("x") ? "z" : "x"));
         }
         int qx = x0 + dx * r.nextInt(w) , qy = y0 + r.nextInt(h), qz = z0 + dz * r.nextInt(w);
         switch (r.nextInt(3)) {
            case 0 -> exec("emptyplace " + qx + " " + qy + " " + qz + " " + (r.nextBoolean() ? "x" : "z"));
            case 1 -> exec("set " + qx + " " + qy + " " + qz + " minecraft:fire"); // BaseFireBlock.onPlace
            default -> exec("set " + qx + " " + qy + " " + qz + " minecraft:soul_fire");
         }
         exec("digest " + bx1 + " " + Math.max(-64, y0 - 4) + " " + bz1 + " " + bx2 + " " + (y0 + 26) + " " + bz2);
         if (r.nextInt(3) == 0) { // break part of the frame with block updates: portal blocks must update their shape
            int a = r.nextInt(w), b = r.nextInt(h);
            exec("set " + (x0 - dx) + " " + (y0 + b) + " " + (z0 - dz) + " minecraft:air");
            exec("digest " + bx1 + " " + Math.max(-64, y0 - 4) + " " + bz1 + " " + bx2 + " " + (y0 + 26) + " " + bz2);
         }
         // rectangles over portal blocks / any equal states
         exec("rect " + (x0 + dx * r.nextInt(w)) + " " + (y0 + r.nextInt(h)) + " " + (z0 + dz * r.nextInt(w)) + " " + axis.getName());
      }
   }

   static void misc() throws Exception {
      Random r = new Random(7);
      for (int i = 0; i < 300; i++) {
         int n = 1 + r.nextInt(i < 100 ? 6 : 40);
         StringBuilder sb = new StringBuilder("maxrect");
         for (int k = 0; k < n; k++) sb.append(' ').append(r.nextInt(6) == 0 ? 0 : r.nextInt(22));
         exec(sb.toString());
      }
      double[] dims = {0.6, 1.8, 0.25, 0.25, 1.0, 1.0, 0.98, 0.98, 2.0, 0.5, 0.9, 0.9};
      for (int i = 0; i < 400; i++) {
         int d = r.nextInt(dims.length / 2) * 2;
         int w = 1 + r.nextInt(8), h = 1 + r.nextInt(8);
         int minx = r.nextInt(40) - 20, miny = r.nextInt(100), minz = r.nextInt(40) - 20;
         String ax = r.nextBoolean() ? "x" : "z";
         double px = (ax.equals("x") ? minx : minz) + r.nextDouble() * (w + 2) - 1;
         double pz = (ax.equals("x") ? minz : minx) + r.nextDouble() * 3 - 1;
         double py = miny + r.nextDouble() * (h + 2) - 1;
         double ex = ax.equals("x") ? px : pz, ez = ax.equals("x") ? pz : px;
         exec("relpos " + minx + " " + miny + " " + minz + " " + w + " " + h + " " + ax + " " + ex + " " + py + " " + ez + " " + dims[d] + " " + dims[d + 1]);
      }
   }

   /** A portal at (x0,y0,z0) (interior lowest corner), quiet: obsidian frame + portal blocks (POIs register through setBlock). */
   static void portal(Random r, Direction.Axis axis, int x0, int y0, int z0, int w, int h) throws Exception {
      frame(r, axis, x0, y0, z0, w, h, "minecraft:obsidian", false);
      int dx = axis == Direction.Axis.X ? 1 : 0, dz = 1 - dx;
      exec("fill " + x0 + " " + y0 + " " + z0 + " " + (x0 + dx * (w - 1)) + " " + (y0 + h - 1) + " " + (z0 + dz * (w - 1)) + " minecraft:nether_portal[axis=" + axis.getName() + "]");
   }

   static void forcer(int worlds) throws Exception {
      Random r = new Random(424242L);
      for (int k = 0; k < worlds; k++) {
         boolean isNether = k % 2 == 1;
         int minY = isNether ? 0 : -64;
         int ox = k * 1000 * (r.nextBoolean() ? 1 : -1), oz = r.nextInt(200) - 100;
         exec("dim " + (isNether ? "nether" : "overworld"));
         OUT.println("# world " + k);
         boolean border = r.nextInt(3) == 0;
         if (border) exec("border " + (ox + r.nextInt(41) - 20) + " " + (oz + r.nextInt(41) - 20) + " " + (40 + r.nextInt(160)));
         else exec("border reset");
         int top = isNether ? 100 : 100;
         int base = isNether ? 30 + r.nextInt(40) : 55 + r.nextInt(30);
         int boxes = r.nextInt(8);
         for (int b = 0; b < boxes; b++) {
            int x1 = ox + r.nextInt(120) - 60, z1 = oz + r.nextInt(120) - 60;
            int x2 = x1 + r.nextInt(40), z2 = z1 + r.nextInt(40);
            int y1 = minY + r.nextInt(isNether ? 30 : 40), y2 = base + r.nextInt(20) - 8;
            String[] blocks = {"minecraft:stone", "minecraft:netherrack", "minecraft:stone", "minecraft:water", "minecraft:lava", "minecraft:oak_leaves", "minecraft:glass", "minecraft:snow", "minecraft:magma_block"};
            exec("fill " + x1 + " " + y1 + " " + z1 + " " + x2 + " " + y2 + " " + z2 + " " + blocks[r.nextInt(blocks.length)]);
            if (r.nextInt(3) == 0) exec("fill " + x1 + " " + (y2 + 1 + r.nextInt(3)) + " " + z1 + " " + x2 + " " + (y2 + 1 + r.nextInt(3)) + " " + z2 + " minecraft:stone"); // thin ceiling slab above
         }
         if (isNether && r.nextBoolean()) { // roof near the logical height
            int x1 = ox + r.nextInt(60) - 40, z1 = oz + r.nextInt(60) - 40;
            exec("fill " + x1 + " 120 " + z1 + " " + (x1 + 30) + " 127 " + (z1 + 30) + " minecraft:netherrack");
         }
         List<int[]> portals = new ArrayList<>();
         int np = r.nextInt(4);
         for (int p = 0; p < np; p++) {
            Direction.Axis axis = r.nextBoolean() ? Direction.Axis.X : Direction.Axis.Z;
            int w = 2 + r.nextInt(r.nextInt(3) == 0 ? 6 : 3), h = 3 + r.nextInt(r.nextInt(3) == 0 ? 6 : 3);
            int x0 = ox + r.nextInt(100) - 50, z0 = oz + r.nextInt(100) - 50, y0 = minY + 20 + r.nextInt(isNether ? 80 : 90);
            portal(r, axis, x0, y0, z0, w, h);
            portals.add(new int[] {x0, y0, z0, w, h, axis == Direction.Axis.X ? 0 : 1});
         }
         for (int[] p : portals) { // ties: query between portal blocks
            int dx = p[5] == 0 ? 1 : 0, dz = 1 - dx;
            for (int t = 0; t < 3; t++) {
               int a = r.nextInt(p[3] + 1), b = r.nextInt(p[4] + 1) - (t == 2 ? 1 : 0);
               exec("find " + (p[0] + dx * a) + " " + (p[1] + b) + " " + (p[2] + dz * a) + " " + (r.nextBoolean() ? "true" : "false"));
            }
         }
         for (int q = 0; q < 14; q++) {
            int x = ox + r.nextInt(240) - 120, z = oz + r.nextInt(240) - 120, y = minY + r.nextInt(isNether ? 120 : 160);
            if (!portals.isEmpty() && r.nextBoolean()) {
               int[] p = portals.get(r.nextInt(portals.size()));
               x = p[0] + r.nextInt(30) - 15;
               z = p[2] + r.nextInt(30) - 15;
               y = p[1] + r.nextInt(10) - 5;
            }
            exec("find " + x + " " + y + " " + z + " " + (r.nextBoolean() ? "true" : "false"));
         }
         int creates = 1 + r.nextInt(3);
         for (int c = 0; c < creates; c++) {
            int x = ox + r.nextInt(100) - 50, z = oz + r.nextInt(100) - 50, y = minY + r.nextInt(isNether ? 130 : 200) + (isNether ? 0 : 20);
            if (r.nextInt(3) == 0) { y = isNether ? r.nextInt(300) - 20 : r.nextInt(500) - 100; }
            String axis = r.nextBoolean() ? "x" : "z";
            exec("create " + x + " " + y + " " + z + " " + axis);
            exec("digest " + (x - 22) + " " + minY + " " + (z - 22) + " " + (x + 22) + " " + (isNether ? 132 : 140) + " " + (z + 22));
            exec("find " + x + " " + y + " " + z + " " + (r.nextBoolean() ? "true" : "false"));
            exec("find " + (x + r.nextInt(9) - 4) + " " + (y + r.nextInt(9) - 4) + " " + (z + r.nextInt(9) - 4) + " " + (r.nextBoolean() ? "true" : "false"));
         }
      }
   }
}
