import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import com.mojang.serialization.DataResult;
import com.mojang.serialization.JsonOps;
import extractor.Setup;
import java.io.PrintWriter;
import java.nio.file.Files;
import java.nio.file.Path;
import net.minecraft.core.RegistryAccess;
import net.minecraft.resources.RegistryOps;
import net.minecraft.world.level.levelgen.material.condition.MaterialCondition;
import net.minecraft.world.level.levelgen.material.rule.MaterialRule;

/**
 * Decodes every case of material_codec_cases.json with the vanilla MaterialRule.CODEC / MaterialCondition.CODEC and prints
 * "name ok|err".
 * Usage: MC_EXTRA_CP=tools/extractor/build scripts/vanilla-java.sh -o crates/mc-worldgen/tests/fixtures/material_codec.txt crates/mc-worldgen/tests/oracle/MaterialCodecOracle.java crates/mc-worldgen/tests/fixtures/material_codec_cases.json
 * Output: crates/mc-worldgen/tests/fixtures/material_codec.txt (the wrapper captures stdout)
 */
public class MaterialCodecOracle {
   public static void main(String[] args) throws Exception {
      RegistryAccess.Frozen full = Setup.bootstrap();
      RegistryOps<JsonElement> ops = RegistryOps.create(JsonOps.INSTANCE, full);
      JsonArray cases = JsonParser.parseString(Files.readString(Path.of(args[0]))).getAsJsonArray();
      PrintWriter out = new PrintWriter(System.out);
      for (JsonElement e : cases) {
         JsonObject c = e.getAsJsonObject();
         String kind = c.get("kind").getAsString();
         JsonElement json = c.get("json");
         boolean ok;
         try {
            DataResult<?> r = kind.equals("rule") ? MaterialRule.CODEC.parse(ops, json) : MaterialCondition.CODEC.parse(ops, json);
            ok = r.result().isPresent();
         } catch (Throwable t) {
            ok = false;
         }
         out.println(c.get("name").getAsString() + " " + (ok ? "ok" : "err"));
      }
      out.flush();
   }
}
