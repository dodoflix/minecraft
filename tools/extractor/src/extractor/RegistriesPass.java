package extractor;

import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import net.minecraft.core.DefaultedRegistry;
import net.minecraft.core.Registry;
import net.minecraft.core.registries.BuiltInRegistries;

/** registries.json: every BuiltInRegistries registry, entries in protocol-id order. */
public final class RegistriesPass implements Pass {
   @Override
   public String name() {
      return "registries";
   }

   @Override
   public void run(Ctx ctx) throws Exception {
      JsonObject root = new JsonObject();
      for (var e : BuiltInRegistries.REGISTRY.entrySet()) {
         Registry<?> reg = e.getValue();
         JsonObject o = new JsonObject();
         o.addProperty("default", reg instanceof DefaultedRegistry<?> d ? d.getDefaultKey().toString() : null);
         JsonArray entries = new JsonArray();
         for (int i = 0; i < reg.size(); i++) {
            Object v = reg.byId(i);
            @SuppressWarnings("unchecked")
            Registry<Object> r = (Registry<Object>)reg;
            entries.add(r.getKey(v).toString());
         }
         o.add("entries", entries);
         root.add(e.getKey().identifier().toString(), o);
      }
      ctx.write("registries.json", Ctx.sortKeys(root));
   }
}
