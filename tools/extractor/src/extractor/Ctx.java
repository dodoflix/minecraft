package extractor;

import com.google.gson.Gson;
import com.google.gson.GsonBuilder;
import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.mojang.serialization.JsonOps;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import net.minecraft.core.Registry;
import net.minecraft.core.HolderLookup;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.RegistryOps;

/** Shared state for all passes: output directory, JSON writer, shape table, codec ops. */
public final class Ctx {
   public final Path out;
   public final Gson gson = new GsonBuilder().disableHtmlEscaping().serializeSpecialFloatingPointValues().create();
   public final ShapeTable shapes = new ShapeTable();
   /** JSON ops with access to all registries (static + dynamic from the vanilla data pack). */
   public final RegistryOps<JsonElement> jsonOps;
   public final HolderLookup.Provider registryAccess;

   public Ctx(Path out, HolderLookup.Provider registryAccess) {
      this.out = out;
      this.registryAccess = registryAccess;
      this.jsonOps = RegistryOps.create(JsonOps.INSTANCE, registryAccess);
   }

   public void write(String file, JsonElement e) throws IOException {
      Files.createDirectories(this.out);
      Files.writeString(this.out.resolve(file), this.gson.toJson(e), StandardCharsets.UTF_8);
      System.out.println("wrote " + file);
   }

   /** Recursively sorts object keys (vanilla codecs emit map entries in hash order, which is not stable across runs). */
   public static JsonElement sortKeys(JsonElement e) {
      if (e.isJsonObject()) {
         JsonObject out = new JsonObject();
         e.getAsJsonObject().entrySet().stream()
            .sorted(java.util.Map.Entry.comparingByKey())
            .forEach(en -> out.add(en.getKey(), sortKeys(en.getValue())));
         return out;
      }
      if (e.isJsonArray()) {
         JsonArray out = new JsonArray();
         e.getAsJsonArray().forEach(x -> out.add(sortKeys(x)));
         return out;
      }
      return e;
   }

   public static JsonObject obj() {
      return new JsonObject();
   }

   public static JsonArray arr() {
      return new JsonArray();
   }

   /** Registry key string ("minecraft:stone") of a value, or null if absent. */
   public static <T> String key(Registry<T> registry, T value) {
      var k = registry.getKey(value);
      return k == null ? null : k.toString();
   }

   public static <T> JsonArray keys(Registry<T> registry, Iterable<? extends T> values) {
      JsonArray a = new JsonArray();
      for (T v : values) {
         a.add(key(registry, v));
      }
      return a;
   }

   @SuppressWarnings("unchecked")
   public static Registry<Object> registryByName(String name) {
      for (var e : BuiltInRegistries.REGISTRY.entrySet()) {
         if (e.getKey().identifier().toString().equals(name)) {
            return (Registry<Object>)(Registry<?>)e.getValue();
         }
      }
      throw new IllegalArgumentException(name);
   }
}
