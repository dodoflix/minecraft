import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.util.*;
import java.util.concurrent.atomic.AtomicLong;
import net.minecraft.commands.arguments.blocks.BlockStateParser;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.Registries;
import net.minecraft.network.protocol.game.ClientboundGameEventPacket;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.util.RandomSource;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.LightningBolt;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.biome.Biome;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.chunk.LevelChunkSection;
import net.minecraft.world.level.chunk.PalettedContainer;
import net.minecraft.world.level.levelgen.LegacyRandomSource;
import net.minecraft.world.level.saveddata.WeatherData;

/**
 * Weather oracle (real vanilla {@code ServerLevel}): the weather cycle ({@code advanceWeatherCycle} through reflection, with a
 * connected player recording the game events), {@code resetWeatherCycle}, {@code tickThunder} until the first lightning strike
 * (level random state, strike position, trap), and {@code tickPrecipitation} on scripted snow / ice / cauldron terrain.
 * Run: {@code scripts/entity-oracle.sh crates/mc-game/tests/oracle/WeatherOracle.java > crates/mc-game/tests/fixtures/weather_oracle.txt}
 *
 * <pre>
 * W scenario seed tick clear= rain= thunder= raining= thundering= rl=o,c tl=o,c isRaining= isThundering= rnd=NEXTLONG
 * G scenario seed tick event param         game event packets the player received during that tick
 * S seed calls x y z trap visual bolts horses rnd=NEXTLONG        first lightning strike of tickThunder
 * P scenario round x y z state             block states after each precipitation round;  P* scenario rnd=NEXTLONG
 * </pre>
 * {@code rnd} is the next long of a COPY of the level random (the live state is not consumed).
 */
public final class WeatherOracle {
   static ServerLevel level;
   static VanillaLevel v;
   static VanillaLevel.FakePlayer player;
   static Method advance;
   static Field seedField;

   static String f(float x) {
      return Float.toString(x);
   }

   static long rnd() throws Exception {
      LegacyRandomSource copy = new LegacyRandomSource(0);
      ((AtomicLong) seedField.get(copy)).set(((AtomicLong) seedField.get(level.getRandom())).get());
      return copy.nextLong();
   }

   static void setWeather(int clear, int rain, int thunder, boolean raining, boolean thundering) {
      WeatherData w = v.server.getWeatherData();
      w.setClearWeatherTime(clear);
      w.setRainTime(rain);
      w.setThunderTime(thunder);
      w.setRaining(raining);
      w.setThundering(thundering);
   }

   static void line(String scenario, long seed, int tick, PrintStreamLike out) throws Exception {
      WeatherData w = v.server.getWeatherData();
      out.println("W " + scenario + " " + seed + " " + tick + " clear=" + w.getClearWeatherTime() + " rain=" + w.getRainTime() + " thunder=" + w.getThunderTime()
         + " raining=" + w.isRaining() + " thundering=" + w.isThundering() + " rl=" + f(level.getRainLevel(0.0F)) + "," + f(level.getRainLevel(1.0F))
         + " tl=" + f(level.getThunderLevel(0.0F)) + "," + f(level.getThunderLevel(1.0F)) + " isRaining=" + level.isRaining() + " isThundering=" + level.isThundering() + " rnd=" + rnd());
      for (var sent : player.drain()) {
         if (sent.packet() instanceof ClientboundGameEventPacket g) {
            out.println("G " + scenario + " " + seed + " " + tick + " " + g.getEvent().id() + " " + f(g.getParam()));
         }
      }
   }

   interface PrintStreamLike {
      void println(String s);
   }

   public static void main(String[] args) throws Exception {
      VanillaLevel.Options o = new VanillaLevel.Options();
      o.preloadRadiusChunks = 4;
      v = VanillaLevel.boot(o);
      level = v.level;
      PrintStreamLike out = VanillaLevel.OUT::println;
      advance = ServerLevel.class.getDeclaredMethod("advanceWeatherCycle");
      advance.setAccessible(true);
      seedField = LegacyRandomSource.class.getDeclaredField("seed");
      seedField.setAccessible(true);
      player = v.connect("Obs", GameType.SURVIVAL, 0.5, -60, 0.5, 0, 0, false);
      player.drain();
      v.command("gamerule advance_weather true");

      // ---- the weather cycle: per scenario (initial weather data) and level random seed, 500 ticks
      int[][] scenarios = {
         {0, 30, 10, 0, 0}, {0, 0, 0, 0, 0}, {50, 100, 100, 1, 1}, {0, 20, 25, 1, 1}, {0, 2, 0, 0, 0}, {3, 0, 4, 1, 0}
      };
      for (int s = 0; s < scenarios.length; s++) {
         for (long seed : new long[] {1, 7, 4242}) {
            level.setRainLevel(0.0F);
            level.setThunderLevel(0.0F);
            int[] sc = scenarios[s];
            setWeather(sc[0], sc[1], sc[2], sc[3] == 1, sc[4] == 1);
            if (sc[3] == 1) {
               level.setRainLevel(1.0F);
               level.setThunderLevel(sc[4] == 1 ? 1.0F : 0.0F);
            }
            level.getRandom().setSeed(seed);
            player.drain();
            String name = "cycle" + s;
            for (int t = 1; t <= 500; t++) {
               advance.invoke(level);
               line(name, seed, t, out);
            }
         }
      }
      // advance_weather false: levels follow the saved flags, times stay
      v.command("gamerule advance_weather false");
      setWeather(0, 100, 100, true, true);
      level.setRainLevel(0.0F);
      level.setThunderLevel(0.0F);
      level.getRandom().setSeed(5);
      player.drain();
      for (int t = 1; t <= 150; t++) {
         advance.invoke(level);
         line("norule", 5, t, out);
      }
      v.command("gamerule advance_weather true");
      // resetWeatherCycle (the sleeping branch of ServerLevel.tick)
      setWeather(0, 77, 88, true, true);
      level.resetWeatherCycle();
      line("reset", 0, 0, out);

      // ---- thunder: tickThunder on chunk (0, 0) until the first strike
      level.setRainLevel(1.0F);
      level.setThunderLevel(1.0F);
      setWeather(0, 1000000, 1000000, true, true);
      LevelChunk chunk = level.getChunk(0, 0);
      for (long seed = 1; seed <= 6; seed++) {
         for (Entity e : new ArrayList<>(level.getAllEntities() instanceof Iterable<Entity> it ? toList(it) : List.<Entity>of())) {
            if (!(e instanceof net.minecraft.server.level.ServerPlayer)) {
               e.discard();
            }
         }
         v.command("gamerule spawn_mobs " + (seed % 2 == 1 ? "true" : "false"));
         level.getRandom().setSeed(seed);
         int calls = 0;
         while (calls < 3_000_000) {
            calls++;
            level.tickThunder(chunk);
            boolean struck = false;
            for (Entity e : level.getAllEntities()) {
               if (e instanceof LightningBolt) {
                  struck = true;
               }
            }
            if (struck) {
               break;
            }
         }
         int bolts = 0, horses = 0;
         String pos = "-";
         boolean visual = false;
         for (Entity e : level.getAllEntities()) {
            if (e instanceof LightningBolt b) {
               bolts++;
               pos = b.getX() + " " + b.getY() + " " + b.getZ();
               Field vo = LightningBolt.class.getDeclaredField("visualOnly");
               vo.setAccessible(true);
               visual = vo.getBoolean(b);
            } else if (e.getType() == net.minecraft.world.entity.EntityType.SKELETON_HORSE) {
               horses++;
            }
         }
         out.println("S " + seed + " " + calls + " " + pos + " trap=" + (horses > 0) + " visual=" + visual + " bolts=" + bolts + " horses=" + horses + " rnd=" + rnd());
      }
      v.command("gamerule spawn_mobs true");
      for (Entity e : new ArrayList<>(toList(level.getAllEntities()))) {
         if (!(e instanceof net.minecraft.server.level.ServerPlayer)) {
            e.discard();
         }
      }

      // ---- precipitation
      setWeather(0, 1000000, 1000000, true, false);
      level.setRainLevel(1.0F);
      level.setThunderLevel(0.0F);
      v.command("gamerule max_snow_accumulation_height 4");
      for (String[] area : new String[][] {{"snowy", "minecraft:snowy_plains"}, {"rainy", "minecraft:plains"}, {"high", "minecraft:windswept_hills"}, {"frozen", "minecraft:frozen_river"}}) {
         setBiome(area[1]);
         v.command("fill -8 -61 -8 8 -61 8 minecraft:grass_block");
         v.command("fill -8 -60 -8 8 100 8 minecraft:air");
         v.command("fill -3 -61 -3 2 -61 2 minecraft:water");
         v.command("setblock 6 -60 6 minecraft:water_cauldron[level=1]");
         v.command("setblock 6 -60 4 minecraft:cauldron");
         v.command("setblock 6 -60 2 minecraft:lava_cauldron");
         v.command("setblock 5 -60 -6 minecraft:powder_snow_cauldron[level=1]");
         v.command("setblock -6 -60 -6 minecraft:snow[layers=3]");
         v.command("setblock -6 -60 -4 minecraft:snow[layers=8]");
         v.command("setblock -6 -60 -2 minecraft:oak_slab[type=bottom]");
         v.command("setblock -6 -60 0 minecraft:glass");
         v.command("fill 4 -60 4 4 80 4 minecraft:stone");
         v.command("fill -4 -60 4 -4 140 4 minecraft:stone");
         v.command("fill 0 -60 6 0 200 6 minecraft:stone");
         v.command("fill 0 -60 -7 0 -60 -7 minecraft:oak_leaves");
         level.getRandom().setSeed(99);
         for (int round = 1; round <= 5; round++) {
            for (int x = -8; x <= 8; x++) {
               for (int z = -8; z <= 8; z++) {
                  level.tickPrecipitation(new BlockPos(x, 0, z));
               }
            }
            if (round == 1 || round == 3 || round == 5) {
               for (int x = -8; x <= 8; x++) {
                  for (int y = -62; y <= 200; y++) {
                     for (int z = -8; z <= 8; z++) {
                        BlockState st = level.getBlockState(new BlockPos(x, y, z));
                        if (!st.isAir() && (y > -61 || st.getBlock() != net.minecraft.world.level.block.Blocks.GRASS_BLOCK) && y > -61) {
                           out.println("P " + area[0] + " " + round + " " + x + " " + y + " " + z + " " + BlockStateParser.serialize(st));
                        } else if (y == -61) {
                           out.println("P " + area[0] + " " + round + " " + x + " " + y + " " + z + " " + BlockStateParser.serialize(st));
                        }
                     }
                  }
               }
            }
         }
         out.println("P* " + area[0] + " rnd=" + rnd());
      }
      VanillaLevel.OUT.flush();
      System.exit(0);
   }

   static List<Entity> toList(Iterable<Entity> it) {
      List<Entity> l = new ArrayList<>();
      it.forEach(l::add);
      return l;
   }

   static void setBiome(String id) {
      Holder<Biome> biome = level.registryAccess().lookupOrThrow(Registries.BIOME).getOrThrow(ResourceKey.create(Registries.BIOME, Identifier.parse(id)));
      for (int cx = -3; cx <= 3; cx++) {
         for (int cz = -3; cz <= 3; cz++) {
            LevelChunk chunk = level.getChunk(cx, cz);
            for (LevelChunkSection section : chunk.getSections()) {
               @SuppressWarnings("unchecked")
               PalettedContainer<Holder<Biome>> container = (PalettedContainer<Holder<Biome>>) section.getBiomes();
               for (int x = 0; x < 4; x++) {
                  for (int y = 0; y < 4; y++) {
                     for (int z = 0; z < 4; z++) {
                        container.getAndSetUnchecked(x, y, z, biome);
                     }
                  }
               }
            }
         }
      }
   }
}
