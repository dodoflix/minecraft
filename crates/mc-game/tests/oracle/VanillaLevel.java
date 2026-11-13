import com.mojang.authlib.GameProfile;
import com.mojang.authlib.services.MinecraftServicesDiscoveryService;
import com.mojang.serialization.Dynamic;
import com.mojang.serialization.Lifecycle;
import io.netty.buffer.ByteBuf;
import io.netty.buffer.Unpooled;
import io.netty.channel.ChannelHandlerContext;
import io.netty.channel.ChannelOutboundHandlerAdapter;
import io.netty.channel.ChannelPromise;
import io.netty.channel.embedded.EmbeddedChannel;
import java.io.IOException;
import java.io.PrintStream;
import java.net.Proxy;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;
import java.util.function.Consumer;
import net.minecraft.SharedConstants;
import net.minecraft.commands.CommandSource;
import net.minecraft.commands.CommandSourceStack;
import net.minecraft.commands.Commands;
import net.minecraft.core.Registry;
import net.minecraft.core.registries.Registries;
import net.minecraft.network.Connection;
import net.minecraft.network.PacketSendListener;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.network.chat.Component;
import net.minecraft.network.protocol.Packet;
import net.minecraft.network.protocol.PacketFlow;
import net.minecraft.network.protocol.common.ServerboundKeepAlivePacket;
import net.minecraft.network.protocol.game.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.Eula;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.Services;
import net.minecraft.server.WorldLoader;
import net.minecraft.server.WorldStem;
import net.minecraft.server.dedicated.DedicatedPlayerList;
import net.minecraft.server.dedicated.DedicatedServer;
import net.minecraft.server.dedicated.DedicatedServerProperties;
import net.minecraft.server.dedicated.DedicatedServerSettings;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.server.network.CommonListenerCookie;
import net.minecraft.server.notifications.NotificationManager;
import net.minecraft.server.packs.repository.PackRepository;
import net.minecraft.server.packs.repository.ServerPacksSource;
import net.minecraft.util.Util;
import net.minecraft.util.datafix.DataFixers;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.LevelSettings;
import net.minecraft.world.level.WorldDataConfiguration;
import net.minecraft.world.level.dimension.LevelStem;
import net.minecraft.world.level.levelgen.RandomSupport;
import net.minecraft.world.level.levelgen.WorldGenSettings;
import net.minecraft.world.level.levelgen.WorldDimensions;
import net.minecraft.world.level.levelgen.WorldOptions;
import net.minecraft.world.level.storage.LevelDataAndDimensions;
import net.minecraft.world.level.storage.LevelStorageSource;
import net.minecraft.world.level.storage.PrimaryLevelData;

/**
 * In-process VANILLA level for the entity oracle: a real {@code DedicatedServer} (real {@code ServerLevel}, chunk system,
 * {@code PlayerList}, commands, game rules, registries and data packs) living in a temp dir, without sockets, console thread or
 * watchdog, ticked by hand on the calling thread (the calling thread IS the server thread). Nothing of the game logic is
 * reimplemented: every observable comes from the real vanilla code paths (level.tick, entity ticks, packet handlers).
 *
 * <p>Run it through the scenario DSL: {@code scripts/entity-oracle.sh crates/mc-game/tests/oracle/scenarios/fall.txt}
 * (see {@code EntityOracleDriver.java} for the command list), or programmatically from a Java file
 * ({@code scripts/entity-oracle.sh MyScenario.java}, which may use {@code VanillaLevel.boot(...)} directly).
 *
 * <p>Determinism: fixed world seed; game time/weather frozen by default ({@code advance_time}/{@code advance_weather} false), no
 * natural spawning; every entity created by the vanilla code on the server thread draws its {@code RandomSource.create()} seed
 * from {@link RandomSupport#reseedUniquifier}'s per-thread counter (the ORACLE SHADOW, {@code tests/oracle/shadow.patch} applied to
 * vanilla's RandomSupport without {@code System.nanoTime()}); the sequence is reset at boot, at the start of every tick and by
 * {@link #reseed}. {@code Level.random} is seeded at boot and by the driver's {@code level_seed};
 * {@code random_tick_speed} defaults to 0 so random ticks (chunk-order dependent) do not consume it. Chunks around
 * the origin are fully loaded and entity-ticking before the first script line. Player connections are real
 * {@link Connection}s over a Netty {@code EmbeddedChannel}: every clientbound packet the vanilla code sends is recorded
 * (bundles flattened with their delimiters, encoded with the real play codec) in {@link FakePlayer#packets}.
 */
public final class VanillaLevel {
   /** The untouched stdout: Bootstrap replaces System.out by a logger, fixtures are printed here. */
   public static final PrintStream OUT = System.out;
   public static final PrintStream ERR = System.err;

   public final Path dir;
   public final Server server;
   public final ServerLevel level;
   public final List<FakePlayer> players = new ArrayList<>();
   public final List<String> commandOutput = new ArrayList<>();
   public long ticks;

   public static final class Options {
      public String levelType = "flat"; // flat | void | normal
      public long seed = 4242L;
      public String difficulty = "normal";
      public String dimension = "overworld"; // overworld | the_nether | the_end: the level `v.level` (blocks, ticks, dumps) refers to
      public String features = ""; // comma list of extra feature flags, e.g. redstone_experiments
      public boolean generateStructures = false; // `generate-structures` of server.properties (a normal world only places structures when true)
      public GameType defaultGameType = GameType.SURVIVAL;
      public int preloadRadiusChunks = 6; // > view distance, so joining a player never waits for async chunk generation
      public int viewDistance = 4;
      public int simulationDistance = 4;
   }

   /** Real DedicatedServer minus network/console/query/rcon/watchdog; used on the calling thread. */
   public static final class Server extends DedicatedServer {
      Server(Thread thread, LevelStorageSource.LevelStorageAccess access, PackRepository packs, WorldStem stem, DedicatedServerSettings settings,
         Services services, NotificationManager notifications) {
         super(thread, access, packs, stem, Optional.empty(), settings, DataFixers.getDataFixer(), services, null, notifications);
      }

      void start() {
         this.setUsesAuthentication(false);
         this.worldData.setGameType(this.settingsGameType());
         this.setPlayerList(new RecordingPlayerList(this, this.registries(), this.playerDataStorage));
         this.loadLevel();
      }

      private GameType settingsGameType() {
         return this.getProperties().gameMode.get();
      }

      /** One vanilla server tick (tickServer: level ticks, connection ticks, player list, ...). */
      public void tickOnce() {
         this.tickServer(() -> true);
      }

      @Override
      public boolean isDedicatedServer() {
         return true;
      }
   }

   /** Event log (block-oracle): the `EV ...` lines of every PlayerList.broadcast the level made since the last `dump events`. */
   public static final List<String> EVENTS = new ArrayList<>();

   /**
    * DedicatedPlayerList that records every {@code broadcast(except, x, y, z, range, dim, packet)} (ServerLevel.levelEvent,
    * playSeededSound, runBlockEvents all go through it) whether or not a player is in range, then delegates. Sound seeds are not
    * printed (they come from a nanoTime seeded generator). Format (the Rust twin is tests/common/block_scenario.rs):
    * {@code EV sound at=x,y,z r=R ex=0|1 event=ID source=NAME vol=V pitch=P}, {@code EV level_event r=R ex= type=N pos=x,y,z data=N global=0|1},
    * {@code EV block_event r=R ex= pos=x,y,z block=ID b0=N b1=N}, otherwise {@code EV packet at=x,y,z r=R ex= id=TYPE}.
    */
   public static final class RecordingPlayerList extends DedicatedPlayerList {
      RecordingPlayerList(DedicatedServer server, net.minecraft.core.LayeredRegistryAccess<net.minecraft.server.RegistryLayer> registries,
         net.minecraft.world.level.storage.PlayerDataStorage storage) {
         super(server, registries, storage);
      }

      @Override
      public void broadcast(Player except, double x, double y, double z, double range, net.minecraft.resources.ResourceKey<net.minecraft.world.level.Level> dimension, Packet<?> packet) {
         String head = "r=" + range + " ex=" + (except != null ? 1 : 0);
         if (packet instanceof ClientboundSoundPacket s) {
            EVENTS.add("EV sound at=" + x + "," + y + "," + z + " " + head + " event=" + s.getSound().getRegisteredName() + " source=" + s.getSource().getName() + " vol=" + s.getVolume() + " pitch=" + s.getPitch());
         } else if (packet instanceof ClientboundLevelEventPacket e) {
            EVENTS.add("EV level_event " + head + " type=" + e.getType() + " pos=" + e.getPos().getX() + "," + e.getPos().getY() + "," + e.getPos().getZ() + " data=" + e.getData() + " global=" + (e.isGlobalEvent() ? 1 : 0));
         } else if (packet instanceof ClientboundBlockEventPacket b) {
            EVENTS.add("EV block_event " + head + " pos=" + b.getPos().getX() + "," + b.getPos().getY() + "," + b.getPos().getZ() + " block=" + net.minecraft.core.registries.BuiltInRegistries.BLOCK.getKey(b.getBlock()) + " b0=" + (b.getB0() & 255) + " b1=" + (b.getB1() & 255));
         } else {
            EVENTS.add("EV packet at=" + x + "," + y + "," + z + " " + head + " id=" + packet.type().id());
         }
         super.broadcast(except, x, y, z, range, dimension, packet);
      }
   }

   /** Server player on a recording connection. */
   public final class FakePlayer {
      public final String name;
      public ServerPlayer player;
      public final Connection connection;
      public final EmbeddedChannel channel;
      public final List<Sent> packets = new ArrayList<>();
      Long pendingKeepAlive;

      FakePlayer(String name, ServerPlayer player, Connection connection, EmbeddedChannel channel) {
         this.name = name;
         this.player = player;
         this.connection = connection;
         this.channel = channel;
      }

      /** After a respawn the PlayerList has a new ServerPlayer object for the same connection. */
      public void replaceServerPlayer(ServerPlayer neu) {
         this.player = neu;
      }

      /** Take all packets sent since the last drain. */
      public List<Sent> drain() {
         List<Sent> r = new ArrayList<>(packets);
         packets.clear();
         return r;
      }
   }

   /**
    * A real ServerPlayer; with {@code sim} it is simulated by the server like a locally controlled player (the physics the
    * vanilla CLIENT runs for LocalPlayer: travel/aiStep/fall damage in Entity.move), because vanilla ServerPlayers are
    * client-authoritative (their motion arrives as movement packets and the server never runs travel for them).
    */
   public static final class SimPlayer extends ServerPlayer {
      public final boolean sim;
      // simulated client input (see EntityOracleDriver `input`)
      public boolean inForward, inBack, inLeft, inRight, inJump, inSneak, inSprint;
      // `track_movement`: the known movement a real client reports with its movement packets (the position change of the last tick)
      public boolean trackMovement;
      private net.minecraft.world.phys.Vec3 lastTickPos;

      /** Before a tick: the movement of the previous tick becomes the known movement (what the client's move packet carries). */
      void trackKnownMovement() {
         net.minecraft.world.phys.Vec3 pos = this.position();
         this.setKnownMovement(lastTickPos == null ? net.minecraft.world.phys.Vec3.ZERO : pos.subtract(lastTickPos));
         lastTickPos = pos;
      }

      SimPlayer(MinecraftServer server, ServerLevel level, GameProfile profile, net.minecraft.server.level.ClientInformation info, boolean sim) {
         super(server, level, profile, info);
         this.sim = sim;
      }

      @Override
      public boolean isClientAuthoritative() {
         return !sim;
      }

      private boolean keepPosition;

      // ServerGamePacketListenerImpl.tickPlayer = doTick() then absSnapTo(firstGood*): it throws away server-side motion of a
      // client-authoritative player. A simulated player keeps the position its physics produced.
      @Override
      public void doTick() {
         super.doTick();
         keepPosition = sim;
      }

      @Override
      public void absSnapTo(double x, double y, double z, float yRot, float xRot) {
         if (keepPosition) {
            keepPosition = false;
            return;
         }
         super.absSnapTo(x, y, z, yRot, xRot);
      }

      /**
       * What LocalPlayer.aiStep does with KeyboardInput before LivingEntity.aiStep (movement vector, jump, sneak, sprint). It
       * overrides LivingEntity.applyInput, which aiStep calls: a player the server does not simulate keeps what its packets set
       * (shift key, sprinting) and only gets vanilla's decay.
       */
      @Override
      public void applyInput() {
         if (!sim) {
            super.applyInput();
            return;
         }
         float f = (inForward ? 1f : 0f) - (inBack ? 1f : 0f);
         float s = (inLeft ? 1f : 0f) - (inRight ? 1f : 0f);
         this.setShiftKeyDown(inSneak);
         if (inSneak && !this.getAbilities().flying) {
            float sneak = (float)this.getAttributeValue(net.minecraft.world.entity.ai.attributes.Attributes.SNEAKING_SPEED);
            f *= sneak;
            s *= sneak;
         }
         this.zza = f;
         this.xxa = s;
         this.setJumping(inJump);
         this.setSprinting(inSprint && inForward);
      }
   }

   public record Sent(String type, String hex, Packet<?> packet) {
   }

   private VanillaLevel(Path dir, Server server, ServerLevel level) {
      this.dir = dir;
      this.server = server;
      this.level = level;
   }

   // ------------------------------------------------------------------------------------------------ boot

   public static VanillaLevel boot(Options o) throws Exception {
      SharedConstants.tryDetectVersion();
      Bootstrap.bootStrap();
      Bootstrap.validate();
      Util.startTimerHackThread();
      BUNDLER = GameProtocols.CLIENTBOUND_TEMPLATE.bind(RegistryFriendlyByteBuf.decorator(null)).bundlerInfo();
      Path dir = Files.createTempDirectory("entity-oracle-");
      Runtime.getRuntime().addShutdownHook(new Thread(() -> deleteTree(dir)));
      String generator = switch (o.levelType) {
         case "flat" -> "level-type=minecraft:flat\n";
         case "void" -> "level-type=minecraft:flat\ngenerator-settings={\"layers\":[],\"biome\":\"minecraft:plains\",\"features\":false,\"lakes\":false,\"structure_overrides\":[]}\n";
         case "normal" -> "level-type=minecraft:normal\n";
         default -> throw new IllegalArgumentException("level type flat|void|normal, got " + o.levelType);
      };
      Files.writeString(dir.resolve("server.properties"),
         generator + "level-seed=" + o.seed + "\nonline-mode=false\nenforce-secure-profile=false\nspawn-protection=0\ndifficulty=" + o.difficulty
            + "\ngamemode=" + o.defaultGameType.getName() + "\ngenerate-structures=" + o.generateStructures + "\nview-distance=" + o.viewDistance
            + "\nsimulation-distance=" + o.simulationDistance + "\nspawn-monsters=false\nspawn-animals=false\nspawn-npcs=false\nallow-nether=false\n"
            + "pause-when-empty-seconds=0\nmax-tick-time=-1\nenable-jmx-monitoring=false\nlevel-name=world\n"
            // feature packs are not selected automatically: a world only gets a feature's flag from its enabled pack
            + (o.features.isBlank() ? "" : "initial-enabled-packs=vanilla," + o.features.strip() + "\n"));
      // a vanilla server refuses to start without an accepted EULA: accepting it is the user's decision, never ours
      if (!"1".equals(System.getenv("MC_ACCEPT_EULA"))) throw new IllegalStateException("starting a vanilla server accepts the Minecraft EULA (https://aka.ms/MinecraftEULA) in your name: set MC_ACCEPT_EULA=1 if you accept it");
      Files.writeString(dir.resolve("eula.txt"), "eula=true\n");
      DedicatedServerSettings settings = new DedicatedServerSettings(dir.resolve("server.properties"));
      Eula ignored = new Eula(dir.resolve("eula.txt"));
      Services services = Services.create(MinecraftServicesDiscoveryService.create(Proxy.NO_PROXY), dir.toFile());
      LevelStorageSource source = LevelStorageSource.createDefault(dir);
      LevelStorageSource.LevelStorageAccess access = source.validateAndCreateAccess("world");
      PackRepository packs = ServerPacksSource.createPackRepository(access);
      DedicatedServerProperties props = settings.getProperties();
      WorldDataConfiguration dataConfig = new WorldDataConfiguration(props.initialDataPackConfiguration, featureSet(o.features));
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
      Server server = new Server(Thread.currentThread(), access, packs, stem, settings, services, new NotificationManager());
      server.start();
      REGISTRIES = server.registryAccess();
      net.minecraft.server.level.ServerLevel level = server.getLevel(net.minecraft.resources.ResourceKey.create(Registries.DIMENSION, net.minecraft.resources.Identifier.withDefaultNamespace(o.dimension)));
      if (level == null) {
         throw new IllegalArgumentException("dimension overworld|the_nether|the_end, got " + o.dimension);
      }
      VanillaLevel v = new VanillaLevel(dir, server, level);
      // frozen, quiet world; scripts may flip any rule with `gamerule`
      for (String rule : new String[] {"advance_time", "advance_weather", "spawn_mobs", "spawn_monsters", "spawn_patrols", "spawn_phantoms", "spawn_wandering_traders", "spawn_wardens", "raids", "log_admin_commands", "send_command_feedback"}) {
         v.command("gamerule " + rule + " false");
      }
      v.command("gamerule random_tick_speed 0"); // random ticks consume Level.random in chunk-order dependent ways
      v.commandOutput.clear();
      v.preload(o.preloadRadiusChunks);
      v.level.getRandom().setSeed(o.seed ^ 0x5DEECE66DL); // Level.random: drop offsets, explosion rays, sound seeds; scripts override with `level_seed`
      v.server.getPlayerList().setViewDistance(o.viewDistance);
      RandomSupport.reseedUniquifier(8682522807148012L);
      return v;
   }

   private static net.minecraft.world.flag.FeatureFlagSet featureSet(String names) {
      List<net.minecraft.resources.Identifier> ids = new ArrayList<>();
      for (String n : names.split(",")) {
         if (!n.isBlank()) {
            ids.add(net.minecraft.resources.Identifier.withDefaultNamespace(n.strip()));
         }
      }
      return FeatureFlags.DEFAULT_FLAGS.join(FeatureFlags.REGISTRY.fromNames(ids));
   }

   private static void deleteTree(Path p) {
      try (var s = Files.walk(p)) {
         s.sorted(Comparator.reverseOrder()).forEach(f -> f.toFile().delete());
      } catch (IOException ignored) {
      }
   }

   /** Fully load the chunks around the origin and keep them entity-ticking (forced tickets), so ticks are timing independent. */
   public void preload(int radius) {
      for (int cx = -radius; cx <= radius; cx++) {
         for (int cz = -radius; cz <= radius; cz++) {
            level.setChunkForced(cx, cz, true);
         }
      }
      level.waitForEntities(new ChunkPos(0, 0), radius);
      for (int cx = -radius; cx <= radius; cx++) {
         for (int cz = -radius; cz <= radius; cz++) {
            level.getChunk(cx, cz);
         }
      }
   }

   /** Restart the unique-seed sequence of RandomSource.create() (entity constructors etc.). */
   public static void reseed(long value) {
      RandomSupport.reseedUniquifier(value);
   }

   // ------------------------------------------------------------------------------------------------ commands

   /** Run a vanilla command (without the slash) as the server console, collecting its output into {@link #commandOutput}. */
   public void command(String command) {
      CommandSource sink = new CommandSource() {
         @Override
         public void sendSystemMessage(Component message) {
            commandOutput.add(message.getString());
         }

         @Override
         public boolean acceptsSuccess() {
            return true;
         }

         @Override
         public boolean acceptsFailure() {
            return true;
         }

         @Override
         public boolean shouldInformAdmins() {
            return false;
         }
      };
      CommandSourceStack stack = server.createCommandSourceStack().withSource(sink);
      server.getCommands().performPrefixedCommand(stack, command);
   }

   // ------------------------------------------------------------------------------------------------ ticking

   /** One real server tick; answers keep-alives so long real-time runs never time out. */
   public void tick() {
      // every tick restarts the entity-seed sequence: calls to RandomSource.create() made by main-thread housekeeping
      // (chunk tasks, light, ...) vary between runs and would otherwise shift the seeds of entities created later
      RandomSupport.reseedUniquifier(8682522807148012L ^ (ticks + 1) * 0x9E3779B97F4A7C15L);
      awaitWorkers();
      for (FakePlayer p : players) {
         if (p.player instanceof SimPlayer sp && sp.sim) {
            if (sp.trackMovement) {
               sp.trackKnownMovement();
            }
            sp.applyInput();
         }
      }
      server.tickOnce();
      ticks++;
      for (FakePlayer p : players) {
         if (p.pendingKeepAlive != null) {
            p.player.connection.handleKeepAlive(new ServerboundKeepAlivePacket(p.pendingKeepAlive));
            p.pendingKeepAlive = null;
         }
      }
   }

   /**
    * Chunk loading and generation run on worker threads and hand their results to the main thread's task queue: whether a result is
    * already queued when a tick polls that queue (and with it the number of chunks that tick, hence the Level.random draws of
    * ServerChunkCache.tickChunks, which shuffles the spawning chunks) would depend on thread timing. Before a tick, let the
    * worker pools finish what they hold; the main thread's own queue is left to the tick, as in the server loop.
    */
   private static void awaitWorkers() {
      long deadline = System.nanoTime() + 60_000_000_000L;
      int calm = 0;
      while (calm < 5 && System.nanoTime() < deadline) {
         boolean idle = true;
         if (Util.backgroundExecutor().service() instanceof java.util.concurrent.ForkJoinPool fj) {
            fj.awaitQuiescence(60, java.util.concurrent.TimeUnit.SECONDS);
            idle = fj.isQuiescent();
         }
         if (Util.ioPool().service() instanceof java.util.concurrent.ThreadPoolExecutor io) {
            idle &= io.getActiveCount() == 0 && io.getQueue().isEmpty();
         }
         calm = idle ? calm + 1 : 0;
         try {
            Thread.sleep(1);
         } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            return;
         }
      }
   }

   public void tick(int n) {
      for (int i = 0; i < n; i++) {
         tick();
      }
   }

   // ------------------------------------------------------------------------------------------------ players

   /** Join a player the vanilla way (PrepareSpawnTask.spawn + PlayerList.placeNewPlayer) with the given default game mode. */
   public FakePlayer connect(String name, GameType gameMode, double x, double y, double z, float yaw, float pitch, boolean sim) throws Exception {
      server.setDefaultGameType(gameMode);
      GameProfile profile = new GameProfile(UUID.nameUUIDFromBytes(("OfflinePlayer:" + name).getBytes(StandardCharsets.UTF_8)), name);
      CommonListenerCookie cookie = CommonListenerCookie.createInitial(profile, false);
      Connection connection = new Connection(PacketFlow.SERVERBOUND);
      EmbeddedChannel channel = new EmbeddedChannel(false, false);
      Connection.configureInMemoryPipeline(channel.pipeline(), PacketFlow.SERVERBOUND);
      connection.configurePacketHandler(channel.pipeline());
      FakePlayer[] holder = new FakePlayer[1];
      var codec = GameProtocols.CLIENTBOUND_TEMPLATE.bind(RegistryFriendlyByteBuf.decorator(server.registryAccess())).codec();
      channel.pipeline().addLast("oracle_recorder", new ChannelOutboundHandlerAdapter() {
         @Override
         public void write(ChannelHandlerContext ctx, Object msg, ChannelPromise promise) throws Exception {
            if (msg instanceof Packet<?> packet) {
               record(holder[0], codec, packet);
               promise.setSuccess();
            } else {
               super.write(ctx, msg, promise);
            }
         }
      });
      channel.register();
      server.getConnection().getConnections().add(connection);
      ChunkPos spawnChunk = ChunkPos.containing(net.minecraft.core.BlockPos.containing(x, y, z));
      // PrepareSpawnTask.Ready.keepAlive: without a ticket the chunks around a spawn far from the origin never load
      level.getChunkSource().addTicketWithRadius(net.minecraft.server.level.TicketType.PLAYER_SPAWN, spawnChunk, 3);
      level.waitForEntities(spawnChunk, 3);
      ServerPlayer player = new SimPlayer(server, level, profile, cookie.clientInformation(), sim);
      FakePlayer fp = new FakePlayer(name, player, connection, channel);
      holder[0] = fp;
      player.snapTo(new net.minecraft.world.phys.Vec3(x, y, z), yaw, pitch);
      server.getPlayerList().placeNewPlayer(connection, player, cookie);
      players.add(fp);
      // the client reports it is in the world, as the vanilla client does after the join sequence
      // ServerGamePacketListenerImpl.tickPlayer records these on its first tick; the client accept below needs them now
      for (String f : new String[] {"firstGood", "lastGood"}) {
         for (String axis : new String[] {"X", "Y", "Z"}) {
            setField(player.connection, f + axis, axis.equals("X") ? player.getX() : axis.equals("Y") ? player.getY() : player.getZ());
         }
      }
      for (Sent sent : fp.packets) {
         if (sent.packet() instanceof ClientboundPlayerPositionPacket pos) {
            player.connection.handleAcceptTeleportPacket(new ServerboundAcceptTeleportationPacket(pos.id(), pos.change().position().x, pos.change().position().y, pos.change().position().z, pos.change().yRot(), pos.change().xRot()));
         }
      }
      // the client reports onGround with its movement packets; a non-simulated player has none, so derive it once (resting on a block)
      if (!sim) {
         player.setOnGround(!level.noCollision(player, player.getBoundingBox().move(0, -1.0E-3, 0)));
      }
      // handlePlayerPositionChange of the accept flags a hovering client (kick after 80 ticks); a real client keeps sending moves
      setField(player.connection, "clientIsFloating", false);
      player.connection.handleAcceptPlayerLoad(new ServerboundPlayerLoadedPacket());
      return fp;
   }

   /**
    * Vanilla encodes a few collections in HashMap iteration order of identity-hashed keys (differs per JVM run); the oracle
    * writes them in a canonical order so fixtures are reproducible (a Rust port cannot reproduce identity hashes anyway):
    * `set_time` clock updates sorted by clock id, particle lists of `set_entity_data` (effect particles of active effects)
    * sorted by their encoded form.
    */
   @SuppressWarnings({"unchecked", "rawtypes"})
   private static Packet<? super ClientGamePacketListener> canonical(Packet<? super ClientGamePacketListener> p) {
      if (p instanceof ClientboundSetTimePacket t) {
         java.util.TreeMap<String, Map.Entry<net.minecraft.core.Holder<net.minecraft.world.clock.WorldClock>, net.minecraft.world.clock.ClockNetworkState>> sorted = new java.util.TreeMap<>();
         for (var e : t.clockUpdates().entrySet()) {
            sorted.put(e.getKey().getRegisteredName(), e);
         }
         Map<net.minecraft.core.Holder<net.minecraft.world.clock.WorldClock>, net.minecraft.world.clock.ClockNetworkState> map = new LinkedHashMap<>();
         sorted.values().forEach(e -> map.put(e.getKey(), e.getValue()));
         return new ClientboundSetTimePacket(t.gameTime(), map);
      }
      if (p instanceof ClientboundSetEntityDataPacket d) {
         List<net.minecraft.network.syncher.SynchedEntityData.DataValue<?>> items = new ArrayList<>();
         for (var v : d.packedItems()) {
            if (v.value() instanceof List<?> list && list.size() > 1 && list.get(0) instanceof net.minecraft.core.particles.ParticleOptions) {
               List<Object> copy = new ArrayList<>(list);
               copy.sort(Comparator.comparing(o -> {
                  RegistryFriendlyByteBuf b = new RegistryFriendlyByteBuf(Unpooled.buffer(), REGISTRIES);
                  net.minecraft.core.particles.ParticleTypes.STREAM_CODEC.encode(b, (net.minecraft.core.particles.ParticleOptions)o);
                  byte[] bytes = new byte[b.readableBytes()];
                  b.readBytes(bytes);
                  return java.util.HexFormat.of().formatHex(bytes);
               }));
               items.add(new net.minecraft.network.syncher.SynchedEntityData.DataValue(v.id(), v.serializer(), copy));
            } else {
               items.add(v);
            }
         }
         return new ClientboundSetEntityDataPacket(d.id(), items);
      }
      return p;
   }

   private static net.minecraft.core.RegistryAccess REGISTRIES;
   private static net.minecraft.network.protocol.BundlerInfo BUNDLER; // set after Bootstrap (GameProtocols needs the registries)

   private static void record(FakePlayer fp, net.minecraft.network.codec.StreamCodec<io.netty.buffer.ByteBuf, Packet<? super net.minecraft.network.protocol.game.ClientGamePacketListener>> codec, Packet<?> packet) {
      if (fp == null) {
         return;
      }
      @SuppressWarnings("unchecked")
      Packet<? super ClientGamePacketListener> p = (Packet<? super ClientGamePacketListener>)packet;
      if (p instanceof net.minecraft.network.protocol.common.ClientboundKeepAlivePacket ka) {
         fp.pendingKeepAlive = ka.getId();
         return;
      }
      if (p instanceof ClientboundBundlePacket bundle) {
         // exactly what PacketBundleUnpacker puts on the wire: delimiter, sub packets, delimiter
         List<Packet<?>> flat = new ArrayList<>();
         BUNDLER.unbundlePacket(bundle, flat::add);
         for (Packet<?> sub : flat) {
            record(fp, codec, sub);
         }
         return;
      }
      p = canonical(p);
      ByteBuf buf = Unpooled.buffer();
      codec.encode(buf, p);
      byte[] bytes = new byte[buf.readableBytes()];
      buf.readBytes(bytes);
      buf.release();
      StringBuilder hex = new StringBuilder(bytes.length * 2);
      for (byte b : bytes) {
         hex.append(Character.forDigit((b >> 4) & 15, 16)).append(Character.forDigit(b & 15, 16));
      }
      String type = p.type().id().toString();
      String text = hex.toString();
      if (type.equals("minecraft:update_advancements")) {
         text = "-"; // carries Instant.now() timestamps of obtained criteria
      } else if (p instanceof ClientboundUpdateAttributesPacket attrs) {
         // the snapshot list is a HashSet iteration (identity hashes): canonical order by attribute id and modifier id
         List<String> entries = new ArrayList<>();
         for (var snap : attrs.getValues()) {
            List<String> mods = new ArrayList<>();
            for (var m : snap.modifiers()) {
               mods.add(m.id() + "/" + m.amount() + "/" + m.operation().getSerializedName());
            }
            Collections.sort(mods);
            entries.add(snap.attribute().getRegisteredName() + "=" + snap.base() + "[" + String.join(",", mods) + "]");
         }
         Collections.sort(entries);
         text = "entity=" + attrs.getEntityId() + ";" + String.join(";", entries);
      } else if (p instanceof net.minecraft.network.protocol.game.ClientboundUpdateRecipesPacket recipes) {
         // the property sets are `Set.copyOf` (per JVM salted order) inside a `HashMap`: canonical order by key and item id
         java.util.TreeMap<String, String> sets = new java.util.TreeMap<>();
         for (var e : recipes.itemSets().entrySet()) {
            java.util.Set<net.minecraft.core.Holder<net.minecraft.world.item.Item>> items;
            try {
               var field = net.minecraft.world.item.crafting.RecipePropertySet.class.getDeclaredField("items");
               field.setAccessible(true);
               items = (java.util.Set<net.minecraft.core.Holder<net.minecraft.world.item.Item>>)field.get(e.getValue());
            } catch (ReflectiveOperationException ex) {
               throw new IllegalStateException(ex);
            }
            sets.put(e.getKey().identifier().toString(), String.join(",", items.stream().map(h -> h.getRegisteredName()).sorted().toList()));
         }
         RegistryFriendlyByteBuf stonecutter = new RegistryFriendlyByteBuf(Unpooled.buffer(), REGISTRIES);
         net.minecraft.world.item.crafting.SelectableRecipe.SingleInputSet.<net.minecraft.world.item.crafting.StonecutterRecipe>noRecipeCodec().encode(stonecutter, recipes.stonecutterRecipes());
         byte[] stonecutterBytes = new byte[stonecutter.readableBytes()];
         stonecutter.readBytes(stonecutterBytes);
         StringBuilder sets_text = new StringBuilder();
         sets.forEach((k, v) -> sets_text.append(k).append('=').append(v).append(';'));
         text = "sets=" + sets_text + " stonecutter=" + java.util.HexFormat.of().formatHex(stonecutterBytes);
      } else if (p instanceof ClientboundLevelChunkWithLightPacket chunk) {
         text = "chunk=" + chunk.x() + "," + chunk.z() + "/len=" + bytes.length; // heightmap map order is identity-hash dependent
      }
      fp.packets.add(new Sent(type, text, p));
   }

   static void setField(Object o, String name, Object value) {
      try {
         var f = o.getClass().getDeclaredField(name);
         f.setAccessible(true);
         f.set(o, value);
      } catch (ReflectiveOperationException e) {
         throw new IllegalStateException(e);
      }
   }

   static Object getField(Object o, String name) {
      try {
         var f = o.getClass().getDeclaredField(name);
         f.setAccessible(true);
         return f.get(o);
      } catch (ReflectiveOperationException e) {
         throw new IllegalStateException(e);
      }
   }

   public FakePlayer player(String name) {
      for (FakePlayer p : players) {
         if (p.name.equals(name)) {
            return p;
         }
      }
      throw new IllegalArgumentException("no player " + name);
   }
}
