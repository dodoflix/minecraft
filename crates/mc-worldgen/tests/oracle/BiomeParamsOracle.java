import com.mojang.datafixers.util.Pair;
import java.io.FileOutputStream;
import java.io.PrintWriter;
import java.lang.reflect.Method;
import java.util.ArrayList;
import java.util.List;
import java.util.Random;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Registry;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.level.biome.Biome;
import net.minecraft.world.level.biome.BiomeManager;
import net.minecraft.world.level.biome.Climate;
import net.minecraft.world.level.biome.MultiNoiseBiomeSourceParameterList;

/**
 * Golden generator for mc-worldgen biome tests (registry order, preset parameter lists, R-tree searches, biome
 * temperatures, seed obfuscation).
 * Usage: MC_EXTRA_CP=tools/extractor/build scripts/vanilla-java.sh -o crates/mc-worldgen/tests/fixtures/biome_params.txt.gz crates/mc-worldgen/tests/oracle/BiomeParamsOracle.java crates/mc-worldgen/tests/fixtures/biome_params.txt.gz
 * Output: crates/mc-worldgen/tests/fixtures/biome_params.txt.gz (the program writes plain text; the wrapper gzips it)
 * (tools/extractor/build: see scripts/extract.sh)
 *
 * Lines: R id name                                   biome registry order
 *        L preset i biome tmin tmax hmin hmax cmin cmax emin emax dmin dmax wmin wmax offset
 *        O seed obfuscated                           BiomeManager.obfuscateSeed
 *        B biome x y z seaLevel tempBits cold warm melt precipitation   Biome temperature queries
 *        Q list childrenPerNode t h c e d w index    R-tree search results, in call order per list
 */
public class BiomeParamsOracle {
   static long[] boundary = {-10000, -4500, -1500, -1000, 0, 500, 2000, 3000, 5500, 10000};

   static Method heightAdjusted;

   public static void main(String[] args) throws Exception {
      RegistryAccess.Frozen reg = extractor.Setup.bootstrap();
      Registry<Biome> biomes = reg.lookupOrThrow(Registries.BIOME);
      PrintWriter out = new PrintWriter(new FileOutputStream(args[0]));
      for (int i = 0; i < biomes.size(); i++) {
         out.println("R " + i + " " + biomes.getKey(biomes.byId(i)));
      }

      // preset parameter lists
      List<Climate.ParameterList<Integer>> lists = new ArrayList<>();
      List<String> listNames = new ArrayList<>();
      var known = MultiNoiseBiomeSourceParameterList.knownPresets();
      for (MultiNoiseBiomeSourceParameterList.Preset preset : new MultiNoiseBiomeSourceParameterList.Preset[]{
         MultiNoiseBiomeSourceParameterList.Preset.NETHER, MultiNoiseBiomeSourceParameterList.Preset.OVERWORLD}) {
         String name = preset.id().getPath();
         List<Pair<Climate.ParameterPoint, Integer>> indexed = new ArrayList<>();
         int i = 0;
         for (Pair<Climate.ParameterPoint, ResourceKey<Biome>> p : known.get(preset).values()) {
            Climate.ParameterPoint pt = p.getFirst();
            int biome = biomes.getId(biomes.getValue(p.getSecond()));
            out.println("L " + name + " " + i + " " + biome + " " + pt.temperature().min() + " " + pt.temperature().max() + " "
               + pt.humidity().min() + " " + pt.humidity().max() + " " + pt.continentalness().min() + " " + pt.continentalness().max() + " "
               + pt.erosion().min() + " " + pt.erosion().max() + " " + pt.depth().min() + " " + pt.depth().max() + " "
               + pt.weirdness().min() + " " + pt.weirdness().max() + " " + pt.offset());
            indexed.add(Pair.of(pt, i));
            i++;
         }
         lists.add(new Climate.ParameterList<>(indexed));
         listNames.add(name);
      }

      // R-tree searches on the preset lists (values = entry index so the exact leaf is compared)
      Random r = new Random(99);
      for (int l = 0; l < lists.size(); l++) {
         for (int cpn : new int[]{19, 4}) {
            Climate.ParameterList<Integer> list = cpn == 19 ? lists.get(l) : lists.get(l).rebuildWithChildrenPerNode(cpn);
            for (int n = 0; n < 5000; n++) {
               long[] t = new long[6];
               for (int d = 0; d < 6; d++) {
                  t[d] = r.nextInt(3) == 0 ? boundary[r.nextInt(boundary.length)] : r.nextInt(24000) - 12000;
               }
               int idx = list.findValue(new Climate.TargetPoint(t[0], t[1], t[2], t[3], t[4], t[5]));
               out.println("Q " + listNames.get(l) + " " + cpn + " " + t[0] + " " + t[1] + " " + t[2] + " " + t[3] + " " + t[4] + " " + t[5] + " " + idx);
            }
         }
      }
      // synthetic lists with many equal-fitness ties
      for (int[] cfg : new int[][]{{3000, 19}, {3000, 3}, {700, 7}, {50, 19}, {1, 19}}) {
         Random lr = new Random(5 + cfg[0]);
         List<Pair<Climate.ParameterPoint, Integer>> values = new ArrayList<>();
         for (int i = 0; i < cfg[0]; i++) {
            Climate.Parameter[] ps = new Climate.Parameter[6];
            for (int d = 0; d < 6; d++) {
               long min = (lr.nextInt(9) - 4) * 2500L;
               long width = lr.nextInt(3) * 2500L;
               ps[d] = new Climate.Parameter(min, min + width);
            }
            values.add(Pair.of(new Climate.ParameterPoint(ps[0], ps[1], ps[2], ps[3], ps[4], ps[5], lr.nextInt(2) * 1000L), i));
         }
         Climate.ParameterList<Integer> list = new Climate.ParameterList<>(values).rebuildWithChildrenPerNode(cfg[1]);
         String name = "synthetic" + cfg[0];
         for (int n = 0; n < 3000; n++) {
            long[] t = new long[6];
            for (int d = 0; d < 6; d++) {
               t[d] = (lr.nextInt(11) - 5) * 2500L + (lr.nextInt(4) == 0 ? lr.nextInt(5000) - 2500 : 0);
            }
            int idx = list.findValue(new Climate.TargetPoint(t[0], t[1], t[2], t[3], t[4], t[5]));
            out.println("Q " + name + " " + cfg[1] + " " + t[0] + " " + t[1] + " " + t[2] + " " + t[3] + " " + t[4] + " " + t[5] + " " + idx);
         }
      }

      // seed obfuscation
      Random sr = new Random(1);
      List<Long> seeds = new ArrayList<>(List.of(0L, 1L, -1L, Long.MAX_VALUE, Long.MIN_VALUE, 12345L, -4172144997902289642L));
      for (int i = 0; i < 40; i++) seeds.add(sr.nextLong());
      for (long s : seeds) out.println("O " + s + " " + BiomeManager.obfuscateSeed(s));

      // biome temperatures
      heightAdjusted = Biome.class.getDeclaredMethod("getHeightAdjustedTemperature", BlockPos.class, int.class);
      heightAdjusted.setAccessible(true);
      int[] ys = {-64, 0, 63, 80, 81, 90, 100, 128, 200, 319};
      int[] seas = {63, 0, -64};
      Random br = new Random(7);
      for (int i = 0; i < biomes.size(); i++) {
         Biome b = biomes.byId(i);
         boolean frozen = biomes.getKey(b).getPath().contains("frozen");
         int count = frozen ? 1500 : 60;
         for (int n = 0; n < count; n++) {
            int x = br.nextInt(6000) - 3000;
            int z = br.nextInt(6000) - 3000;
            int y = ys[br.nextInt(ys.length)];
            int sea = seas[br.nextInt(seas.length)];
            BlockPos pos = new BlockPos(x, y, z);
            float temp = (Float) heightAdjusted.invoke(b, pos, sea);
            out.println("B " + i + " " + x + " " + y + " " + z + " " + sea + " " + Integer.toHexString(Float.floatToRawIntBits(temp)) + " "
               + (b.coldEnoughToSnow(pos, sea) ? 1 : 0) + " " + (b.warmEnoughToRain(pos, sea) ? 1 : 0) + " "
               + (b.shouldMeltFrozenOceanIcebergSlightly(pos, sea) ? 1 : 0) + " " + b.getPrecipitationAt(pos, sea).ordinal());
         }
      }
      out.close();
   }
}
