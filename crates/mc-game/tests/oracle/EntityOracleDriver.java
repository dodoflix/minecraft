import com.mojang.brigadier.StringReader;
import java.io.PrintStream;
import java.nio.file.*;
import java.util.*;
import net.minecraft.commands.arguments.blocks.BlockStateParser;
import net.minecraft.commands.arguments.item.ItemInput;
import net.minecraft.commands.arguments.item.ItemParser;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.TagParser;
import net.minecraft.network.protocol.game.*;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.util.ProblemReporter;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.damagesource.DamageSource;
import net.minecraft.world.damagesource.DamageType;
import net.minecraft.world.effect.MobEffect;
import net.minecraft.world.effect.MobEffectInstance;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntitySpawnReason;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.EquipmentSlot;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.ai.attributes.Attribute;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.storage.TagValueInput;
import net.minecraft.world.phys.BlockHitResult;
import net.minecraft.world.phys.Vec3;

/**
 * Scenario driver of the in-process vanilla oracle ({@link VanillaLevel}). Usage:
 * <pre>  scripts/entity-oracle.sh crates/mc-game/tests/oracle/scenarios/NAME.txt [-o crates/mc-game/tests/fixtures/entity_oracle/NAME.txt]</pre>
 * The scenario is a line-based script, the fixture is the text this driver prints (stdout; vanilla log noise goes to stderr).
 * Commit scenario + fixture; Rust reads the fixture with {@code tests/common/entity_oracle.rs}.
 *
 * <p>Syntax: one command per line, tokens split on whitespace ({@code [..]}, {@code {..}} and "..." keep spaces), {@code #}
 * starts a comment line. {@code <ref>} is a player name (from spawn_player) or the name given to spawn/spawn_item; every
 * number is Java-parsed ({@code 1.5}, {@code -3}); coordinates are absolute. Flat world: the surface block is at y=-61
 * (bedrock -64, dirt -63..-62, grass -61), so a standing player has y=-60.
 *
 * <pre>
 * -- options (only before the first world command) --
 * level flat|void|normal      world type (default flat)            seed N              world seed (default 4242)
 * difficulty peaceful|easy|normal|hard (default normal)            preload R           chunk radius kept loaded (6)
 * features NAME[,NAME]       extra feature flags besides vanilla, e.g. redstone_experiments
 * -- world --
 * gamerule NAME VALUE         e.g. gamerule keep_inventory true    time N       (game clock stays frozen otherwise)
 * block X Y Z STATE           e.g. block 0 -60 0 minecraft:water[level=0]   (setBlock flags 3, like /setblock)
 * fill X1 Y1 Z1 X2 Y2 Z2 STATE
 * cmd COMMAND...              any vanilla command as the console (output appears as "C ..." lines)
 * note TEXT...                prints "# TEXT"
 * reseed N                    restart the entity random-seed sequence (entities created afterwards: see VanillaLevel header)
 * level_seed N                Level.random.setSeed(N) (explosion rays, drop spread, ... draw from it; LegacyRandomSource)
 * rand_value N                Level.randValue = N (the LCG getBlockRandomPos draws from; set it before random ticks)
 * tick_chunk CX CZ SPEED      ServerLevel.tickChunk(chunk, SPEED): random ticks of one chunk (use instead of gamerule random_tick_speed)
 * random_tick X Y Z           randomTick of that block (+ its fluid) as tickChunk's inner loop does for one chosen position
 * be_nbt X Y Z SNBT           merge SNBT into the block entity (`/data merge block`: saveWithFullMetadata.merge, loadWithComponents, setChanged)
 * -- entities --
 * spawn REF TYPE X Y Z [yaw pitch] [SNBT]        real vanilla entity (EntityType.create + addFreshEntity), random seeded from REF
 * spawn_item REF STACK X Y Z [vx vy vz] [count]  ItemEntity (constructor random motion unless vx vy vz given)
 * entity_seed REF N           entity.getRandom().setSeed(N)
 * spawn_player NAME survival|creative|adventure|spectator X Y Z [yaw pitch] [sim]
 *                             joins a real ServerPlayer through PlayerList.placeNewPlayer; `sim` = the server simulates its
 *                             physics (client-side LocalPlayer behaviour, see VanillaLevel.SimPlayer), otherwise it only moves
 *                             through `move_packet`s like a real vanilla server player
 * give REF SLOT STACK [count] SLOT: mainhand offhand head chest legs feet hotbarN (0-8) invN (container slot index)
 * select PLAYER N             hotbar slot (ServerboundSetCarriedItemPacket)
 * effect REF EFFECT AMPLIFIER TICKS [hidden]      addEffect
 * attribute REF ATTRIBUTE BASEVALUE               setBaseValue
 * set REF health|food|saturation|exhaustion|air|fire|fall_distance|xp_level|absorption|invulnerable|no_gravity|tag VALUE
 * nbt REF SNBT                merge SNBT into the entity (Entity.load of the merged save data)
 * snap REF X Y Z [yaw pitch]  Entity.snapTo (no packets)          tp REF X Y Z [yaw pitch]   teleportTo
 * velocity REF X Y Z          setDeltaMovement
 * kill REF                    discard (no death logic)
 * -- inputs / actions (vanilla packet handlers of the player's real ServerGamePacketListenerImpl) --
 * input PLAYER FLAGS          sim player: forward,back,left,right,jump,sneak,sprint (comma list) or `none`; applied each tick
 * move_packet PLAYER X Y Z ONGROUND [yaw pitch]   ServerboundMovePlayerPacket (server-authoritative path)
 * dig PLAYER X Y Z [TICKS]    client-like digging: START_DESTROY_BLOCK, per tick client progress, STOP_DESTROY_BLOCK when the
 *                             client would send it (or after TICKS ticks); prints "DIG ticks=N broken=B"; ticks the server
 * use PLAYER [main|off]       ServerboundUseItemPacket              release PLAYER   RELEASE_USE_ITEM
 * use_on PLAYER X Y Z FACE [main|off] [cx cy cz]  ServerboundUseItemOnPacket (FACE: down up north south west east)
 * attack PLAYER REF           ServerboundAttackPacket               swing PLAYER   ServerboundPunchPacket (main hand)
 * drop PLAYER [all]           DROP_ITEM / DROP_ALL_ITEMS            respawn PLAYER   ServerboundClientCommandPacket
 * pcommand PLAYER ACTION [data]   ServerboundPlayerCommandPacket (START_SPRINTING STOP_SPRINTING START_FALL_FLYING STOP_SLEEPING ...)
 * hurt REF DAMAGETYPE AMOUNT [by=REF] [direct=REF] [at=X,Y,Z]    entity.hurtServer(level, source, amount); prints "HURT result"
 * explode X Y Z RADIUS [fire] [none|block|mob|tnt|trigger] [source=REF]    Level.explode
 * -- time --
 * tick [N]                    N real server ticks (default 1); each prints the `watch` dumps after the tick
 * watch WHAT...               dump after every tick: player:REF entity:REF entities inventory:REF packets:REF   (watch off clears)
 * dump WHAT...                same WHATs immediately, plus blocks:X1,Y1,Z1,X2,Y2,Z2   (`dump` alone = everything)
 *                             block oracle extras: be:X,Y,Z -> `BE x y z SNBT` (saveWithFullMetadata, sorted keys; `-` = none),
 *                             ticks -> `TICK block|fluid x y z id delay=N prio=P` for every pending scheduled tick in execution order,
 *                             events -> the `EV ...` lines of level events / sounds / block events broadcast since the last dump (see
 *                             VanillaLevel.RecordingPlayerList; also valid in `watch`, printed after each tick)
 * </pre>
 *
 * <p>Output lines: {@code T n} (tick counter header before per-tick dumps), {@code P ...} player, {@code E ...} entity,
 * {@code I ...} inventory slot, {@code PKT player type hex} (hex = id + payload; `update_advancements` prints `-` (timestamps), `level_chunk_with_light` prints
 * `chunk=x,z/len=n` (heightmap map order varies per JVM run), `update_attributes` prints `entity=ID;attr=base[mod/amount/op,..];..` sorted
 * (HashSet order varies per JVM run)), {@code B x y z state}, {@code C text} (command output),
 * {@code DIG/HURT ...} action results (entities created by game logic are named {@code type_path#n}, n counting per type in
 * creation order, e.g. {@code item#1}, {@code experience_orb#2}; `dump entities` lists all live non-player entities by name), {@code # ...} notes. Numbers use Java's Double/Float.toString (shortest round trip;
 * the Rust comparator parses numeric tokens instead of comparing text). Entity ids are NOT printed (not stable across servers).
 */
public final class EntityOracleDriver {
   final PrintStream out = VanillaLevel.OUT;
   VanillaLevel v;
   VanillaLevel.Options options = new VanillaLevel.Options();
   final Map<String, Entity> refs = new LinkedHashMap<>();
   final List<String[]> watches = new ArrayList<>();
   final Map<String, Integer> sequence = new HashMap<>();
   final Map<String, Integer> autoCounter = new HashMap<>();
   final Set<Integer> known = new HashSet<>();
   int lineNo;
   String line = "";

   public static void main(String[] args) throws Exception {
      EntityOracleDriver d = new EntityOracleDriver();
      List<String> lines = Files.readAllLines(Path.of(args[0]));
      try {
         for (int i = 0; i < lines.size(); i++) {
            d.lineNo = i + 1;
            d.line = lines.get(i).strip();
            if (d.line.isEmpty() || d.line.startsWith("#")) {
               continue;
            }
            d.run(tokens(d.line));
         }
      } catch (Throwable t) {
         VanillaLevel.ERR.println("scenario " + args[0] + ":" + d.lineNo + ": " + d.line);
         t.printStackTrace(VanillaLevel.ERR);
         d.out.flush();
         Runtime.getRuntime().halt(1);
      }
      d.out.flush();
      System.exit(0);
   }

   static List<String> tokens(String s) {
      List<String> r = new ArrayList<>();
      StringBuilder cur = new StringBuilder();
      int depth = 0;
      boolean quote = false;
      for (char c : s.toCharArray()) {
         if (c == '"') {
            quote = !quote;
         }
         if (!quote) {
            if (c == '[' || c == '{') {
               depth++;
            } else if (c == ']' || c == '}') {
               depth--;
            }
         }
         if (Character.isWhitespace(c) && depth == 0 && !quote) {
            if (cur.length() > 0) {
               r.add(cur.toString());
               cur.setLength(0);
            }
         } else {
            cur.append(c);
         }
      }
      if (cur.length() > 0) {
         r.add(cur.toString());
      }
      return r;
   }

   // ------------------------------------------------------------------------------------------------ helpers

   void world() throws Exception {
      if (v == null) {
         v = VanillaLevel.boot(options);
      }
   }

   static Identifier id(String s) {
      return Identifier.parse(s.contains(":") ? s : "minecraft:" + s);
   }

   static double d(String s) {
      return Double.parseDouble(s);
   }

   static String f(double d) {
      return Double.toString(d);
   }

   static String f(float d) {
      return Float.toString(d);
   }

   Entity ref(String name) {
      Entity e = refs.get(name);
      if (e == null) {
         throw new IllegalArgumentException("unknown ref " + name);
      }
      return e;
   }

   ServerPlayer player(String name) {
      return v.player(name).player;
   }

   ItemStack stack(String spec, int count) throws Exception {
      ItemInput in = new ItemParser(v.server.registryAccess()).parse(new StringReader(spec));
      // like /data merge: counts above the item's max stack size are allowed (oversized stacks exist in vanilla worlds)
      ItemStack st = in.createItemStack(1);
      st.setCount(count);
      return st;
   }

   BlockState state(String spec) throws Exception {
      return BlockStateParser.parseForBlock(v.server.registryAccess().lookupOrThrow(Registries.BLOCK), spec, false).blockState();
   }

   static InteractionHand hand(List<String> a, int i) {
      return a.size() > i && a.get(i).equals("off") ? InteractionHand.OFF_HAND : InteractionHand.MAIN_HAND;
   }

   Map<String, String> kv(List<String> a, int from) {
      Map<String, String> m = new HashMap<>();
      for (int i = from; i < a.size(); i++) {
         int eq = a.get(i).indexOf('=');
         m.put(a.get(i).substring(0, eq), a.get(i).substring(eq + 1));
      }
      return m;
   }

   int seq(String player) {
      return sequence.merge(player, 1, Integer::sum);
   }

   static long hash(String s) {
      long h = 1125899906842597L;
      for (char c : s.toCharArray()) {
         h = 31 * h + c;
      }
      return h;
   }

   // ------------------------------------------------------------------------------------------------ commands

   void run(List<String> a) throws Exception {
      String cmd = a.get(0);
      if (v != null && !cmd.equals("reseed")) {
         VanillaLevel.reseed(hash(line)); // entities created by this command (drops, spawns) draw reproducible seeds
      }
      switch (cmd) {
         case "level" -> options.levelType = a.get(1);
         case "seed" -> options.seed = Long.parseLong(a.get(1));
         case "difficulty" -> options.difficulty = a.get(1);
         case "features" -> options.features = a.get(1);
         case "preload" -> options.preloadRadiusChunks = Integer.parseInt(a.get(1));
         case "note" -> out.println("# " + line.substring(4).strip());
         case "gamerule" -> {
            world();
            command("gamerule " + a.get(1) + " " + a.get(2));
         }
         case "time" -> {
            world();
            command("time set " + a.get(1));
         }
         case "cmd" -> {
            world();
            command(line.substring(3).strip());
         }
         case "level_seed" -> {
            world();
            v.level.getRandom().setSeed(Long.parseLong(a.get(1)));
         }
         case "rand_value" -> {
            world();
            priv(v.level, Level.class, "randValue", Integer.parseInt(a.get(1))); // Level.randValue: the LCG behind getBlockRandomPos (random ticks)
         }
         case "tick_chunk" -> {
            world();
            v.level.tickChunk(v.level.getChunk(Integer.parseInt(a.get(1)), Integer.parseInt(a.get(2))), Integer.parseInt(a.get(3)));
         }
         case "random_tick" -> {
            world();
            BlockPos pos = new BlockPos(Integer.parseInt(a.get(1)), Integer.parseInt(a.get(2)), Integer.parseInt(a.get(3)));
            BlockState st = v.level.getBlockState(pos); // the body of ServerLevel.tickChunk's inner loop for one chosen position
            if (st.isRandomlyTicking()) {
               st.randomTick(v.level, pos, v.level.getRandom());
            }
            net.minecraft.world.level.material.FluidState fs = st.getFluidState();
            if (fs.isRandomlyTicking()) {
               fs.randomTick(v.level, pos, v.level.getRandom());
            }
         }
         case "be_nbt" -> {
            world();
            BlockPos pos = new BlockPos(Integer.parseInt(a.get(1)), Integer.parseInt(a.get(2)), Integer.parseInt(a.get(3)));
            net.minecraft.world.level.block.entity.BlockEntity be = v.level.getBlockEntity(pos);
            if (be == null) {
               throw new IllegalStateException("no block entity at " + pos);
            }
            try (ProblemReporter.ScopedCollector rep = new ProblemReporter.ScopedCollector(be.problemPath(), org.slf4j.LoggerFactory.getLogger("oracle"))) {
               CompoundTag merged = be.saveWithFullMetadata(v.server.registryAccess()).merge(TagParser.parseCompoundFully(line.substring(line.indexOf(a.get(4)))));
               be.loadWithComponents(TagValueInput.create(rep, v.server.registryAccess(), merged));
            }
            be.setChanged(); // /data merge block
            v.level.sendBlockUpdated(pos, be.getBlockState(), be.getBlockState(), 3);
         }
         case "reseed" -> {
            world();
            VanillaLevel.reseed(Long.parseLong(a.get(1)));
         }
         case "block" -> {
            world();
            v.level.setBlock(new BlockPos(Integer.parseInt(a.get(1)), Integer.parseInt(a.get(2)), Integer.parseInt(a.get(3))), state(a.get(4)), 3);
         }
         case "fill" -> {
            world();
            BlockState st = state(a.get(7));
            int x1 = Integer.parseInt(a.get(1)), y1 = Integer.parseInt(a.get(2)), z1 = Integer.parseInt(a.get(3));
            int x2 = Integer.parseInt(a.get(4)), y2 = Integer.parseInt(a.get(5)), z2 = Integer.parseInt(a.get(6));
            for (int x = Math.min(x1, x2); x <= Math.max(x1, x2); x++) {
               for (int y = Math.min(y1, y2); y <= Math.max(y1, y2); y++) {
                  for (int z = Math.min(z1, z2); z <= Math.max(z1, z2); z++) {
                     v.level.setBlock(new BlockPos(x, y, z), st, 3);
                  }
               }
            }
         }
         case "spawn" -> {
            world();
            spawn(a);
         }
         case "spawn_item" -> {
            world();
            VanillaLevel.reseed(hash(a.get(1)));
            ItemStack st = stack(a.get(2), a.size() > 9 ? Integer.parseInt(a.get(9)) : 1);
            double x = d(a.get(3)), y = d(a.get(4)), z = d(a.get(5));
            ItemEntity ie = a.size() >= 9 ? new ItemEntity(v.level, x, y, z, st, d(a.get(6)), d(a.get(7)), d(a.get(8))) : new ItemEntity(v.level, x, y, z, st);
            v.level.addFreshEntity(ie);
            refs.put(a.get(1), ie);
         }
         case "entity_seed" -> ref(a.get(1)).getRandom().setSeed(Long.parseLong(a.get(2)));
         case "spawn_player" -> {
            world();
            GameType gm = GameType.byName(a.get(2));
            boolean sim = a.get(a.size() - 1).equals("sim");
            int n = a.size() - (sim ? 1 : 0);
            VanillaLevel.reseed(hash(a.get(1)));
            VanillaLevel.FakePlayer p = v.connect(a.get(1), gm, d(a.get(3)), d(a.get(4)), d(a.get(5)), n > 6 ? (float)d(a.get(6)) : 0f, n > 7 ? (float)d(a.get(7)) : 0f, sim);
            refs.put(a.get(1), p.player);
            p.drain(); // the join burst is deterministic but huge; `packets:NAME` dumps start after the join
         }
         case "give" -> {
            world();
            give(ref(a.get(1)), a.get(2), stack(a.get(3), a.size() > 4 ? Integer.parseInt(a.get(4)) : 1));
         }
         case "select" -> {
            world();
            ServerPlayer p = player(a.get(1));
            p.connection.handleSetCarriedItem(new ServerboundSetCarriedItemPacket(Integer.parseInt(a.get(2))));
         }
         case "effect" -> {
            world();
            LivingEntity e = (LivingEntity)ref(a.get(1));
            Holder<MobEffect> eff = v.server.registryAccess().lookupOrThrow(Registries.MOB_EFFECT).getOrThrow(ResourceKey.create(Registries.MOB_EFFECT, id(a.get(2))));
            boolean ok = e.addEffect(new MobEffectInstance(eff, Integer.parseInt(a.get(4)), Integer.parseInt(a.get(3)), false, !a.contains("hidden"), !a.contains("hidden")));
            out.println("EFFECT " + a.get(1) + " " + a.get(2) + " added=" + ok);
         }
         case "attribute" -> {
            world();
            LivingEntity e = (LivingEntity)ref(a.get(1));
            Holder<Attribute> at = v.server.registryAccess().lookupOrThrow(Registries.ATTRIBUTE).getOrThrow(ResourceKey.create(Registries.ATTRIBUTE, id(a.get(2))));
            e.getAttribute(at).setBaseValue(d(a.get(3)));
         }
         case "set" -> {
            world();
            set(ref(a.get(1)), a.get(2), a.get(3));
         }
         case "nbt" -> {
            world();
            Entity e = ref(a.get(1));
            try (ProblemReporter.ScopedCollector rep = new ProblemReporter.ScopedCollector(e.problemPath(), org.slf4j.LoggerFactory.getLogger("oracle"))) {
               net.minecraft.world.level.storage.TagValueOutput o = net.minecraft.world.level.storage.TagValueOutput.createWithContext(rep, v.server.registryAccess());
               e.saveWithoutId(o);
               CompoundTag merged = o.buildResult().merge(TagParser.parseCompoundFully(line.substring(line.indexOf(a.get(2)))));
               e.load(TagValueInput.create(rep, v.server.registryAccess(), merged));
            }
         }
         case "snap" -> {
            world();
            Entity e = ref(a.get(1));
            e.snapTo(d(a.get(2)), d(a.get(3)), d(a.get(4)), a.size() > 5 ? (float)d(a.get(5)) : e.getYRot(), a.size() > 6 ? (float)d(a.get(6)) : e.getXRot());
         }
         case "tp" -> {
            world();
            Entity e = ref(a.get(1));
            e.teleportTo(v.level, d(a.get(2)), d(a.get(3)), d(a.get(4)), Set.of(), a.size() > 5 ? (float)d(a.get(5)) : e.getYRot(), a.size() > 6 ? (float)d(a.get(6)) : e.getXRot(), false);
         }
         case "velocity" -> {
            world();
            ref(a.get(1)).setDeltaMovement(d(a.get(2)), d(a.get(3)), d(a.get(4)));
         }
         case "kill" -> {
            world();
            ref(a.get(1)).discard();
         }
         case "input" -> {
            world();
            VanillaLevel.SimPlayer p = (VanillaLevel.SimPlayer)player(a.get(1));
            Set<String> fl = new HashSet<>(Arrays.asList(a.get(2).split(",")));
            p.inForward = fl.contains("forward");
            p.inBack = fl.contains("back");
            p.inLeft = fl.contains("left");
            p.inRight = fl.contains("right");
            p.inJump = fl.contains("jump");
            p.inSneak = fl.contains("sneak");
            p.inSprint = fl.contains("sprint");
            p.connection.handlePlayerInput(new ServerboundPlayerInputPacket(new net.minecraft.world.entity.player.Input(p.inForward, p.inBack, p.inLeft, p.inRight, p.inJump, p.inSneak, p.inSprint)));
         }
         case "move_packet" -> {
            world();
            ServerPlayer p = player(a.get(1));
            double x = d(a.get(2)), y = d(a.get(3)), z = d(a.get(4));
            boolean og = Boolean.parseBoolean(a.get(5));
            ServerboundMovePlayerPacket pk = a.size() > 7
               ? new ServerboundMovePlayerPacket.PosRot(new Vec3(x, y, z), (float)d(a.get(6)), (float)d(a.get(7)), og, false)
               : new ServerboundMovePlayerPacket.Pos(new Vec3(x, y, z), og, false);
            p.connection.handleMovePlayer(pk);
         }
         case "dig" -> {
            world();
            dig(a);
         }
         case "use" -> {
            world();
            ServerPlayer p = player(a.get(1));
            p.connection.handleUseItem(new ServerboundUseItemPacket(hand(a, 2), seq(a.get(1)), p.getYRot(), p.getXRot()));
         }
         case "release" -> {
            world();
            player(a.get(1)).connection.handlePlayerAction(new ServerboundPlayerActionPacket(ServerboundPlayerActionPacket.Action.RELEASE_USE_ITEM, BlockPos.ZERO, Direction.DOWN));
         }
         case "use_on" -> {
            world();
            ServerPlayer p = player(a.get(1));
            BlockPos pos = new BlockPos(Integer.parseInt(a.get(2)), Integer.parseInt(a.get(3)), Integer.parseInt(a.get(4)));
            Direction face = Direction.byName(a.get(5));
            boolean cur = a.size() > 9;
            Vec3 hit = cur ? new Vec3(pos.getX() + d(a.get(a.size() - 3)), pos.getY() + d(a.get(a.size() - 2)), pos.getZ() + d(a.get(a.size() - 1)))
               : Vec3.atCenterOf(pos).add(face.getStepX() * 0.5, face.getStepY() * 0.5, face.getStepZ() * 0.5);
            int handIdx = a.size() > 6 && (a.get(6).equals("main") || a.get(6).equals("off")) ? 6 : -1;
            p.connection.handleUseItemOn(new ServerboundUseItemOnPacket(handIdx > 0 ? hand(a, 6) : InteractionHand.MAIN_HAND, new BlockHitResult(hit, face, pos, false), seq(a.get(1))));
         }
         case "attack" -> {
            world();
            player(a.get(1)).connection.handleAttack(new ServerboundAttackPacket(ref(a.get(2)).getId()));
         }
         case "swing" -> {
            world();
            player(a.get(1)).connection.handlePunch(new ServerboundPunchPacket());
         }
         case "drop" -> {
            world();
            boolean all = a.size() > 2 && a.get(2).equals("all");
            player(a.get(1)).connection.handlePlayerAction(new ServerboundPlayerActionPacket(all ? ServerboundPlayerActionPacket.Action.DROP_ALL_ITEMS : ServerboundPlayerActionPacket.Action.DROP_ITEM, BlockPos.ZERO, Direction.DOWN));
         }
         case "respawn" -> {
            world();
            ServerPlayer old = player(a.get(1));
            old.connection.handleClientCommand(new ServerboundClientCommandPacket(ServerboundClientCommandPacket.Action.PERFORM_RESPAWN));
            ServerPlayer neu = v.server.getPlayerList().getPlayerByName(a.get(1));
            refs.put(a.get(1), neu);
            v.player(a.get(1)).replaceServerPlayer(neu);
         }
         case "pcommand" -> {
            world();
            ServerPlayer p = player(a.get(1));
            p.connection.handlePlayerCommand(new ServerboundPlayerCommandPacket(p, ServerboundPlayerCommandPacket.Action.valueOf(a.get(2).toUpperCase(Locale.ROOT)), a.size() > 3 ? Integer.parseInt(a.get(3)) : 0));
         }
         case "hurt" -> {
            world();
            Entity e = ref(a.get(1));
            Map<String, String> o = kv(a, 4);
            Holder<DamageType> type = v.server.registryAccess().lookupOrThrow(Registries.DAMAGE_TYPE).getOrThrow(ResourceKey.create(Registries.DAMAGE_TYPE, id(a.get(2))));
            Entity by = o.containsKey("by") ? ref(o.get("by")) : null;
            Entity direct = o.containsKey("direct") ? ref(o.get("direct")) : by;
            DamageSource src;
            if (o.containsKey("at")) {
               String[] p = o.get("at").split(",");
               var ctor = DamageSource.class.getDeclaredConstructor(Holder.class, Entity.class, Entity.class, Vec3.class);
               ctor.setAccessible(true);
               src = ctor.newInstance(type, direct, by, new Vec3(d(p[0]), d(p[1]), d(p[2])));
            } else {
               src = new DamageSource(type, direct, by);
            }
            boolean r = e.hurtServer(v.level, src, (float)d(a.get(3)));
            out.println("HURT " + a.get(1) + " " + a.get(2) + " " + a.get(3) + " -> " + r);
         }
         case "explode" -> {
            world();
            Level.ExplosionInteraction inter = Level.ExplosionInteraction.NONE;
            Entity src = null;
            boolean fire = false;
            for (int i = 5; i < a.size(); i++) {
               String t = a.get(i);
               if (t.equals("fire")) {
                  fire = true;
               } else if (t.startsWith("source=")) {
                  src = ref(t.substring(7));
               } else {
                  inter = Level.ExplosionInteraction.valueOf(t.toUpperCase(Locale.ROOT));
               }
            }
            v.level.explode(src, d(a.get(1)), d(a.get(2)), d(a.get(3)), (float)d(a.get(4)), fire, inter);
         }
         case "tick" -> {
            world();
            int n = a.size() > 1 ? Integer.parseInt(a.get(1)) : 1;
            for (int i = 0; i < n; i++) {
               tickOnce();
            }
         }
         case "watch" -> {
            if (a.get(1).equals("off")) {
               watches.clear();
            } else {
               watches.add(a.subList(1, a.size()).toArray(new String[0]));
            }
         }
         case "dump" -> {
            world();
            dump(a.size() > 1 ? a.subList(1, a.size()).toArray(new String[0]) : new String[] {"entities", "all_players"});
         }
         default -> throw new IllegalArgumentException("unknown command " + cmd);
      }
      nameNewEntities();
      flushCommandOutput();
   }

   void command(String c) {
      v.command(c);
   }

   /** Entities created by game logic (drops, orbs, falling blocks, ...) get stable names `type_path#n` in creation order. */
   void nameNewEntities() {
      if (v == null) {
         return;
      }
      List<Entity> all = new ArrayList<>();
      for (Entity e : v.level.getAllEntities()) {
         all.add(e);
      }
      all.sort(Comparator.comparingInt(Entity::getId));
      for (Entity e : all) {
         if (known.add(e.getId()) && !refs.containsValue(e)) {
            String path = BuiltInRegistries.ENTITY_TYPE.getKey(e.getType()).getPath();
            refs.put(path + "#" + autoCounter.merge(path, 1, Integer::sum) , e);
         }
      }
   }

   void flushCommandOutput() {
      if (v != null) {
         for (String s : v.commandOutput) {
            out.println("C " + s);
         }
         v.commandOutput.clear();
      }
   }

   void tickOnce() {
      v.tick();
      nameNewEntities();
      if (!watches.isEmpty()) {
         out.println("T " + v.ticks);
         for (String[] w : watches) {
            dump(w);
         }
      }
   }

   void spawn(List<String> a) throws Exception {
      String name = a.get(1);
      VanillaLevel.reseed(hash(name));
      EntityType<?> type = BuiltInRegistries.ENTITY_TYPE.getValue(id(a.get(2)));
      Entity e = type.create(v.level, EntitySpawnReason.COMMAND);
      if (e == null) {
         throw new IllegalStateException("cannot create " + a.get(2));
      }
      int n = a.size();
      String snbt = n > 6 && (a.get(n - 1).startsWith("{")) ? a.get(--n) : null;
      e.snapTo(d(a.get(3)), d(a.get(4)), d(a.get(5)), n > 7 ? (float)d(a.get(6)) : 0f, n > 7 ? (float)d(a.get(7)) : 0f);
      if (snbt != null) {
         try (ProblemReporter.ScopedCollector rep = new ProblemReporter.ScopedCollector(e.problemPath(), org.slf4j.LoggerFactory.getLogger("oracle"))) {
            net.minecraft.world.level.storage.TagValueOutput o = net.minecraft.world.level.storage.TagValueOutput.createWithContext(rep, v.server.registryAccess());
            e.saveWithoutId(o);
            CompoundTag merged = o.buildResult().merge(TagParser.parseCompoundFully(snbt));
            e.load(TagValueInput.create(rep, v.server.registryAccess(), merged));
         }
      }
      v.level.addFreshEntityWithPassengers(e);
      refs.put(name, e);
   }

   void give(Entity target, String slot, ItemStack st) {
      if (target instanceof Player p && slot.startsWith("hotbar")) {
         p.getInventory().setItem(Integer.parseInt(slot.substring(6)), st);
      } else if (target instanceof Player p && slot.startsWith("inv")) {
         p.getInventory().setItem(Integer.parseInt(slot.substring(3)), st);
      } else {
         EquipmentSlot es = switch (slot) {
            case "mainhand" -> EquipmentSlot.MAINHAND;
            case "offhand" -> EquipmentSlot.OFFHAND;
            case "head" -> EquipmentSlot.HEAD;
            case "chest" -> EquipmentSlot.CHEST;
            case "legs" -> EquipmentSlot.LEGS;
            case "feet" -> EquipmentSlot.FEET;
            default -> throw new IllegalArgumentException("slot " + slot);
         };
         ((LivingEntity)target).setItemSlot(es, st);
      }
   }

   void set(Entity e, String what, String value) {
      switch (what) {
         case "health" -> ((LivingEntity)e).setHealth(Float.parseFloat(value));
         case "food" -> ((Player)e).getFoodData().setFoodLevel(Integer.parseInt(value));
         case "saturation" -> ((Player)e).getFoodData().setSaturation(Float.parseFloat(value));
         case "exhaustion" -> priv(((Player)e).getFoodData(), "exhaustionLevel", Float.parseFloat(value));
         case "air" -> e.setAirSupply(Integer.parseInt(value));
         case "fire" -> e.setRemainingFireTicks(Integer.parseInt(value));
         case "fall_distance" -> e.fallDistance = d(value);
         case "xp_level" -> ((ServerPlayer)e).setExperienceLevels(Integer.parseInt(value));
         case "absorption" -> ((LivingEntity)e).setAbsorptionAmount(Float.parseFloat(value));
         case "invulnerable" -> priv(e, Entity.class, "permanentlyInvulnerable", Boolean.parseBoolean(value));
         case "no_gravity" -> e.setNoGravity(Boolean.parseBoolean(value));
         case "tag" -> e.addTag(value);
         default -> throw new IllegalArgumentException("set " + what);
      }
   }

   void dig(List<String> a) {
      ServerPlayer p = player(a.get(1));
      BlockPos pos = new BlockPos(Integer.parseInt(a.get(2)), Integer.parseInt(a.get(3)), Integer.parseInt(a.get(4)));
      int limit = a.size() > 5 ? Integer.parseInt(a.get(5)) : -1;
      Direction face = Direction.UP;
      var conn = p.connection;
      conn.handlePlayerAction(new ServerboundPlayerActionPacket(ServerboundPlayerActionPacket.Action.START_DESTROY_BLOCK, pos, face, seq(a.get(1))));
      // MultiPlayerGameMode.startDestroyBlock: survival client breaks instantly when the per-tick progress is >= 1
      BlockState st = v.level.getBlockState(pos);
      boolean creative = p.isCreative();
      float progress = 0f;
      int ticks = 0;
      boolean stopped = creative;
      if (!creative && !st.isAir() && st.getDestroyProgress(p, v.level, pos) >= 1.0f) {
         stopped = true; // client destroyBlock; the START packet already broke it server side
      }
      while (!stopped) {
         ticks++;
         // client tick: continueDestroyBlock accumulates progress, then sends STOP at >= 1
         float step = v.level.getBlockState(pos).getDestroyProgress(p, v.level, pos);
         if (System.getenv("ORACLE_DEBUG") != null && System.getenv("ORACLE_DEBUG").equals("2") ? step != 0.06666667f && step != 0.24444444f : System.getenv("ORACLE_DEBUG") != null) {
            VanillaLevel.ERR.println("tick " + ticks + " step=" + step + " og=" + p.onGround() + " y=" + p.getY() + " inWater=" + p.isEyeInFluid(net.minecraft.tags.FluidTags.WATER));
         }
         progress += step;
         if (limit >= 0 ? ticks >= limit : progress >= 1.0f) {
            if (System.getenv("ORACLE_DEBUG") != null) {
               var gm = p.gameMode;
               VanillaLevel.ERR.println("STOP gameTicks=" + priv(gm, "gameTicks") + " start=" + priv(gm, "destroyProgressStart") + " destroying=" + priv(gm, "isDestroyingBlock") + " pos=" + priv(gm, "destroyPos") + " serverProgress=" + v.level.getBlockState(pos).getDestroyProgress(p, v.level, pos) + " clientProgress=" + progress + " og=" + p.onGround());
            }
            conn.handlePlayerAction(new ServerboundPlayerActionPacket(ServerboundPlayerActionPacket.Action.STOP_DESTROY_BLOCK, pos, face, seq(a.get(1))));
            stopped = true;
            tickOnce();
            break;
         }
         tickOnce();
         if (ticks > 100000) {
            throw new IllegalStateException("dig never finishes");
         }
      }
      out.println("DIG " + a.get(1) + " ticks=" + ticks + " broken=" + v.level.getBlockState(pos).isAir());
   }

   // ------------------------------------------------------------------------------------------------ dumps

   void dump(String[] what) {
      for (String w : what) {
         String kind = w.contains(":") ? w.substring(0, w.indexOf(':')) : w;
         String arg = w.contains(":") ? w.substring(w.indexOf(':') + 1) : null;
         switch (kind) {
            case "player" -> describe(arg, ref(arg));
            case "all_players" -> {
               for (VanillaLevel.FakePlayer p : v.players) {
                  describe(p.name, p.player);
               }
            }
            case "entity" -> describe(arg, ref(arg));
            case "entities" -> {
               nameNewEntities();
               List<Map.Entry<String, Entity>> all = new ArrayList<>();
               for (var en : refs.entrySet()) {
                  if (!(en.getValue() instanceof ServerPlayer) && en.getValue().isAlive()) {
                     all.add(en);
                  }
               }
               all.sort(Comparator.comparingInt(en -> en.getValue().getId()));
               for (var en : all) {
                  describe(en.getKey(), en.getValue());
               }
            }
            case "inventory" -> inventory(arg, (Player)ref(arg));
            case "packets" -> {
               VanillaLevel.FakePlayer p = v.player(arg);
               for (VanillaLevel.Sent s : p.drain()) {
                  out.println("PKT " + arg + " " + s.type() + " " + s.hex());
               }
            }
            case "events" -> {
               for (String e : VanillaLevel.EVENTS) {
                  out.println(e);
               }
               VanillaLevel.EVENTS.clear();
            }
            case "be" -> {
               String[] c = arg.split(",");
               BlockPos pos = new BlockPos(Integer.parseInt(c[0]), Integer.parseInt(c[1]), Integer.parseInt(c[2]));
               net.minecraft.world.level.block.entity.BlockEntity be = v.level.getBlockEntity(pos);
               out.println("BE " + c[0] + " " + c[1] + " " + c[2] + " " + (be == null ? "-" : be.saveWithFullMetadata(v.server.registryAccess()).toString()));
            }
            case "ticks" -> {
               // every pending scheduled tick of the loaded chunks in execution order (trigger tick, priority, sub tick order)
               List<net.minecraft.world.ticks.ScheduledTick<?>> all = new ArrayList<>();
               for (int cx = -options.preloadRadiusChunks; cx <= options.preloadRadiusChunks; cx++) {
                  for (int cz = -options.preloadRadiusChunks; cz <= options.preloadRadiusChunks; cz++) {
                     var chunk = v.level.getChunk(cx, cz);
                     ((net.minecraft.world.ticks.LevelChunkTicks<?>)priv(chunk, net.minecraft.world.level.chunk.LevelChunk.class, "blockTicks")).getAll().forEach(all::add);
                     ((net.minecraft.world.ticks.LevelChunkTicks<?>)priv(chunk, net.minecraft.world.level.chunk.LevelChunk.class, "fluidTicks")).getAll().forEach(all::add);
                  }
               }
               all.sort(net.minecraft.world.ticks.ScheduledTick.DRAIN_ORDER);
               for (var t : all) {
                  String tk = t.type() instanceof net.minecraft.world.level.block.Block ? "block" : "fluid";
                  String name = t.type() instanceof net.minecraft.world.level.block.Block b ? BuiltInRegistries.BLOCK.getKey(b).toString() : BuiltInRegistries.FLUID.getKey((net.minecraft.world.level.material.Fluid)t.type()).toString();
                  out.println("TICK " + tk + " " + t.pos().getX() + " " + t.pos().getY() + " " + t.pos().getZ() + " " + name + " delay=" + (t.triggerTick() - v.level.getGameTime()) + " prio=" + t.priority().getValue());
               }
            }
            case "blocks" -> {
               String[] c = arg.split(",");
               int[] n = new int[6];
               for (int i = 0; i < 6; i++) {
                  n[i] = Integer.parseInt(c[i]);
               }
               for (int x = Math.min(n[0], n[3]); x <= Math.max(n[0], n[3]); x++) {
                  for (int y = Math.min(n[1], n[4]); y <= Math.max(n[1], n[4]); y++) {
                     for (int z = Math.min(n[2], n[5]); z <= Math.max(n[2], n[5]); z++) {
                        BlockState st = v.level.getBlockState(new BlockPos(x, y, z));
                        if (!st.isAir()) {
                           out.println("B " + x + " " + y + " " + z + " " + BlockStateParser.serialize(st));
                        }
                     }
                  }
               }
            }
            default -> throw new IllegalArgumentException("dump " + w);
         }
      }
   }

   static Object priv(Object o, String name) {
      return priv(o, o.getClass(), name);
   }

   static Object priv(Object o, Class<?> c, String name) {
      try {
         var f = c.getDeclaredField(name);
         f.setAccessible(true);
         return f.get(o);
      } catch (ReflectiveOperationException ex) {
         throw new IllegalStateException(ex);
      }
   }

   static void priv(Object o, String name, Object value) {
      priv(o, o.getClass(), name, value);
   }

   static void priv(Object o, Class<?> c, String name, Object value) {
      try {
         var f = c.getDeclaredField(name);
         f.setAccessible(true);
         f.set(o, value);
      } catch (ReflectiveOperationException ex) {
         throw new IllegalStateException(ex);
      }
   }

   static String vec(double x, double y, double z) {
      return f(x) + "," + f(y) + "," + f(z);
   }

   static String item(ItemStack s) {
      if (s.isEmpty()) {
         return "empty";
      }
      StringBuilder b = new StringBuilder(BuiltInRegistries.ITEM.getKey(s.getItem()).toString()).append('*').append(s.getCount());
      if (s.isDamageableItem() && s.getDamageValue() != 0) {
         b.append("/dmg=").append(s.getDamageValue());
      }
      return b.toString();
   }

   void describe(String name, Entity e) {
      StringBuilder b = new StringBuilder(e instanceof Player ? "P " : "E ").append(name).append(' ').append(BuiltInRegistries.ENTITY_TYPE.getKey(e.getType()));
      b.append(" pos=").append(vec(e.getX(), e.getY(), e.getZ()));
      Vec3 dm = e.getDeltaMovement();
      b.append(" delta=").append(vec(dm.x, dm.y, dm.z));
      b.append(" rot=").append(f(e.getYRot())).append(',').append(f(e.getXRot()));
      b.append(" og=").append(e.onGround() ? 1 : 0);
      b.append(" fall=").append(f(e.fallDistance));
      b.append(" fire=").append(e.getRemainingFireTicks());
      b.append(" air=").append(e.getAirSupply());
      b.append(" alive=").append(e.isAlive() ? 1 : 0);
      b.append(" pose=").append(e.getPose().name().toLowerCase(Locale.ROOT));
      if (e instanceof LivingEntity le) {
         b.append(" hp=").append(f(le.getHealth()));
         b.append(" abs=").append(f(le.getAbsorptionAmount()));
         b.append(" hurt=").append(le.hurtTime).append('/').append(le.damageCooldownTime);
         b.append(" death=").append(le.deathTime);
         b.append(" fx=");
         List<MobEffectInstance> fx = new ArrayList<>(le.getActiveEffects());
         fx.sort(Comparator.comparing(i -> i.getEffect().getRegisteredName()));
         if (fx.isEmpty()) {
            b.append('-');
         }
         for (int i = 0; i < fx.size(); i++) {
            MobEffectInstance m = fx.get(i);
            b.append(i > 0 ? ";" : "").append(m.getEffect().getRegisteredName()).append('/').append(m.getAmplifier()).append('/').append(m.getDuration());
         }
      }
      if (e instanceof Player p) {
         b.append(" gm=").append(((ServerPlayer)p).gameMode.getGameModeForPlayer().getName());
         b.append(" food=").append(p.getFoodData().getFoodLevel());
         b.append(" sat=").append(f(p.getFoodData().getSaturationLevel()));
         b.append(" exh=").append(f((Float)priv(p.getFoodData(), "exhaustionLevel")));
         b.append(" xp=").append(p.experienceLevel).append('/').append(p.totalExperience).append('/').append(f(p.experienceProgress));
         b.append(" sel=").append(p.getInventory().getSelectedSlot());
         b.append(" main=").append(item(p.getMainHandItem()));
      } else if (e instanceof ItemEntity ie) {
         b.append(" item=").append(item(ie.getItem()));
         b.append(" age=").append(ie.getAge());
      } else if (e instanceof net.minecraft.world.entity.ExperienceOrb o) {
         b.append(" value=").append(o.getValue());
      } else if (e instanceof net.minecraft.world.entity.item.PrimedTnt t) {
         b.append(" fuse=").append(t.getFuse());
      }
      if (e instanceof ItemEntity ie) {
         b.append(" pickup=").append(priv(ie, "pickupDelay")).append(" health=").append(priv(ie, "health"));
         b.append(" owner=").append(uuidName((UUID)priv(ie, "target")));
         var th = (net.minecraft.world.entity.EntityReference<?>)priv(ie, "thrower");
         b.append(" thrower=").append(uuidName(th == null ? null : th.getUUID()));
      } else if (e instanceof net.minecraft.world.entity.ExperienceOrb o) {
         b.append(" count=").append(priv(o, "count")).append(" age=").append(priv(o, "age")).append(" health=").append(priv(o, "health"));
      } else if (e instanceof net.minecraft.world.entity.item.FallingBlockEntity fb) {
         b.append(" state=").append(BlockStateParser.serialize(fb.getBlockState())).append(" time=").append(fb.time);
         b.append(" hurt=").append(priv(fb, "hurtEntities")).append(" cancel=").append(priv(fb, "cancelDrop")).append(" drop=").append(fb.dropItem);
      }
      out.println(b);
   }

   String uuidName(UUID u) {
      if (u == null) {
         return "-";
      }
      for (var en : refs.entrySet()) {
         if (en.getValue().getUUID().equals(u)) {
            return en.getKey();
         }
      }
      return u.toString();
   }

   void inventory(String name, Player p) {
      for (int i = 0; i < p.getInventory().getContainerSize(); i++) {
         ItemStack s = p.getInventory().getItem(i);
         if (!s.isEmpty()) {
            out.println("I " + name + " " + i + " " + item(s));
         }
      }
   }
}
