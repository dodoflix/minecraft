package extractor;

import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import java.lang.reflect.Field;
import java.lang.reflect.Modifier;
import java.util.IdentityHashMap;
import java.util.Map;
import java.util.Set;
import net.minecraft.core.Holder;
import net.minecraft.core.Registry;
import net.minecraft.core.component.DataComponentType;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.particles.ParticleType;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.sounds.SoundEvent;
import net.minecraft.world.effect.MobEffect;
import net.minecraft.world.effect.MobEffectInstance;
import net.minecraft.world.entity.ai.attributes.Attribute;
import net.minecraft.world.entity.ai.attributes.RangedAttribute;
import net.minecraft.world.flag.FeatureFlagSet;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.inventory.MenuType;
import net.minecraft.world.item.alchemy.Potion;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.entity.BlockEntityType;
import net.minecraft.world.level.gameevent.GameEvent;

/** registry_data.json: code-defined data of the small registries (attributes, effects, potions, sounds, particles, ...). */
public final class SimpleRegistriesPass implements Pass {
   @Override
   public String name() {
      return "simple_registries";
   }

   private static JsonArray feats(FeatureFlagSet s) {
      JsonArray a = new JsonArray();
      FeatureFlags.REGISTRY.toNames(s).forEach(i -> a.add(i.toString()));
      return a;
   }

   private static <T> JsonObject base(Registry<T> reg, T v) {
      JsonObject o = new JsonObject();
      o.addProperty("id", reg.getId(v));
      o.addProperty("name", reg.getKey(v).toString());
      return o;
   }

   private static <T> void each(Registry<T> reg, JsonArray out, java.util.function.BiConsumer<T, JsonObject> f) {
      for (int i = 0; i < reg.size(); i++) {
         T v = reg.byId(i);
         JsonObject o = base(reg, v);
         f.accept(v, o);
         out.add(o);
      }
   }

   @Override
   public void run(Ctx ctx) throws Exception {
      JsonObject root = new JsonObject();

      JsonArray attributes = new JsonArray();
      each(BuiltInRegistries.ATTRIBUTE, attributes, (a, o) -> {
         o.addProperty("description_id", a.getDescriptionId());
         o.addProperty("default", a.getDefaultValue());
         if (a instanceof RangedAttribute r) {
            o.addProperty("min", r.getMinValue());
            o.addProperty("max", r.getMaxValue());
         }
         o.addProperty("syncable", a.isClientSyncable());
         o.addProperty("sentiment", ((Enum<?>)R.get(a, "sentiment")).name());
      });
      root.add("attributes", attributes);

      JsonArray effects = new JsonArray();
      each(BuiltInRegistries.MOB_EFFECT, effects, (e, o) -> {
         o.addProperty("class", R.simpleName(e.getClass()));
         o.addProperty("description_id", e.getDescriptionId());
         o.addProperty("category", e.getCategory().name());
         o.addProperty("color", e.getColor());
         o.addProperty("instantaneous", e.isInstantaneous());
         o.addProperty("blend_in", e.getBlendInDurationTicks());
         o.addProperty("blend_out", e.getBlendOutDurationTicks());
         o.addProperty("blend_out_advance", e.getBlendOutAdvanceTicks());
         java.util.Optional<SoundEvent> snd = R.get(e, "soundOnAdded");
         o.addProperty("sound_on_added", snd.isPresent() ? snd.get().location().toString() : null);
         o.add("required_features", feats(e.requiredFeatures()));
         JsonArray mods = new JsonArray();
         Map<Holder<Attribute>, Object> m = R.get(e, "attributeModifiers");
         for (var en : m.entrySet()) {
            JsonObject mo = new JsonObject();
            mo.addProperty("attribute", en.getKey().getRegisteredName());
            mo.addProperty("id", R.<Object>get(en.getValue(), "id").toString());
            mo.addProperty("amount", (double)R.get(en.getValue(), "amount"));
            mo.addProperty("operation", ((Enum<?>)R.get(en.getValue(), "operation")).name());
            mods.add(mo);
         }
         o.add("attribute_modifiers", mods);
      });
      root.add("mob_effects", effects);

      JsonArray potions = new JsonArray();
      each(BuiltInRegistries.POTION, potions, (p, o) -> {
         o.addProperty("base_name", (String)R.get(p, "name"));
         o.add("required_features", feats(p.requiredFeatures()));
         JsonArray es = new JsonArray();
         for (MobEffectInstance mi : p.getEffects()) {
            JsonObject eo = new JsonObject();
            eo.addProperty("effect", mi.getEffect().getRegisteredName());
            eo.addProperty("duration", mi.getDuration());
            eo.addProperty("amplifier", mi.getAmplifier());
            eo.addProperty("ambient", mi.isAmbient());
            eo.addProperty("visible", mi.isVisible());
            eo.addProperty("show_icon", mi.showIcon());
            es.add(eo);
         }
         o.add("effects", es);
      });
      root.add("potions", potions);

      JsonArray events = new JsonArray();
      each(BuiltInRegistries.GAME_EVENT, events, (g, o) -> o.addProperty("notification_radius", g.notificationRadius()));
      root.add("game_events", events);

      JsonArray sounds = new JsonArray();
      each(BuiltInRegistries.SOUND_EVENT, sounds, (s, o) -> {
         o.addProperty("fixed_range", s.fixedRange().orElse(null));
      });
      root.add("sound_events", sounds);

      JsonArray particles = new JsonArray();
      each(BuiltInRegistries.PARTICLE_TYPE, particles, (p, o) -> {
         o.addProperty("override_limiter", p.getOverrideLimiter());
         o.addProperty("class", R.simpleName(p.getClass()));
      });
      root.add("particle_types", particles);

      JsonArray menus = new JsonArray();
      each(BuiltInRegistries.MENU, menus, (m, o) -> o.add("required_features", feats(m.requiredFeatures())));
      root.add("menu_types", menus);

      Map<DataComponentType<?>, String> compConsts = new IdentityHashMap<>();
      for (Field f : DataComponents.class.getFields()) {
         if (Modifier.isStatic(f.getModifiers()) && DataComponentType.class.isAssignableFrom(f.getType())) {
            compConsts.put((DataComponentType<?>)f.get(null), f.getName());
         }
      }
      JsonArray comps = new JsonArray();
      each(BuiltInRegistries.DATA_COMPONENT_TYPE, comps, (c, o) -> {
         o.addProperty("const", compConsts.get(c));
         o.addProperty("persistent", !c.isTransient());
         o.addProperty("network_synchronized", c.streamCodec() != null);
         o.addProperty("ignore_swap_animation", c.ignoreSwapAnimation());
      });
      root.add("data_component_types", comps);

      JsonArray bets = new JsonArray();
      each(BuiltInRegistries.BLOCK_ENTITY_TYPE, bets, (b, o) -> {
         Set<Block> valid = R.get(b, "validBlocks");
         JsonArray vb = new JsonArray();
         valid.stream().map(BuiltInRegistries.BLOCK::getKey).map(Object::toString).sorted().forEach(vb::add);
         o.add("valid_blocks", vb);
         o.addProperty("only_op_can_set_nbt", b.onlyOpCanSetNbt());
      });
      root.add("block_entity_types", bets);

      ctx.write("registry_data.json", root);
   }
}
