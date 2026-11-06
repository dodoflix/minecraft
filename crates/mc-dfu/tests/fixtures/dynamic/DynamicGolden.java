import com.google.gson.JsonElement;
import com.google.gson.JsonParser;
import com.mojang.datafixers.util.Pair;
import com.mojang.serialization.DataResult;
import com.mojang.serialization.Dynamic;
import com.mojang.serialization.DynamicOps;
import com.mojang.serialization.JsonOps;
import com.mojang.serialization.OptionalDynamic;
import java.nio.ByteBuffer;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.*;
import java.util.function.Function;
import net.minecraft.nbt.*;

/**
 * Golden generator for mc_dfu::dynamic_like (Dynamic / OptionalDynamic API over NbtOps and JsonOps).
 * Line: op TAB kind(nbt|json) TAB input TAB nargs TAB args... TAB result. Compound/object iteration order is part of the result.
 * Usage: crates/mc-dfu/tests/fixtures/dynamic/gen.sh
 * Output: crates/mc-dfu/tests/fixtures/dynamic/golden.txt.gz
 */
public class DynamicGolden {
   static final StringBuilder OUT = new StringBuilder();
   static int lines = 0;

   // ---- dumping (iteration order of compounds is preserved) ----
   static String dumpTag(Tag t) {
      switch (t) {
         case EndTag e: return "END";
         case ByteTag b: return b.value() + "b";
         case ShortTag s: return s.value() + "s";
         case IntTag i: return "" + i.value();
         case LongTag l: return l.value() + "L";
         case FloatTag f: return Float.toString(f.value()) + "f";
         case DoubleTag d: return Double.toString(d.value()) + "d";
         case StringTag s: return "\"" + s.value().replace("\\", "\\\\").replace("\"", "\\\"") + "\"";
         case ListTag l: {
            StringBuilder sb = new StringBuilder("[");
            boolean first = true;
            for (Tag e : l) { if (!first) sb.append(','); first = false; sb.append(dumpTag(e)); }
            return sb.append(']').toString();
         }
         case CompoundTag c: {
            StringBuilder sb = new StringBuilder("{");
            boolean first = true;
            for (Map.Entry<String, Tag> e : c.entrySet()) { if (!first) sb.append(','); first = false; sb.append(e.getKey()).append(':').append(dumpTag(e.getValue())); }
            return sb.append('}').toString();
         }
         case ByteArrayTag a: return "[B;" + join(a.getAsByteArray()) + "]";
         case IntArrayTag a: return "[I;" + join(a.getAsIntArray()) + "]";
         case LongArrayTag a: return "[L;" + join(a.getAsLongArray()) + "]";
         default: throw new IllegalStateException();
      }
   }

   static String join(Object arr) {
      StringBuilder sb = new StringBuilder();
      int n = java.lang.reflect.Array.getLength(arr);
      for (int i = 0; i < n; i++) { if (i > 0) sb.append(','); sb.append(java.lang.reflect.Array.get(arr, i)); }
      return sb.toString();
   }

   static String dumpVal(DynamicOps<?> ops, Object v) {
      return ops == NbtOps.INSTANCE ? dumpTag((Tag)v) : v.toString();
   }

   static String dyn(Dynamic<?> d) { return dumpVal(d.getOps(), d.getValue()); }

   static String num(Number n) {
      return switch (n) {
         case Byte b -> "b:" + b;
         case Short s -> "s:" + s;
         case Integer i -> "i:" + i;
         case Long l -> "l:" + l;
         case Float f -> "f:" + f;
         case Double d -> "d:" + d;
         default -> "lazy:" + n;
      };
   }

   static <T> String res(DataResult<T> r, Function<T, String> f) {
      Optional<T> v = r.result();
      if (v.isPresent()) return "OK:" + f.apply(v.get());
      return "ERR:" + r.error().get().message();
   }

   static String list(Collection<? extends Dynamic<?>> c) {
      StringBuilder sb = new StringBuilder("[");
      boolean first = true;
      for (Dynamic<?> d : c) { if (!first) sb.append(';'); first = false; sb.append(dyn(d)); }
      return sb.append(']').toString();
   }

   static String pairs(Map<? extends Dynamic<?>, ? extends Dynamic<?>> m) {
      StringBuilder sb = new StringBuilder("{");
      boolean first = true;
      for (Map.Entry<? extends Dynamic<?>, ? extends Dynamic<?>> e : m.entrySet()) { if (!first) sb.append(';'); first = false; sb.append(dyn(e.getKey())).append('=').append(dyn(e.getValue())); }
      return sb.append('}').toString();
   }

   static Object parse(DynamicOps<?> ops, String s) {
      try {
         if (ops == NbtOps.INSTANCE) return TagParser.create(NbtOps.INSTANCE).parseFully(s);
         return JsonParser.parseString(s);
      } catch (Exception e) {
         throw new RuntimeException(s, e);
      }
   }

   @SuppressWarnings({"unchecked", "rawtypes"})
   static Dynamic<?> mk(DynamicOps ops, String s) { return new Dynamic(ops, parse(ops, s)); }

   @SuppressWarnings({"unchecked", "rawtypes"})
   static String exec(String op, Dynamic d, String[] a) {
      DynamicOps ops = d.getOps();
      switch (op) {
         case "get": return res(d.get(a[0]).get(), DynamicGolden::dyn);
         case "get2": return res(d.get(a[0]).get(a[1]).get(), DynamicGolden::dyn);
         case "as_number": return res(d.asNumber(), DynamicGolden::num);
         case "as_string": return res(d.asString(), s -> (String)s);
         case "as_boolean": return res(d.asBoolean(), b -> "" + b);
         case "as_stream_opt": return res(d.asStreamOpt(), s -> list(((java.util.stream.Stream<Dynamic<?>>)s).toList()));
         case "as_map_opt": return res(d.asMapOpt(), s -> {
            StringBuilder sb = new StringBuilder("{");
            boolean first = true;
            for (Pair<Dynamic<?>, Dynamic<?>> p : ((java.util.stream.Stream<Pair<Dynamic<?>, Dynamic<?>>>)s).toList()) { if (!first) sb.append(';'); first = false; sb.append(dyn(p.getFirst())).append('=').append(dyn(p.getSecond())); }
            return sb.append('}').toString();
         });
         case "as_byte_buffer": return res(d.asByteBufferOpt(), b -> Arrays.toString(((ByteBuffer)b).array()));
         case "as_int_stream": return res(d.asIntStreamOpt(), s -> Arrays.toString(((java.util.stream.IntStream)s).toArray()));
         case "as_long_stream": return res(d.asLongStreamOpt(), s -> Arrays.toString(((java.util.stream.LongStream)s).toArray()));
         case "as_stream": return "OK:" + list(d.asStream().toList());
         case "as_byte_buffer_plain": return "OK:" + Arrays.toString(d.asByteBuffer().array());
         case "as_int_stream_plain": return "OK:" + Arrays.toString(d.asIntStream().toArray());
         case "as_long_stream_plain": return "OK:" + Arrays.toString(d.asLongStream().toArray());
         case "as_list_dump": return "OK:" + d.asList(x -> dyn((Dynamic<?>)x));
         case "as_map_dump": return "OK:" + d.asMap(x -> dyn((Dynamic<?>)x), x -> dyn((Dynamic<?>)x));
         case "as_number_or": return "OK:" + num(d.asNumber(Integer.valueOf(a[0])));
         case "as_int_or": return "OK:" + d.asInt(Integer.parseInt(a[0]));
         case "as_long_or": return "OK:" + d.asLong(Long.parseLong(a[0]));
         case "as_float_or": return "OK:" + d.asFloat(Float.parseFloat(a[0]));
         case "as_double_or": return "OK:" + d.asDouble(Double.parseDouble(a[0]));
         case "as_byte_or": return "OK:" + d.asByte(Byte.parseByte(a[0]));
         case "as_short_or": return "OK:" + d.asShort(Short.parseShort(a[0]));
         case "as_boolean_or": return "OK:" + d.asBoolean(Boolean.parseBoolean(a[0]));
         case "as_string_or": return "OK:" + d.asString(a[0]);
         case "get_element": return res(d.getElement(a[0]), v -> dumpVal(ops, v));
         case "get_generic": return res(d.getGeneric(ops.createString(a[0])), v -> dumpVal(ops, v));
         case "get_element_or": return "OK:" + dumpVal(ops, d.getElement(a[0], ops.createInt(-1)));
         case "opt_as_string": return res(d.get(a[0]).asString(), s -> (String)s);
         case "opt_as_string_or": return "OK:" + d.get(a[0]).asString(a[1]);
         case "opt_as_int_or": return "OK:" + d.get(a[0]).asInt(Integer.parseInt(a[1]));
         case "opt_as_number": return res(d.get(a[0]).asNumber(), DynamicGolden::num);
         case "opt_as_stream": return res(d.get(a[0]).asStreamOpt(), s -> list(((java.util.stream.Stream<Dynamic<?>>)s).toList()));
         case "opt_as_stream_plain": return "OK:" + list(d.get(a[0]).asStream().toList());
         case "opt_or_empty_map": return "OK:" + dyn(d.get(a[0]).orElseEmptyMap());
         case "opt_or_empty_list": return "OK:" + dyn(d.get(a[0]).orElseEmptyList());
         case "opt_get_element": return res(d.get(a[0]).getElement(a[1]), v -> dumpVal(ops, v));
         case "set": return "OK:" + dyn(d.set(a[0], mk(ops, a[1])));
         case "remove": return "OK:" + dyn(d.remove(a[0]));
         case "rename": return "OK:" + dyn(d.renameField(a[0], a[1]));
         case "rename_fix": return "OK:" + dyn(d.renameAndFixField(a[0], a[1], x -> ((Dynamic<?>)x).createInt(((Dynamic<?>)x).asInt(0) + 1)));
         case "update": return "OK:" + dyn(d.update(a[0], x -> ((Dynamic<?>)x).createInt(((Dynamic<?>)x).asInt(0) + 1)));
         case "update_generic": return "OK:" + dyn(d.updateGeneric(ops.createString(a[0]), v -> ops.createString("g")));
         case "replace": return "OK:" + dyn(d.replaceField(a[0], a[1], a[2].equals("none") ? Optional.empty() : Optional.of(mk(ops, a[2]))));
         case "set_if_present": return "OK:" + dyn(d.setFieldIfPresent(a[0], a[1].equals("none") ? Optional.empty() : Optional.of(mk(ops, a[1]))));
         case "copy_field": return "OK:" + dyn(Dynamic.copyField(d, a[0], d, a[1]));
         case "copy_fix_field": return "OK:" + dyn(Dynamic.copyAndFixField(d, a[0], d, a[1], (java.util.function.UnaryOperator)(x -> ((Dynamic)x).createString("c"))));
         case "merge": return res(d.merge(mk(ops, a[0])).get(), DynamicGolden::dyn);
         case "merge_entry": return res(d.merge(d.createString(a[0]), mk(ops, a[1])).get(), DynamicGolden::dyn);
         case "get_map_values": return res(d.getMapValues(), m -> pairs((Map<Dynamic<?>, Dynamic<?>>)m));
         case "update_map_values": {
            int mode = Integer.parseInt(a[0]);
            return "OK:" + dyn(d.updateMapValues(p -> {
               Pair<Dynamic<?>, Dynamic<?>> pp = (Pair<Dynamic<?>, Dynamic<?>>)p;
               Dynamic<?> k = pp.getFirst();
               Dynamic<?> v = pp.getSecond();
               String ks = k.asString("");
               return switch (mode) {
                  case 0 -> pp;
                  case 1 -> Pair.of(k.createString(ks + "_x"), v);
                  case 2 -> Pair.of(k.createString(new StringBuilder(ks).reverse().toString()), v);
                  case 3 -> Pair.of(k.createString("z"), v);
                  case 4 -> Pair.of(k, v.createInt(v.asInt(0) + 1));
                  default -> Pair.of(k.createString(ks.toUpperCase(Locale.ROOT)), v);
               };
            }));
         }
         case "create_map_rev": {
            DataResult<Map<Dynamic<?>, Dynamic<?>>> r = d.getMapValues();
            if (r.result().isEmpty()) return "ERR:" + r.error().get().message();
            List<Map.Entry<Dynamic<?>, Dynamic<?>>> es = new ArrayList<>(r.result().get().entrySet());
            Collections.reverse(es);
            LinkedHashMap<Dynamic<?>, Dynamic<?>> m = new LinkedHashMap<>();
            for (Map.Entry<Dynamic<?>, Dynamic<?>> e : es) m.put(e.getKey(), e.getValue());
            return "OK:" + dyn(d.createMap(m));
         }
         case "create_list": return "OK:" + dyn(d.createList(java.util.stream.Stream.of(d.createInt(1), d.createString("x"), d.emptyMap())));
         case "create_misc": return "OK:" + dyn(d.createByte((byte)1)) + "|" + dyn(d.createShort((short)2)) + "|" + dyn(d.createLong(3L)) + "|" + dyn(d.createFloat(-0.0f)) + "|" + dyn(d.createDouble(-0.0)) + "|"
            + dyn(d.createBoolean(true)) + "|" + dyn(d.createNumeric(Byte.valueOf((byte)5))) + "|" + dyn(d.createNumeric(Float.valueOf(1.5f))) + "|" + dyn(d.createNumeric(Long.valueOf(7L))) + "|" + dyn(d.emptyMap()) + "|" + dyn(d.emptyList())
            + "|" + dyn(d.createByteList(ByteBuffer.wrap(new byte[]{1, 2}))) + "|" + dyn(d.createIntList(java.util.stream.IntStream.of(3, 4))) + "|" + dyn(d.createLongList(java.util.stream.LongStream.of(5L, 6L)));
         case "convert": {
            DynamicOps<?> out = a[0].equals("nbt") ? NbtOps.INSTANCE : JsonOps.INSTANCE;
            return "OK:" + dyn(d.convert(out));
         }
         case "to_string": return "OK:" + d.toString().replace("\n", "\\n");
         case "hash": return "OK:" + (d.hashCode() - ops.hashCode());
         case "eq": return "OK:" + d.equals(mk(ops, a[0]));
         case "chain": {
            Dynamic<?> r = d.set(a[0], d.createInt(1)).remove(a[1]).set(a[2], d.createInt(2)).remove(a[3]).renameField(a[4], a[5]);
            return "OK:" + dyn(r);
         }
         default: throw new IllegalStateException(op);
      }
   }

   static void emit(String op, String kind, String input, String... args) {
      Dynamic<?> d = mk(kind.equals("nbt") ? NbtOps.INSTANCE : JsonOps.INSTANCE, input);
      String r;
      try {
         r = exec(op, d, args);
      } catch (RuntimeException e) {
         r = "THROW";
      }
      OUT.append(op).append('\t').append(kind).append('\t').append(input).append('\t').append(args.length);
      for (String a : args) OUT.append('\t').append(a);
      OUT.append('\t').append(r.replace("\n", "\\n").replace("\t", "\\t")).append('\n');
      lines++;
   }

   static final String[] NBT_INPUTS = {
      "1b", "0b", "-1b", "127b", "1s", "300s", "0", "1", "-5", "2147483647", "1L", "5000000000L", "1.5f", "0.0f", "-0.0f", "2.5d", "1.0e30d", "1.0e-5f", "3.9d", "-3.9f",
      "\"abc\"", "\"12\"", "\"1.5\"", "\"true\"", "\"\"", "[]", "[1,2,3]", "[1b,2b]", "[B;1,2,3]", "[I;4,5]", "[L;6L]", "[B;]", "[I;]", "{}", "{a:1}", "{a:\"x\"}", "{a:{b:2}}",
      "{a:[1,2]}", "{a:1b,b:2s,c:3L,d:4.0f,e:5.0d}", "[\"a\",\"b\"]", "[{a:1},{a:2}]", "[[1],[2]]", "{a:[B;1,2],b:[I;3],c:[L;4L]}", "{a:\"abc\",b:300s}", "{a:2b}", "{a:0b,b:\"7\"}",
   };
   static final String[] JSON_INPUTS = {
      "null", "true", "false", "0", "1", "-5", "1.5", "2e3", "12345678901", "1.0", "3.00", "99999999999999999999", "\"abc\"", "\"12\"", "\"\"", "[]", "[1,2]", "[1,\"a\",true,null]", "[1.5,2]",
      "{}", "{\"a\":1}", "{\"a\":null}", "{\"a\":{\"b\":[1,2]}}", "{\"a\":\"x\",\"b\":true}", "{\"b\":1,\"a\":2}", "{\"a\":2.5}", "{\"a\":\"7\"}", "{\"a\":true}",
   };

   static void inputOps(String kind, String in) {
      for (String op : new String[]{"as_number", "as_string", "as_boolean", "as_stream_opt", "as_map_opt", "as_byte_buffer", "as_int_stream", "as_long_stream", "as_stream",
         "as_byte_buffer_plain", "as_int_stream_plain", "as_long_stream_plain", "as_list_dump", "as_map_dump", "get_map_values", "create_map_rev", "create_list", "create_misc", "to_string", "hash"}) {
         emit(op, kind, in);
      }
      for (String def : new String[]{"7", "-1"}) {
         emit("as_number_or", kind, in, def);
         emit("as_int_or", kind, in, def);
         emit("as_long_or", kind, in, def);
         emit("as_byte_or", kind, in, def);
         emit("as_short_or", kind, in, def);
      }
      emit("as_float_or", kind, in, "7.5");
      emit("as_double_or", kind, in, "7.5");
      emit("as_boolean_or", kind, in, "true");
      emit("as_boolean_or", kind, in, "false");
      emit("as_string_or", kind, in, "dflt");
      emit("convert", kind, in, "nbt");
      emit("convert", kind, in, "json");
      for (String k : new String[]{"a", "b", "missing", ""}) {
         emit("get", kind, in, k);
         emit("get_element", kind, in, k);
         emit("get_generic", kind, in, k);
         emit("get_element_or", kind, in, k);
         emit("remove", kind, in, k);
         emit("update", kind, in, k);
         emit("update_generic", kind, in, k);
         emit("opt_as_string", kind, in, k);
         emit("opt_as_number", kind, in, k);
         emit("opt_as_stream", kind, in, k);
         emit("opt_as_stream_plain", kind, in, k);
         emit("opt_or_empty_map", kind, in, k);
         emit("opt_or_empty_list", kind, in, k);
         emit("opt_as_string_or", kind, in, k, "dflt");
         emit("opt_as_int_or", kind, in, k, "9");
         emit("opt_get_element", kind, in, k, "a");
         emit("set", kind, in, k, "1");
         emit("set", kind, in, k, kind.equals("nbt") ? "{x:1}" : "{\"x\":1}");
         emit("set", kind, in, k, kind.equals("nbt") ? "\"s\"" : "\"s\"");
         emit("set_if_present", kind, in, k, "none");
         emit("set_if_present", kind, in, k, "3");
         emit("merge_entry", kind, in, k, "2");
         emit("copy_field", kind, in, k, "copied");
         emit("copy_fix_field", kind, in, k, "copied");
      }
      emit("get2", kind, in, "a", "b");
      emit("get2", kind, in, "a", "x");
      emit("rename", kind, in, "a", "z");
      emit("rename", kind, in, "missing", "z");
      emit("rename_fix", kind, in, "a", "z");
      emit("replace", kind, in, "a", "z", "9");
      emit("replace", kind, in, "a", "z", "none");
      emit("merge", kind, in, "5");
      emit("merge", kind, in, kind.equals("nbt") ? "[1]" : "[1]");
      for (int m = 0; m <= 5; m++) emit("update_map_values", kind, in, "" + m);
   }

   // ---- map order tests ----
   static final String[] POOL = {
      "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r", "s", "t", "u", "v", "w", "x", "y", "z",
      "Aa", "BB", "AaAa", "BBBB", "AaBB", "BBAa", "key0", "key1", "key2", "key3", "id", "Count", "Slot", "tag", "Items", "Pos", "Motion", "Rotation", "UUID", "CustomName", "Fire", "Air", "OnGround", "Invulnerable",
      "k24", "k25", "k26", "k27", "k28", "k29", "k30", "k31", "k32", "k33", "k34", "k35", "k36", "k37", "k38", "k39", "k40", "k41", "k42", "k43",
   };

   static void mapOrder(Random r, int n) {
      List<String> pool = new ArrayList<>(Arrays.asList(POOL));
      Collections.shuffle(pool, r);
      StringBuilder sb = new StringBuilder("{");
      for (int i = 0; i < n; i++) { if (i > 0) sb.append(','); sb.append(pool.get(i)).append(':').append(i); }
      String in = sb.append('}').toString();
      String first = n > 0 ? pool.get(0) : "a";
      String last = n > 0 ? pool.get(n - 1) : "b";
      String fresh = pool.get(n);
      String fresh2 = pool.get(n + 1);
      String fresh3 = pool.get(n + 2);
      emit("get_map_values", "nbt", in);
      emit("set", "nbt", in, fresh, "99");
      emit("set", "nbt", in, first, "98b");
      emit("remove", "nbt", in, first);
      emit("remove", "nbt", in, last);
      emit("remove", "nbt", in, fresh);
      emit("rename", "nbt", in, first, fresh);
      emit("rename_fix", "nbt", in, last, fresh);
      emit("update", "nbt", in, first);
      emit("merge_entry", "nbt", in, fresh, "5");
      emit("copy_field", "nbt", in, first, fresh);
      for (int m = 0; m <= 5; m++) emit("update_map_values", "nbt", in, "" + m);
      emit("create_map_rev", "nbt", in);
      emit("chain", "nbt", in, fresh, first, fresh2, fresh, last, fresh3);
      emit("chain", "nbt", in, fresh, last, fresh2, first, fresh2, fresh3);
      emit("convert", "nbt", in, "json");
      emit("hash", "nbt", in);
      // the same map through JSON -> NBT
      StringBuilder js = new StringBuilder("{");
      for (int i = 0; i < n; i++) { if (i > 0) js.append(','); js.append('"').append(pool.get(i)).append("\":").append(i); }
      String jin = js.append('}').toString();
      emit("convert", "json", jin, "nbt");
      emit("set", "json", jin, fresh, "99");
      emit("remove", "json", jin, first);
      emit("update_map_values", "json", jin, "1");
      emit("update_map_values", "json", jin, "2");
      emit("update_map_values", "json", jin, "5");
      emit("hash", "json", jin);
   }

   static void equality() {
      String[][] nbt = {
         {"1b", "1b"}, {"1b", "1"}, {"1", "1L"}, {"1.5f", "1.5f"}, {"0.0f", "-0.0f"}, {"{a:1,b:2}", "{b:2,a:1}"}, {"[1,2]", "[2,1]"}, {"[B;1]", "[B;1]"}, {"[B;1]", "[I;1]"}, {"{}", "{}"},
         {"\"a\"", "\"a\""}, {"\"a\"", "\"b\""}, {"{a:{b:1}}", "{a:{b:1}}"}, {"{a:{b:1}}", "{a:{b:2}}"}, {"[]", "{}"}, {"1.5f", "1.5d"}, {"{a:1,b:2,c:3}", "{c:3,a:1,b:2}"}, {"{a:1}", "{a:1,b:2}"},
      };
      for (String[] p : nbt) emit("eq", "nbt", p[0], p[1]);
      String[][] json = {
         {"1", "1"}, {"1", "1.0"}, {"1.5", "1.5"}, {"2e3", "2000"}, {"{\"a\":1,\"b\":2}", "{\"b\":2,\"a\":1}"}, {"[1]", "[1.0]"}, {"null", "null"}, {"\"1\"", "1"}, {"true", "true"}, {"true", "false"},
         {"12345678901", "12345678901"}, {"0.1", "0.10"}, {"[]", "[]"}, {"{}", "[]"}, {"{\"a\":null}", "{\"a\":null}"}, {"{\"a\":null}", "{}"}, {"1e2", "100"}, {"100", "100.0"},
      };
      for (String[] p : json) emit("eq", "json", p[0], p[1]);
   }

   public static void main(String[] args) throws Exception {
      for (String in : NBT_INPUTS) inputOps("nbt", in);
      for (String in : JSON_INPUTS) inputOps("json", in);
      equality();
      for (int seed = 0; seed < 2; seed++) {
         Random r = new Random(7000 + seed);
         for (int n = 0; n <= 40; n++) mapOrder(r, n);
      }
      Files.writeString(Path.of(args[0]), OUT.toString());
      System.err.println(lines + " cases");
   }
}
