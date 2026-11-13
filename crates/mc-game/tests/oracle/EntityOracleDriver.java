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
import java.lang.reflect.Method;
import net.minecraft.core.GlobalPos;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.ListTag;
import net.minecraft.nbt.NbtOps;
import net.minecraft.nbt.Tag;
import net.minecraft.resources.RegistryOps;
import net.minecraft.server.level.ServerChunkCache;
import net.minecraft.world.Difficulty;
import net.minecraft.world.DifficultyInstance;
import net.minecraft.world.entity.AgeableMob;
import net.minecraft.world.entity.Leashable;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.entity.MobCategory;
import net.minecraft.world.entity.TamableAnimal;
import net.minecraft.world.entity.ai.Brain;
import net.minecraft.world.entity.ai.attributes.AttributeInstance;
import net.minecraft.world.entity.ai.behavior.BehaviorControl;
import net.minecraft.world.entity.ai.behavior.EntityTracker;
import net.minecraft.world.entity.ai.behavior.PositionTracker;
import net.minecraft.world.entity.ai.goal.Goal;
import net.minecraft.world.entity.ai.goal.GoalSelector;
import net.minecraft.world.entity.ai.goal.WrappedGoal;
import net.minecraft.world.entity.ai.memory.MemoryModuleType;
import net.minecraft.world.entity.ai.memory.WalkTarget;
import net.minecraft.world.entity.ai.navigation.PathNavigation;
import net.minecraft.world.entity.animal.Animal;
import net.minecraft.world.entity.npc.villager.AbstractVillager;
import net.minecraft.world.entity.raid.Raid;
import net.minecraft.world.entity.schedule.Activity;
import net.minecraft.world.item.trading.MerchantOffer;
import net.minecraft.world.item.trading.MerchantOffers;
import net.minecraft.world.level.LightLayer;
import net.minecraft.world.level.LocalMobCapCalculator;
import net.minecraft.world.level.NaturalSpawner;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.pathfinder.Node;
import net.minecraft.world.level.pathfinder.Path;
import net.minecraft.util.RandomSource;

/**
 * Scenario driver of the in-process vanilla oracle ({@link VanillaLevel}).
 * Usage: scripts/entity-oracle.sh crates/mc-game/tests/oracle/scenarios/NAME.txt -o crates/mc-game/tests/fixtures/entity_oracle/NAME.txt
 * Output: crates/mc-game/tests/fixtures/entity_oracle/NAME.txt (scripts/entity-oracle.sh --regen rewrites all of them)
 * <p>The scenario is a line-based script, the fixture is the text this driver prints (stdout; vanilla log noise goes to stderr).
 * Commit scenario + fixture (a scenario over 1 MB is committed as NAME.txt.gz); Rust reads the fixture with {@code tests/common/entity_oracle.rs}.
 *
 * <p>Syntax: one command per line, tokens split on whitespace ({@code [..]}, {@code {..}} and "..." keep spaces), {@code #}
 * starts a comment line. {@code <ref>} is a player name (from spawn_player) or the name given to spawn/spawn_item; every
 * number is Java-parsed ({@code 1.5}, {@code -3}); coordinates are absolute. Flat world: the surface block is at y=-61
 * (bedrock -64, dirt -63..-62, grass -61), so a standing player has y=-60.
 *
 * <pre>
 * -- options (only before the first world command) --
 * level flat|void|normal      world type (default flat)            seed N              world seed (default 4242)
 * dimension overworld|the_nether|the_end   the level every world command acts on (default overworld; the nether and end keep their real generator)
 * difficulty peaceful|easy|normal|hard (default normal)            preload R           chunk radius kept loaded (6)
 * features NAME[,NAME]       extra feature flags besides vanilla, e.g. redstone_experiments       structures true|false   `generate-structures` (default false: a normal world has no structures)
 * -- world --
 * gamerule NAME VALUE         e.g. gamerule keep_inventory true    time N       (game clock stays frozen otherwise)
 * block X Y Z STATE           e.g. block 0 -60 0 minecraft:water[level=0]   (setBlock flags 3, like /setblock)
 * fill X1 Y1 Z1 X2 Y2 Z2 STATE
 * cmd COMMAND...              any vanilla command as the console (output appears as "C ..." lines)
 * note TEXT...                prints "# TEXT"
 * reseed N                    restart the entity random-seed sequence (entities created afterwards: see VanillaLevel header)
 * level_seed N                Level.random.setSeed(N) (explosion rays, drop spread, ... draw from it; LegacyRandomSource)
 * entity_counter N            ServerLevel.ENTITY_COUNTER = N (the next created entity gets id N+1; ids are not printed)
 * award X Y Z AMOUNT [dx dy dz]   ExperienceOrb.award / awardWithDirection (orb split, merge into existing orbs)
 * rand_value N                Level.randValue = N (the LCG getBlockRandomPos draws from; set it before random ticks)
 * tick_chunk CX CZ SPEED      ServerLevel.tickChunk(chunk, SPEED): random ticks of one chunk (use instead of gamerule random_tick_speed)
 * random_tick X Y Z           randomTick of that block (+ its fluid) as tickChunk's inner loop does for one chosen position
 * bonemeal X Y Z            BoneMealItem.growCrop without a player; prints `BONEMEAL true|false` (true = the block was a valid target)
 * destroy X Y Z              Level.destroyBlock(pos, true) (no breaker: destroy particles event, drops, BLOCK_DESTROY); prints `DESTROY true|false`
 * command_env               (before the first world command) no-op here, the Rust replay installs a command environment (server access) for the scenario
 * jigsaw_generate PLAYER X Y Z LEVELS KEEP   ServerboundJigsawGeneratePacket from an operator in creative mode (`op PLAYER`): JigsawBlockEntity.generate(level, LEVELS, KEEP=true|false)
 * be_nbt X Y Z SNBT           merge SNBT into the block entity (`/data merge block`: saveWithFullMetadata.merge, loadWithComponents, setChanged)
 * -- entities --
 * spawn REF TYPE X Y Z [yaw pitch] [SNBT]        real vanilla entity (EntityType.create + addFreshEntity), random seeded from REF
 * spawn_item REF STACK X Y Z [vx vy vz] [count] [rot=YAW]  ItemEntity (constructor random motion unless vx vy vz given; rot= overrides the random yaw)
 * entity_seed REF N           entity.getRandom().setSeed(N)
 * spawn_player NAME survival|creative|adventure|spectator X Y Z [yaw pitch] [sim] [joinburst]
 *                             joins a real ServerPlayer through PlayerList.placeNewPlayer; `sim` = the server simulates its
 *                             physics (client-side LocalPlayer behaviour, see VanillaLevel.SimPlayer), otherwise it only moves
 *                             through `move_packet`s like a real vanilla server player; `joinburst` keeps the packets of the join for the
 *                             first `packets:NAME` dump (otherwise they are discarded): compare them with a type filter (`:set_held_slot,update_recipes,recipe_book_settings,recipe_book_add,container_set_content`)
 * give REF SLOT STACK [count] SLOT: mainhand offhand head chest legs feet hotbarN (0-8) invN (container slot index)
 * select PLAYER N             hotbar slot (ServerboundSetCarriedItemPacket)
 * effect REF EFFECT AMPLIFIER TICKS [hidden]      addEffect
 * attribute REF ATTRIBUTE BASEVALUE               setBaseValue
 * set REF health|food|saturation|exhaustion|air|fire|fall_distance|ticks_frozen|xp_level|enchant_seed|absorption|invulnerable|no_gravity|on_ground|tag VALUE
 * nbt REF SNBT                merge SNBT into the entity (Entity.load of the merged save data)
 * signal REF X Y Z           EyeOfEnder.signalTo(target)
 * snap REF X Y Z [yaw pitch]  Entity.snapTo (no packets)          tp REF X Y Z [yaw pitch]   teleportTo
 * velocity REF X Y Z          setDeltaMovement
 * kill REF                    discard (no death logic)
 * -- inputs / actions (vanilla packet handlers of the player's real ServerGamePacketListenerImpl) --
 * input PLAYER FLAGS          sim player: forward,back,left,right,jump,sneak,sprint (comma list) or `none`; applied each tick
 * ride REF VEHICLE           REF.startRiding(VEHICLE, true, true)
 * track_movement PLAYER      sim player: from now on its known movement (`getKnownMovement`/`getKnownSpeed`, what a client reports with its move packets) is the position change
 *                             of the previous tick, set before each tick (kinetic weapons, sweep attacks and projectile deflection read it); without it a sim player reports none
 * op PLAYER                   PlayerList.op (the server's operator permissions)
 * pick_block PLAYER X Y Z [true|false]   ServerboundPickItemFromBlockPacket (the flag = include block data, creative only)
 * move_packet PLAYER X Y Z ONGROUND [yaw pitch]   ServerboundMovePlayerPacket (server-authoritative path)
 * dig PLAYER X Y Z [TICKS]    client-like digging: START_DESTROY_BLOCK, per tick client progress, STOP_DESTROY_BLOCK when the
 *                             client would send it (or after TICKS ticks); prints "DIG ticks=N broken=B"; ticks the server
 * use PLAYER [main|off]       ServerboundUseItemPacket              release PLAYER   RELEASE_USE_ITEM
 * use_on PLAYER X Y Z FACE [main|off] [cx cy cz] [inside]  ServerboundUseItemOnPacket (FACE: down up north south west east; `inside` = BlockHitResult.inside, the click was inside the block)
 * swap PLAYER                SWAP_ITEM_WITH_OFFHAND (also stops using the item)         stab PLAYER   STAB (spear attack)
 * dispense X Y Z              BlockStateBase.tick of the block there (a dispenser/dropper dispenses once: no neighbour signal, no 4-tick delay)
 * menu_button PLAYER N       ServerboundContainerButtonClickPacket for the player's open menu (lectern: 1 previous page, 2 next page, 3 take the book, 100+N page N)
 * close_menu PLAYER           ServerboundContainerClosePacket for the player's open menu
 * sign_update PLAYER X Y Z front|back L1 L2 L3 L4   ServerboundSignUpdatePacket (each line one token: "quote" it to keep spaces; missing = "")
 * brush X Y Z PLAYER DIR    BrushableBlockEntity.brush(gameTime, level, PLAYER, DIR, main hand item) called directly; prints "BRUSH x y z result"
 * attack PLAYER REF           ServerboundAttackPacket               swing PLAYER   ServerboundPunchPacket (main hand)
 * client_tick_end PLAYER    ServerboundClientTickEndPacket (re-arms the one position packet per tick; zeroes the known movement without one)
 * mob_attack MOB REF        Mob.doHurtTarget(level, REF); prints "MOBATTACK mob ref -> result"
 * paddle PLAYER LEFT RIGHT   ServerboundPaddleBoatPacket (true|false)       move_vehicle PLAYER X Y Z YROT XROT ONGROUND   ServerboundMoveVehiclePacket (the player controls the vehicle it rides)
 * accept_teleport PLAYER    ServerboundAcceptTeleportationPacket for the teleport the server waits on (mounting a vehicle teleports the rider; its movement
 *                             packets are ignored until the client acknowledged it, as a real client does at once); nothing when none is pending
 * drop PLAYER [all]           DROP_ITEM / DROP_ALL_ITEMS            respawn PLAYER   ServerboundClientCommandPacket
 * pcommand PLAYER ACTION [data]   ServerboundPlayerCommandPacket (START_SPRINTING STOP_SPRINTING START_FALL_FLYING STOP_SLEEPING START_RIDING_JUMP [power]
 *                             STOP_RIDING_JUMP OPEN_INVENTORY ...)
 * hurt REF DAMAGETYPE AMOUNT [by=REF] [direct=REF] [at=X,Y,Z]    entity.hurtServer(level, source, amount); prints "HURT result"
 * explode X Y Z RADIUS [fire] [none|block|mob|tnt|trigger] [source=REF]    Level.explode
 * -- mobs (added for the mob goldens; every `E` line of a Mob is only extended when dumped through `mob:REF` / `mobs`, so the older
 *    `entity:`/`entities` output and all committed fixtures are unchanged; the Rust replays run these verbs and dumps through one shared module
 *    (`tests/golden/common/mob_verbs.rs`) on both the flat entity tier and the ticking block tier, so a script may use them on either) --
 * difficulty NAME             before the first world command: world option (as above); afterwards: `/difficulty NAME`
 * biome X1 Y1 Z1 X2 Y2 Z2 BIOME   `/fillbiome` (biome overrides; the flat world is plains)       light X Y Z N   minecraft:light[level=N] block
 * mobspawn REF TYPE X Y Z [yaw pitch] [reason=NAME] [nofinalize] [SNBT]
 *                             EntityType.create(level, reason) (default reason COMMAND), merge SNBT, then `Mob.finalizeSpawn(level,
 *                             getCurrentDifficultyAt(pos), reason, null)` like /summon, addFreshEntityWithPassengers. `nofinalize` skips it.
 * target REF REF2|-           Mob.setTarget            love REF [PLAYER]   Animal.setInLove(player|null)      age REF N   AgeableMob.setAge
 * breed A B                   A.spawnChildFromBreeding(level, B) (the BreedGoal path; baby = `type#n`)        tame REF PLAYER   TamableAnimal.tame / AbstractHorse.tameWithName
 * leash REF HOLDER|-          Leashable.setLeashedTo(holder, true) / removeLeash()
 * nav REF X Y Z [speed] [reach]   Mob navigation moveTo(x,y,z,reach,speed) (reach default 1, speed default 1.0); prints `NAV` + the path (`PATH`/`PN` lines)
 * path_query REF X Y Z [reach]    navigation.createPath(x,y,z,reach) WITHOUT moving (prints `PATHQ` + path)         nav_stop REF   navigation.stop()
 * goal REF / brain REF / path REF   = dump goals:REF / brain:REF / path:REF
 * trades REF PROFESSION LEVEL SEED [type=plains]   villager REF with that profession/level, entity random seeded with SEED, offers generated by
 *                             getOffers() and dumped (`OFFER` lines)           offers REF   dump the (generated) offers of REF
 * spawnpass SEED [chunk=CX,CZ] [cats=monster,creature,..] [enemies=B] [persistent=B]
 *                             Level.random.setSeed(SEED); NaturalSpawner.createState (like ServerChunkCache.tickChunks) + ONE
 *                             spawnForChunk pass for the chunk (default 0,0; categories default getFilteredSpawningCategories; persistent
 *                             default gameTime%400==0). Spawned mobs are named `type#n` and printed as rich `E` lines, then `SPAWNSTATE`.
 * raid_waves DIFFICULTY OMEN SEED   composition of every Raid wave (+ bonus wave when omen>1) with Raid's own RNG seeded: `RAID_WAVE` lines
 * raid_start PLAYER X Y Z [omen=N]  Raids.createOrExtendRaid (a village = POI blocks (bell, beds) placed by the script; the player needs
 *                             the raid_omen effect to carry an omen level; omen=N overrides)       dump raids   all raids (`RAID` lines)
 * interact PLAYER REF [main|off] [sneak] [center] [at X Y Z]   ServerboundInteractPacket (use the entity: board a boat, open a chest boat, feed, tame); location = entity origin, `center` = bounding box centre relative to the origin, `at X Y Z` = that offset from the origin
 * set_command_minecart PLAYER REF COMMAND [true|false]   ServerboundSetCommandMinecartPacket (the editor of a command block minecart; "quote" COMMAND to keep spaces; the flag = track output, default true)
 * trade_select PLAYER N       ServerboundSelectTradePacket
 * click, creative_slot, rename, select_trade, set_beacon, slot_state, place_recipe, recipe_seen, recipe_settings, `dump menu:PLAYER`, `dump recipes:PLAYER`: see MenuVerbs.java
 * local difficulty / light / time of day: `dump difficulty:X,Y,Z` (`DIFFICULTY ..`), `dump light:X,Y,Z` (`LIGHT x y z block=B sky=S`, after ticks),
 *                             `time N` (day time, `/time set`; the sky darkens on the next tick), `gamerule`.
 * -- time --
 * tick [N]                    N real server ticks (default 1); each prints the `watch` dumps after the tick
 * watch WHAT...               dump after every tick: player:REF entity:REF entities inventory:REF packets:REF[:type,..|:!type,..]   (watch off clears;
 *                             `packets` drains the player's packets; `:type,..` keeps only those, `:!type,..` drops those; `:none` drains silently;
 *                             runs of chunk change packets are sorted by chunk, see chunkOrder)
 * dump WHAT...                same WHATs immediately (`stats:PLAYER` = `STAT PLAYER type.name:value.name N` per earned statistic, sorted; `advancements:PLAYER` = `ADV PLAYER id criterion,..` per advancement with completed criteria, sorted;
 *                             `use:PLAYER` = `USE PLAYER hand=main|off item=ID*COUNT remaining=useItemRemaining ticks=getTicksUsingItem kinetic=recentKineticEnemies.size`; `cooldowns:PLAYER` = `CD PLAYER group=ID remaining=endTime-tickCount` per running cooldown, sorted by group), plus blocks:X1,Y1,Z1,X2,Y2,Z2   (`dump` alone = everything)
 *                             block oracle extras: be:X,Y,Z -> `BE x y z SNBT` (saveWithFullMetadata, sorted keys; `-` = none),
 *                             beu:X,Y,Z -> `BEU x y z TYPE SNBT` (getUpdatePacket tag; `-` = none),
 *                             bes:CX,CZ -> `BES cx cz`, `BEM x y z type` per block entity in map iteration order, then `BET x y z` / `BETP x y z`
 *                             (ticker list / pending ticker list order) for the tickers inside that chunk,
 *                             chunkbes:CX,CZ -> `BEP x y z type` per block entity of the chunk's `ClientboundLevelChunkPacketData`, in packet order,
 *                             ticks[:X1,Y1,Z1,X2,Y2,Z2] -> `TICK block|fluid x y z id delay=N prio=P` for every pending scheduled tick (inside the optional box) in execution order,
 *                             events -> the `EV ...` lines of level events / sounds / block events broadcast since the last dump (see
 *                             VanillaLevel.RecordingPlayerList; also valid in `watch`, printed after each tick)

 * dump/watch kinds for mobs: mob:REF (rich E line), mobs (all live Mobs, rich), attrs:REF, goals:REF (`G` lines, every registered goal), brain:REF
 *   (`BRAIN`/`BM`/`BR` lines), path:REF (`PATH`/`PN`), nbt:REF (`N REF {canonical sorted SNBT, UUID removed}`), offers:REF, trader:REF
 *   (`TRADER REF xp=villagerXp level=villagerLevel|- trading=REF|-`), difficulty:X,Y,Z, light:X,Y,Z, spawn_state (`SPAWNSTATE`), raids.
 * dump/watch kinds for every entity: xentity:REF / xentities (all live non-player entities): the normal `E` line + the fields of the classes registered in
 *   EntityExtras.java (`mob:REF` also carries them); edata:REF = `ED REF id:serializer:hex ..` for every synched data item (accessor id, serializer id, wire form of the value);
 *   rng:level / rng:REF = `RNG level|REF STATE` (the 48-bit state of Level.random / of the entity's own random).
 * </pre>
 *
 * <p>Output lines: {@code T n} (tick counter header before per-tick dumps), {@code P ...} player, {@code E ...} entity,
 * {@code I ...} inventory slot, {@code PKT player type hex} (hex = id + payload; `update_advancements` prints `-` (timestamps), `level_chunk_with_light` prints
 * `chunk=x,z/len=n` (heightmap map order varies per JVM run), `update_attributes` prints `entity=ID;attr=base[mod/amount/op,..];..` sorted
 * (HashSet order varies per JVM run)), {@code B x y z state}, {@code C text} (command output),
 * {@code DIG/HURT ...} action results (entities created by game logic are named {@code type_path#n}, n counting per type in
 * creation order, e.g. {@code item#1}, {@code experience_orb#2}; `dump entities` lists all live non-player entities by name), {@code # ...} notes. Numbers use Java's Double/Float.toString (shortest round trip;
 * the Rust comparator parses numeric tokens instead of comparing text). Entity ids are NOT printed (not stable across servers).
 *
 * <p>Rich mob `E` line = the normal `E` line + (in this order) {@code target= hurtby= aggr= noai= left= persist= canpickup= home=
 * leash= speed= zza= xxa= yya= head= body= nav= stuck= navspeed= ntarget= path= nodes= goals= tgoals= acts= mem= equip= attrs=},
 * then {@code age= baby= forcedage=} (AgeableMob), {@code love= lovecause=} (Animal), {@code tame= owner= sitting= ordered=}
 * (TamableAnimal), {@code vdata= vxp=} (Villager: type/profession/level, xp), {@code horse...} none yet.
 * Entity refs print as the scenario name (`-` = none). {@code path=next/count/reached/target}, {@code nodes=x,y,z:TYPE:malus;..},
 * {@code goals=PRIO:SimpleClassName:FLAGS;..} (RUNNING goals of the goal selector, registration order, FLAGS = MOVE+LOOK.. or none;
 * `tgoals` = target selector), {@code acts=} sorted active brain activities, {@code mem=name:value:ttl;..} sorted memories with a value
 * (ttl `inf` = never expires; BlockPos `x,y,z`, GlobalPos `dim@x,y,z`, entities by ref, booleans 0/1, collections `(a;b)`, other values toString),
 * {@code equip=slot:item;..} non-empty slots (slot names of EquipmentSlot.getName(), item as `id*count` + canonical SNBT of the component patch),
 * {@code attrs=id:value;..} the attributes of the mob's supplier whose final value differs from the attribute default, sorted by id (`dump attrs:REF` prints all: `ATTR REF id:value ..`). Unmodelled values print `-`.
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
      java.nio.file.Path scenario = java.nio.file.Path.of(args[0]);
      List<String> lines;
      if (args[0].endsWith(".gz")) {
         try (java.io.BufferedReader r = new java.io.BufferedReader(new java.io.InputStreamReader(
               new java.util.zip.GZIPInputStream(Files.newInputStream(scenario)), java.nio.charset.StandardCharsets.UTF_8))) {
            lines = r.lines().toList();
         }
      } else {
         lines = Files.readAllLines(scenario);
      }
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
         case "dimension" -> options.dimension = a.get(1);
         case "seed" -> options.seed = Long.parseLong(a.get(1));
         case "features" -> options.features = a.get(1);
         case "structures" -> options.generateStructures = Boolean.parseBoolean(a.get(1));
         case "difficulty" -> {
            if (v == null) {
               options.difficulty = a.get(1);
            } else {
               command("difficulty " + a.get(1));
            }
         }
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
         case "bonemeal" -> {
            world();
            BlockPos pos = new BlockPos(Integer.parseInt(a.get(1)), Integer.parseInt(a.get(2)), Integer.parseInt(a.get(3)));
            out.println("BONEMEAL " + net.minecraft.world.item.BoneMealItem.growCrop(new ItemStack(net.minecraft.world.item.Items.BONE_MEAL, 64), v.level, pos));
         }
         case "command_env" -> {
            // the real server always has its command environment; the Rust replay installs one for the whole scenario
         }
         case "jigsaw_generate" -> {
            world();
            ServerPlayer p = player(a.get(1));
            BlockPos pos = new BlockPos(Integer.parseInt(a.get(2)), Integer.parseInt(a.get(3)), Integer.parseInt(a.get(4)));
            p.connection.handleJigsawGenerate(new ServerboundJigsawGeneratePacket(pos, Integer.parseInt(a.get(5)), Boolean.parseBoolean(a.get(6))));
         }
         case "destroy" -> {
            world();
            BlockPos pos = new BlockPos(Integer.parseInt(a.get(1)), Integer.parseInt(a.get(2)), Integer.parseInt(a.get(3)));
            out.println("DESTROY " + v.level.destroyBlock(pos, true));
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
         case "entity_counter" -> { // ServerLevel.ENTITY_COUNTER: the next created entity gets N+1 (pins ids, which only the sleeping-item phase and orb merging observe)
            world();
            ((java.util.concurrent.atomic.AtomicInteger)priv(null, net.minecraft.server.level.ServerLevel.class, "ENTITY_COUNTER")).set(Integer.parseInt(a.get(1)));
         }
         case "award" -> { // award X Y Z AMOUNT [dx dy dz]: ExperienceOrb.award / awardWithDirection
            world();
            Vec3 at = new Vec3(d(a.get(1)), d(a.get(2)), d(a.get(3)));
            int amount = Integer.parseInt(a.get(4));
            if (a.size() >= 8) {
               net.minecraft.world.entity.ExperienceOrb.awardWithDirection(v.level, at, new Vec3(d(a.get(5)), d(a.get(6)), d(a.get(7))), amount);
            } else {
               net.minecraft.world.entity.ExperienceOrb.award(v.level, at, amount);
            }
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
            Float rot = null;
            a = new ArrayList<>(a);
            for (int i = a.size() - 1; i > 2; i--) {
               if (a.get(i).startsWith("rot=")) {
                  rot = Float.parseFloat(a.remove(i).substring(4)); // the constructor draws a random yaw that Rust cannot reproduce
               }
            }
            ItemStack st = stack(a.get(2), a.size() > 9 ? Integer.parseInt(a.get(9)) : 1);
            double x = d(a.get(3)), y = d(a.get(4)), z = d(a.get(5));
            ItemEntity ie = a.size() >= 9 ? new ItemEntity(v.level, x, y, z, st, d(a.get(6)), d(a.get(7)), d(a.get(8))) : new ItemEntity(v.level, x, y, z, st);
            if (rot != null) {
               ie.setYRot(rot);
            }
            v.level.addFreshEntity(ie);
            refs.put(a.get(1), ie);
         }
         case "entity_seed" -> ref(a.get(1)).getRandom().setSeed(Long.parseLong(a.get(2)));
         case "spawn_player" -> {
            world();
            GameType gm = GameType.byName(a.get(2));
            boolean sim = a.subList(6, a.size()).contains("sim");
            boolean joinburst = a.subList(6, a.size()).contains("joinburst");
            int n = a.size() - (sim ? 1 : 0) - (joinburst ? 1 : 0);
            VanillaLevel.reseed(hash(a.get(1)));
            VanillaLevel.FakePlayer p = v.connect(a.get(1), gm, d(a.get(3)), d(a.get(4)), d(a.get(5)), n > 6 ? (float)d(a.get(6)) : 0f, n > 7 ? (float)d(a.get(7)) : 0f, sim);
            refs.put(a.get(1), p.player);
            if (!joinburst) {
               p.drain(); // the join burst is deterministic but huge; `packets:NAME` dumps start after the join
            }
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
         case "signal" -> {
            world();
            ((net.minecraft.world.entity.projectile.EyeOfEnder)ref(a.get(1))).signalTo(new net.minecraft.world.phys.Vec3(d(a.get(2)), d(a.get(3)), d(a.get(4))));
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
         case "track_movement" -> {
            world();
            ((VanillaLevel.SimPlayer)player(a.get(1))).trackMovement = true;
         }
         case "ride" -> {
            world();
            ref(a.get(1)).startRiding(ref(a.get(2)), true, true);
         }
         case "op" -> {
            world();
            v.server.getPlayerList().op(player(a.get(1)).nameAndId());
         }
         case "pick_block" -> {
            world();
            ServerPlayer p = player(a.get(1));
            p.connection.handlePickItemFromBlock(new ServerboundPickItemFromBlockPacket(
               new BlockPos(Integer.parseInt(a.get(2)), Integer.parseInt(a.get(3)), Integer.parseInt(a.get(4))), a.size() > 5 && Boolean.parseBoolean(a.get(5))));
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
            List<String> u = new ArrayList<>(a);
            boolean inside = u.remove("inside");
            BlockPos pos = new BlockPos(Integer.parseInt(u.get(2)), Integer.parseInt(u.get(3)), Integer.parseInt(u.get(4)));
            Direction face = Direction.byName(u.get(5));
            boolean cur = u.size() > 9;
            Vec3 hit = cur ? new Vec3(pos.getX() + d(u.get(u.size() - 3)), pos.getY() + d(u.get(u.size() - 2)), pos.getZ() + d(u.get(u.size() - 1)))
               : Vec3.atCenterOf(pos).add(face.getStepX() * 0.5, face.getStepY() * 0.5, face.getStepZ() * 0.5);
            int handIdx = u.size() > 6 && (u.get(6).equals("main") || u.get(6).equals("off")) ? 6 : -1;
            p.connection.handleUseItemOn(new ServerboundUseItemOnPacket(handIdx > 0 ? hand(u, 6) : InteractionHand.MAIN_HAND, new BlockHitResult(hit, face, pos, inside), seq(a.get(1))));
         }
         case "swap" -> {
            world();
            player(a.get(1)).connection.handlePlayerAction(new ServerboundPlayerActionPacket(ServerboundPlayerActionPacket.Action.SWAP_ITEM_WITH_OFFHAND, BlockPos.ZERO, Direction.DOWN));
         }
         case "stab" -> {
            world();
            player(a.get(1)).connection.handlePlayerAction(new ServerboundPlayerActionPacket(ServerboundPlayerActionPacket.Action.STAB, BlockPos.ZERO, Direction.DOWN));
         }
         case "dispense" -> {
            world();
            BlockPos pos = new BlockPos(Integer.parseInt(a.get(1)), Integer.parseInt(a.get(2)), Integer.parseInt(a.get(3)));
            v.level.getBlockState(pos).tick(v.level, pos, v.level.getRandom());
         }
         case "menu_button" -> {
            world();
            ServerPlayer p = player(a.get(1));
            p.connection.handleContainerButtonClick(new ServerboundContainerButtonClickPacket(p.containerMenu.containerId, Integer.parseInt(a.get(2))));
         }
         case "close_menu" -> {
            world();
            ServerPlayer p = player(a.get(1));
            p.connection.handleContainerClose(new ServerboundContainerClosePacket(p.containerMenu.containerId));
         }
         case "sign_update" -> {
            world();
            List<String> lines = new ArrayList<>();
            for (int i = 6; i < 10; i++) {
               String l = a.size() > i ? a.get(i) : "";
               lines.add(l.length() >= 2 && l.startsWith("\"") && l.endsWith("\"") ? l.substring(1, l.length() - 1) : l);
            }
            player(a.get(1)).connection.handleSignUpdate(new ServerboundSignUpdatePacket(
               new BlockPos(Integer.parseInt(a.get(2)), Integer.parseInt(a.get(3)), Integer.parseInt(a.get(4))), lines,
               a.get(5).equals("front") ? net.minecraft.world.level.block.entity.SignTextSlot.FRONT : net.minecraft.world.level.block.entity.SignTextSlot.BACK));
         }
         case "brush" -> { // brush X Y Z PLAYER DIR: BrushableBlockEntity.brush(gameTime, level, player, DIR, main hand); prints "BRUSH x y z result"
            world();
            BlockPos pos = new BlockPos(Integer.parseInt(a.get(1)), Integer.parseInt(a.get(2)), Integer.parseInt(a.get(3)));
            ServerPlayer p = player(a.get(4));
            boolean r = v.level.getBlockEntity(pos) instanceof net.minecraft.world.level.block.entity.BrushableBlockEntity be
               && be.brush(v.level.getGameTime(), v.level, p, Direction.byName(a.get(5)), p.getMainHandItem());
            out.println("BRUSH " + a.get(1) + " " + a.get(2) + " " + a.get(3) + " " + r);
         }
         case "attack" -> {
            world();
            player(a.get(1)).connection.handleAttack(new ServerboundAttackPacket(ref(a.get(2)).getId()));
         }
         case "client_tick_end" -> {
            world();
            player(a.get(1)).connection.handleClientTickEnd(new ServerboundClientTickEndPacket());
         }
         case "mob_attack" -> {
            world();
            // Mob.doHurtTarget(level, target) (the melee attack of a mob; no AI, no attack animation)
            boolean r = ((net.minecraft.world.entity.Mob)ref(a.get(1))).doHurtTarget(v.level, ref(a.get(2)));
            out.println("MOBATTACK " + a.get(1) + " " + a.get(2) + " -> " + r);
         }
         case "interact" -> {
            world();
            // interact PLAYER REF [main|off] [sneak] [center] [at X Y Z]: ServerboundInteractPacket; location = entity origin, `center` = bounding box centre,
            // `at X Y Z` = that offset from the origin (an armor stand slot is picked by the click height)
            boolean sneak = a.contains("sneak");
            int handIdx = a.size() > 3 && (a.get(3).equals("main") || a.get(3).equals("off")) ? 3 : -1;
            Entity target = ref(a.get(2));
            int at = a.indexOf("at");
            Vec3 loc = at > 0 ? new Vec3(d(a.get(at + 1)), d(a.get(at + 2)), d(a.get(at + 3)))
               : a.contains("center") ? target.getBoundingBox().getCenter().subtract(target.position()) : Vec3.ZERO;
            player(a.get(1)).connection.handleInteract(new ServerboundInteractPacket(target.getId(), handIdx > 0 ? hand(a, 3) : InteractionHand.MAIN_HAND, loc, sneak));
         }
         case "set_command_minecart" -> {
            world();
            String command = a.get(3);
            if (command.length() >= 2 && command.startsWith("\"") && command.endsWith("\"")) {
               command = command.substring(1, command.length() - 1);
            }
            player(a.get(1)).connection.handleSetCommandMinecart(new ServerboundSetCommandMinecartPacket(
               ref(a.get(2)).getId(), command, a.size() < 5 || Boolean.parseBoolean(a.get(4))));
         }
         case "paddle" -> {
            world();
            player(a.get(1)).connection.handlePaddleBoat(new ServerboundPaddleBoatPacket(Boolean.parseBoolean(a.get(2)), Boolean.parseBoolean(a.get(3))));
         }
         case "move_vehicle" -> {
            world();
            // move_vehicle PLAYER X Y Z YROT XROT ONGROUND: ServerboundMoveVehiclePacket of the vehicle the player controls
            player(a.get(1)).connection.handleMoveVehicle(new ServerboundMoveVehiclePacket(
               net.minecraft.core.PositionAndRotation.of(new Vec3(d(a.get(2)), d(a.get(3)), d(a.get(4))), (float)d(a.get(5)), (float)d(a.get(6))), Boolean.parseBoolean(a.get(7))));
         }
         case "accept_teleport" -> {
            world();
            ServerPlayer p = player(a.get(1));
            Vec3 awaiting = (Vec3)VanillaLevel.getField(p.connection, "awaitingPositionFromClient");
            if (awaiting != null) {
               p.connection.handleAcceptTeleportPacket(new ServerboundAcceptTeleportationPacket((int)VanillaLevel.getField(p.connection, "awaitingTeleport"),
                  awaiting.x, awaiting.y, awaiting.z, p.getYRot(), p.getXRot()));
            }
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
            // a real client reports the respawn world loaded (ServerboundPlayerLoadedPacket); until then hasClientLoaded() is false and the player is invulnerable
            neu.connection.handleAcceptPlayerLoad(new ServerboundPlayerLoadedPacket());
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
         case "biome" -> {
            world();
            command("fillbiome " + String.join(" ", a.subList(1, 7)) + " " + a.get(7));
         }
         case "light" -> {
            world();
            v.level.setBlock(new BlockPos(Integer.parseInt(a.get(1)), Integer.parseInt(a.get(2)), Integer.parseInt(a.get(3))), state("minecraft:light[level=" + a.get(4) + "]"), 3);
         }
         case "mobspawn" -> {
            world();
            mobSpawn(a);
         }
         case "target" -> {
            world();
            ((Mob)ref(a.get(1))).setTarget(a.get(2).equals("-") ? null : (LivingEntity)ref(a.get(2)));
         }
         case "love" -> {
            world();
            ((Animal)ref(a.get(1))).setInLove(a.size() > 2 ? player(a.get(2)) : null);
         }
         case "age" -> {
            world();
            ((AgeableMob)ref(a.get(1))).setAge(Integer.parseInt(a.get(2)));
         }
         case "breed" -> {
            world();
            ((Animal)ref(a.get(1))).spawnChildFromBreeding(v.level, (Animal)ref(a.get(2)));
         }
         case "tame" -> {
            world();
            Entity e = ref(a.get(1));
            if (e instanceof TamableAnimal t) {
               t.tame(player(a.get(2)));
            } else {
               ((net.minecraft.world.entity.animal.equine.AbstractHorse)e).tameWithName(player(a.get(2)));
            }
         }
         case "leash" -> {
            world();
            Leashable l = (Leashable)ref(a.get(1));
            if (a.get(2).equals("-")) {
               l.removeLeash();
            } else {
               l.setLeashedTo(ref(a.get(2)), true);
            }
         }
         case "nav" -> {
            world();
            nav(a, true);
         }
         case "path_query" -> {
            world();
            nav(a, false);
         }
         case "nav_stop" -> {
            world();
            ((Mob)ref(a.get(1))).getNavigation().stop();
         }
         case "goal" -> {
            world();
            dump(new String[] {"goals:" + a.get(1)});
         }
         case "brain" -> {
            world();
            dump(new String[] {"brain:" + a.get(1)});
         }
         case "path" -> {
            world();
            dump(new String[] {"path:" + a.get(1)});
         }
         case "trades" -> {
            world();
            trades(a);
         }
         case "offers" -> {
            world();
            dump(new String[] {"offers:" + a.get(1)});
         }
         case "spawnpass" -> {
            world();
            spawnPass(a);
         }
         case "raid_waves" -> {
            world();
            raidWaves(a);
         }
         case "raid_start" -> {
            world();
            raidStart(a);
         }
         case "trade_select" -> {
            world();
            player(a.get(1)).connection.handleSelectTrade(new ServerboundSelectTradePacket(Integer.parseInt(a.get(2))));
         }
         default -> {
            if (!MenuVerbs.run(this, a)) {
               throw new IllegalArgumentException("unknown command " + cmd);
            }
         }
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

   /**
    * ServerChunkCache.broadcastChangedChunks walks an identity-hashed set, so the chunks of one tick send their block changes in an
    * order that differs per JVM run: every run of consecutive chunk change packets (block_update, section_blocks_update,
    * block_entity_data, light_update) is stably sorted by chunk. The Rust replay (`packet_chunk` in block_scenario_actors.rs) does the same.
    */
   static List<VanillaLevel.Sent> chunkOrder(List<VanillaLevel.Sent> sent) {
      List<VanillaLevel.Sent> result = new ArrayList<>();
      List<VanillaLevel.Sent> run = new ArrayList<>();
      for (VanillaLevel.Sent s : sent) {
         if (packetChunk(s) != null) {
            run.add(s);
         } else {
            flushRun(run, result);
            result.add(s);
         }
      }
      flushRun(run, result);
      return result;
   }

   private static void flushRun(List<VanillaLevel.Sent> run, List<VanillaLevel.Sent> result) {
      run.sort(Comparator.<VanillaLevel.Sent>comparingLong(s -> packetChunk(s)[0]).thenComparingLong(s -> packetChunk(s)[1]));
      result.addAll(run);
      run.clear();
   }

   /** The chunk (x, z) a chunk change packet belongs to, read from its wire form; null for other packets. */
   private static long[] packetChunk(VanillaLevel.Sent s) {
      String type = s.type();
      if (!type.equals("minecraft:block_update") && !type.equals("minecraft:block_entity_data")
         && !type.equals("minecraft:section_blocks_update") && !type.equals("minecraft:light_update")) {
         return null;
      }
      java.nio.ByteBuffer b = java.nio.ByteBuffer.wrap(HexFormat.of().parseHex(s.hex()));
      skipVarInt(b);
      return switch (type) {
         case "minecraft:section_blocks_update" -> {
            long v = b.getLong();
            yield new long[] {v >> 42, (v << 22) >> 42};
         }
         case "minecraft:light_update" -> new long[] {readVarInt(b), readVarInt(b)};
         default -> {
            long v = b.getLong();
            yield new long[] {(v >> 38) >> 4, ((v << 26) >> 38) >> 4};
         }
      };
   }

   private static int readVarInt(java.nio.ByteBuffer b) {
      int value = 0;
      int shift = 0;
      byte next;
      do {
         next = b.get();
         value |= (next & 0x7F) << shift;
         shift += 7;
      } while ((next & 0x80) != 0);
      return value;
   }

   private static void skipVarInt(java.nio.ByteBuffer b) {
      readVarInt(b);
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
         case "ticks_frozen" -> e.setTicksFrozen(Integer.parseInt(value));
         case "xp_level" -> ((ServerPlayer)e).setExperienceLevels(Integer.parseInt(value));
         case "enchant_seed" -> priv(e, Player.class, "enchantmentSeed", Integer.parseInt(value));
         case "absorption" -> ((LivingEntity)e).setAbsorptionAmount(Float.parseFloat(value));
         case "invulnerable" -> priv(e, Entity.class, "permanentlyInvulnerable", Boolean.parseBoolean(value));
         case "no_gravity" -> e.setNoGravity(Boolean.parseBoolean(value));
         case "on_ground" -> e.setOnGround(Boolean.parseBoolean(value));
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

   // ------------------------------------------------------------------------------------------------ mobs

   static String bool(boolean b) {
      return b ? "1" : "0";
   }

   String entName(Entity e) {
      return e == null ? "-" : uuidName(e.getUUID());
   }

   static String pos(BlockPos p) {
      return p.getX() + "," + p.getY() + "," + p.getZ();
   }

   static String simple(Class<?> c) {
      String n = c.getName();
      return n.substring(n.lastIndexOf('.') + 1);
   }

   /** Canonical SNBT: compound keys sorted and printed raw, everything else as Tag.toString. */
   static String canon(Tag t) {
      if (t instanceof CompoundTag c) {
         List<String> keys = new ArrayList<>(c.keySet());
         Collections.sort(keys);
         StringJoiner j = new StringJoiner(",", "{", "}");
         for (String k : keys) {
            j.add(k + ":" + canon(c.get(k)));
         }
         return j.toString();
      }
      if (t instanceof ListTag l) {
         StringJoiner j = new StringJoiner(",", "[", "]");
         for (Tag x : l) {
            j.add(canon(x));
         }
         return j.toString();
      }
      return t.toString();
   }

   /** `id*count[/dmg=N]` plus the canonical SNBT of the component patch. */
   String itemFull(ItemStack s) {
      String b = item(s);
      if (s.isEmpty() || s.getComponentsPatch().isEmpty()) {
         return b;
      }
      RegistryOps<Tag> ops = RegistryOps.create(NbtOps.INSTANCE, v.server.registryAccess());
      Tag tag = net.minecraft.core.component.DataComponentPatch.CODEC.encodeStart(ops, s.getComponentsPatch()).getOrThrow();
      return b + canon(tag);
   }

   String memval(Object o) {
      if (o == null) {
         return "-";
      }
      if (o instanceof Optional<?> op) {
         return memval(op.orElse(null));
      }
      if (o instanceof Entity e) {
         return entName(e);
      }
      if (o instanceof UUID u) {
         return uuidName(u);
      }
      if (o instanceof BlockPos p) {
         return pos(p);
      }
      if (o instanceof GlobalPos g) {
         return g.dimension().identifier() + "@" + pos(g.pos());
      }
      if (o instanceof Boolean b) {
         return bool(b);
      }
      if (o instanceof WalkTarget w) {
         return "walk(" + memval(w.getTarget()) + ";" + f(w.getSpeedModifier()) + ";" + w.getCloseEnoughDist() + ")";
      }
      if (o instanceof PositionTracker t) {
         return t instanceof EntityTracker et ? "entity(" + entName(et.getEntity()) + ")" : "pos(" + pos(t.currentBlockPosition()) + ")";
      }
      if (o instanceof Path p) {
         return "path(" + p.getNextNodeIndex() + ";" + p.getNodeCount() + ")";
      }
      if (o instanceof net.minecraft.world.damagesource.DamageSource ds) {
         return "damage(" + ds.getMsgId() + ")";
      }
      if (o instanceof net.minecraft.world.entity.ai.memory.NearestVisibleLivingEntities n) {
         return memval(n.nearbyEntities());
      }
      if (o instanceof Collection<?> c) {
         List<String> l = new ArrayList<>();
         for (Object x : c) {
            l.add(memval(x));
         }
         if (o instanceof Set) {
            Collections.sort(l);
         }
         return "(" + String.join(";", l) + ")";
      }
      String s = String.valueOf(o);
      if (s.matches(".*@[0-9a-f]+$")) {
         s = simple(o.getClass());
      }
      return s.replaceAll("\\s+", "_");
   }

   static String memName(MemoryModuleType<?> t) {
      return BuiltInRegistries.MEMORY_MODULE_TYPE.getKey(t).getPath();
   }

   /** (name, value, ttl) of every memory with a value, sorted by name. */
   List<String[]> memoryEntries(LivingEntity le) {
      List<String[]> l = new ArrayList<>();
      le.getBrain().forEach(new Brain.Visitor() {
         @Override
         public <U> void acceptEmpty(MemoryModuleType<U> type) {
         }

         @Override
         public <U> void accept(MemoryModuleType<U> type, U value) {
            l.add(new String[] {memName(type), memval(value), "inf"});
         }

         @Override
         public <U> void accept(MemoryModuleType<U> type, U value, long timeToLive) {
            l.add(new String[] {memName(type), memval(value), Long.toString(timeToLive)});
         }
      });
      l.sort(Comparator.comparing(x -> x[0]));
      return l;
   }

   static String joinOrDash(Collection<String> l, String sep) {
      return l.isEmpty() ? "-" : String.join(sep, l);
   }

   static String activities(LivingEntity le) {
      List<String> l = new ArrayList<>();
      for (Activity a : le.getBrain().getActiveActivities()) {
         l.add(a.getName());
      }
      Collections.sort(l);
      return joinOrDash(l, ";");
   }

   static String flagsOf(Collection<Goal.Flag> flags) {
      List<String> l = new ArrayList<>();
      for (Goal.Flag fl : flags) {
         l.add(fl.name());
      }
      return joinOrDash(l, "+").replace("-", "none");
   }

   static String runningGoals(GoalSelector gs) {
      List<String> l = new ArrayList<>();
      for (WrappedGoal w : gs.getAvailableGoals()) {
         if (w.isRunning()) {
            l.add(w.getPriority() + ":" + simple(w.getGoal().getClass()) + ":" + flagsOf(w.getFlags()));
         }
      }
      return joinOrDash(l, ";");
   }

   static String pathNodes(Path p) {
      if (p == null || p.getNodeCount() == 0) {
         return "-";
      }
      StringJoiner j = new StringJoiner(";");
      for (int i = 0; i < p.getNodeCount(); i++) {
         Node n = p.getNode(i);
         j.add(n.x + "," + n.y + "," + n.z + ":" + n.type.name() + ":" + f(n.costMalus));
      }
      return j.toString();
   }

   static String equipment(LivingEntity le, EntityOracleDriver d) {
      List<String> l = new ArrayList<>();
      for (EquipmentSlot s : EquipmentSlot.values()) {
         ItemStack st = le.getItemBySlot(s);
         if (!st.isEmpty()) {
            l.add(s.getName() + ":" + d.itemFull(st));
         }
      }
      return joinOrDash(l, ";");
   }

   @SuppressWarnings("unchecked")
   static String attributes(LivingEntity le, boolean all) {
      Object supplier = priv(le.getAttributes(), "supplier");
      Map<Holder<Attribute>, AttributeInstance> map = (Map<Holder<Attribute>, AttributeInstance>)priv(supplier, "instances");
      List<String> l = new ArrayList<>();
      for (Holder<Attribute> h : map.keySet()) {
         double value = le.getAttributeValue(h);
         if (all || value != h.value().getDefaultValue()) {
            l.add(h.getRegisteredName() + ":" + f(value));
         }
      }
      Collections.sort(l);
      return joinOrDash(l, ";");
   }

   /** The mob part of a rich `E` line (see the header). */
   void describeMob(StringBuilder b, Mob m) {
      b.append(" target=").append(entName(m.getTarget()));
      b.append(" hurtby=").append(entName(m.getLastHurtByMob()));
      b.append(" aggr=").append(bool(m.isAggressive()));
      b.append(" noai=").append(bool(m.isNoAi()));
      b.append(" left=").append(bool(m.isLeftHanded()));
      b.append(" persist=").append(bool(m.isPersistenceRequired()));
      b.append(" canpickup=").append(bool(m.canPickUpLoot()));
      b.append(" home=").append(m.hasHome() ? pos(m.getHomePosition()) + "/" + m.getHomeRadius() : "-");
      b.append(" leash=").append(entName(m.getLeashHolder()));
      b.append(" speed=").append(f(m.getSpeed()));
      b.append(" zza=").append(f(m.zza)).append(" xxa=").append(f(m.xxa)).append(" yya=").append(f(m.yya));
      b.append(" head=").append(f(m.getYHeadRot())).append(" body=").append(f(m.yBodyRot));
      PathNavigation nav = m.getNavigation();
      Path p = nav.getPath();
      b.append(" nav=").append(nav.isDone() ? "done" : "moving");
      b.append(" stuck=").append(bool(nav.isStuck()));
      b.append(" navspeed=").append(f((Double)priv(nav, PathNavigation.class, "speedModifier")));
      b.append(" ntarget=").append(nav.getTargetPos() == null ? "-" : pos(nav.getTargetPos()));
      b.append(" path=").append(p == null ? "-" : p.getNextNodeIndex() + "/" + p.getNodeCount() + "/" + bool(p.canReach()) + "/" + pos(p.getTarget()));
      b.append(" nodes=").append(pathNodes(p));
      b.append(" goals=").append(runningGoals(m.getGoalSelector()));
      b.append(" tgoals=").append(runningGoals((GoalSelector)priv(m, Mob.class, "targetSelector")));
      b.append(" acts=").append(activities(m));
      List<String> mem = new ArrayList<>();
      for (String[] e : memoryEntries(m)) {
         mem.add(e[0] + ":" + e[1] + ":" + e[2]);
      }
      b.append(" mem=").append(joinOrDash(mem, ";"));
      b.append(" equip=").append(equipment(m, this));
      b.append(" attrs=").append(attributes(m, false));
      if (m instanceof AgeableMob am) {
         b.append(" age=").append(am.getAge()).append(" baby=").append(bool(am.isBaby())).append(" forcedage=").append(am.getForcedAge());
      }
      if (m instanceof Animal an) {
         b.append(" love=").append(an.getInLoveTime()).append(" lovecause=").append(entName(an.getLoveCause()));
      }
      if (m instanceof TamableAnimal ta) {
         var owner = ta.getOwnerReference();
         b.append(" tame=").append(bool(ta.isTame())).append(" owner=").append(owner == null ? "-" : uuidName(owner.getUUID()));
         b.append(" sitting=").append(bool(ta.isInSittingPose())).append(" ordered=").append(bool(ta.isOrderedToSit()));
      }
      if (m instanceof net.minecraft.world.entity.npc.villager.Villager vil) {
         var vd = vil.getVillagerData();
         b.append(" vdata=").append(vd.type().getRegisteredName()).append('/').append(vd.profession().getRegisteredName()).append('/').append(vd.level());
         b.append(" vxp=").append(vil.getVillagerXp());
      }
   }

   void printPath(String name, Path p) {
      if (p == null) {
         out.println("PATH " + name + " none");
         return;
      }
      out.println("PATH " + name + " next=" + p.getNextNodeIndex() + " count=" + p.getNodeCount() + " reached=" + bool(p.canReach()) + " target=" + pos(p.getTarget()) + " dist=" + f(p.getDistToTarget()));
      for (int i = 0; i < p.getNodeCount(); i++) {
         Node n = p.getNode(i);
         out.println("PN " + name + " " + i + " pos=" + n.x + "," + n.y + "," + n.z + " type=" + n.type.name() + " malus=" + f(n.costMalus) + " walked=" + f(n.walkedDistance));
      }
   }

   void nav(List<String> a, boolean move) {
      Mob m = (Mob)ref(a.get(1));
      double x = d(a.get(2)), y = d(a.get(3)), z = d(a.get(4));
      PathNavigation nav = m.getNavigation();
      Path p;
      if (move) {
         double speed = a.size() > 5 ? d(a.get(5)) : 1.0;
         int reach = a.size() > 6 ? Integer.parseInt(a.get(6)) : 1;
         boolean ok = nav.moveTo(x, y, z, reach, speed);
         p = nav.getPath();
         out.println("NAV " + a.get(1) + " " + vec(x, y, z) + " moved=" + ok);
      } else {
         p = nav.createPath(x, y, z, a.size() > 5 ? Integer.parseInt(a.get(5)) : 1);
         out.println("PATHQ " + a.get(1) + " " + vec(x, y, z) + " found=" + (p != null));
      }
      printPath(a.get(1), p);
   }

   void goalsDump(String name, Mob m) {
      for (var sel : new Object[][] {{"goal", m.getGoalSelector()}, {"target", priv(m, Mob.class, "targetSelector")}}) {
         for (WrappedGoal w : ((GoalSelector)sel[1]).getAvailableGoals()) {
            out.println("G " + name + " " + sel[0] + " " + w.getPriority() + " " + simple(w.getGoal().getClass()) + " flags=" + flagsOf(w.getFlags()) + " running=" + bool(w.isRunning()));
         }
      }
   }

   void brainDump(String name, LivingEntity le) {
      out.println("BRAIN " + name + " acts=" + activities(le));
      for (String[] e : memoryEntries(le)) {
         out.println("BM " + name + " type=" + e[0] + " value=" + e[1] + " ttl=" + e[2]);
      }
      for (Object bc : le.getBrain().getRunningBehaviors()) {
         out.println("BR " + name + " " + ((BehaviorControl<?>)bc).debugString());
      }
   }

   void nbtDump(String name, Entity e) {
      try (ProblemReporter.ScopedCollector rep = new ProblemReporter.ScopedCollector(e.problemPath(), org.slf4j.LoggerFactory.getLogger("oracle"))) {
         net.minecraft.world.level.storage.TagValueOutput o = net.minecraft.world.level.storage.TagValueOutput.createWithContext(rep, v.server.registryAccess());
         e.saveWithoutId(o);
         CompoundTag tag = o.buildResult();
         tag.remove("UUID");
         out.println("N " + name + " " + canon(tag));
      }
   }

   void offersDump(String name, MerchantOffers offers) {
      out.println("OFFERS " + name + " count=" + offers.size());
      for (int i = 0; i < offers.size(); i++) {
         MerchantOffer o = offers.get(i);
         out.println("OFFER " + name + " " + i + " buyA=" + itemFull(o.getBaseCostA()) + " buyB=" + (o.getCostB().isEmpty() ? "-" : itemFull(o.getCostB())) + " sell=" + itemFull(o.getResult())
            + " uses=" + o.getUses() + " max=" + o.getMaxUses() + " xp=" + o.getXp() + " mult=" + f(o.getPriceMultiplier()) + " demand=" + o.getDemand() + " special=" + o.getSpecialPriceDiff());
      }
   }

   void mobSpawn(List<String> a) throws Exception {
      String name = a.get(1);
      VanillaLevel.reseed(hash(name));
      EntitySpawnReason reason = EntitySpawnReason.COMMAND;
      boolean finalize = true;
      String snbt = null;
      List<String> rot = new ArrayList<>();
      for (int i = 6; i < a.size(); i++) {
         String t = a.get(i);
         if (t.startsWith("{")) {
            snbt = t;
         } else if (t.startsWith("reason=")) {
            reason = EntitySpawnReason.valueOf(t.substring(7).toUpperCase(Locale.ROOT));
         } else if (t.equals("nofinalize")) {
            finalize = false;
         } else {
            rot.add(t);
         }
      }
      EntityType<?> type = BuiltInRegistries.ENTITY_TYPE.getValue(id(a.get(2)));
      Entity e = type.create(v.level, reason);
      if (e == null) {
         throw new IllegalStateException("cannot create " + a.get(2));
      }
      e.snapTo(d(a.get(3)), d(a.get(4)), d(a.get(5)), rot.size() > 1 ? (float)d(rot.get(0)) : 0f, rot.size() > 1 ? (float)d(rot.get(1)) : 0f);
      if (snbt != null) {
         mergeSnbt(e, snbt);
      }
      if (finalize && e instanceof Mob mob) {
         mob.finalizeSpawn(v.level, v.level.getCurrentDifficultyAt(e.blockPosition()), reason, null);
      }
      v.level.addFreshEntityWithPassengers(e);
      refs.put(name, e);
   }

   void mergeSnbt(Entity e, String snbt) throws Exception {
      try (ProblemReporter.ScopedCollector rep = new ProblemReporter.ScopedCollector(e.problemPath(), org.slf4j.LoggerFactory.getLogger("oracle"))) {
         net.minecraft.world.level.storage.TagValueOutput o = net.minecraft.world.level.storage.TagValueOutput.createWithContext(rep, v.server.registryAccess());
         e.saveWithoutId(o);
         CompoundTag merged = o.buildResult().merge(TagParser.parseCompoundFully(snbt));
         e.load(TagValueInput.create(rep, v.server.registryAccess(), merged));
      }
   }

   void trades(List<String> a) throws Exception {
      String name = a.get(1);
      String type = "minecraft:plains";
      for (int i = 5; i < a.size(); i++) {
         if (a.get(i).startsWith("type=")) {
            type = id(a.get(i).substring(5)).toString();
         }
      }
      VanillaLevel.reseed(hash(name));
      Entity e = net.minecraft.world.entity.EntityTypes.VILLAGER.create(v.level, EntitySpawnReason.COMMAND);
      e.snapTo(0.5, -60, 0.5, 0f, 0f);
      mergeSnbt(e, "{VillagerData:{type:\"" + type + "\",profession:\"" + id(a.get(2)) + "\",level:" + Integer.parseInt(a.get(3)) + "}}");
      v.level.addFreshEntity(e);
      refs.put(name, e);
      e.getRandom().setSeed(Long.parseLong(a.get(4)));
      offersDump(name, ((AbstractVillager)e).getOffers());
   }

   NaturalSpawner.SpawnState createSpawnState() throws Exception {
      ServerChunkCache cache = v.level.getChunkSource();
      int count = cache.chunkMap.getDistanceManager().getNaturalSpawnChunkCount();
      Method m = ServerChunkCache.class.getDeclaredMethod("getFullChunk", long.class, java.util.function.Consumer.class);
      m.setAccessible(true);
      NaturalSpawner.ChunkGetter getter = (key, sink) -> {
         try {
            m.invoke(cache, key, sink);
         } catch (ReflectiveOperationException ex) {
            throw new IllegalStateException(ex);
         }
      };
      return NaturalSpawner.createState(count, v.level, getter, new LocalMobCapCalculator(cache.chunkMap));
   }

   void printSpawnState(NaturalSpawner.SpawnState st) {
      StringJoiner counts = new StringJoiner(";");
      StringJoiner caps = new StringJoiner(";");
      for (MobCategory c : MobCategory.values()) {
         counts.add(c.getSerializedName() + ":" + st.getMobCategoryCounts().getInt(c));
         caps.add(c.getSerializedName() + ":" + c.getMaxInstancesPerChunk() * st.getSpawnableChunkCount() / 289);
      }
      List<?> charges = (List<?>)priv(priv(st, "spawnPotential"), "charges");
      List<String> ch = new ArrayList<>();
      for (Object c : charges) {
         ch.add(pos((BlockPos)priv(c, "pos")) + ":" + f((Double)priv(c, "charge")));
      }
      out.println("SPAWNSTATE chunks=" + st.getSpawnableChunkCount() + " counts=" + counts + " caps=" + caps + " charges=" + joinOrDash(ch, ";"));
   }

   void spawnPass(List<String> a) throws Exception {
      v.level.getRandom().setSeed(Long.parseLong(a.get(1)));
      Map<String, String> o = kv(a, 2);
      String[] ch = o.getOrDefault("chunk", "0,0").split(",");
      int cx = Integer.parseInt(ch[0]), cz = Integer.parseInt(ch[1]);
      NaturalSpawner.SpawnState state = createSpawnState();
      List<MobCategory> cats = new ArrayList<>();
      if (o.containsKey("cats")) {
         for (String c : o.get("cats").split(",")) {
            cats.add(MobCategory.valueOf(c.toUpperCase(Locale.ROOT)));
         }
      } else {
         cats = NaturalSpawner.getFilteredSpawningCategories(state, Boolean.parseBoolean(o.getOrDefault("enemies", "true")),
            Boolean.parseBoolean(o.getOrDefault("persistent", String.valueOf(v.level.getGameTime() % 400L == 0L))));
      }
      Set<Integer> before = new HashSet<>();
      for (Entity e : v.level.getAllEntities()) {
         before.add(e.getId());
      }
      LevelChunk chunk = v.level.getChunk(cx, cz);
      StringJoiner names = new StringJoiner(",");
      for (MobCategory c : cats) {
         names.add(c.getSerializedName());
      }
      boolean allowed = v.level.canSpawnEntitiesInChunk(chunk.getPos());
      out.println("SPAWNPASS seed=" + a.get(1) + " chunk=" + cx + "," + cz + " cats=" + (cats.isEmpty() ? "-" : names) + " allowed=" + bool(allowed));
      if (!cats.isEmpty() && allowed) {
         NaturalSpawner.spawnForChunk(v.level, chunk, state, cats);
      }
      nameNewEntities();
      List<Map.Entry<String, Entity>> fresh = new ArrayList<>();
      for (var en : refs.entrySet()) {
         if (!before.contains(en.getValue().getId())) {
            fresh.add(en);
         }
      }
      fresh.sort(Comparator.comparingInt(en -> en.getValue().getId()));
      for (var en : fresh) {
         describe(en.getKey(), en.getValue(), true);
      }
      printSpawnState(state);
   }

   void raidWaves(List<String> a) throws Exception {
      Difficulty diff = Difficulty.byName(a.get(1));
      int omen = Integer.parseInt(a.get(2));
      Raid raid = new Raid(BlockPos.ZERO, diff);
      raid.setRaidOmenLevel(omen);
      ((RandomSource)priv(raid, "random")).setSeed(Long.parseLong(a.get(3)));
      DifficultyInstance di = new DifficultyInstance(diff, 0L, 0L, 0f);
      Class<?> rt = Class.forName("net.minecraft.world.entity.raid.Raid$RaiderType");
      Object[] types = rt.getEnumConstants();
      Method dflt = Raid.class.getDeclaredMethod("getDefaultNumSpawns", rt, int.class, boolean.class);
      Method bonus = Raid.class.getDeclaredMethod("getPotentialBonusSpawns", rt, RandomSource.class, int.class, DifficultyInstance.class, boolean.class);
      dflt.setAccessible(true);
      bonus.setAccessible(true);
      RandomSource rnd = (RandomSource)priv(raid, "random");
      int groups = raid.getNumGroups(diff);
      int last = omen > 1 ? groups + 1 : groups;
      for (int g = 1; g <= last; g++) {
         boolean isBonus = g == groups + 1;
         StringBuilder b = new StringBuilder("RAID_WAVE diff=").append(a.get(1)).append(" omen=").append(omen).append(" wave=").append(g).append(" bonus=").append(bool(isBonus));
         for (Object t : types) {
            int n = (Integer)dflt.invoke(raid, t, g, isBonus) + (Integer)bonus.invoke(raid, t, rnd, g, di, isBonus);
            b.append(' ').append(((Enum<?>)t).name().toLowerCase(Locale.ROOT)).append('=').append(n);
         }
         b.append(" enchant=").append(f(raid.getEnchantOdds()));
         out.println(b);
      }
   }

   void printRaid(int id, Raid r) {
      List<String> raiders = new ArrayList<>();
      for (var rd : r.getAllRaiders()) {
         raiders.add(entName(rd));
      }
      Collections.sort(raiders);
      out.println("RAID " + id + " center=" + pos(r.getCenter()) + " started=" + bool(r.isStarted()) + " active=" + bool(r.isActive()) + " status=" + priv(r, "status")
         + " omen=" + r.getRaidOmenLevel() + " groups=" + r.getGroupsSpawned() + "/" + priv(r, "numGroups") + " health=" + f(r.getTotalHealth()) + " raiders=" + joinOrDash(raiders, ";"));
   }

   @SuppressWarnings("unchecked")
   void raidsDump() {
      var map = (it.unimi.dsi.fastutil.ints.Int2ObjectMap<Raid>)priv(v.level.getRaids(), "raidMap");
      for (int id : new TreeSet<>(map.keySet())) {
         printRaid(id, map.get(id));
      }
   }

   void raidStart(List<String> a) {
      ServerPlayer p = player(a.get(1));
      BlockPos pos = new BlockPos(Integer.parseInt(a.get(2)), Integer.parseInt(a.get(3)), Integer.parseInt(a.get(4)));
      Raid raid = v.level.getRaids().createOrExtendRaid(p, pos);
      if (raid == null) {
         out.println("RAID none");
         return;
      }
      for (int i = 5; i < a.size(); i++) {
         if (a.get(i).startsWith("omen=")) {
            raid.setRaidOmenLevel(Integer.parseInt(a.get(i).substring(5)));
         }
      }
      printRaid(v.level.getRaids().getId(raid).getAsInt(), raid);
   }

   void localDifficulty(String arg) {
      String[] c = arg.split(",");
      BlockPos pos = new BlockPos(Integer.parseInt(c[0]), Integer.parseInt(c[1]), Integer.parseInt(c[2]));
      DifficultyInstance di = v.level.getCurrentDifficultyAt(pos);
      out.println("DIFFICULTY " + c[0] + " " + c[1] + " " + c[2] + " base=" + di.getDifficulty().name().toLowerCase(Locale.ROOT) + " effective=" + f(di.getEffectiveDifficulty())
         + " special=" + f(di.getSpecialMultiplier()) + " hard=" + bool(di.isHard()) + " inhabited=" + v.level.getChunkAt(pos).getInhabitedTime());
   }

   void lightDump(String arg) {
      String[] c = arg.split(",");
      BlockPos pos = new BlockPos(Integer.parseInt(c[0]), Integer.parseInt(c[1]), Integer.parseInt(c[2]));
      out.println("LIGHT " + c[0] + " " + c[1] + " " + c[2] + " block=" + v.level.getBrightness(LightLayer.BLOCK, pos) + " sky=" + v.level.getBrightness(LightLayer.SKY, pos));
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
            case "xentity" -> describe(arg, ref(arg), true);
            case "xentities" -> {
               nameNewEntities();
               List<Map.Entry<String, Entity>> all = new ArrayList<>();
               for (var en : refs.entrySet()) {
                  if (!(en.getValue() instanceof ServerPlayer) && en.getValue().isAlive()) {
                     all.add(en);
                  }
               }
               all.sort(Comparator.comparingInt(en -> en.getValue().getId()));
               for (var en : all) {
                  describe(en.getKey(), en.getValue(), true);
               }
            }
            case "edata" -> entityData(arg, ref(arg));
            case "rng" -> {
               // RNG level|REF STATE: the 48-bit state of Level.random or of the entity's own random
               RandomSource random = arg.equals("level") ? v.level.getRandom() : ref(arg).getRandom();
               out.println("RNG " + arg + " " + ((java.util.concurrent.atomic.AtomicLong)priv(random, net.minecraft.world.level.levelgen.LegacyRandomSource.class, "seed")).get());
            }
            case "inventory" -> inventory(arg, (Player)ref(arg));
            case "use" -> {
               ServerPlayer p = (ServerPlayer)ref(arg);
               Object kinetic = priv(p, LivingEntity.class, "recentKineticEnemies");
               out.println("USE " + arg + " hand=" + (p.getUsedItemHand() == InteractionHand.MAIN_HAND ? "main" : "off") + " item=" + item(p.getUseItem()) + " remaining=" + p.getUseItemRemainingTicks()
                  + " ticks=" + p.getTicksUsingItem() + " kinetic=" + (kinetic == null ? 0 : ((Map<?, ?>)kinetic).size()));
            }
            case "cooldowns" -> {
               // CD PLAYER group=ID remaining=N, sorted by group: the running cooldowns (endTime - tickCount)
               net.minecraft.world.item.ItemCooldowns cooldowns = ((ServerPlayer)ref(arg)).getCooldowns();
               int tickCount = (Integer)priv(cooldowns, net.minecraft.world.item.ItemCooldowns.class, "tickCount");
               Map<?, ?> running = (Map<?, ?>)priv(cooldowns, net.minecraft.world.item.ItemCooldowns.class, "cooldowns");
               TreeMap<String, Integer> sorted = new TreeMap<>();
               running.forEach((group, instance) -> sorted.put(group.toString(), (Integer)priv(instance, "endTime") - tickCount));
               sorted.forEach((group, remaining) -> out.println("CD " + arg + " group=" + group + " remaining=" + remaining));
            }
            case "stats" -> {
               // STAT PLAYER type.name:value.name N, sorted by name (the statistics a player has earned)
               @SuppressWarnings("unchecked")
               it.unimi.dsi.fastutil.objects.Object2IntMap<net.minecraft.stats.Stat<?>> counter = (it.unimi.dsi.fastutil.objects.Object2IntMap<net.minecraft.stats.Stat<?>>)priv(((ServerPlayer)ref(arg)).getStats(), net.minecraft.stats.StatsCounter.class, "stats");
               TreeMap<String, Integer> sorted = new TreeMap<>();
               counter.forEach((stat, n) -> sorted.put(stat.getName(), n));
               sorted.forEach((name, n) -> out.println("STAT " + arg + " " + name + " " + n));
            }
            case "advancements" -> {
               // ADV PLAYER advancement-id criterion,criterion: the completed criteria of every advancement with progress, sorted
               @SuppressWarnings("unchecked")
               Map<net.minecraft.advancements.AdvancementHolder, net.minecraft.advancements.AdvancementProgress> progress = (Map<net.minecraft.advancements.AdvancementHolder, net.minecraft.advancements.AdvancementProgress>)priv(((ServerPlayer)ref(arg)).getAdvancements(), net.minecraft.server.PlayerAdvancements.class, "progress");
               TreeMap<String, String> sorted = new TreeMap<>();
               progress.forEach((holder, p) -> {
                  TreeSet<String> done = new TreeSet<>();
                  p.getCompletedCriteria().forEach(done::add);
                  if (!done.isEmpty()) {
                     sorted.put(holder.id().toString(), String.join(",", done));
                  }
               });
               sorted.forEach((id, criteria) -> out.println("ADV " + arg + " " + id + " " + criteria));
            }
            case "packets" -> {
               // packets:NAME[:type,type] -- the optional filter (bare type names, e.g. open_sign_editor) drops the other packets
               String[] pa = arg.split(":", 2);
               boolean except = pa.length > 1 && pa[1].startsWith("!"); // packets:NAME:!type,type = everything but those types
               Set<String> only = pa.length > 1 ? new HashSet<>(Arrays.asList(pa[1].substring(except ? 1 : 0).split(","))) : null;
               VanillaLevel.FakePlayer p = v.player(pa[0]);
               for (VanillaLevel.Sent s : chunkOrder(p.drain())) {
                  if (only == null || only.contains(s.type().replace("minecraft:", "")) != except) {
                     out.println("PKT " + pa[0] + " " + s.type() + " " + s.hex());
                  }
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
            case "beu" -> {
               String[] c = arg.split(",");
               net.minecraft.world.level.block.entity.BlockEntity be = v.level.getBlockEntity(new BlockPos(Integer.parseInt(c[0]), Integer.parseInt(c[1]), Integer.parseInt(c[2])));
               var pk = be == null ? null : (net.minecraft.network.protocol.game.ClientboundBlockEntityDataPacket)be.getUpdatePacket();
               out.println("BEU " + c[0] + " " + c[1] + " " + c[2] + " " + (pk == null ? "-" : BuiltInRegistries.BLOCK_ENTITY_TYPE.getKey(pk.getType()) + " " + pk.getTag()));
            }
            case "bes" -> { // bes:CX,CZ: the chunk's block entity map in iteration order, then the level's ticker lists restricted to the chunk
               String[] c = arg.split(",");
               int cx = Integer.parseInt(c[0]);
               int cz = Integer.parseInt(c[1]);
               out.println("BES " + cx + " " + cz);
               for (var en : v.level.getChunk(cx, cz).getBlockEntities().entrySet()) {
                  BlockPos bp = en.getKey();
                  out.println("BEM " + bp.getX() + " " + bp.getY() + " " + bp.getZ() + " " + BuiltInRegistries.BLOCK_ENTITY_TYPE.getKey(en.getValue().getType()));
               }
               for (String list : new String[] {"blockEntityTickers", "pendingBlockEntityTickers"}) {
                  for (Object t : (List<?>)priv(v.level, Level.class, list)) {
                     BlockPos bp = ((net.minecraft.world.level.block.entity.TickingBlockEntity)t).getPos();
                     if (bp.getX() >> 4 == cx && bp.getZ() >> 4 == cz) {
                        out.println((list.startsWith("pending") ? "BETP " : "BET ") + bp.getX() + " " + bp.getY() + " " + bp.getZ());
                     }
                  }
               }
            }
            case "chunkbes" -> { // chunkbes:CX,CZ: the block entity list of the chunk packet
               String[] c = arg.split(",");
               int cx = Integer.parseInt(c[0]);
               int cz = Integer.parseInt(c[1]);
               out.println("CHUNKBES " + cx + " " + cz);
               new net.minecraft.network.protocol.game.ClientboundLevelChunkPacketData(v.level.getChunk(cx, cz)).forEachBlockEntityTag(cx, cz, (bp, type, tag) ->
                  out.println("BEP " + bp.getX() + " " + bp.getY() + " " + bp.getZ() + " " + BuiltInRegistries.BLOCK_ENTITY_TYPE.getKey(type)));
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
               int[] box = arg == null ? null : Arrays.stream(arg.split(",")).mapToInt(Integer::parseInt).toArray(); // ticks:x1,y1,z1,x2,y2,z2 keeps the ticks inside that box
               for (var t : all) {
                  if (box != null && !(t.pos().getX() >= Math.min(box[0], box[3]) && t.pos().getX() <= Math.max(box[0], box[3]) && t.pos().getY() >= Math.min(box[1], box[4]) && t.pos().getY() <= Math.max(box[1], box[4])
                     && t.pos().getZ() >= Math.min(box[2], box[5]) && t.pos().getZ() <= Math.max(box[2], box[5]))) {
                     continue;
                  }
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
            case "mob" -> describe(arg, ref(arg), true);
            case "mobs" -> {
               nameNewEntities();
               List<Map.Entry<String, Entity>> all = new ArrayList<>();
               for (var en : refs.entrySet()) {
                  if (en.getValue() instanceof Mob && en.getValue().isAlive()) {
                     all.add(en);
                  }
               }
               all.sort(Comparator.comparingInt(en -> en.getValue().getId()));
               for (var en : all) {
                  describe(en.getKey(), en.getValue(), true);
               }
            }
            case "attrs" -> out.println("ATTR " + arg + " " + attributes((LivingEntity)ref(arg), true).replace(';', ' '));
            case "goals" -> goalsDump(arg, (Mob)ref(arg));
            case "brain" -> brainDump(arg, (LivingEntity)ref(arg));
            case "path" -> {
               printPath(arg, ((Mob)ref(arg)).getNavigation().getPath());
            }
            case "nbt" -> nbtDump(arg, ref(arg));
            case "offers" -> offersDump(arg, ((AbstractVillager)ref(arg)).getOffers());
            case "trader" -> {
               var trader = (AbstractVillager)ref(arg);
               out.println("TRADER " + arg + " xp=" + trader.getVillagerXp() + " level=" + (trader instanceof net.minecraft.world.entity.npc.villager.Villager vil ? String.valueOf(vil.getVillagerData().level()) : "-")
                  + " trading=" + entName(trader.getTradingPlayer()));
            }
            case "difficulty" -> localDifficulty(arg);
            case "light" -> lightDump(arg);
            case "spawn_state" -> {
               try {
                  printSpawnState(createSpawnState());
               } catch (Exception ex) {
                  throw new IllegalStateException(ex);
               }
            }
            case "raids" -> raidsDump();
            case "menu" -> MenuVerbs.dump(this, arg);
            case "recipes" -> MenuVerbs.dumpRecipes(this, arg);
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
      describe(name, e, false);
   }

   void describe(String name, Entity e, boolean rich) {
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
         b.append(" main=").append(itemFull(p.getMainHandItem()));
      } else if (e instanceof ItemEntity ie) {
         b.append(" item=").append(item(ie.getItem()));
         b.append(" age=").append(ie.getAge());
      } else if (e instanceof net.minecraft.world.entity.ExperienceOrb o) {
         b.append(" value=").append(o.getValue());
      } else if (e instanceof net.minecraft.world.entity.item.PrimedTnt t) {
         b.append(" fuse=").append(t.getFuse());
      } else if (e instanceof net.minecraft.world.entity.projectile.arrow.AbstractArrow a) {
         @SuppressWarnings("unchecked")
         var inGround = (net.minecraft.network.syncher.EntityDataAccessor<Boolean>)priv(a, net.minecraft.world.entity.projectile.arrow.AbstractArrow.class, "IN_GROUND");
         b.append(" ground=").append(a.getEntityData().get(inGround) ? 1 : 0);
         b.append(" crit=").append(a.isCritArrow() ? 1 : 0).append(" pierce=").append(a.getPierceLevel());
         b.append(" pickup=").append(a.pickup.ordinal()).append(" shake=").append(a.shakeTime);
         b.append(" life=").append(priv(a, net.minecraft.world.entity.projectile.arrow.AbstractArrow.class, "life"));
         b.append(" dmg=").append(priv(a, net.minecraft.world.entity.projectile.arrow.AbstractArrow.class, "baseDamage"));
      } else if (e instanceof net.minecraft.world.entity.LightningBolt lb) {
         b.append(" life=").append(priv(lb, "life")).append(" flashes=").append(priv(lb, "flashes")).append(" seed=").append(lb.seed).append(" fires=").append(lb.getBlocksSetOnFire());
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
      } else if (e instanceof net.minecraft.world.entity.vehicle.boat.AbstractBoat boat) {
         Class<?> ab = net.minecraft.world.entity.vehicle.boat.AbstractBoat.class;
         Object status = priv(boat, ab, "status");
         b.append(" status=").append(status == null ? "-" : status.toString().toLowerCase(Locale.ROOT));
         b.append(" dmg=").append(f(boat.getDamage())).append(" hurt=").append(boat.getHurtTime()).append('/').append(boat.getHurtDir());
         b.append(" bubble=").append(priv(boat, ab, "isAboveBubbleColumn")).append('/').append(boat.getEntityData().get((net.minecraft.network.syncher.EntityDataAccessor<Integer>)priv(null, ab, "DATA_ID_BUBBLE_TIME")));
         b.append(" paddle=").append(boat.getPaddleState(0) ? 1 : 0).append(boat.getPaddleState(1) ? 1 : 0);
         b.append(" pass=");
         if (boat.getPassengers().isEmpty()) {
            b.append('-');
         }
         for (int i = 0; i < boat.getPassengers().size(); i++) {
            b.append(i > 0 ? ";" : "").append(uuidName(boat.getPassengers().get(i).getUUID()));
         }
         if (boat instanceof net.minecraft.world.entity.vehicle.boat.AbstractChestBoat cb) {
            b.append(" items=");
            int n = 0;
            for (int i = 0; i < cb.getItemStacks().size(); i++) { // the raw list: getItem would unpack a pending loot table
               if (!cb.getItemStacks().get(i).isEmpty()) {
                  b.append(n++ > 0 ? ";" : "").append(i).append(':').append(item(cb.getItemStacks().get(i)));
               }
            }
            if (n == 0) {
               b.append('-');
            }
         }
      }
      if (rich) {
         if (e instanceof Mob m) {
            describeMob(b, m);
         }
         EntityExtras.append(this, b, e);
      }
      out.println(b);
   }

   /** `ED REF id:serializer:hex ..`: every synched data item of the entity (accessor id, serializer id, wire form of the value). */
   void entityData(String name, Entity e) {
      var items = (net.minecraft.network.syncher.SynchedEntityData.DataItem<?>[])priv(e.getEntityData(), net.minecraft.network.syncher.SynchedEntityData.class, "itemsById");
      StringBuilder b = new StringBuilder("ED ").append(name);
      for (var item : items) {
         var buf = new net.minecraft.network.RegistryFriendlyByteBuf(io.netty.buffer.Unpooled.buffer(), v.server.registryAccess());
         item.value().write(buf);
         byte[] wire = new byte[buf.readableBytes()];
         buf.readBytes(wire);
         // the accessor id and the serializer id (one byte: there are fewer than 128 serializers) come first
         b.append(' ').append(wire[0] & 0xFF).append(':').append(wire[1]).append(':');
         for (int i = 2; i < wire.length; i++) {
            b.append(String.format("%02x", wire[i]));
         }
         if (wire.length == 2) {
            b.append('-');
         }
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
            out.println("I " + name + " " + i + " " + itemFull(s));
         }
      }
   }
}
