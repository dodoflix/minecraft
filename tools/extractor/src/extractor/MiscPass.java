package extractor;

import com.google.common.collect.BiMap;
import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import com.mojang.brigadier.arguments.IntegerArgumentType;
import java.lang.reflect.Field;
import java.lang.reflect.Modifier;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Collection;
import java.util.Enumeration;
import java.util.List;
import java.util.Map;
import java.util.jar.JarFile;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.syncher.EntityDataAccessor;
import net.minecraft.network.syncher.EntityDataSerializer;
import net.minecraft.network.syncher.EntityDataSerializers;
import net.minecraft.util.StringRepresentable;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityDimensions;
import net.minecraft.world.entity.Pose;
import net.minecraft.world.entity.ai.village.poi.PoiType;
import net.minecraft.world.item.DyeColor;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.FireBlock;
import net.minecraft.world.level.block.FlowerPotBlock;
import net.minecraft.world.level.block.WeatheringCopper;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.NoteBlockInstrument;
import net.minecraft.world.level.gamerules.GameRule;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.level.material.MapColor;

/** misc.json: other code-defined tables (not in the data pack) that later waves need. Each section is independent. */
public final class MiscPass implements Pass {
   @Override
   public String name() {
      return "misc";
   }

   private static String blockKey(Block b) {
      return BuiltInRegistries.BLOCK.getKey(b).toString();
   }

   private static JsonObject blockMap(Map<Block, Block> m) {
      JsonObject o = new JsonObject();
      m.entrySet().stream()
         .sorted(java.util.Comparator.comparing(e -> blockKey(e.getKey())))
         .forEach(e -> o.addProperty(blockKey(e.getKey()), blockKey(e.getValue())));
      return o;
   }

   @SuppressWarnings("unchecked")
   @Override
   public void run(Ctx ctx) throws Exception {
      JsonObject root = new JsonObject();

      // FireBlock flammability (igniteOdds = "encouragement", burnOdds = "flammability")
      FireBlock fire = (FireBlock)Blocks.FIRE;
      JsonObject fireOdds = new JsonObject();
      it.unimi.dsi.fastutil.objects.Object2IntMap<Block> ignite = R.get(fire, "igniteOdds");
      it.unimi.dsi.fastutil.objects.Object2IntMap<Block> burn = R.get(fire, "burnOdds");
      ignite.object2IntEntrySet().stream()
         .sorted(java.util.Comparator.comparing(e -> blockKey(e.getKey())))
         .forEach(e -> {
            JsonObject o = new JsonObject();
            o.addProperty("ignite_odds", e.getIntValue());
            o.addProperty("burn_odds", burn.getInt(e.getKey()));
            fireOdds.add(blockKey(e.getKey()), o);
         });
      root.add("fire_odds", fireOdds);

      root.add("weathering_next", blockMap((BiMap<Block, Block>)WeatheringCopper.NEXT_BY_BLOCK.get()));
      root.add("waxables", blockMap((BiMap<Block, Block>)net.minecraft.world.item.HoneycombItem.WAXABLES.get()));
      root.add("potted_by_content", blockMap(R.getStatic(FlowerPotBlock.class, "POTTED_BY_CONTENT")));

      JsonArray rules = new JsonArray();
      for (int i = 0; i < BuiltInRegistries.GAME_RULE.size(); i++) {
         GameRule<?> r = BuiltInRegistries.GAME_RULE.byId(i);
         JsonObject o = new JsonObject();
         o.addProperty("id", i);
         o.addProperty("name", BuiltInRegistries.GAME_RULE.getKey(r).toString());
         o.addProperty("type", ((Enum<?>)R.get(r, "gameRuleType")).name());
         Object def = R.get(r, "defaultValue");
         if (def instanceof Boolean bv) {
            o.addProperty("default", bv);
         } else {
            o.addProperty("default", (Number)def);
         }
         Object arg = R.get(r, "argument");
         if (arg instanceof IntegerArgumentType ia) {
            o.addProperty("min", ia.getMinimum());
            o.addProperty("max", ia.getMaximum());
         }
         o.addProperty("category", ((net.minecraft.world.level.gamerules.GameRuleCategory)R.get(r, "category")).id().toString());
         rules.add(o);
      }
      root.add("game_rules", rules);

      JsonArray mapColors = new JsonArray();
      for (int i = 0; i < 64; i++) {
         MapColor c = MapColor.byId(i);
         if (c.id == i) {
            JsonObject o = new JsonObject();
            o.addProperty("id", c.id);
            o.addProperty("col", c.col);
            mapColors.add(o);
         }
      }
      root.add("map_colors", mapColors);

      JsonArray dyes = new JsonArray();
      for (DyeColor d : DyeColor.values()) {
         JsonObject o = new JsonObject();
         o.addProperty("id", d.getId());
         o.addProperty("name", d.getName());
         o.addProperty("const", d.name());
         o.addProperty("map_color", d.getMapColor().id);
         o.addProperty("terracotta_color", d.getTerracottaColor().id);
         o.addProperty("texture_diffuse_color", d.getTextureDiffuseColor());
         o.addProperty("firework_color", d.getFireworkColor());
         o.addProperty("text_color", d.getTextColor());
         dyes.add(o);
      }
      root.add("dye_colors", dyes);

      JsonArray instruments = new JsonArray();
      for (NoteBlockInstrument n : NoteBlockInstrument.values()) {
         JsonObject o = new JsonObject();
         o.addProperty("const", n.name());
         o.addProperty("name", n.getSerializedName());
         o.addProperty("sound", n.getSoundEvent().getRegisteredName());
         o.addProperty("tunable", n.isTunable());
         o.addProperty("has_custom_sound", n.hasCustomSound());
         o.addProperty("works_above_note_block", n.worksAboveNoteBlock());
         instruments.add(o);
      }
      root.add("note_block_instruments", instruments);

      JsonArray hm = new JsonArray();
      for (Heightmap.Types t : Heightmap.Types.values()) {
         JsonObject o = new JsonObject();
         o.addProperty("const", t.name());
         o.addProperty("id", t.ordinal());
         o.addProperty("serialized_name", t.getSerializedName());
         o.addProperty("usage", ((Enum<?>)R.get(t, "usage")).name());
         hm.add(o);
      }
      root.add("heightmap_types", hm);

      JsonArray poi = new JsonArray();
      for (int i = 0; i < BuiltInRegistries.POINT_OF_INTEREST_TYPE.size(); i++) {
         PoiType p = BuiltInRegistries.POINT_OF_INTEREST_TYPE.byId(i);
         JsonObject o = new JsonObject();
         o.addProperty("name", BuiltInRegistries.POINT_OF_INTEREST_TYPE.getKey(p).toString());
         JsonArray states = new JsonArray();
         p.matchingStates().stream().map(Block::getId).sorted().forEach(states::add);
         o.add("matching_states", states);
         o.addProperty("max_tickets", p.maxTickets());
         o.addProperty("valid_range", p.validRange());
         poi.add(o);
      }
      root.add("poi_types", poi);

      // Avatar (player) per-pose dimensions
      JsonObject poses = new JsonObject();
      Map<Pose, EntityDimensions> avatarPoses = R.getStatic(net.minecraft.world.entity.Avatar.class, "POSES");
      for (var e : avatarPoses.entrySet()) {
         EntityDimensions d = e.getValue();
         JsonObject o = new JsonObject();
         o.addProperty("width", d.width());
         o.addProperty("height", d.height());
         o.addProperty("eye_height", d.eyeHeight());
         o.addProperty("fixed", d.fixed());
         poses.add(e.getKey().name(), o);
      }
      root.add("avatar_pose_dimensions", poses);

      // enums that appear on the wire / in saved data, in ordinal order
      JsonObject enums = new JsonObject();
      for (Class<?> c : new Class<?>[]{
         Pose.class, net.minecraft.world.entity.EquipmentSlot.class, net.minecraft.world.entity.HumanoidArm.class,
         net.minecraft.world.InteractionHand.class, net.minecraft.world.Difficulty.class, net.minecraft.world.level.GameType.class,
         net.minecraft.world.level.block.Rotation.class, net.minecraft.world.level.block.Mirror.class,
         net.minecraft.world.entity.player.ChatVisiblity.class, net.minecraft.world.item.ItemDisplayContext.class,
         net.minecraft.world.entity.EntitySpawnReason.class, net.minecraft.world.level.material.PushReaction.class,
         net.minecraft.world.entity.EntityAttachment.class, net.minecraft.world.effect.MobEffectCategory.class,
         net.minecraft.world.entity.ai.attributes.Attribute.Sentiment.class,
         net.minecraft.world.entity.ai.attributes.AttributeModifier.Operation.class,
         net.minecraft.world.level.levelgen.Heightmap.Usage.class, net.minecraft.world.level.levelgen.Heightmap.Types.class, net.minecraft.world.level.gamerules.GameRuleType.class,
         net.minecraft.world.entity.MobCategory.class,
      }) {
         JsonArray a = new JsonArray();
         for (Object k : c.getEnumConstants()) {
            JsonObject o = new JsonObject();
            o.addProperty("name", ((Enum<?>)k).name());
            if (k instanceof StringRepresentable sr) {
               o.addProperty("serialized_name", sr.getSerializedName());
            }
            a.add(o);
         }
         enums.add(c.getName(), a);
      }
      root.add("enums", enums);

      JsonArray placements = new JsonArray();
      for (Field f : net.minecraft.world.entity.SpawnPlacementTypes.class.getFields()) {
         if (Modifier.isStatic(f.getModifiers())) {
            placements.add(f.getName());
         }
      }
      root.add("spawn_placement_types", placements);

      // synched entity data: serializer ids + every static EntityDataAccessor (SynchedEntityData.defineId) per entity class
      JsonArray sers = new JsonArray();
      Map<EntityDataSerializer<?>, String> serNames = new java.util.IdentityHashMap<>();
      for (Field f : EntityDataSerializers.class.getFields()) {
         if (Modifier.isStatic(f.getModifiers()) && EntityDataSerializer.class.isAssignableFrom(f.getType())) {
            serNames.put((EntityDataSerializer<?>)f.get(null), f.getName());
         }
      }
      for (int i = 0; ; i++) {
         EntityDataSerializer<?> s = EntityDataSerializers.getSerializer(i);
         if (s == null) {
            break;
         }
         JsonObject o = new JsonObject();
         o.addProperty("id", i);
         o.addProperty("const", serNames.get(s));
         sers.add(o);
      }
      root.add("entity_data_serializers", sers);

      JsonArray accessors = new JsonArray();
      Path jar = Path.of(Entity.class.getProtectionDomain().getCodeSource().getLocation().toURI());
      List<String> classNames = new ArrayList<>();
      try (JarFile jf = new JarFile(jar.toFile())) {
         Enumeration<java.util.jar.JarEntry> en = jf.entries();
         while (en.hasMoreElements()) {
            String n = en.nextElement().getName();
            if (n.startsWith("net/minecraft/world/entity/") && n.endsWith(".class") && !n.contains("$")) {
               classNames.add(n.substring(0, n.length() - 6).replace('/', '.'));
            }
         }
      }
      classNames.sort(null);
      for (String cn : classNames) {
         Class<?> c;
         try {
            c = Class.forName(cn);
         } catch (Throwable t) {
            continue;
         }
         if (!Entity.class.isAssignableFrom(c)) {
            continue;
         }
         for (Field f : c.getDeclaredFields()) {
            if (Modifier.isStatic(f.getModifiers()) && f.getType() == EntityDataAccessor.class) {
               f.setAccessible(true);
               EntityDataAccessor<?> a = (EntityDataAccessor<?>)f.get(null);
               JsonObject o = new JsonObject();
               o.addProperty("class", c.getSimpleName());
               o.addProperty("field", f.getName());
               o.addProperty("id", a.id());
               o.addProperty("serializer", serNames.get(a.serializer()));
               o.addProperty("serializer_id", EntityDataSerializers.getSerializedId(a.serializer()));
               accessors.add(o);
            }
         }
      }
      root.add("entity_data_accessors", accessors);

      ctx.write("misc.json", root);
   }
}
