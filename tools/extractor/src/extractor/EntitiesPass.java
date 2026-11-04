package extractor;

import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import java.lang.reflect.Field;
import java.lang.reflect.Modifier;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.tags.TagKey;
import net.minecraft.world.entity.EntityAttachment;
import net.minecraft.world.entity.EntityDimensions;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.MobCategory;
import net.minecraft.world.entity.SpawnPlacementType;
import net.minecraft.world.entity.SpawnPlacementTypes;
import net.minecraft.world.entity.ai.attributes.Attribute;
import net.minecraft.world.entity.ai.attributes.AttributeInstance;
import net.minecraft.world.entity.ai.attributes.AttributeSupplier;
import net.minecraft.world.entity.ai.attributes.DefaultAttributes;
import net.minecraft.world.phys.Vec3;

/** entity_types.json + mob_categories (in the same file). */
public final class EntitiesPass implements Pass {
   @Override
   public String name() {
      return "entities";
   }

   private static String placementName(SpawnPlacementType t) throws Exception {
      for (Field f : SpawnPlacementTypes.class.getFields()) {
         if (Modifier.isStatic(f.getModifiers()) && f.get(null) == t) {
            return f.getName();
         }
      }
      return t.getClass().getName();
   }

   @SuppressWarnings("unchecked")
   @Override
   public void run(Ctx ctx) throws Exception {
      Map<EntityType<?>, Object> spawnData = R.getStatic(net.minecraft.world.entity.SpawnPlacements.class, "DATA_BY_TYPE");
      JsonArray types = new JsonArray();
      for (int i = 0; i < BuiltInRegistries.ENTITY_TYPE.size(); i++) {
         EntityType<?> t = BuiltInRegistries.ENTITY_TYPE.byId(i);
         JsonObject o = new JsonObject();
         o.addProperty("id", i);
         o.addProperty("name", BuiltInRegistries.ENTITY_TYPE.getKey(t).toString());
         o.addProperty("description_id", t.getDescriptionId());
         MobCategory cat = t.getCategory();
         o.addProperty("category", cat.name());
         EntityDimensions d = t.getDimensions();
         JsonObject dim = new JsonObject();
         dim.addProperty("width", d.width());
         dim.addProperty("height", d.height());
         dim.addProperty("eye_height", d.eyeHeight());
         dim.addProperty("fixed", d.fixed());
         Map<EntityAttachment, List<Vec3>> att = R.get(d.attachments(), "attachments");
         JsonObject ao = new JsonObject();
         for (var e : att.entrySet()) {
            JsonArray pts = new JsonArray();
            for (Vec3 v : e.getValue()) {
               JsonArray p = new JsonArray();
               p.add(v.x);
               p.add(v.y);
               p.add(v.z);
               pts.add(p);
            }
            ao.add(e.getKey().name(), pts);
         }
         dim.add("attachments", ao);
         o.add("dimensions", dim);
         o.addProperty("spawn_dimensions_scale", (float)R.get(t, "spawnDimensionsScale"));
         o.addProperty("client_tracking_range", t.clientTrackingRange());
         o.addProperty("update_interval", t.updateInterval());
         o.addProperty("track_deltas", t.trackDeltas());
         o.addProperty("fire_immune", t.fireImmune());
         o.addProperty("summonable", t.canSummon());
         o.addProperty("serializable", t.canSerialize());
         o.addProperty("can_spawn_far_from_player", t.canSpawnFarFromPlayer());
         o.addProperty("allowed_in_peaceful", t.isAllowedInPeaceful());
         o.addProperty("immune_to", ((TagKey<?>)R.get(t, "immuneTo")).location().toString());
         Optional<net.minecraft.resources.ResourceKey<?>> loot = (Optional<net.minecraft.resources.ResourceKey<?>>)(Optional<?>)t.getDefaultLootTable();
         o.addProperty("loot_table", loot.isPresent() ? loot.get().identifier().toString() : null);
         JsonArray feats = new JsonArray();
         net.minecraft.world.flag.FeatureFlags.REGISTRY.toNames(t.requiredFeatures()).forEach(f -> feats.add(f.toString()));
         o.add("required_features", feats);
         Object sd = spawnData.get(t);
         if (sd != null) {
            JsonObject sp = new JsonObject();
            sp.addProperty("placement", placementName(R.get(sd, "placement")));
            sp.addProperty("heightmap", ((Enum<?>)R.get(sd, "heightMap")).name());
            o.add("spawn_placement", sp);
         }
         if (DefaultAttributes.hasSupplier(t)) {
            AttributeSupplier sup = DefaultAttributes.getSupplier((EntityType<? extends net.minecraft.world.entity.LivingEntity>)t);
            Map<Holder<Attribute>, AttributeInstance> inst = R.get(sup, "instances");
            JsonObject attrs = new JsonObject();
            for (var e : inst.entrySet()) {
               attrs.addProperty(e.getKey().getRegisteredName(), e.getValue().getBaseValue());
            }
            o.add("default_attributes", attrs);
         }
         types.add(o);
      }
      JsonArray cats = new JsonArray();
      for (MobCategory c : MobCategory.values()) {
         JsonObject o = new JsonObject();
         o.addProperty("name", c.name());
         o.addProperty("serialized_name", c.getSerializedName());
         o.addProperty("max_instances_per_chunk", c.getMaxInstancesPerChunk());
         o.addProperty("friendly", c.isFriendly());
         o.addProperty("persistent", c.isPersistent());
         o.addProperty("despawn_distance", c.getDespawnDistance());
         o.addProperty("no_despawn_distance", c.getNoDespawnDistance());
         cats.add(o);
      }
      JsonObject root = new JsonObject();
      root.add("mob_categories", cats);
      root.add("entity_types", types);
      ctx.write("entity_types.json", root);
   }
}
