package extractor;

import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.properties.Property;
import net.minecraft.world.level.material.Fluid;
import net.minecraft.world.level.material.FluidState;

/** fluids.json: fluid types and their fluid states (ids as in Fluid.FLUID_STATE_REGISTRY). */
public final class FluidsPass implements Pass {
   @Override
   public String name() {
      return "fluids";
   }

   @SuppressWarnings({"unchecked", "rawtypes"})
   @Override
   public void run(Ctx ctx) throws Exception {
      JsonArray fluids = new JsonArray();
      for (int i = 0; i < BuiltInRegistries.FLUID.size(); i++) {
         Fluid f = BuiltInRegistries.FLUID.byId(i);
         JsonObject o = new JsonObject();
         o.addProperty("id", i);
         o.addProperty("name", BuiltInRegistries.FLUID.getKey(f).toString());
         o.addProperty("class", R.simpleName(f.getClass()));
         var bucket = f.getBucket();
         o.addProperty("bucket", bucket == net.minecraft.world.item.Items.AIR ? null : BuiltInRegistries.ITEM.getKey(bucket).toString());
         o.addProperty("explosion_resistance", (float)R.call(f, "getExplosionResistance"));
         var pickup = f.getPickupSound();
         o.addProperty("pickup_sound", pickup.isPresent() ? pickup.get().location().toString() : null);
         o.addProperty("default_state", Fluid.FLUID_STATE_REGISTRY.getId(f.defaultFluidState()));
         JsonArray states = new JsonArray();
         for (FluidState s : f.getStateDefinition().getPossibleStates()) {
            JsonObject so = new JsonObject();
            so.addProperty("id", Fluid.FLUID_STATE_REGISTRY.getId(s));
            JsonObject props = new JsonObject();
            for (Property<?> p : s.getProperties()) {
               props.addProperty(p.getName(), ((Property)p).getName((Comparable)s.getValue((Property)p)));
            }
            so.add("properties", props);
            so.addProperty("amount", s.getAmount());
            so.addProperty("is_source", s.isSource());
            so.addProperty("is_empty", s.isEmpty());
            so.addProperty("own_height", s.getOwnHeight());
            so.addProperty("random_ticking", s.isRandomlyTicking());
            so.addProperty("legacy_block_state", Block.getId(s.createLegacyBlock()));
            so.addProperty("shape", ctx.shapes.intern(f.getShape(s, net.minecraft.world.level.EmptyBlockGetter.INSTANCE, net.minecraft.core.BlockPos.ZERO)));
            states.add(so);
         }
         o.add("states", states);
         fluids.add(o);
      }
      ctx.write("fluids.json", fluids);
   }
}
