package extractor;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import net.minecraft.core.component.DataComponentPatch;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.item.BlockItem;
import net.minecraft.world.item.Item;

/** items.json: per item class, block form, default components (DataComponentPatch JSON, as in the vanilla components report). */
public final class ItemsPass implements Pass {
   @Override
   public String name() {
      return "items";
   }

   @Override
   public void run(Ctx ctx) throws Exception {
      JsonArray items = new JsonArray();
      for (int i = 0; i < BuiltInRegistries.ITEM.size(); i++) {
         Item item = BuiltInRegistries.ITEM.byId(i);
         JsonObject o = new JsonObject();
         o.addProperty("id", i);
         o.addProperty("name", BuiltInRegistries.ITEM.getKey(item).toString());
         o.addProperty("class", R.simpleName(item.getClass()));
         JsonArray chain = new JsonArray();
         for (Class<?> c = item.getClass(); c != null && c != Object.class; c = c.getSuperclass()) {
            if (!c.getSimpleName().isEmpty()) {
               chain.add(c.getSimpleName());
            }
         }
         o.add("classes", chain);
         o.addProperty("description_id", item.getDescriptionId());
         o.addProperty("max_stack_size", item.getDefaultMaxStackSize());
         o.addProperty("block", item instanceof BlockItem bi ? BuiltInRegistries.BLOCK.getKey(bi.getBlock()).toString() : null);
         var rem = item.getCraftingRemainder();
         o.addProperty("crafting_remainder", rem == null ? null : BuiltInRegistries.ITEM.getKey(rem.item().value()).toString());
         JsonArray feats = new JsonArray();
         net.minecraft.world.flag.FeatureFlags.REGISTRY.toNames(item.requiredFeatures()).forEach(f -> feats.add(f.toString()));
         o.add("required_features", feats);
         DataComponentPatch patch = DataComponentPatch.builder().set(item.components()).build();
         JsonElement comps = DataComponentPatch.CODEC
            .encodeStart(ctx.jsonOps, patch)
            .getOrThrow(err -> new IllegalStateException("components of " + o.get("name") + ": " + err));
         o.add("components", Ctx.sortKeys(comps));
         items.add(o);
      }
      ctx.write("items.json", items);
   }
}
