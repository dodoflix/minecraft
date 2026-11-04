package extractor;

import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import java.nio.file.Path;
import java.util.List;
import net.minecraft.SharedConstants;

/**
 * Runs the extract passes over a bootstrapped vanilla server (see {@link Setup}).
 *
 * <p>Usage: {@code Main <output-dir> [pass-name ...]} (no pass names = all passes).
 */
public final class Main {
   /** All passes, in dependency order. Add new passes here. */
   static List<Pass> passes() {
      return List.of(
         new RegistriesPass(),
         new BlocksPass(),
         new FluidsPass(),
         new ItemsPass(),
         new EntitiesPass(),
         new SimpleRegistriesPass(),
         new MiscPass(),
         new TagsPass()
      );
   }

   public static void main(String[] args) throws Exception {
      Path out = Path.of(args[0]);
      Ctx ctx = new Ctx(out, Setup.bootstrap());
      List<String> only = List.of(args).subList(1, args.length);
      JsonObject manifest = new JsonObject();
      JsonArray done = new JsonArray();
      for (Pass p : passes()) {
         if (only.isEmpty() || only.contains(p.name())) {
            System.out.println("== pass " + p.name());
            p.run(ctx);
            done.add(p.name());
         }
      }
      // shapes are filled by the passes above, so written last
      if (ctx.shapes.table().size() > 0) {
         ctx.write("shapes.json", ctx.shapes.table());
      }
      manifest.addProperty("version", SharedConstants.getCurrentVersion().name());
      manifest.addProperty("protocol", SharedConstants.getProtocolVersion());
      manifest.add("passes", done);
      ctx.write("manifest.json", manifest);
   }
}
