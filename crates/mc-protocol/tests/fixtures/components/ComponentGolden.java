import com.google.common.hash.HashCode;
import com.google.gson.JsonElement;
import com.mojang.serialization.Codec;
import com.mojang.serialization.DataResult;
import com.mojang.serialization.DynamicOps;
import com.mojang.serialization.JsonOps;
import io.netty.buffer.ByteBuf;
import io.netty.buffer.Unpooled;
import java.nio.file.*;
import java.util.*;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.component.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.*;
import net.minecraft.network.HashedPatchMap;
import net.minecraft.network.HashedStack;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.network.codec.StreamCodec;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.RegistryOps;
import net.minecraft.util.HashOps;
import net.minecraft.util.StrictJsonParser;
import net.minecraft.world.item.ItemStack;

/**
 * Golden generator for data components / item stacks. Corpus line: kind TAB name TAB payload (tab free).
 * kinds: json|snbt|wire (component `name`), stack-json|stack-snbt|ostack-json|ostack-snbt|stack-wire|stack-wire-untrusted
 * (name ignored, use "-"), patch-json|patch-snbt|patch-wire. Output: kind TAB name TAB payload TAB result, result =
 * ERR or OK followed by key=value fields (TAB separated). Wire inputs that decode get mutated variants appended.
 */
public class ComponentGolden {
   static RegistryAccess.Frozen access;
   static RegistryOps<JsonElement> jsonOps;
   static RegistryOps<Tag> nbtOps;
   static RegistryOps<HashCode> hashOps;

   static RegistryFriendlyByteBuf buf(byte[] data) {
      return new RegistryFriendlyByteBuf(Unpooled.wrappedBuffer(data), access);
   }

   static String hex(byte[] b) {
      StringBuilder sb = new StringBuilder();
      for (byte x : b) sb.append(String.format("%02x", x));
      return sb.toString();
   }

   static byte[] unhex(String s) {
      byte[] b = new byte[s.length() / 2];
      for (int i = 0; i < b.length; i++) b[i] = (byte)Integer.parseInt(s.substring(2 * i, 2 * i + 2), 16);
      return b;
   }

   interface Enc {
      void run(RegistryFriendlyByteBuf b);
   }

   static String wire(Enc e) {
      try {
         RegistryFriendlyByteBuf b = new RegistryFriendlyByteBuf(Unpooled.buffer(), access);
         e.run(b);
         byte[] out = new byte[b.readableBytes()];
         b.readBytes(out);
         return hex(out);
      } catch (Throwable t) {
         return "ENCERR";
      }
   }

   static <T> String enc(Codec<T> codec, DynamicOps<?> ops, T v, boolean json) {
      try {
         if (codec == null) return "-";
         DataResult<?> r = codec.encodeStart((DynamicOps<Object>)ops, v);
         if (r.isError()) return "ENCERR";
         Object o = r.getOrThrow();
         return o.toString();
      } catch (Throwable t) {
         return "ENCERR";
      }
   }

   static <T> String hash(Codec<T> codec, T v) {
      try {
         if (codec == null) return "-";
         DataResult<HashCode> r = codec.encodeStart(hashOps, v);
         if (r.isError()) return "ENCERR";
         return Integer.toString(r.getOrThrow().asInt());
      } catch (Throwable t) {
         return "ENCERR";
      }
   }

   static Tag parseSnbt(String s) throws Exception {
      return TagParser.create(NbtOps.INSTANCE).parseFully(s);
   }

   static final List<String> mutations = new ArrayList<>();
   static String lastKind = "";

   static String component(String kind, String name, String payload, boolean mutate) throws Exception {
      DataComponentType<Object> type = (DataComponentType<Object>)BuiltInRegistries.DATA_COMPONENT_TYPE.getValue(Identifier.parse(name));
      Codec<Object> codec = type.codec();
      StreamCodec<RegistryFriendlyByteBuf, Object> sc = (StreamCodec<RegistryFriendlyByteBuf, Object>)type.streamCodec();
      Object value;
      int left = 0;
      switch (kind) {
         case "json" -> {
            if (codec == null) throw new IllegalStateException("transient");
            value = codec.parse(jsonOps, StrictJsonParser.parse(payload)).getOrThrow();
         }
         case "snbt" -> {
            if (codec == null) throw new IllegalStateException("transient");
            value = codec.parse(nbtOps, parseSnbt(payload)).getOrThrow();
         }
         case "wire" -> {
            RegistryFriendlyByteBuf b = buf(unhex(payload));
            value = sc.decode(b);
            left = b.readableBytes();
         }
         default -> throw new IllegalStateException("kind " + kind);
      }
      String w = wire(b -> sc.encode(b, value));
      String res = "OK\tjson=" + enc(codec, jsonOps, value, true) + "\tsnbt=" + enc(codec, nbtOps, value, false) + "\twire=" + w
         + "\thash=" + hash(codec, value) + (kind.equals("wire") ? "\tleft=" + left : "");
      if (mutate && kind.equals("wire")) {
         mutations.add(payload);
      } else if (mutate && w.length() > 0 && !w.equals("ENCERR")) {
         mutations.add(w); // json/snbt record: also exercise the wire decoder on its encoding
      }
      lastKind = kind;
      return res;
   }

   static String patch(String kind, String payload) throws Exception {
      DataComponentPatch p;
      int left = 0;
      switch (kind) {
         case "patch-json" -> p = DataComponentPatch.CODEC.parse(jsonOps, StrictJsonParser.parse(payload)).getOrThrow();
         case "patch-snbt" -> p = DataComponentPatch.CODEC.parse(nbtOps, parseSnbt(payload)).getOrThrow();
         default -> {
            RegistryFriendlyByteBuf b = buf(unhex(payload));
            p = DataComponentPatch.STREAM_CODEC.decode(b);
            left = b.readableBytes();
         }
      }
      DataComponentPatch fp = p;
      return "OK\tjson=" + enc(DataComponentPatch.CODEC, jsonOps, p, true) + "\tsnbt=" + enc(DataComponentPatch.CODEC, nbtOps, p, false)
         + "\twire=" + wire(b -> DataComponentPatch.STREAM_CODEC.encode(b, fp))
         + "\twiredelim=" + wire(b -> DataComponentPatch.DELIMITED_STREAM_CODEC.encode(b, fp)) + "\tleft=" + left;
   }

   static String stack(String kind, String payload) throws Exception {
      ItemStack s;
      int left = 0;
      switch (kind) {
         case "stack-json" -> s = ItemStack.CODEC.parse(jsonOps, StrictJsonParser.parse(payload)).getOrThrow();
         case "stack-snbt" -> s = ItemStack.CODEC.parse(nbtOps, parseSnbt(payload)).getOrThrow();
         case "ostack-json" -> s = ItemStack.OPTIONAL_CODEC.parse(jsonOps, StrictJsonParser.parse(payload)).getOrThrow();
         case "ostack-snbt" -> s = ItemStack.OPTIONAL_CODEC.parse(nbtOps, parseSnbt(payload)).getOrThrow();
         case "stack-wire-untrusted" -> {
            RegistryFriendlyByteBuf b = buf(unhex(payload));
            s = ItemStack.OPTIONAL_UNTRUSTED_STREAM_CODEC.decode(b);
            left = b.readableBytes();
         }
         default -> {
            RegistryFriendlyByteBuf b = buf(unhex(payload));
            s = ItemStack.OPTIONAL_STREAM_CODEC.decode(b);
            left = b.readableBytes();
         }
      }
      ItemStack fs = s;
      HashedPatchMap.HashGenerator gen = c -> c.encodeValue(hashOps).getOrThrow().asInt();
      String valid;
      DataResult<ItemStack> v = ItemStack.validateStrict(s);
      valid = v.isError() ? "invalid" : "valid";
      return "OK\tjson=" + enc(ItemStack.CODEC, jsonOps, s, true) + "\tsnbt=" + enc(ItemStack.CODEC, nbtOps, s, false)
         + "\tosnbt=" + enc(ItemStack.OPTIONAL_CODEC, nbtOps, s, false)
         + "\twire=" + wire(b -> ItemStack.OPTIONAL_STREAM_CODEC.encode(b, fs))
         + "\twireu=" + wire(b -> ItemStack.OPTIONAL_UNTRUSTED_STREAM_CODEC.encode(b, fs))
         + "\tpatch=" + wire(b -> DataComponentPatch.STREAM_CODEC.encode(b, fs.getComponentsPatch()))
         + "\thashed=" + wire(b -> HashedStack.STREAM_CODEC.encode(b, HashedStack.create(fs, gen)))
         + "\tmaxstack=" + (s.isEmpty() ? 0 : s.getMaxStackSize()) + "\t" + valid + "\tleft=" + left;
   }

   static String process(String kind, String name, String payload) {
      try {
         if (kind.startsWith("stack") || kind.startsWith("ostack")) return stack(kind, payload);
         if (kind.startsWith("patch")) return patch(kind, payload);
         return component(kind, name, payload, true);
      } catch (Throwable t) {
         String m = String.valueOf(t.getMessage()).replace('\t', ' ').replace('\n', ' ');
         return "ERR\t" + (m.length() > 200 ? m.substring(0, 200) : m);
      }
   }

   public static void main(String[] args) throws Exception {
      access = extractor.Setup.bootstrap();
      jsonOps = access.createSerializationContext(JsonOps.INSTANCE);
      nbtOps = access.createSerializationContext(NbtOps.INSTANCE);
      hashOps = access.createSerializationContext(HashOps.CRC32C_INSTANCE);
      if (args[0].equals("registries")) {
         StringBuilder sb = new StringBuilder();
         access.registries().forEach(e -> {
            net.minecraft.core.Registry<Object> reg = (net.minecraft.core.Registry<Object>)e.value();
            if (BuiltInRegistries.REGISTRY.containsKey(e.key().identifier())) return;
            StringBuilder names = new StringBuilder();
            reg.keySet().stream().sorted(java.util.Comparator.comparingInt(id -> reg.getId(reg.getValue(id)))).forEach(id -> names.append(names.length() > 0 ? "," : "").append(id));
            sb.append(e.key().identifier()).append('\t').append(names).append('\n');
         });
         Files.writeString(Path.of(args[1]), sb.toString());
         return;
      }
      List<String> lines = Files.readAllLines(Path.of(args[0]));
      StringBuilder out = new StringBuilder();
      Random rnd = new Random(12345);
      for (String line : lines) {
         if (line.isBlank() || line.startsWith("#")) continue;
         String[] f = line.split("\t", 3);
         if (f.length < 3) continue;
         mutations.clear();
         String r = process(f[0], f[1], f[2]);
         out.append(f[0]).append('\t').append(f[1]).append('\t').append(f[2]).append('\t').append(r).append('\n');
         if (!mutations.isEmpty()) {
            byte[] orig = unhex(mutations.get(0));
            List<byte[]> variants = new ArrayList<>();
            if (!f[0].equals("wire")) variants.add(orig);
            if (orig.length > 1) variants.add(Arrays.copyOf(orig, orig.length - 1));
            if (orig.length > 2) variants.add(Arrays.copyOf(orig, orig.length / 2));
            for (int i = 0; i < 4 && orig.length > 0; i++) {
               byte[] m = orig.clone();
               m[rnd.nextInt(m.length)] = (byte)rnd.nextInt(256);
               variants.add(m);
            }
            for (byte[] v : variants) {
               String hv = hex(v);
               mutations.clear();
               String rr;
               try {
                  rr = component("wire", f[1], hv, false);
               } catch (Throwable t) {
                  rr = "ERR\t" + String.valueOf(t.getMessage()).replace('\t', ' ').replace('\n', ' ');
               }
               out.append("wire\t").append(f[1]).append('\t').append(hv).append('\t').append(rr).append('\n');
            }
         }
      }
      Files.writeString(Path.of(args[1]), out.toString());
   }
}
