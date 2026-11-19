// Schema-driven sample generator: walks the DFU type of a Reference in the schema of a data version and emits NBT that conforms
// to it (the input of the `batch` verb), so the generator needs no knowledge of Minecraft formats and follows every schema.
//
// Usage: scripts/datafix-oracle.sh gen [--type T[,T..]|all] [--from F[,F..]] [--to V] [--seed S] [--n N] [--invalid P]
//                                      [--mutate FILE] [--matrix step|chain|random] [--shard I/K] [--report FILE]
//   --type     DataFixTypes constants or `ref:NAME` (a References type); `all` = every DataFixTypes constant
//   --from     data versions whose schema describes the samples (-1 and everything below the first schema read as 99)
//   --to       the version written into every batch line (default: the current data version)
//   --seed     master seed (default 1); every sample draws from its own SplittableRandom(sha256(seed, type, from, index, attempt)),
//              so the output is a pure function of the arguments and the vocabulary file (no clock, no ordering by hash maps)
//   --n        samples per (type, from) group; with --mutate the mutants per input line; with --matrix random the number of
//              (type, from, to) triples (defaults: 20, 20, step 20 / chain 200 / random 20000)
//   --invalid  fraction of samples that are damaged on purpose (a node replaced by one of another kind) and kept without the
//              readTyped filter, to compare the error path (default 0)
//   --mutate   FILE of batch lines (`ID TYPE FROM TO BASE64`, real samples): per line N mutants with 1-3 seeded mutations
//              (drop or duplicate a field, swap a string for a vocabulary id, change a tag type, empty a list or compound, boundary
//              numbers, insert a foreign key); TYPE, FROM and TO come from the line
//   --matrix   step: for every distinct fix version V, every References type of the schema at V-1, samples at V-1 for V-1 -> V;
//              chain: every DataFixTypes constant from every era milestone to the current version;
//              random: seeded (type, from, to) triples with to below the current version, from >= to and from -1/99/100, sorted
//              by (type, from, to) so a runner reuses one rule per group
//   --shard    I/K: only the groups whose position in the matrix order is I modulo K
//   --report   file for one TSV row per group: TYPE FROM TO STATE N FIRST_OK EMITTED DROPPED INVALID, STATE ok | absent (the
//              type does not exist in that schema) | noncompound (the root is not a compound: no batch line can carry it)
// The vocabulary (old ids and field names the fixers mention, registry ids) is read from the file named by the system property
// datafix.vocab (scripts/datafix-oracle.sh harvests it into target/datafix/vocab.txt); without it only the built-in strings are used.
// Output: batch lines on stdout (ID TYPE FROM TO BASE64 of NbtIo.write); a summary on stderr.
//
// Shapes: Tag/OptionalField (Sum of a Tag and the empty part) become compound entries (optional: present with p = 0.7), a
// TaggedChoice writes its key and picks a case (half of the time one the vocabulary knows), a RecursivePoint is unfolded at most
// 3 deep (beyond that lists are empty and optional fields absent), the remainder of an open product gets 0-3 foreign keys (none
// next to a CompoundList, whose codec reads every key), strings and numbers are drawn from the vocabulary, boundary values and
// legacy ids. A sample is kept only if type.readTyped(NbtOps, tag) succeeds on the vanilla side (up to 20 attempts per sample);
// the first-try rate is reported.
import com.mojang.datafixers.DSL;
import com.mojang.datafixers.DataFix;
import com.mojang.datafixers.DataFixUtils;
import com.mojang.datafixers.DataFixer;
import com.mojang.datafixers.DataFixerUpper;
import com.mojang.datafixers.schemas.Schema;
import com.mojang.datafixers.types.Type;
import com.mojang.datafixers.types.constant.EmptyPart;
import com.mojang.datafixers.types.constant.EmptyPartPassthrough;
import com.mojang.datafixers.types.templates.Check;
import com.mojang.datafixers.types.templates.CompoundList;
import com.mojang.datafixers.types.templates.Const;
import com.mojang.datafixers.types.templates.Hook;
import com.mojang.datafixers.types.templates.Named;
import com.mojang.datafixers.types.templates.Product;
import com.mojang.datafixers.types.templates.RecursivePoint;
import com.mojang.datafixers.types.templates.Sum;
import com.mojang.datafixers.types.templates.TaggedChoice;
import java.io.*;
import java.lang.reflect.Field;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.MessageDigest;
import java.util.*;
import java.util.function.Predicate;
import net.minecraft.SharedConstants;
import net.minecraft.nbt.ByteTag;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.DoubleTag;
import net.minecraft.nbt.FloatTag;
import net.minecraft.nbt.IntTag;
import net.minecraft.nbt.ListTag;
import net.minecraft.nbt.LongTag;
import net.minecraft.nbt.NbtAccounter;
import net.minecraft.nbt.NbtIo;
import net.minecraft.nbt.NbtOps;
import net.minecraft.nbt.NumericTag;
import net.minecraft.nbt.ShortTag;
import net.minecraft.nbt.StringTag;
import net.minecraft.nbt.Tag;
import net.minecraft.util.datafix.DataFixTypes;
import net.minecraft.util.datafix.DataFixers;

final class SampleGen {
  private SampleGen() {}

  /** The process's real stderr: Bootstrap wraps System.err in a logger. */
  static final PrintStream ERR = new PrintStream(new FileOutputStream(FileDescriptor.err), true, StandardCharsets.UTF_8);
  static final int MAX_DEPTH = 3;
  static final int MAX_ATTEMPTS = 20;
  /** Era milestones of the chain matrix: no DataVersion at all, the first schema, then the release boundaries and the file fixer steps. */
  static final int[] MILESTONES = {
    -1, 99, 184, 512, 922, 1343, 1519, 1631, 1976, 2230, 2586, 2730, 2975, 3337, 3465, 3700, 3839, 3955, 4189, 4325, 4671, 4772, 4773, 4786, 4899, 4903, 5016
  };
  static final long[] BOUNDS = {0, -1, 1, 127, 128, 255, 256, 32767, 32768, 65535, -128, -32768, Integer.MAX_VALUE, Integer.MIN_VALUE, Long.MAX_VALUE, Long.MIN_VALUE};
  static final double[] REAL_BOUNDS = {0.0, -0.0, 1.0, -1.0, 0.5, 1e-9, 1e9, Float.MAX_VALUE, Float.MIN_VALUE, Double.MAX_VALUE, Double.NaN, Double.POSITIVE_INFINITY, Double.NEGATIVE_INFINITY};
  /** Text the vocabulary cannot hold: JSON text components, legacy section signs, numbers as text, unicode, a long string. */
  static final String[] EXTRA_STRINGS = {
    "{\"text\":\"hi\"}",
    "\"plain\"",
    "[\"a\",\"b\"]",
    "{\"extra\":[{\"text\":\"x\",\"bold\":true}],\"text\":\"\"}",
    "{\"translate\":\"block.minecraft.stone\"}",
    "\u00a7aGreen \u00a7lBold\u00a7r",
    "h\u00e9llo w\u00f6rld \u2713 \ud83d\ude00",
    "x".repeat(300),
    " ",
    "0",
    "1.5",
    "+5",
    "true",
    "{\"text\":\"\u00a7cred\"}",
  };

  // ---- vocabulary ------------------------------------------------------------------------------------------------------

  static final class Vocab {
    final List<String> all = new ArrayList<>();
    final List<String> ids = new ArrayList<>();
    final List<String> words = new ArrayList<>();
    final Set<String> set = new HashSet<>();

    static Vocab load() throws IOException {
      Vocab v = new Vocab();
      String file = System.getProperty("datafix.vocab");
      if (file != null) {
        for (String line : Files.readAllLines(Path.of(file), StandardCharsets.UTF_8)) {
          if (line.isEmpty()) continue;
          v.all.add(line);
          if (line.matches("[a-z0-9_.-]+:[a-z0-9_./-]+")) v.ids.add(line);
          else if (line.matches("[A-Za-z_][A-Za-z0-9_]*")) v.words.add(line);
        }
        v.set.addAll(v.all);
      }
      return v;
    }
  }

  // ---- per-sample generator --------------------------------------------------------------------------------------------

  record Slot(Tag parent, String key, int index, Tag value) {}

  static long mix(Object... parts) {
    try {
      StringBuilder sb = new StringBuilder();
      for (Object p : parts) sb.append(p).append('|');
      return ByteBuffer.wrap(MessageDigest.getInstance("SHA-256").digest(sb.toString().getBytes(StandardCharsets.UTF_8))).getLong();
    } catch (java.security.NoSuchAlgorithmException e) {
      throw new IllegalStateException(e);
    }
  }

  static Object delegate(Object o) {
    return SchemaWalk.field(o, "delegate");
  }

  static final class Gen {
    final SplittableRandom r;
    final Vocab v;
    int depth;
    /** > 0 inside a list: numbers keep their canonical tag type so a list stays homogeneous. */
    int canon;
    boolean foreignOk = true;

    Gen(long seed, Vocab v) {
      this.r = new SplittableRandom(seed);
      this.v = v;
    }

    boolean chance(double p) {
      return r.nextDouble() < p;
    }

    int upto(int n) {
      return r.nextInt(n);
    }

    <T> T pick(List<T> l) {
      return l.get(upto(l.size()));
    }

    // -- types -> tags

    Tag value(Type<?> t) {
      if (t instanceof Const.PrimitiveType<?> p) return prim(String.valueOf(p.codec()));
      if (t instanceof com.mojang.datafixers.types.templates.List.ListType<?> l) {
        int n = depth >= MAX_DEPTH ? 0 : new int[] {0, 1, 1, 1, 2, 2, 3}[upto(7)];
        ListTag list = new ListTag();
        canon++;
        for (int i = 0; i < n; i++) list.add(value(l.getElement()));
        canon--;
        return list;
      }
      if (t instanceof Named.NamedType<?> n) return value(n.element());
      if (t instanceof Check.CheckType<?>) return value((Type<?>) delegate(t));
      if (t instanceof Hook.HookType<?>) return value((Type<?>) delegate(t));
      if (t instanceof RecursivePoint.RecursivePointType<?> m) {
        depth++;
        try {
          return value(m.unfold());
        } finally {
          depth--;
        }
      }
      if (t instanceof Sum.SumType<?, ?> s) return value(side(s));
      CompoundTag c = new CompoundTag();
      boolean saved = foreignOk;
      foreignOk = true;
      fill(t, c);
      foreignOk = saved;
      return c;
    }

    void fill(Type<?> t, CompoundTag c) {
      if (t instanceof Product.ProductType<?, ?> p) {
        List<Type<?>> parts = new ArrayList<>();
        flatten(p, parts);
        boolean saved = foreignOk;
        foreignOk = parts.stream().noneMatch(x -> x instanceof CompoundList.CompoundListType);
        for (Type<?> x : parts) fill(x, c);
        foreignOk = saved;
      } else if (t instanceof Sum.SumType<?, ?> s) {
        fill(side(s), c);
      } else if (t instanceof com.mojang.datafixers.types.templates.Tag.TagType<?> g) {
        c.put(g.name(), value(g.element()));
      } else if (t instanceof TaggedChoice.TaggedChoiceType<?> ch) {
        List<Map.Entry<?, Type<?>>> entries = new ArrayList<>(ch.types().entrySet());
        entries.sort(Comparator.comparing(e -> String.valueOf(e.getKey())));
        List<Map.Entry<?, Type<?>>> known = entries.stream().filter(e -> v.set.contains(String.valueOf(e.getKey()))).toList();
        Map.Entry<?, Type<?>> e = !known.isEmpty() && chance(0.5) ? pick(known) : pick(entries);
        c.put(ch.getName(), e.getKey() instanceof Integer i ? IntTag.valueOf(i) : StringTag.valueOf(String.valueOf(e.getKey())));
        boolean saved = foreignOk;
        foreignOk = true;
        fill(e.getValue(), c);
        foreignOk = saved;
      } else if (t instanceof EmptyPartPassthrough) {
        if (foreignOk) foreign(c);
      } else if (t instanceof CompoundList.CompoundListType<?, ?> cl) {
        int n = depth >= MAX_DEPTH ? 0 : new int[] {0, 1, 1, 2, 3}[upto(5)];
        for (int i = 0; i < n; i++) {
          Tag k = value(cl.getKey());
          String key = k instanceof StringTag s ? s.value() : String.valueOf(k);
          c.put(key, value(cl.getElement()));
        }
      } else if (t instanceof Named.NamedType<?> n) {
        fill(n.element(), c);
      } else if (t instanceof Check.CheckType<?> || t instanceof Hook.HookType<?>) {
        fill((Type<?>) delegate(t), c);
      } else if (t instanceof RecursivePoint.RecursivePointType<?> m) {
        depth++;
        try {
          fill(m.unfold(), c);
        } finally {
          depth--;
        }
      }
      // EmptyPart, and the value kinds (primitive, list), add nothing to a compound
    }

    static void flatten(Type<?> t, List<Type<?>> out) {
      if (t instanceof Product.ProductType<?, ?> p) {
        flatten(p.first(), out);
        flatten(p.second(), out);
      } else {
        out.add(t);
      }
    }

    /** A Check whose index differs from the expected one never decodes, and a Sum of such checks neither. */
    static boolean viable(Type<?> t) {
      if (t instanceof Check.CheckType<?>) return SchemaWalk.field(t, "index").equals(SchemaWalk.field(t, "expectedIndex"));
      if (t instanceof Sum.SumType<?, ?> s) return viable(s.first()) || viable(s.second());
      return true;
    }

    /** The branch of a Sum: an optional field (the other side is the empty part) is present with p = 0.7 and never below the depth cap. */
    Type<?> side(Sum.SumType<?, ?> s) {
      boolean first = viable(s.first()), second = viable(s.second());
      if (first != second) return first ? s.first() : s.second();
      if (s.second() instanceof EmptyPart) return depth >= MAX_DEPTH || !chance(0.7) ? s.second() : s.first();
      if (s.first() instanceof EmptyPart) return depth >= MAX_DEPTH || !chance(0.7) ? s.first() : s.second();
      return chance(0.5) ? s.first() : s.second();
    }

    void foreign(CompoundTag c) {
      int n = new int[] {0, 0, 0, 1, 1, 1, 2, 2, 3}[upto(9)];
      for (int i = 0; i < n; i++) {
        String key = !v.words.isEmpty() && chance(0.35) ? pick(v.words) : "x_" + upto(100);
        if (!c.contains(key)) c.put(key, simple());
      }
    }

    Tag simple() {
      return switch (upto(7)) {
        case 0 -> ByteTag.valueOf((byte) upto(2));
        case 1 -> ShortTag.valueOf((short) upto(100));
        case 2 -> IntTag.valueOf((int) intVal());
        case 3 -> LongTag.valueOf(r.nextLong());
        case 4 -> StringTag.valueOf(string());
        case 5 -> new ListTag();
        default -> new CompoundTag();
      };
    }

    // -- leaves

    long intVal() {
      int k = upto(20);
      if (k < 8) return upto(64);
      if (k < 11) return upto(4097);
      if (k < 15) return BOUNDS[upto(BOUNDS.length - 2)];
      return r.nextInt();
    }

    double realVal() {
      int k = upto(10);
      if (k < 4) return upto(20);
      if (k == 4) return REAL_BOUNDS[upto(REAL_BOUNDS.length)];
      return Math.round((r.nextDouble() - 0.5) * 2000.0) / 10.0;
    }

    /** A number of tag kind 0 byte, 1 short, 2 int, 3 long, 4 float, 5 double. */
    static Tag number(int kind, long x, double d) {
      return switch (kind) {
        case 0 -> ByteTag.valueOf((byte) x);
        case 1 -> ShortTag.valueOf((short) x);
        case 2 -> IntTag.valueOf((int) x);
        case 3 -> LongTag.valueOf(x);
        case 4 -> FloatTag.valueOf((float) d);
        default -> DoubleTag.valueOf(d);
      };
    }

    /** Outside a list one number in ten gets another numeric tag type than the schema says (every numeric codec reads them all). */
    int kindOf(int canonical) {
      return canon == 0 && chance(0.1) ? upto(6) : canonical;
    }

    Tag prim(String kind) {
      return switch (kind) {
        case "Bool" -> number(kindOf(0), upto(2), upto(2));
        case "Byte" -> number(kindOf(0), (byte) (chance(0.6) ? upto(8) : intVal()), upto(8));
        case "Short" -> number(kindOf(1), (short) intVal(), upto(100));
        case "Int" -> number(kindOf(2), (int) intVal(), upto(100));
        case "Long" -> number(kindOf(3), chance(0.3) ? r.nextLong() : intVal(), upto(100));
        case "Float" -> number(kindOf(4), upto(100), realVal());
        case "Double" -> number(kindOf(5), upto(100), realVal());
        case "String" -> StringTag.valueOf(string());
        case "NamespacedString" -> StringTag.valueOf(namespaced());
        default -> throw new IllegalStateException("unknown primitive type " + kind);
      };
    }

    String word() {
      int n = 1 + upto(8);
      StringBuilder sb = new StringBuilder();
      for (int i = 0; i < n; i++) sb.append((char) ('a' + upto(26)));
      return sb.toString();
    }

    String string() {
      int k = upto(20);
      if (k < 9 && !v.all.isEmpty()) return pick(v.all);
      if (k == 9) return "";
      if (k < 12) return EXTRA_STRINGS[upto(EXTRA_STRINGS.length)];
      return word();
    }

    String namespaced() {
      int k = upto(18);
      if (k < 11 && !v.ids.isEmpty()) return pick(v.ids);
      if (k < 14) return "minecraft:" + word();
      if (k == 14) return word();
      if (k == 15 && !v.words.isEmpty()) return pick(v.words);
      if (k == 16) return "Minecraft:" + word().toUpperCase(Locale.ROOT);
      return "mod:" + word();
    }

    // -- invalid and mutated samples

    static void collect(Tag parent, List<Slot> out) {
      if (parent instanceof CompoundTag c) {
        for (String k : new TreeSet<>(c.keySet())) {
          Tag t = c.get(k);
          out.add(new Slot(c, k, -1, t));
          collect(t, out);
        }
      } else if (parent instanceof ListTag l) {
        for (int i = 0; i < l.size(); i++) {
          out.add(new Slot(l, null, i, l.get(i)));
          collect(l.get(i), out);
        }
      }
    }

    static void replace(Slot s, Tag with) {
      if (s.parent() instanceof CompoundTag c) c.put(s.key(), with);
      else ((ListTag) s.parent()).set(s.index(), with);
    }

    static List<Slot> where(List<Slot> slots, Predicate<Tag> p) {
      return slots.stream().filter(s -> p.test(s.value())).toList();
    }

    /** One node replaced by a node of another kind. */
    void corrupt(CompoundTag root) {
      List<Slot> slots = new ArrayList<>();
      collect(root, slots);
      if (slots.isEmpty()) {
        root.put("x_corrupt", StringTag.valueOf("corrupt"));
        return;
      }
      Slot s = pick(slots);
      replace(s, s.value() instanceof CompoundTag ? StringTag.valueOf("corrupt") : s.value() instanceof ListTag ? IntTag.valueOf(7) : s.value() instanceof StringTag ? new CompoundTag() : StringTag.valueOf("corrupt"));
    }

    void mutate(CompoundTag root) {
      List<Slot> slots = new ArrayList<>();
      collect(root, slots);
      for (int attempt = 0; attempt < 16; attempt++) {
        switch (upto(7)) {
          case 0 -> {
            if (slots.isEmpty()) continue;
            Slot s = pick(slots);
            if (s.parent() instanceof CompoundTag c) c.remove(s.key());
            else ((ListTag) s.parent()).remove(s.index());
            return;
          }
          case 1 -> {
            if (slots.isEmpty()) continue;
            Slot s = pick(slots);
            if (s.parent() instanceof CompoundTag c) {
              String key = !v.words.isEmpty() && chance(0.5) ? pick(v.words) : s.key() + "_2";
              if (!c.contains(key)) c.put(key, s.value().copy());
            } else {
              ((ListTag) s.parent()).add(s.index(), s.value().copy());
            }
            return;
          }
          case 2 -> {
            List<Slot> strings = where(slots, t -> t instanceof StringTag);
            if (strings.isEmpty() || v.all.isEmpty()) continue;
            replace(pick(strings), StringTag.valueOf(chance(0.7) && !v.ids.isEmpty() ? pick(v.ids) : pick(v.all)));
            return;
          }
          case 3 -> {
            if (slots.isEmpty()) continue;
            Slot s = pick(slots);
            Tag t = s.value();
            replace(
                s,
                t instanceof NumericTag n ? (chance(0.7) ? number(upto(6), n.longValue(), n.doubleValue()) : StringTag.valueOf(String.valueOf(n.box())))
                    : t instanceof StringTag ? IntTag.valueOf(upto(100))
                    : t instanceof ListTag ? new CompoundTag()
                    : t instanceof CompoundTag ? new ListTag()
                    : StringTag.valueOf(""));
            return;
          }
          case 4 -> {
            List<Slot> containers = where(slots, t -> t instanceof ListTag || t instanceof CompoundTag);
            if (containers.isEmpty()) continue;
            Slot s = pick(containers);
            replace(s, s.value() instanceof ListTag ? new ListTag() : new CompoundTag());
            return;
          }
          case 5 -> {
            List<Slot> numbers = where(slots, t -> t instanceof NumericTag);
            if (numbers.isEmpty()) continue;
            Slot s = pick(numbers);
            int kind = s.value() instanceof ByteTag ? 0 : s.value() instanceof ShortTag ? 1 : s.value() instanceof IntTag ? 2 : s.value() instanceof LongTag ? 3 : s.value() instanceof FloatTag ? 4 : 5;
            replace(s, number(kind, BOUNDS[upto(BOUNDS.length)], REAL_BOUNDS[upto(REAL_BOUNDS.length)]));
            return;
          }
          default -> {
            List<CompoundTag> compounds = new ArrayList<>(List.of(root));
            for (Slot s : slots) if (s.value() instanceof CompoundTag c) compounds.add(c);
            CompoundTag c = pick(compounds);
            String key = !v.words.isEmpty() && chance(0.4) ? pick(v.words) : "x_" + upto(100);
            if (!c.contains(key)) c.put(key, simple());
            return;
          }
        }
      }
    }
  }

  // ---- output ----------------------------------------------------------------------------------------------------------

  static String encode(CompoundTag tag) throws IOException {
    ByteArrayOutputStream bytes = new ByteArrayOutputStream();
    NbtIo.write(tag, new DataOutputStream(bytes));
    return Base64.getEncoder().encodeToString(bytes.toByteArray());
  }

  static CompoundTag decode(String base64) throws IOException {
    return NbtIo.read(new DataInputStream(new ByteArrayInputStream(Base64.getDecoder().decode(base64))), NbtAccounter.unlimitedHeap());
  }

  static boolean readable(Type<?> type, CompoundTag tag) {
    try {
      return type.readTyped(NbtOps.INSTANCE, tag).result().isPresent();
    } catch (RuntimeException e) {
      return false;
    }
  }

  static int current() {
    return SharedConstants.getCurrentVersion().dataVersion().version();
  }

  static DSL.TypeReference typeRef(String type) throws ReflectiveOperationException {
    if (type.startsWith("ref:")) return DataFixerOracle.reference(type.substring(4));
    Field f = DataFixTypes.class.getDeclaredField("type");
    f.setAccessible(true);
    return (DSL.TypeReference) f.get(DataFixTypes.valueOf(type));
  }

  static final class Emitter {
    final DataFixer fixer = DataFixers.getDataFixer();
    final Vocab vocab;
    final long seed;
    final double invalid;
    final int to;
    final int shardI, shardK;
    final boolean quiet;
    final PrintStream out;
    final List<String> report = new ArrayList<>();
    int groups, total, firstOk, emitted, dropped, damaged, absent, nonCompound;

    Emitter(Vocab vocab, long seed, double invalid, int to, int shardI, int shardK, boolean quiet, PrintStream out) {
      this.vocab = vocab;
      this.seed = seed;
      this.invalid = invalid;
      this.to = to;
      this.shardI = shardI;
      this.shardK = shardK;
      this.quiet = quiet;
      this.out = out;
    }

    void line(String id, String type, int from, int toVersion, CompoundTag tag) throws IOException {
      out.println(id + "\t" + type + "\t" + from + "\t" + toVersion + "\t" + encode(tag));
    }

    /** Samples [base, base + n) of one (type, from, to) group. */
    void group(String type, int from, int toVersion, int n, int base) throws Exception {
      if (shardK > 1 && groups++ % shardK != shardI) return;
      DSL.TypeReference ref = typeRef(type);
      Type<?> root;
      try {
        root = fixer.getSchema(DataFixUtils.makeKey(from)).getTypeRaw(ref);
      } catch (IllegalArgumentException e) {
        absent++;
        report.add(type + "\t" + from + "\t" + toVersion + "\tabsent\t0\t0\t0\t0\t0");
        return;
      }
      int ok = 0, kept = 0, lost = 0, bad = 0;
      boolean compound = true;
      for (int i = base; i < base + n && compound; i++) {
        CompoundTag sample = null;
        boolean sawCompound = false;
        for (int attempt = 0; attempt < MAX_ATTEMPTS; attempt++) {
          // a root that is a sum of a compound and other kinds (a text component) draws a non-compound half of the time
          if (!(new Gen(mix(seed, type, from, i, attempt), vocab).value(root) instanceof CompoundTag c)) continue;
          sawCompound = true;
          boolean readable = readable(root, c);
          if (attempt == 0 && readable) ok++;
          if (readable) {
            sample = c;
            break;
          }
        }
        if (sample == null) {
          if (!sawCompound && i == base) {
            compound = false;
            break;
          }
          lost++;
          continue;
        }
        String id = "g" + seed + "-" + type + "-" + from + "-" + toVersion + "-" + i;
        Gen damage = new Gen(mix(seed, type, from, i, "invalid"), vocab);
        if (damage.chance(invalid)) {
          // seven in ten are damaged until the type no longer reads them (the error path); the rest may stay readable
          int tries = damage.chance(0.7) ? 30 : 1;
          for (int t = 0; t < tries; t++) {
            CompoundTag c = sample.copy();
            damage.corrupt(c);
            sample = c;
            if (!readable(root, c)) break;
          }
          id += "-bad";
          bad++;
        }
        line(id, type, from, toVersion, sample);
        kept++;
      }
      String state = compound ? "ok" : "noncompound";
      if (!compound) nonCompound++;
      total += compound ? n : 0;
      firstOk += ok;
      emitted += kept;
      dropped += lost;
      damaged += bad;
      String row = type + "\t" + from + "\t" + toVersion + "\t" + state + "\t" + (compound ? n : 0) + "\t" + ok + "\t" + kept + "\t" + lost + "\t" + bad;
      report.add(row);
      if (!quiet) ERR.println("gen " + row.replace('\t', ' '));
    }

    void mutate(String file, int per) throws IOException {
      for (String line : Files.readAllLines(Path.of(file), StandardCharsets.UTF_8)) {
        if (line.isEmpty()) continue;
        String[] f = line.split("\t", -1);
        if (f.length != 5) throw new IllegalArgumentException("--mutate lines need 5 tab separated fields: " + line);
        CompoundTag in = decode(f[4]);
        for (int k = 0; k < per; k++) {
          CompoundTag c = in.copy();
          Gen g = new Gen(mix(seed, "mutate", f[0], k), vocab);
          for (int ops = 1 + g.upto(3); ops > 0; ops--) g.mutate(c);
          line(f[0] + "~" + k, f[1], Integer.parseInt(f[2]), Integer.parseInt(f[3]), c);
          emitted++;
        }
      }
    }

    void finish(String reportFile) throws IOException {
      out.flush();
      ERR.println(
          "gen: samples " + total + ", readable on the first try " + firstOk + ", emitted " + emitted + ", dropped " + dropped + ", damaged on purpose " + damaged + ", absent groups " + absent + ", non-compound groups " + nonCompound);
      if (reportFile != null) Files.write(Path.of(reportFile), (String.join("\n", report) + "\n").getBytes(StandardCharsets.UTF_8));
    }
  }

  // ---- verb ------------------------------------------------------------------------------------------------------------

  static final Set<String> OPTIONS = Set.of("type", "from", "to", "seed", "n", "invalid", "mutate", "matrix", "shard", "report");

  static Map<String, String> options(String[] args) {
    Map<String, String> o = new HashMap<>();
    for (int i = 1; i < args.length; i += 2) {
      if (!args[i].startsWith("--") || !OPTIONS.contains(args[i].substring(2)) || i + 1 >= args.length) {
        throw new IllegalArgumentException("gen: bad option " + args[i] + " (see the header of SampleGen.java)");
      }
      o.put(args[i].substring(2), args[i + 1]);
    }
    return o;
  }

  @SuppressWarnings("unchecked")
  static void run(String[] args, PrintStream out) throws Exception {
    Map<String, String> o = options(args);
    int[] shard = {0, 1};
    if (o.containsKey("shard")) {
      String[] s = o.get("shard").split("/");
      shard = new int[] {Integer.parseInt(s[0]), Integer.parseInt(s[1])};
      if (shard[0] < 0 || shard[0] >= shard[1]) throw new IllegalArgumentException("gen: --shard I/K needs 0 <= I < K");
    }
    int to = o.containsKey("to") ? Integer.parseInt(o.get("to")) : current();
    String matrix = o.get("matrix");
    Emitter e = new Emitter(Vocab.load(), Long.parseLong(o.getOrDefault("seed", "1")), Double.parseDouble(o.getOrDefault("invalid", "0")), to, shard[0], shard[1], matrix != null, out);
    if (o.containsKey("mutate")) {
      e.mutate(o.get("mutate"), Integer.parseInt(o.getOrDefault("n", "20")));
    } else if (matrix == null) {
      if (!o.containsKey("type") || !o.containsKey("from")) throw new IllegalArgumentException("gen needs --type and --from, --mutate or --matrix");
      int n = Integer.parseInt(o.getOrDefault("n", "20"));
      List<String> types = o.get("type").equals("all") ? Arrays.stream(DataFixTypes.values()).map(Enum::name).toList() : List.of(o.get("type").split(","));
      for (String type : types) {
        if (!type.startsWith("ref:")) DataFixTypes.valueOf(type);
        for (String from : o.get("from").split(",")) e.group(type, Integer.parseInt(from), to, n, 0);
      }
    } else {
      int cur = current();
      switch (matrix) {
        case "step" -> {
          int n = Integer.parseInt(o.getOrDefault("n", "20"));
          List<DataFix> fixes = (List<DataFix>) DataFixerOracle.privateField(e.fixer, DataFixerUpper.class, "globalList");
          TreeSet<Integer> versions = new TreeSet<>();
          for (DataFix f : fixes) versions.add(DataFixUtils.getVersion(f.getVersionKey()));
          for (int v : versions) {
            Schema before = e.fixer.getSchema(DataFixUtils.makeKey(v - 1));
            for (String name : new TreeSet<>(before.types())) e.group("ref:" + name, v - 1, v, n, 0);
          }
        }
        case "chain" -> {
          int n = Integer.parseInt(o.getOrDefault("n", "200"));
          for (DataFixTypes t : DataFixTypes.values()) for (int m : MILESTONES) if (m < cur) e.group(t.name(), m, cur, n, 0);
        }
        case "random" -> {
          int n = Integer.parseInt(o.getOrDefault("n", "20000"));
          SplittableRandom r = new SplittableRandom(mix(e.seed, "random"));
          DataFixTypes[] types = DataFixTypes.values();
          List<Object[]> triples = new ArrayList<>();
          for (int i = 0; i < n; i++) {
            DataFixTypes t = types[r.nextInt(types.length)];
            int from = switch (r.nextInt(16)) {
              case 0 -> -1;
              case 1 -> 99;
              case 2 -> 100;
              case 3 -> cur;
              case 4 -> cur + 1 + r.nextInt(20);
              default -> 99 + r.nextInt(cur - 98);
            };
            int toV = switch (r.nextInt(10)) {
              case 0 -> from;
              case 1 -> from - 1 - r.nextInt(50);
              case 2, 3 -> from + r.nextInt(Math.max(1, cur - from + 1));
              case 4 -> 99 + r.nextInt(cur - 98);
              default -> cur;
            };
            triples.add(new Object[] {t.name(), from, toV, i});
          }
          triples.sort(Comparator.<Object[], String>comparing(x -> (String) x[0]).thenComparing(x -> (Integer) x[1]).thenComparing(x -> (Integer) x[2]).thenComparing(x -> (Integer) x[3]));
          for (Object[] x : triples) e.group((String) x[0], (Integer) x[1], (Integer) x[2], 1, (Integer) x[3]);
        }
        default -> throw new IllegalArgumentException("gen: --matrix is step, chain or random");
      }
    }
    e.finish(o.get("report"));
  }
}
