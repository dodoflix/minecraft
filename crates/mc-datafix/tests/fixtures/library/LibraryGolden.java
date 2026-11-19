import com.mojang.datafixers.DSL;
import com.mojang.datafixers.DataFixUtils;
import com.mojang.datafixers.OpticFinder;
import com.mojang.datafixers.Typed;
import com.mojang.datafixers.types.Type;
import com.mojang.serialization.Dynamic;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.IdentityHashMap;
import java.util.Locale;
import java.util.Map;
import java.util.Optional;
import java.util.Random;
import java.util.function.UnaryOperator;
import net.minecraft.nbt.*;
import net.minecraft.util.CrudeIncrementalIntIdentityHashBiMap;
import net.minecraft.util.datafix.ExtraDataFixUtils;
import net.minecraft.util.datafix.PackedBitStorage;

/**
 * Golden generator for the small DataFixerUpper library pieces the fixes use: DataFixUtils.ceillog2, PackedBitStorage,
 * CrudeIncrementalIntIdentityHashBiMap and the Dynamic helpers of ExtraDataFixUtils (NBT ops).
 * Line: op TAB input TAB result (no tabs or newlines inside the fields).
 * Usage: crates/mc-datafix/tests/fixtures/library/gen.sh
 * Output: crates/mc-datafix/tests/fixtures/library.tsv
 */
public class LibraryGolden {
   static final StringBuilder OUT = new StringBuilder();

   static void line(String op, String input, String result) {
      OUT.append(op).append('\t').append(input).append('\t').append(result).append('\n');
   }

   static String dump(Tag t) {
      switch (t) {
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
            for (Tag e : l) { if (!first) sb.append(','); first = false; sb.append(dump(e)); }
            return sb.append(']').toString();
         }
         case CompoundTag c: {
            StringBuilder sb = new StringBuilder("{");
            boolean first = true;
            for (Map.Entry<String, Tag> e : c.entrySet()) { if (!first) sb.append(','); first = false; sb.append(e.getKey()).append(':').append(dump(e.getValue())); }
            return sb.append('}').toString();
         }
         case IntArrayTag a: {
            StringBuilder sb = new StringBuilder("[I;");
            int[] v = a.getAsIntArray();
            for (int i = 0; i < v.length; i++) sb.append(i > 0 ? "," : "").append(v[i]);
            return sb.append(']').toString();
         }
         default: throw new IllegalStateException(t.getClass().getName());
      }
   }

   static Dynamic<Tag> dyn(String snbt) throws Exception {
      return new Dynamic<>(NbtOps.INSTANCE, TagParser.create(NbtOps.INSTANCE).parseFully(snbt));
   }

   static String dumpDyn(Dynamic<?> d) {
      return dump((Tag) d.getValue());
   }

   static String ex(Throwable t) {
      return "EX:" + t.getClass().getSimpleName() + ":" + t.getMessage();
   }

   // ---- ceillog2 ----
   static void ceillog2() {
      int[] fixed = {0, 1, 2, 3, 4, 5, 7, 8, 9, 15, 16, 17, 31, 32, 33, 100, 255, 256, 257, 1023, 1024, 1025, 4095, 4096, 4097, 65535, 65536, 65537,
         1 << 30, (1 << 30) + 1, Integer.MAX_VALUE, -1, -2, -16, Integer.MIN_VALUE};
      for (int v : fixed) line("ceillog2", "" + v, "" + DataFixUtils.ceillog2(v));
      for (int v = 1; v <= 300; v++) line("ceillog2", "" + v, "" + DataFixUtils.ceillog2(v));
   }

   // ---- PackedBitStorage ----
   static long digest(long[] a) {
      long h = 1125899906842597L;
      for (long x : a) h = 31 * h + x;
      return h;
   }

   static String head(long[] a) {
      StringBuilder sb = new StringBuilder();
      for (int i = 0; i < Math.min(4, a.length); i++) sb.append(sb.length() > 0 ? "," : "").append(a[i]);
      return a.length + ":" + sb;
   }

   static long readAll(PackedBitStorage s, int size) {
      long h = 7;
      for (int i = 0; i < size; i++) h = 31 * h + s.get(i);
      return h;
   }

   static void packed() {
      int[] sizes = {1, 16, 64, 100, 256, 4096};
      for (int bits = 1; bits <= 32; bits++) {
         for (int size : sizes) {
            long mask = (1L << bits) - 1L;
            PackedBitStorage s = new PackedBitStorage(bits, size);
            for (int i = 0; i < size; i++) s.set(i, (int) (((long) i * 2654435761L + bits * 40503L + 12345L) & mask & 0x7fffffffL));
            String a = head(s.getRaw()) + "/" + digest(s.getRaw()) + "/" + readAll(s, size);
            for (int i = 0; i < size; i += 3) s.set(i, (int) ((((long) i * 40503L) ^ 0x5bd1e995L ^ (bits * 977L)) & mask & 0x7fffffffL));
            String b = digest(s.getRaw()) + "/" + readAll(s, size);
            PackedBitStorage copy = new PackedBitStorage(bits, size, s.getRaw().clone());
            String c = readAll(copy, size) + "/" + copy.getBits();
            line("packed", bits + "," + size, a + "|" + b + "|" + c);
         }
      }
      // error paths
      String[] names = {"bits0", "bits33", "bits_neg", "short_data", "long_data", "ok_len", "set_neg", "set_oob", "set_value_big", "set_value_neg", "get_oob", "get_neg", "bits32_big_value"};
      for (String n : names) {
         String r;
         try {
            if (n.equals("ok_len")) {
               new PackedBitStorage(5, 13, new long[2]);
            } else {
               switch (n) {
                  case "bits0" -> new PackedBitStorage(0, 16);
                  case "bits33" -> new PackedBitStorage(33, 16);
                  case "bits_neg" -> new PackedBitStorage(-1, 16);
                  case "short_data" -> new PackedBitStorage(4, 16, new long[0]);
                  case "long_data" -> new PackedBitStorage(4, 16, new long[5]);
                  case "set_neg" -> new PackedBitStorage(4, 16).set(-1, 0);
                  case "set_oob" -> new PackedBitStorage(4, 16).set(16, 0);
                  case "set_value_big" -> new PackedBitStorage(4, 16).set(0, 16);
                  case "set_value_neg" -> new PackedBitStorage(4, 16).set(0, -1);
                  case "get_oob" -> new PackedBitStorage(4, 16).get(16);
                  case "get_neg" -> new PackedBitStorage(4, 16).get(-1);
                  case "bits32_big_value" -> new PackedBitStorage(32, 4).set(0, -1);
                  default -> throw new IllegalStateException();
               }
            }
            r = "OK";
         } catch (Throwable t) {
            r = ex(t);
         }
         line("packed_case", n, r);
      }
   }

   // ---- CrudeIncrementalIntIdentityHashBiMap ----
   static void bimap() {
      Object[] pool = new Object[64];
      IdentityHashMap<Object, Integer> index = new IdentityHashMap<>();
      for (int i = 0; i < pool.length; i++) {
         pool[i] = new Object();
         index.put(pool[i], i);
      }
      int[] capacities = {1, 2, 3, 4, 8, 16, 20, 32, 64};
      Random r = new Random(42);
      for (int seq = 0; seq < 400; seq++) {
         int capacity = capacities[seq % capacities.length];
         CrudeIncrementalIntIdentityHashBiMap<Object> m = CrudeIncrementalIntIdentityHashBiMap.create(capacity);
         CrudeIncrementalIntIdentityHashBiMap<Object> orig = m;
         StringBuilder ops = new StringBuilder();
         StringBuilder res = new StringBuilder();
         int n = 20 + r.nextInt(160);
         for (int i = 0; i < n; i++) {
            int k = r.nextInt(pool.length);
            int pick = r.nextInt(100);
            // duplicate keys or ids make the result depend on the identity hash (grow order): only valid uses are recorded
            if (pick < 40 && m.contains(pool[k])) pick = 40;
            int mid = r.nextInt(70);
            if (pick >= 75 && pick < 85 && (m.contains(pool[k]) || m.contains(mid))) pick = 65;
            String op = "";
            String result;
            try {
               if (pick < 40) { op = "a" + k; result = "" + m.add(pool[k]); }
               else if (pick < 55) { op = "g" + k; result = "" + m.getId(pool[k]); }
               else if (pick < 65) { int id = r.nextInt(80); op = "b" + id; Object o = m.byId(id); result = o == null ? "-" : "" + index.get(o); }
               else if (pick < 70) { op = "c" + k; result = "" + m.contains(pool[k]); }
               else if (pick < 75) { int id = r.nextInt(80); op = "i" + id; result = "" + m.contains(id); }
               else if (pick < 85) { op = "m" + k + ":" + mid; m.addMapping(pool[k], mid); result = "ok"; }
               else if (pick < 87) { op = "x"; m.clear(); result = "ok"; }
               else if (pick < 90) { op = "y"; m = m.copy(); result = "ok"; }
               else if (pick < 95) { op = "s"; result = "" + m.size(); }
               else {
                  op = "n";
                  StringBuilder sb = new StringBuilder();
                  for (Object o : m) sb.append(sb.length() > 0 ? "," : "").append(index.get(o));
                  result = sb.toString();
               }
            } catch (Throwable t) {
               result = "EX";
            }
            ops.append(op).append(' ');
            res.append(result).append(';');
         }
         StringBuilder fin = new StringBuilder();
         for (Object o : m) fin.append(fin.length() > 0 ? "," : "").append(index.get(o));
         StringBuilder fin0 = new StringBuilder();
         for (Object o : orig) fin0.append(fin0.length() > 0 ? "," : "").append(index.get(o));
         line("bimap", capacity + " " + ops.toString().trim(), res + " | " + fin + " | " + fin0);
      }
   }

   // ---- ExtraDataFixUtils ----
   static void extra() throws Exception {
      String[] pos = {"{X:1,Y:2,Z:3}", "{X:1b,Y:2s,Z:3L}", "{X:1.9d,Y:-2.9f,Z:3.5}", "{X:1,Y:2}", "{X:\"1\",Y:2,Z:3}", "{x:1,y:2,z:3}", "{X:1,Y:2,Z:3,extra:5}",
         "{}", "5", "\"s\"", "[1,2,3]", "{X:2147483648L,Y:-1L,Z:4294967296L}", "{X:1.0E20d,Y:-1.0E20d,Z:0.0d}", "{X:[I;1],Y:2,Z:3}", "{X:{},Y:2,Z:3}",
         "{X:true,Y:false,Z:true}", "{X:1,Y:2,Z:3.99999f}", "{X:-0.5d,Y:0.5d,Z:-1.5d}", "{X:1,Y:2,Z:3,Y2:4}"};
      for (String p : pos) {
         line("fix_block_pos", p, dumpDyn(ExtraDataFixUtils.fixBlockPos(dyn(p))));
      }
      String[][] inl = {{"x", "y", "z", "pos"}, {"SpawnX", "SpawnY", "SpawnZ", "spawn"}, {"x", "y", "z", "x"}, {"x", "x", "z", "pos"}};
      String[] inputs = {"{x:1,y:2,z:3,a:1}", "{x:1,y:2}", "{x:1,y:2,z:3}", "{SpawnX:1,SpawnY:2,SpawnZ:3}", "{x:1,y:2,z:3,pos:9}", "{x:1.5,y:\"2\",z:3}",
         "{x:1,y:2,z:3,spawn:[1]}", "5", "{x:7b,y:8s,z:9L,k:\"v\"}"};
      for (String s : inputs) {
         for (String[] f : inl) {
            line("fix_inline_block_pos", s + " " + String.join(" ", f), dumpDyn(ExtraDataFixUtils.fixInlineBlockPos(dyn(s), f[0], f[1], f[2], f[3])));
         }
      }
      int[][] xyz = {{0, 0, 0}, {1, 2, 3}, {-1, -2, -3}, {Integer.MAX_VALUE, Integer.MIN_VALUE, 0}};
      for (int[] v : xyz) {
         line("create_block_pos", "{} " + v[0] + " " + v[1] + " " + v[2], dumpDyn(ExtraDataFixUtils.createBlockPos(dyn("{}"), v[0], v[1], v[2])));
      }
      String[] strs = {"{a:\"Hello\",b:5}", "{a:\" padded \",b:\"x\"}", "{a:[\"x\"],b:{}}", "{a:\"stra\u00dfe\"}", "{}", "5", "{a:\"\"}"};
      String[] fixes = {"upper", "trim", "ell", "const", "empty"};
      for (String s : strs) {
         for (String field : new String[] {"a", "b", "c"}) {
            for (String fix : fixes) {
               UnaryOperator<String> f = switch (fix) {
                  case "upper" -> v -> v.toUpperCase(Locale.ROOT);
                  case "trim" -> String::trim;
                  case "ell" -> v -> v.replace("l", "L");
                  case "const" -> v -> "X";
                  default -> v -> "";
               };
               line("fix_string_field", s + " " + field + " " + fix, dumpDyn(ExtraDataFixUtils.fixStringField(dyn(s), field, f)));
            }
         }
      }
      String[] keys = {"{translate:\"a\"}", "{translate:\"a\",x:1}", "{translate:1}", "{}", "{text:\"a\"}", "\"a\"", "{translate:[\"a\"]}", "5", "[1]", "{translate:\"\"}"};
      for (String k : keys) {
         Optional<String> r = ExtraDataFixUtils.getPlainTranslationKey(dyn(k));
         line("plain_translation_key", k, r.map(v -> "some:" + v).orElse("none"));
      }
      Type<?> t = DSL.and(DSL.field("a", DSL.intType()), DSL.and(DSL.field("b", DSL.string()), DSL.remainderType()));
      OpticFinder<Integer> fa = DSL.fieldFinder("a", DSL.intType());
      OpticFinder<String> fb = DSL.fieldFinder("b", DSL.string());
      String[] targets = {"{a:1,b:\"s\",c:2}", "{a:1,b:\"s\"}", "{a:1,b:\"s\",c:{d:[1]}}"};
      String[] values = {"7", "7b", "7.9d", "\"x\"", "[1]", "{}", "\"7\"", "9999999999L"};
      for (String tg : targets) {
         for (String v : values) {
            for (int which = 0; which < 2; which++) {
               String r;
               try {
                  Typed<?> typed = t.readTyped(dyn(tg)).result().orElseThrow().getFirst();
                  Typed<?> out = which == 0 ? ExtraDataFixUtils.readAndSet(typed, fa, dyn(v)) : ExtraDataFixUtils.readAndSet(typed, fb, dyn(v));
                  r = out.write().result().map(d -> dumpDyn(d)).orElse("write failed");
               } catch (Throwable e) {
                  r = "EX";
               }
               line("read_and_set", tg + " " + (which == 0 ? "a" : "b") + " " + v, r);
            }
         }
      }
   }

   public static void main(String[] args) throws Exception {
      net.minecraft.SharedConstants.tryDetectVersion();
      ceillog2();
      packed();
      bimap();
      extra();
      Files.writeString(Path.of(args[0]), OUT.toString());
   }
}
