// Coverage of the fix chain by the committed samples: which registered data fix changes the data of at least one sample.
//
// Usage: scripts/datafix-oracle.sh coverage --samples DIR
//   DIR holds the sample fixtures (*.tsv.gz): corpus fixtures (`S ID TYPE FROM BASE64` / `R ID TO ...` lines) and STEP fixtures
//   (alternating request `ID TYPE FROM TO INPUT` and record lines). Every request runs once, as the fixtures record it, through
//   the real DataFixers chain in which every fix is wrapped by a watch: a fix is EXERCISED by a sample when its rule changed the
//   value it was given (the prefix chains P(i-1) and P(i) differ), wrote it differently under the type it retypes it to (a field
//   renamed by the new schema), or threw. The watch sits on the whole-type view of the
//   fix, so it sees the chain in its real order and with its real intermediate types, and costs one pass per sample.
// Output (stdout): `#` header lines (the fixtures with sha256 and number of runs), then one row per data fix in registration
//   order: INDEX VERSION SUBVERSION CLASS CHANGED_BY_REAL CHANGED_BY_SYNTH (the numbers of samples that fix changed). A sample is
//   synthetic when its id is `g<seed>-...` (schema generated) or a fixture is hand written; real when it comes from the world
//   of an old server (corpus ids without the `g<seed>-` prefix, `real-...` ids).
// File fixes work on whole world directories and are not measured here.
import com.mojang.datafixers.DataFix;
import com.mojang.datafixers.DataFixer;
import com.mojang.datafixers.DataFixerUpper;
import com.mojang.datafixers.RewriteResult;
import com.mojang.datafixers.TypeRewriteRule;
import com.mojang.datafixers.View;
import com.mojang.datafixers.functions.PointFree;
import com.mojang.datafixers.schemas.Schema;
import com.mojang.datafixers.types.Type;
import com.mojang.serialization.DynamicOps;
import it.unimi.dsi.fastutil.ints.Int2ObjectSortedMap;
import it.unimi.dsi.fastutil.ints.IntSortedSet;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.*;
import java.util.function.Function;
import java.util.regex.Pattern;
import java.util.stream.Stream;
import java.util.zip.GZIPInputStream;
import net.minecraft.util.datafix.DataFixers;

public class Coverage {
  /** The fixes whose watch saw a change in the run that is executing. */
  static final BitSet hit = new BitSet();

  static final Pattern GENERATED = Pattern.compile("g\\d+-.*");

  /** The real chain with a watch around every fix. */
  static final class Watched extends DataFixerUpper {
    Watched(Int2ObjectSortedMap<Schema> schemas, List<DataFix> fixes, IntSortedSet versions) {
      super(schemas, fixes, versions);
    }

    @SuppressWarnings("unchecked")
    static Watched of(DataFixer real) throws ReflectiveOperationException {
      List<DataFix> list = (List<DataFix>) DataFixerOracle.privateField(real, DataFixerUpper.class, "globalList");
      List<DataFix> wrapped = new ArrayList<>();
      for (int i = 0; i < list.size(); i++) {
        Schema output = (Schema) DataFixerOracle.privateField(list.get(i), DataFix.class, "outputSchema");
        wrapped.add(new Watch(i, list.get(i), output));
      }
      return new Watched(
          (Int2ObjectSortedMap<Schema>) DataFixerOracle.privateField(real, DataFixerUpper.class, "schemas"),
          wrapped,
          (IntSortedSet) DataFixerOracle.privateField(real, DataFixerUpper.class, "fixerVersions"));
    }

    static final class Watch extends DataFix {
      final int index;
      final DataFix fix;

      Watch(int index, DataFix fix, Schema output) {
        super(output, false);
        this.index = index;
        this.fix = fix;
      }

      @Override
      protected TypeRewriteRule makeRule() {
        TypeRewriteRule rule = fix.getRule();
        if (rule == TypeRewriteRule.nop()) return rule;
        return new TypeRewriteRule() {
          @Override
          public <A> Optional<RewriteResult<A, ?>> rewrite(Type<A> type) {
            return rule.rewrite(type).map(r -> watch(r));
          }
        };
      }

      <A, B> RewriteResult<A, B> watch(RewriteResult<A, B> result) {
        if (result.view().isNop()) return result;
        // the fix view as the chain would run it: optimised on its own, the optimiser cannot look inside the watch
        View<A, B> view = result.view().rewriteOrNop(OPTIMIZATION_RULE);
        PointFree<Function<A, B>> inner = view.function();
        // a fix that retypes its value (a field renamed by the new type, a re-read under another shape) cannot be judged by comparing
        // values of two types: compare what each type writes
        boolean retypes = !view.type().equals(view.newType(), false, true);
        PointFree<Function<A, B>> watched = new PointFree<>() {
          @Override
          public Type<Function<A, B>> type() {
            return inner.type();
          }

          @Override
          public Function<DynamicOps<?>, Function<A, B>> eval() {
            Function<DynamicOps<?>, Function<A, B>> run = inner.evalCached();
            return ops -> {
              Function<A, B> f = run.apply(ops);
              return a -> {
                B b;
                try {
                  b = f.apply(a);
                } catch (Throwable t) {
                  hit.set(index);
                  throw t;
                }
                if (retypes ? writesDiffer(ops, view, a, b) : !Objects.equals(a, b)) hit.set(index);
                return b;
              };
            };
          }

          @Override
          public String toString(int level) {
            return inner.toString(level);
          }
        };
        return RewriteResult.create(View.create(watched), result.recData());
      }

      static <T, A, B> boolean writesDiffer(DynamicOps<T> ops, View<A, B> view, A a, B b) {
        return !Objects.equals(view.type().codec().encodeStart(ops, a).result(), view.newType().codec().encodeStart(ops, b).result());
      }
    }
  }

  /**
   * One request of a fixture: the text of the batch line fields. recorded: STATUS, SHA256 and BASE64 vanilla answered (STEP
   * fixtures), null for a corpus run whose newest answer is not kept here.
   */
  record Run(String id, String type, int from, int to, String input, String recorded) {
    boolean real() {
      return !GENERATED.matcher(id).matches() && !id.startsWith("hand-");
    }
  }

  static List<String> lines(Path file) throws IOException {
    try (BufferedReader r = new BufferedReader(new InputStreamReader(new GZIPInputStream(Files.newInputStream(file)), StandardCharsets.UTF_8))) {
      List<String> out = new ArrayList<>();
      for (String l; (l = r.readLine()) != null; ) if (!l.isEmpty() && !l.startsWith("#")) out.add(l);
      return out;
    }
  }

  /** The runs of one fixture; hand written STEP samples (ids outside `g<seed>-` and `real-`) count as synthetic. */
  static List<Run> runs(Path file) throws IOException {
    List<String> lines = lines(file);
    List<Run> out = new ArrayList<>();
    boolean corpus = file.getFileName().toString().startsWith("corpus_");
    if (corpus) {
      // a sample is answered for several targets; the update to the newest one runs every fix of the shorter ones
      Map<String, String[]> samples = new LinkedHashMap<>();
      Map<String, Integer> newest = new HashMap<>();
      for (String l : lines) {
        String[] f = l.split("\t", -1);
        if (f[0].equals("S")) samples.put(f[1], new String[] {f[2], f[3], f[4]});
        else if (f[0].equals("R")) newest.merge(f[1], Integer.parseInt(f[2]), Math::max);
        else throw new IllegalArgumentException("malformed corpus line in " + file);
      }
      for (var e : samples.entrySet()) {
        String[] s = e.getValue();
        out.add(new Run(e.getKey(), s[0], Integer.parseInt(s[1]), newest.get(e.getKey()), s[2], null));
      }
    } else {
      for (int i = 0; i < lines.size(); i += 2) {
        String[] f = lines.get(i).split("\t", -1);
        if (f.length != 5) throw new IllegalArgumentException("malformed request line in " + file + ": " + lines.get(i));
        // a hand written sample has none of the generated or real prefixes: prefix it so real() tells it from a world sample
        String id = f[0].startsWith("real-") || GENERATED.matcher(f[0]).matches() ? f[0] : "hand-" + f[0];
        String[] answer = lines.get(i + 1).split("\t", -1);
        out.add(new Run(id, f[1], Integer.parseInt(f[2]), Integer.parseInt(f[3]), f[4], answer[1] + "\t" + answer[2] + "\t" + answer[3]));
      }
    }
    return out;
  }

  /** Drops the rewrite caches: a rule built for one (type, from, to) is not reused by the next group, and they hold gigabytes. */
  static void dropCaches(DataFixer fixer) throws ReflectiveOperationException {
    ((Map<?, ?>) DataFixerOracle.privateField(null, Type.class, "REWRITE_CACHE")).clear();
    ((Map<?, ?>) DataFixerOracle.privateField(fixer, DataFixerUpper.class, "rules")).clear();
  }

  static void run(String[] args, PrintStream out) throws Exception {
    if (args.length != 3 || !args[1].equals("--samples")) throw new IllegalArgumentException("coverage --samples DIR");
    List<Path> files;
    try (Stream<Path> s = Files.list(Path.of(args[2]))) {
      files = s.filter(p -> p.getFileName().toString().endsWith(".tsv.gz")).sorted().toList();
    }
    DataFixer real = DataFixers.getDataFixer();
    Watched watched = Watched.of(real);
    DataFixerOracle.fixerInUse = watched;
    @SuppressWarnings("unchecked")
    List<DataFix> chain = (List<DataFix>) DataFixerOracle.privateField(real, DataFixerUpper.class, "globalList");
    int[] byReal = new int[chain.size()], bySynth = new int[chain.size()];
    long realRuns = 0, synthRuns = 0, orderOnly = 0, different = 0, compared = 0;
    StringBuilder header = new StringBuilder();
    for (Path file : files) {
      List<Run> runs = new ArrayList<>(runs(file));
      runs.sort(Comparator.comparing(Run::type).thenComparingInt(Run::from).thenComparingInt(Run::to));
      String group = "";
      int done = 0;
      for (Run r : runs) {
        if (++done % 500 == 0) SampleGen.ERR.println("coverage: " + file.getFileName() + ": " + done + " of " + runs.size());
        String key = r.type + " " + r.from + " " + r.to;
        if (!key.equals(group)) {
          dropCaches(watched);
          group = key;
        }
        hit.clear();
        DataFixerOracle.Outcome o = r.type.startsWith("json:")
            ? DataFixerOracle.updateJson(r.type.substring(5), r.from, r.to, r.input)
            : DataFixerOracle.update(r.type, r.from, r.to, DataFixerOracle.input(r.input));
        if (o.status().equals("bad")) throw new IllegalStateException("unreadable sample " + r.id + " of " + file.getFileName());
        if (r.recorded != null) {
          compared++;
          String[] rec = r.recorded.split("\t", -1);
          boolean same = rec[0].equals(o.status()) && rec[1].equals(o.bytes() == null ? "-" : DataFixerOracle.sha256(o.bytes()));
          if (!same && o.bytes() != null && rec[0].equals("ok") && !r.type.startsWith("json:")) {
            // the watched chain must give vanilla's tree; key order may differ (the fused and the unfused chain build maps in another sequence)
            net.minecraft.nbt.CompoundTag mine = net.minecraft.nbt.NbtIo.read(new DataInputStream(new ByteArrayInputStream(o.bytes())), net.minecraft.nbt.NbtAccounter.unlimitedHeap());
            net.minecraft.nbt.CompoundTag theirs = net.minecraft.nbt.NbtIo.read(new DataInputStream(new ByteArrayInputStream(Base64.getDecoder().decode(rec[2]))), net.minecraft.nbt.NbtAccounter.unlimitedHeap());
            same = mine.equals(theirs);
            if (same) orderOnly++;
          }
          // the unfused chain differs from vanilla's fused one on a few inputs (a string boolean kept where the fused chain read it as a byte)
          if (!same) {
            different++;
            SampleGen.ERR.println("coverage: the watched chain differs from the recording for " + r.id + " of " + file.getFileName());
          }
        }
        int[] counts = r.real() ? byReal : bySynth;
        for (int i = hit.nextSetBit(0); i >= 0; i = hit.nextSetBit(i + 1)) counts[i]++;
        if (r.real()) realRuns++;
        else synthRuns++;
      }
      header.append("# fixture\t").append(file.getFileName()).append('\t').append(DataFixerOracle.sha256(Files.readAllBytes(file))).append('\t').append(runs.size()).append('\n');
      SampleGen.ERR.println("coverage: " + file.getFileName() + ": " + runs.size() + " runs");
    }
    out.print("# INDEX VERSION SUBVERSION CLASS CHANGED_BY_REAL CHANGED_BY_SYNTH (samples the fix changed; runs: " + realRuns + " real, " + synthRuns + " synthetic)\n");
    out.print("# of " + compared + " runs recorded in STEP fixtures, " + different + " answer differently from vanilla's own chain in content and " + orderOnly + " in key order only\n");
    out.print(header);
    for (int i = 0; i < chain.size(); i++) {
      int key = chain.get(i).getVersionKey();
      out.print(i + "\t" + key / 10 + "\t" + key % 10 + "\t" + DataFixerOracle.simpleName(chain.get(i)) + "\t" + byReal[i] + "\t" + bySynth[i] + "\n");
    }
  }
}
