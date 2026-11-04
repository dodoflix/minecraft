package extractor;

import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import java.util.ArrayList;
import java.util.List;
import net.minecraft.core.Holder;
import net.minecraft.core.HolderSet;
import net.minecraft.core.Registry;
import net.minecraft.core.registries.BuiltInRegistries;

/** tags.json: every tag of every built-in registry as resolved by vanilla's TagLoader (registry -> tag -> sorted entry names). */
public final class TagsPass implements Pass {
   @Override
   public String name() {
      return "tags";
   }

   @Override
   public void run(Ctx ctx) throws Exception {
      JsonObject root = new JsonObject();
      for (var e : BuiltInRegistries.REGISTRY.entrySet()) {
         Registry<?> reg = e.getValue();
         JsonObject tags = new JsonObject();
         reg.listTags().forEach((HolderSet.Named<?> set) -> {
            List<String> names = new ArrayList<>();
            for (Holder<?> h : set) {
               names.add(h.unwrapKey().get().identifier().toString());
            }
            names.sort(null);
            JsonArray a = new JsonArray();
            names.forEach(a::add);
            tags.add(set.key().location().toString(), a);
         });
         if (tags.size() > 0) {
            root.add(e.getKey().identifier().toString(), tags);
         }
      }
      ctx.write("tags.json", Ctx.sortKeys(root));
   }
}
