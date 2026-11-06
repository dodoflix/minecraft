import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import com.mojang.serialization.JsonOps;
import java.io.PrintWriter;
import java.util.ArrayList;
import java.util.List;
import net.minecraft.core.Holder;
import net.minecraft.core.Registry;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.registries.Registries;
import net.minecraft.nbt.NbtOps;
import net.minecraft.nbt.Tag;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.RegistryOps;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.level.biome.Biome;
import net.minecraft.world.level.dimension.DimensionType;
import net.minecraft.world.level.dimension.LevelStem;
import net.minecraft.world.level.levelgen.presets.WorldPreset;

/**
 * Goldens for `LevelStem.CODEC` as the dimension registry / `world_gen_settings.dat` use it: for every crafted stem the
 * canonical NBT the codec re-encodes (inline dimension types, inline-biome flat settings, inline biomes in biome
 * sources, malformed variants of each) or ERR.
 * Usage: MC_EXTRA_CP=tools/extractor/build scripts/vanilla-java.sh -o crates/mc-game/tests/fixtures/world_dimensions_stems.txt crates/mc-game/tests/oracle/WorldDimensionsOracle.java crates/mc-game/tests/fixtures/world_dimensions_stems.txt
 * Output: crates/mc-game/tests/fixtures/world_dimensions_stems.txt (tests/golden/world_dimensions_golden.rs replays every `STEM` line)
 * Cases named `noenc:` are compared for acceptance only (see the test).
 */
public class WorldDimensionsOracle {
   static RegistryAccess.Frozen access;
   static RegistryOps<JsonElement> jsonOps;
   static RegistryOps<Tag> nbtOps;

   static JsonObject presetStems(String name) {
      Registry<WorldPreset> presets = access.lookupOrThrow(Registries.WORLD_PRESET);
      JsonObject stems = new JsonObject();
      for (var entry : presets.getValue(Identifier.withDefaultNamespace(name)).createWorldDimensions().dimensions().entrySet()) {
         stems.add(entry.getKey().identifier().toString(), LevelStem.CODEC.encodeStart(jsonOps, entry.getValue()).result().orElseThrow());
      }
      return stems;
   }

   static JsonObject dimensionTypeJson(String name) {
      return DimensionType.DIRECT_CODEC.encodeStart(jsonOps,
         access.lookupOrThrow(Registries.DIMENSION_TYPE).getValue(Identifier.withDefaultNamespace(name))).result().orElseThrow().getAsJsonObject();
   }

   static JsonObject biomeJson(String name) {
      Holder<Biome> h = access.lookupOrThrow(Registries.BIOME).getOrThrow(ResourceKey.create(Registries.BIOME, Identifier.withDefaultNamespace(name)));
      return Biome.DIRECT_CODEC.encodeStart(jsonOps, h.value()).result().orElseThrow().getAsJsonObject();
   }

   static JsonObject stem(JsonElement type, JsonElement generator) {
      JsonObject stem = new JsonObject();
      stem.add("type", type);
      stem.add("generator", generator);
      return stem;
   }

   static JsonObject flat(String settings) {
      JsonObject gen = new JsonObject();
      gen.addProperty("type", "minecraft:flat");
      gen.add("settings", JsonParser.parseString(settings));
      return gen;
   }

   static String flatWith(String biomeField, String extra) {
      return "{\"layers\":[{\"block\":\"minecraft:bedrock\",\"height\":1},{\"block\":\"minecraft:dirt\",\"height\":2},{\"block\":\"minecraft:snow\",\"height\":1}]"
         + (biomeField == null ? "" : ",\"biome\":" + biomeField) + extra + "}";
   }

   static void stemCase(PrintWriter out, String name, JsonElement json) {
      out.println("STEM " + name + " " + json);
      try {
         LevelStem stem = LevelStem.CODEC.parse(jsonOps, json).getOrThrow();
         Tag tag = LevelStem.CODEC.encodeStart(nbtOps, stem).getOrThrow();
         out.println("ENC " + name + " OK " + tag);
      } catch (Throwable t) {
         out.println("ENC " + name + " ERR");
      }
   }

   public static void main(String[] args) throws Exception {
      access = extractor.Setup.bootstrap();
      jsonOps = RegistryOps.create(JsonOps.INSTANCE, access);
      nbtOps = RegistryOps.create(NbtOps.INSTANCE, access);
      try (PrintWriter out = new PrintWriter(args[0])) {
         String[] presets = {"normal", "flat", "debug_all_block_states", "large_biomes", "amplified", "single_biome_surface", "flat_all_dimensions"};
         for (String preset : presets) {
            for (var entry : presetStems(preset).entrySet()) {
               stemCase(out, "preset:" + preset + ":" + entry.getKey(), entry.getValue());
            }
         }
         JsonObject normal = presetStems("normal");
         JsonObject overworldGenerator = normal.getAsJsonObject("minecraft:overworld").getAsJsonObject("generator");

         // ---- inline dimension types
         for (String type : new String[]{"overworld", "overworld_caves", "the_nether", "the_end"}) {
            stemCase(out, "inline_type:" + type, stem(dimensionTypeJson(type), overworldGenerator));
         }
         JsonObject base = dimensionTypeJson("overworld");
         String[][] mutations = {
            {"height", "100"}, {"height", "8"}, {"height", "4100"}, {"min_y", "-8"}, {"min_y", "-4000"}, {"min_y", "2040"},
            {"logical_height", "500"}, {"logical_height", "-1"}, {"coordinate_scale", "0"}, {"coordinate_scale", "0.00001"},
            {"coordinate_scale", "30000001"}, {"coordinate_scale", "8"}, {"ambient_light", "0.5"}, {"ambient_light", "1"},
            {"monster_spawn_block_light_limit", "16"}, {"monster_spawn_block_light_limit", "7"},
            {"skybox", "\"end\""}, {"skybox", "\"none\""}, {"skybox", "\"sideways\""}, {"cardinal_light", "\"nether\""},
            {"cardinal_light", "\"x\""}, {"has_skylight", "1"}, {"has_skylight", "\"true\""}, {"has_fixed_time", "true"},
            {"has_ceiling", "null"}, {"infiniburn", "\"#minecraft:infiniburn_nether\""}, {"infiniburn", "\"#minecraft:nope\""},
            {"infiniburn", "[\"minecraft:stone\",\"minecraft:dirt\"]"}, {"infiniburn", "[\"minecraft:nope\"]"},
            {"infiniburn", "\"minecraft:stone\""}, {"monster_spawn_light_level", "5"},
            {"monster_spawn_light_level", "{\"type\":\"minecraft:uniform\",\"min_inclusive\":0,\"max_inclusive\":7}"},
            {"monster_spawn_light_level", "{\"type\":\"minecraft:constant\",\"value\":16}"},
            {"monster_spawn_light_level", "{\"type\":\"minecraft:uniform\",\"min_inclusive\":9,\"max_inclusive\":1}"},
            {"timelines", "\"#minecraft:in_overworld\""}, {"timelines", "[\"minecraft:day\"]"}, {"timelines", "[\"minecraft:nope\"]"},
            {"default_clock", "\"minecraft:overworld\""}, {"default_clock", "\"minecraft:nope\""},
            {"unknown_key", "5"}, {"attributes", "{}"}, {"attributes", "{\"minecraft:visual/fog_color\":\"#ff0000\"}"},
            {"attributes", "{\"minecraft:visual/fog_color\":16711680}"}, {"attributes", "{\"minecraft:visual/nope\":1}"},
            {"attributes", "{\"minecraft:visual/sky_color\":5}"},
         };
         int index = 0;
         for (String[] m : mutations) {
            JsonObject type = base.deepCopy();
            type.add(m[0], JsonParser.parseString(m[1]));
            stemCase(out, "type_mut" + index++ + ":" + m[0], stem(type, overworldGenerator));
         }
         for (String key : new String[]{"has_skylight", "has_ceiling", "has_ender_dragon_fight", "coordinate_scale", "min_y", "height", "logical_height",
            "infiniburn", "ambient_light", "monster_spawn_light_level", "monster_spawn_block_light_limit"}) {
            JsonObject type = base.deepCopy();
            type.remove(key);
            stemCase(out, "type_missing:" + key, stem(type, overworldGenerator));
         }
         stemCase(out, "type_empty_object", stem(new JsonObject(), overworldGenerator));
         stemCase(out, "type_unknown_ref", stem(JsonParser.parseString("\"minecraft:nope\""), overworldGenerator));
         stemCase(out, "type_bad_id", stem(JsonParser.parseString("\"Bad Id\""), overworldGenerator));
         stemCase(out, "type_number", stem(JsonParser.parseString("5"), overworldGenerator));
         stemCase(out, "type_array", stem(JsonParser.parseString("[]"), overworldGenerator));
         stemCase(out, "type_null", stem(JsonParser.parseString("null"), overworldGenerator));
         stemCase(out, "type_ref_nether", stem(JsonParser.parseString("\"minecraft:the_nether\""), overworldGenerator));
         stemCase(out, "stem_missing_type", overworldGenerator.deepCopy());
         JsonObject noGenerator = new JsonObject();
         noGenerator.addProperty("type", "minecraft:overworld");
         stemCase(out, "stem_missing_generator", noGenerator);
         stemCase(out, "stem_unknown_generator", stem(JsonParser.parseString("\"minecraft:overworld\""), JsonParser.parseString("{\"type\":\"minecraft:nope\"}")));
         stemCase(out, "stem_generator_no_type", stem(JsonParser.parseString("\"minecraft:overworld\""), new JsonObject()));

         // ---- inline biomes in flat settings (re-encode of the biome holder and of the defaulted fields)
         JsonObject plains = biomeJson("plains");
         JsonObject desert = biomeJson("desert");
         JsonObject theVoid = biomeJson("the_void");
         JsonObject frozen = biomeJson("frozen_ocean");
         JsonElement overworldType = JsonParser.parseString("\"minecraft:overworld\"");
         List<String> flats = new ArrayList<>();
         flats.add(flatWith(plains.toString(), ""));
         flats.add(flatWith(desert.toString(), ",\"features\":true,\"lakes\":true"));
         flats.add(flatWith(theVoid.toString(), ",\"features\":true"));
         flats.add(flatWith(frozen.toString(), ",\"features\":true,\"lakes\":false"));
         JsonObject warmer = plains.deepCopy();
         warmer.addProperty("temperature", 1.25);
         warmer.addProperty("downfall", 0.0);
         flats.add(flatWith(warmer.toString(), ""));
         JsonObject extraKey = plains.deepCopy();
         extraKey.addProperty("unknown_key", 5);
         flats.add(flatWith(extraKey.toString(), ""));
         JsonObject noTemperature = plains.deepCopy();
         noTemperature.remove("temperature");
         flats.add(flatWith(noTemperature.toString(), ""));
         JsonObject noEffects = plains.deepCopy();
         noEffects.remove("effects");
         flats.add(flatWith(noEffects.toString(), ""));
         JsonObject emptyFeatures = plains.deepCopy();
         emptyFeatures.add("features", new JsonArray());
         flats.add(flatWith(emptyFeatures.toString(), ""));
         JsonObject badCarver = plains.deepCopy();
         badCarver.addProperty("carvers", "minecraft:nope");
         flats.add(flatWith(badCarver.toString(), ""));
         JsonObject badFeature = plains.deepCopy();
         JsonArray steps = new JsonArray();
         JsonArray step = new JsonArray();
         step.add("minecraft:nope");
         steps.add(step);
         badFeature.add("features", steps);
         flats.add(flatWith(badFeature.toString(), ""));
         JsonObject colorsAsInts = plains.deepCopy();
         JsonObject effects = colorsAsInts.getAsJsonObject("effects");
         effects.addProperty("water_color", 4159204);
         effects.add("grass_color", JsonParser.parseString("[0.5, 0.25, 1.0]"));
         flats.add(flatWith(colorsAsInts.toString(), ""));
         JsonObject badColor = plains.deepCopy();
         badColor.getAsJsonObject("effects").addProperty("water_color", "red");
         flats.add(flatWith(badColor.toString(), ""));
         JsonObject modifierFrozen = plains.deepCopy();
         modifierFrozen.addProperty("temperature_modifier", "frozen");
         flats.add(flatWith(modifierFrozen.toString(), ""));
         JsonObject modifierBad = plains.deepCopy();
         modifierBad.addProperty("temperature_modifier", "hot");
         flats.add(flatWith(modifierBad.toString(), ""));
         JsonObject badAttribute = plains.deepCopy();
         badAttribute.add("attributes", JsonParser.parseString("{\"minecraft:visual/nope\":1}"));
         flats.add(flatWith(badAttribute.toString(), ""));
         for (String lenient : new String[]{"{}", "[]", "null", "true", "5", "\"minecraft:nope\"", "\"bad id\"", "\"#minecraft:is_ocean\"", "\"minecraft:desert\"", "\"desert\""}) {
            flats.add(flatWith(lenient, ",\"features\":true"));
         }
         flats.add(flatWith(null, ""));
         flats.add(flatWith(null, ",\"lakes\":true,\"structure_overrides\":\"minecraft:villages\""));
         flats.add(flatWith(desert.toString(), ",\"structure_overrides\":[\"minecraft:villages\",\"minecraft:strongholds\"]"));
         flats.add(flatWith("\"minecraft:plains\"", ",\"structure_overrides\":\"minecraft:nope\""));
         flats.add(flatWith("\"minecraft:plains\"", ",\"structure_overrides\":\"#minecraft:nope\""));
         flats.add(flatWith("\"minecraft:plains\"", ",\"structure_overrides\":5"));
         flats.add(flatWith("\"minecraft:plains\"", ",\"structure_overrides\":null"));
         flats.add(flatWith("\"minecraft:plains\"", ",\"lakes\":\"yes\""));
         flats.add(flatWith("\"minecraft:plains\"", ",\"features\":1"));
         flats.add(flatWith("\"minecraft:plains\"", ",\"unknown\":1"));
         flats.add("{\"layers\":[{\"block\":\"dirt\",\"height\":1.0},{\"block\":\"stone\",\"height\":2.9}],\"biome\":\"plains\"}");
         flats.add("{\"layers\":[]}");
         flats.add("{\"layers\":[{\"block\":\"minecraft:dirt\",\"height\":4064}]}");
         flats.add("{\"layers\":[{\"block\":\"minecraft:dirt\",\"height\":4064},{\"block\":\"minecraft:stone\",\"height\":1}]}");
         flats.add("{\"layers\":[{\"block\":\"minecraft:dirt\",\"height\":4065}]}");
         flats.add("{\"layers\":[{\"block\":\"minecraft:dirt\",\"height\":-1}]}");
         flats.add("{\"layers\":[{\"block\":\"minecraft:nope\",\"height\":1}]}");
         flats.add("{\"layers\":[{\"block\":\"minecraft:dirt\"}]}");
         flats.add("{\"layers\":{\"block\":\"minecraft:dirt\",\"height\":1}}");
         flats.add("{\"biome\":\"minecraft:plains\"}");
         flats.add("[]");
         flats.add("5");
         int i = 0;
         for (String settings : flats) {
            // `flat10`: an unresolvable placed feature of an inline biome is dropped from the partial result vanilla keeps
            // (`lenientOptionalFieldOf`); the accepted stem is compared, not its re-encode
            stemCase(out, (i == 10 ? "noenc:flat" : "flat") + i, stem(overworldType, flat(settings)));
            i++;
         }

         // ---- inline biomes in the biome sources of noise generators
         JsonObject noise = overworldGenerator.deepCopy();
         String[] sources = {
            "{\"type\":\"minecraft:fixed\",\"biome\":" + desert + "}",
            "{\"type\":\"minecraft:fixed\",\"biome\":\"minecraft:desert\"}",
            "{\"type\":\"minecraft:fixed\",\"biome\":{}}",
            "{\"type\":\"minecraft:fixed\",\"biome\":\"minecraft:nope\"}",
            "{\"type\":\"minecraft:fixed\"}",
            "{\"type\":\"minecraft:checkerboard\",\"biomes\":" + desert + ",\"scale\":3}",
            "{\"type\":\"minecraft:checkerboard\",\"biomes\":[" + desert + ",\"minecraft:plains\"]}",
            "{\"type\":\"minecraft:checkerboard\",\"biomes\":[\"minecraft:plains\"],\"scale\":63}",
            "{\"type\":\"minecraft:checkerboard\",\"biomes\":\"#minecraft:is_ocean\"}",
            "{\"type\":\"minecraft:checkerboard\",\"biomes\":\"minecraft:plains\"}",
            "{\"type\":\"minecraft:checkerboard\",\"biomes\":[{}]}",
            "{\"type\":\"minecraft:multi_noise\",\"biomes\":[{\"biome\":" + desert + ",\"parameters\":{\"temperature\":0,\"humidity\":0,\"continentalness\":0,\"erosion\":0,\"weirdness\":0,\"depth\":0,\"offset\":0}}]}",
            "{\"type\":\"minecraft:multi_noise\",\"biomes\":[{\"biome\":\"minecraft:plains\",\"parameters\":{\"temperature\":[-0.5,0.5],\"humidity\":0,\"continentalness\":0,\"erosion\":0,\"weirdness\":0,\"depth\":0,\"offset\":0}}]}",
            "{\"type\":\"minecraft:multi_noise\",\"preset\":\"minecraft:nether\"}",
            "{\"type\":\"minecraft:multi_noise\",\"preset\":\"minecraft:nope\"}",
            "{\"type\":\"minecraft:the_end\"}",
            "{\"type\":\"minecraft:nope\"}",
         };
         i = 0;
         for (String source : sources) {
            JsonObject gen = noise.deepCopy();
            gen.add("biome_source", JsonParser.parseString(source));
            stemCase(out, "source" + i++, stem(overworldType, gen));
         }
         // the noise settings holder: reference, inline, bad
         JsonObject inlineSettings = noise.deepCopy();
         inlineSettings.add("settings", net.minecraft.world.level.levelgen.NoiseGeneratorSettings.DIRECT_CODEC.encodeStart(jsonOps,
            access.lookupOrThrow(Registries.NOISE_SETTINGS).getValue(Identifier.withDefaultNamespace("nether"))).result().orElseThrow());
         stemCase(out, "noenc:noise_inline_settings", stem(overworldType, inlineSettings));
         JsonObject badSettings = noise.deepCopy();
         badSettings.addProperty("settings", "minecraft:nope");
         stemCase(out, "noise_unknown_settings", stem(overworldType, badSettings));
         JsonObject debug = new JsonObject();
         debug.addProperty("type", "minecraft:debug");
         stemCase(out, "debug", stem(overworldType, debug));
      }
   }
}
