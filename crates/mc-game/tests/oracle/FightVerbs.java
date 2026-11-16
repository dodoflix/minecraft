import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.stream.Collectors;
import net.minecraft.commands.arguments.blocks.BlockStateParser;
import net.minecraft.core.BlockPos;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.NbtAccounter;
import net.minecraft.nbt.NbtIo;
import net.minecraft.server.level.ServerBossEvent;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.util.Mth;
import net.minecraft.world.entity.boss.enderdragon.EnderDragon;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.dimension.end.EnderDragonFight;

/**
 * The dragon fight verbs and dumps of {@link EntityOracleDriver} (the level's {@code EnderDragonFight} exists only after the option `end fight`;
 * the verbs act on the level world verbs act on, `in the_end fight_try_respawn`):
 *
 * <pre>
 * fight_skip_arena            EnderDragonFight.skipArenaLoadedCheck(): the fight no longer waits for the 17 chunks along z = +8 to be loaded
 * fight_try_respawn           EnderDragonFight.tryRespawn() (what placing the fourth end crystal around the portal calls)
 * fight_remove_gateways       EnderDragonFight.removeAllGateways(): the gateway list is emptied (a kill then spawns none)
 * dragon_killed DRAGON        EnderDragonFight.setDragonKilled(DRAGON): exit portal, gateway, egg (first kill only), bar hidden; nothing happens for a dragon that is not the fight's
 * dump fight                  `FIGHT killed= prev= stage= rtime= alive= dragon= portal= gateways= scan= seen= pscan= cscan= crystals=`: the fight's fields by reflection
 *                             (stage `-` = null; dragon = the scenario name of the entity with the fight's dragon uuid, `gone` for an uuid no entity has, `-` for none;
 *                             portal = `x,y,z|-`; gateways = the list in order; crystals = the respawn crystal references)
 * dump fightbar               `FIGHTBAR visible= progress= players=NAME,.. name= color= overlay= music= fog=`: the fight's boss bar (the bar uuid is random and not printed)
 * dump fightfile              `FIGHTFILE HEX`: the data storage is saved and the bytes of the real `ender_dragon_fight.dat` (uncompressed NBT) are printed
 * dump fighthash              `FIGHTHASH area N HASH` (FNV-1a over `x,y,z,state` of every non-air block of x,z -64..63, y 0..159) and `FIGHTHASH gateways N HASH` (the same over x-1..x+1,
 *                             y 72..82, z-1..z+1 of the 20 gateway sites); blocks of chunks that are not loaded count as air
 * dump fightpackets:PLAYER    `packets:PLAYER:boss_event,level_event` with the boss bar uuid zeroed (the bar's uuid comes from the level's random)
 * </pre>
 */
final class FightVerbs {
   private FightVerbs() {
   }

   static boolean run(EntityOracleDriver d, List<String> a) throws Exception {
      if (!a.get(0).startsWith("fight_") && !a.get(0).equals("dragon_killed")) {
         return false;
      }
      d.world();
      switch (a.get(0)) {
         case "fight_skip_arena" -> fight(d).skipArenaLoadedCheck();
         case "fight_try_respawn" -> fight(d).tryRespawn();
         case "fight_remove_gateways" -> fight(d).removeAllGateways();
         case "dragon_killed" -> fight(d).setDragonKilled((EnderDragon)d.ref(a.get(1)));
         default -> {
            return false;
         }
      }
      return true;
   }

   static boolean dump(EntityOracleDriver d, String kind) {
      switch (kind) {
         case "fight" -> d.out.println(state(d, fight(d)));
         case "fightbar" -> d.out.println(bar(fight(d)));
         case "fightfile" -> d.out.println("FIGHTFILE " + hex(file(d)));
         case "fighthash" -> {
            d.out.println("FIGHTHASH area " + areaHash(d.v.level));
            d.out.println("FIGHTHASH gateways " + gatewayHash(d.v.level));
         }
         default -> {
            return false;
         }
      }
      return true;
   }

   /** The `fightpackets:` filter: only the packets the fight sends, the boss bar uuid zeroed. */
   static String packetText(String type, String hex) {
      if (type.equals("minecraft:boss_event") && hex.length() >= 34) {
         return hex.substring(0, 2) + "0".repeat(32) + hex.substring(34);
      }
      return hex;
   }

   private static EnderDragonFight fight(EntityOracleDriver d) {
      EnderDragonFight fight = d.v.level.getDragonFight();
      if (fight == null) {
         throw new IllegalStateException("no dragon fight in " + d.v.level.dimension().identifier() + " (`end fight`)");
      }
      return fight;
   }

   private static String state(EntityOracleDriver d, EnderDragonFight f) {
      Object stage = EntityOracleDriver.priv(f, EnderDragonFight.class, "respawnStage");
      BlockPos portal = (BlockPos)EntityOracleDriver.priv(f, EnderDragonFight.class, "exitPortalLocation");
      List<?> gateways = (List<?>)EntityOracleDriver.priv(f, EnderDragonFight.class, "gateways");
      java.util.UUID uuid = f.dragonUUID();
      String dragon = uuid == null ? "-" : "gone";
      for (var en : d.refs.entrySet()) {
         if (en.getValue().getUUID().equals(uuid)) {
            dragon = en.getKey();
            break;
         }
      }
      return "FIGHT killed=" + EntityOracleDriver.priv(f, EnderDragonFight.class, "dragonKilled") + " prev=" + f.hasPreviouslyKilledDragon()
         + " stage=" + (stage == null ? "-" : stage) + " rtime=" + EntityOracleDriver.priv(f, EnderDragonFight.class, "respawnTime") + " alive=" + f.aliveCrystals()
         + " dragon=" + dragon + " portal=" + (portal == null ? "-" : portal.getX() + "," + portal.getY() + "," + portal.getZ())
         + " gateways=" + gateways.stream().map(String::valueOf).collect(Collectors.joining(",")) + " scan=" + EntityOracleDriver.priv(f, EnderDragonFight.class, "needsStateScanning")
         + " seen=" + EntityOracleDriver.priv(f, EnderDragonFight.class, "ticksSinceDragonSeen") + " pscan=" + EntityOracleDriver.priv(f, EnderDragonFight.class, "ticksSinceLastPlayerScan")
         + " cscan=" + EntityOracleDriver.priv(f, EnderDragonFight.class, "ticksSinceCrystalsScanned") + " crystals=" + ((List<?>)EntityOracleDriver.priv(f, EnderDragonFight.class, "respawnCrystals")).size();
   }

   private static String bar(EnderDragonFight f) {
      ServerBossEvent bar = (ServerBossEvent)EntityOracleDriver.priv(f, EnderDragonFight.class, "dragonEvent");
      List<String> players = new ArrayList<>();
      bar.getPlayers().forEach(p -> players.add(p.getPlainTextName()));
      Collections.sort(players);
      return "FIGHTBAR visible=" + bar.isVisible() + " progress=" + EntityOracleDriver.f(bar.getProgress()) + " players=" + (players.isEmpty() ? "-" : String.join(",", players))
         + " name=" + bar.getName().getString() + " color=" + bar.getColor().getName() + " overlay=" + bar.getOverlay().getName() + " music=" + bar.shouldPlayBossMusic()
         + " fog=" + bar.shouldCreateWorldFog();
   }

   /** The bytes of the saved fight: `DimensionDataStorage.saveAndJoin` and the file it wrote for this level. */
   private static byte[] file(EntityOracleDriver d) {
      fight(d);
      ServerLevel level = d.v.level;
      level.getDataStorage().saveAndJoin();
      String dimension = level.dimension().identifier().getPath();
      try (var walk = Files.walk(d.v.dir.resolve("world"))) {
         Path file = walk.filter(f -> f.getFileName().toString().equals("ender_dragon_fight.dat") && f.toString().contains(dimension)).findFirst().orElseThrow();
         CompoundTag saved = NbtIo.readCompressed(file, NbtAccounter.unlimitedHeap());
         ByteArrayOutputStream bytes = new ByteArrayOutputStream();
         NbtIo.write(saved, new DataOutputStream(bytes));
         return bytes.toByteArray();
      } catch (java.io.IOException ex) {
         throw new IllegalStateException(ex);
      }
   }

   private static String hex(byte[] bytes) {
      StringBuilder b = new StringBuilder();
      for (byte x : bytes) {
         b.append(String.format("%02x", x));
      }
      return b.toString();
   }

   private static String areaHash(ServerLevel level) {
      long h = 0xcbf29ce484222325L;
      long count = 0;
      BlockPos.MutableBlockPos pos = new BlockPos.MutableBlockPos();
      for (int x = -64; x < 64; x++) {
         for (int z = -64; z < 64; z++) {
            for (int y = 0; y < 160; y++) {
               BlockState s = state(level, pos.set(x, y, z));
               if (!s.isAir()) {
                  count++;
                  h = fnv(h, x + "," + y + "," + z + "," + BlockStateParser.serialize(s));
               }
            }
         }
      }
      return count + " " + Long.toHexString(h);
   }

   /** The 20 possible gateway sites (`spawnNewGateway`): the states of x-1..x+1, y 72..82, z-1..z+1. */
   private static String gatewayHash(ServerLevel level) {
      long h = 0xcbf29ce484222325L;
      long count = 0;
      for (int i = 0; i < 20; i++) {
         int gx = Mth.floor(96.0 * Math.cos(2.0 * (-Math.PI + (Math.PI / 20) * i)));
         int gz = Mth.floor(96.0 * Math.sin(2.0 * (-Math.PI + (Math.PI / 20) * i)));
         for (int x = gx - 1; x <= gx + 1; x++) {
            for (int y = 72; y <= 82; y++) {
               for (int z = gz - 1; z <= gz + 1; z++) {
                  BlockState s = state(level, new BlockPos(x, y, z));
                  if (!s.isAir()) {
                     count++;
                     h = fnv(h, x + "," + y + "," + z + "," + BlockStateParser.serialize(s));
                  }
               }
            }
         }
      }
      return count + " " + Long.toHexString(h);
   }

   /** The block at `pos`; a chunk that is not loaded reads as air (the hash must not generate chunks). */
   private static BlockState state(ServerLevel level, BlockPos pos) {
      var chunk = level.getChunkSource().getChunkNow(pos.getX() >> 4, pos.getZ() >> 4);
      return chunk == null ? net.minecraft.world.level.block.Blocks.AIR.defaultBlockState() : chunk.getBlockState(pos);
   }

   private static long fnv(long h, String s) {
      for (int i = 0; i < s.length(); i++) {
         h ^= s.charAt(i);
         h *= 0x100000001b3L;
      }
      return h;
   }
}
