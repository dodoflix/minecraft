package mcagent;

import java.io.BufferedWriter;
import java.io.IOException;
import java.io.OutputStreamWriter;
import java.lang.classfile.ClassFile;
import java.lang.classfile.ClassHierarchyResolver;
import java.lang.classfile.ClassModel;
import java.lang.classfile.CodeBuilder;
import java.lang.classfile.CodeElement;
import java.lang.classfile.CodeModel;
import java.lang.classfile.CodeTransform;
import java.lang.classfile.Instruction;
import java.lang.classfile.Label;
import java.lang.classfile.MethodModel;
import java.lang.classfile.instruction.BranchInstruction;
import java.lang.classfile.instruction.ExceptionCatch;
import java.lang.classfile.instruction.LabelTarget;
import java.lang.classfile.instruction.LineNumber;
import java.lang.classfile.instruction.LookupSwitchInstruction;
import java.lang.classfile.instruction.SwitchCase;
import java.lang.classfile.instruction.TableSwitchInstruction;
import java.lang.constant.ClassDesc;
import java.lang.constant.ConstantDescs;
import java.lang.instrument.ClassFileTransformer;
import java.lang.instrument.Instrumentation;
import java.lang.invoke.VarHandle;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.security.ProtectionDomain;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Comparator;
import java.util.HashSet;
import java.util.List;
import java.util.Set;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.SynchronousQueue;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.zip.GZIPOutputStream;

/**
 * Execution-coverage agent for the vanilla server and the oracle programs (JDK 25+, no dependency).
 *
 * <p>{@code -javaagent:agent.jar=out=DIR[,prefix=net/minecraft/][,mode=flag|count]} rewrites every method of the classes whose
 * internal name starts with {@code prefix}: one hit marker at the method entry and one per basic block (branch targets,
 * exception handlers, the fall-through of every conditional branch). {@code flag} stores a 1 (cheap, race free), {@code count}
 * increments an int (approximate under threads). The result is {@code DIR/LABEL.PID.cov.gz}, written at JVM shutdown and
 * whenever the control file {@code DIR/LABEL.PID.ctl} (or {@code MC_VANILLA_AGENT_CONTROL}) holds {@code dump [PATH]} or
 * {@code reset}. {@code LABEL} is {@code MC_VANILLA_AGENT_LABEL} or the main class.
 *
 * <p>The agent must not change what the program does, down to identity hash codes: a class is rewritten on a thread of its own
 * (the loading thread only waits), so the loader threads draw no hash codes for the library's objects, and every
 * original attribute (line numbers, local variables) is kept.
 *
 * <p>File format: {@code #cov 1 ...} header lines, then one tab separated row per method:
 * {@code class name desc access n hexmask lines [counts]}; marker 0 is the method entry, bit {@code i} of the little-endian hex
 * mask is marker {@code i}, {@code lines} the source line of each marker.
 */
public final class Agent {
    private static final int CAPACITY = 1 << 23;
    private static final ClassDesc HITS = ClassDesc.of("mcagent.Hits");
    private static final ClassDesc BYTES = ConstantDescs.CD_byte.arrayType();
    private static final ClassDesc INTS = ConstantDescs.CD_int.arrayType();

    private record Method(String cls, String name, String desc, int access, int base, int[] lines) {}

    private record Request(ClassLoader loader, String name, byte[] bytes, CountDownLatch done, byte[][] result, AtomicBoolean abandoned) {}

    private static final Object LOCK = new Object();
    private static final List<Method> METHODS = new ArrayList<>();
    private static final List<String> SKIPPED = new ArrayList<>();
    private static final SynchronousQueue<Request> QUEUE = new SynchronousQueue<>();
    // a handful of loaders at most: a linear scan avoids hashing (and so hash-coding) the loader objects
    private static final List<ClassLoader> LOADERS = new ArrayList<>();
    private static final List<ClassHierarchyResolver> RESOLVERS = new ArrayList<>();
    private static int next;
    private static boolean count;
    private static String prefix = "net/minecraft/";
    private static Path outFile;
    private static Path controlFile;
    private static String label;

    private Agent() {}

    public static void premain(String args, Instrumentation inst) throws Exception {
        Path dir = Path.of("coverage");
        for (String kv : args == null || args.isBlank() ? new String[0] : args.split(",")) {
            int eq = kv.indexOf('=');
            String k = eq < 0 ? kv : kv.substring(0, eq);
            String v = eq < 0 ? "" : kv.substring(eq + 1);
            switch (k) {
                case "out" -> dir = Path.of(v);
                case "prefix" -> prefix = v;
                case "mode" -> count = v.equals("count");
                default -> throw new IllegalArgumentException("mcagent: unknown option " + k);
            }
        }
        Files.createDirectories(dir);
        label = sanitize(System.getenv("MC_VANILLA_AGENT_LABEL") != null ? System.getenv("MC_VANILLA_AGENT_LABEL") : mainClass());
        String stem = label + "." + ProcessHandle.current().pid();
        outFile = dir.resolve(stem + ".cov.gz");
        String ctl = System.getenv("MC_VANILLA_AGENT_CONTROL");
        controlFile = ctl != null && !ctl.isEmpty() ? Path.of(ctl) : dir.resolve(stem + ".ctl");
        Hits.F = new byte[CAPACITY];
        if (count) {
            Hits.C = new int[CAPACITY];
        }
        // the transformation classes are loaded now, not while a vanilla class is half loaded
        instrument(Agent.class.getClassLoader(), Agent.class.getResourceAsStream("Hits.class").readAllBytes(), true, new ArrayList<>());
        Thread worker = new Thread(Agent::work, "mcagent");
        worker.setDaemon(true);
        worker.start();
        Runtime.getRuntime().addShutdownHook(new Thread(() -> dumpQuietly(outFile), "mcagent-dump"));
        inst.addTransformer(new ClassFileTransformer() {
            @Override
            public byte[] transform(ClassLoader loader, String name, Class<?> redefined, ProtectionDomain pd, byte[] bytes) {
                if (name == null || redefined != null || !name.startsWith(prefix) || name.startsWith("mcagent/")) {
                    return null;
                }
                Request r = new Request(loader, name, bytes, new CountDownLatch(1), new byte[1][], new AtomicBoolean());
                try {
                    // never hang the program on the agent: a class the worker does not take or finish in time loads as it is
                    if (!QUEUE.offer(r, 60, TimeUnit.SECONDS) || !r.done().await(60, TimeUnit.SECONDS)) {
                        r.abandoned().set(true);
                        return null;
                    }
                } catch (InterruptedException e) {
                    r.abandoned().set(true);
                    Thread.currentThread().interrupt();
                    return null;
                }
                return r.result()[0];
            }
        });
    }

    private static String mainClass() {
        String cmd = System.getProperty("sun.java.command", "java");
        String first = cmd.split(" ")[0];
        return first.substring(first.lastIndexOf('/') + 1).replaceAll("\\.(jar|java)$", "");
    }

    private static String sanitize(String s) {
        return s.replaceAll("[^A-Za-z0-9._-]", "_");
    }

    private static void work() {
        while (true) {
            try {
                Request r = QUEUE.poll(250, TimeUnit.MILLISECONDS);
                if (r == null) {
                    control();
                    continue;
                }
                r.result()[0] = rewrite(r);
                r.done().countDown();
            } catch (InterruptedException e) {
                return;
            } catch (Throwable t) {
                t.printStackTrace();
            }
        }
    }

    private static byte[] rewrite(Request r) {
        List<Method> found = new ArrayList<>();
        String why;
        try {
            byte[] out = instrument(r.loader(), r.bytes(), true, found);
            register(r, found);
            return out;
        } catch (Throwable t) {
            why = t.toString();
        }
        // a method that outgrows the 64 KiB limit with block markers still gets its entry marker
        found.clear();
        try {
            byte[] out = instrument(r.loader(), r.bytes(), false, found);
            register(r, found);
            synchronized (LOCK) {
                SKIPPED.add(r.name() + "\tmethod markers only\t" + why);
            }
            return out;
        } catch (Throwable t) {
            synchronized (LOCK) {
                SKIPPED.add(r.name() + "\tnot instrumented\t" + t);
            }
            return null;
        }
    }

    private static void register(Request r, List<Method> found) {
        synchronized (LOCK) {
            // an abandoned class loaded without markers: its methods must not be listed as never entered
            if (!r.abandoned().get()) {
                METHODS.addAll(found);
            }
        }
    }

    private static int reserve(int n) {
        synchronized (LOCK) {
            if (next + n > CAPACITY) {
                throw new IllegalStateException("marker capacity exhausted");
            }
            int base = next;
            next += n;
            return base;
        }
    }

    private static ClassHierarchyResolver resolver(ClassLoader loader) {
        for (int i = 0; i < LOADERS.size(); i++) {
            if (LOADERS.get(i) == loader) {
                return RESOLVERS.get(i);
            }
        }
        // parsing resources never loads a class: the default resolver's Class.forName would wait for a lock the
        // thread that waits for us holds
        ClassHierarchyResolver r = ClassHierarchyResolver.ofResourceParsing(loader == null ? ClassLoader.getSystemClassLoader() : loader);
        LOADERS.add(loader);
        RESOLVERS.add(r);
        return r;
    }

    private static byte[] instrument(ClassLoader loader, byte[] bytes, boolean blocks, List<Method> found) {
        ClassFile cf = ClassFile.of(
                ClassFile.StackMapsOption.GENERATE_STACK_MAPS,
                ClassFile.ClassHierarchyResolverOption.of(resolver(loader)));
        ClassModel cm = cf.parse(bytes);
        String cls = cm.thisClass().asInternalName();
        return cf.transformClass(cm, (cb, ce) -> {
            if (ce instanceof MethodModel mm && mm.code().isPresent()) {
                cb.transformMethod(mm, (mb, me) -> {
                    if (me instanceof CodeModel code) {
                        mb.transformCode(code, new Markers(code, blocks, cls, mm, found));
                    } else {
                        mb.with(me);
                    }
                });
            } else {
                cb.with(ce);
            }
        });
    }

    private static boolean conditional(BranchInstruction b) {
        return switch (b.opcode()) {
            case GOTO, GOTO_W -> false;
            default -> true;
        };
    }

    /** Collects the labels that start a block into {@code targets}; returns the number of conditional branches. */
    static int scan(CodeModel code, Set<Label> targets) {
        int conditional = 0;
        for (CodeElement e : code.elementList()) {
            switch (e) {
                case BranchInstruction b -> {
                    targets.add(b.target());
                    if (conditional(b)) {
                        conditional++;
                    }
                }
                case LookupSwitchInstruction s -> addSwitch(targets, s.defaultTarget(), s.cases());
                case TableSwitchInstruction s -> addSwitch(targets, s.defaultTarget(), s.cases());
                case ExceptionCatch c -> targets.add(c.handler());
                default -> { }
            }
        }
        return conditional;
    }

    private static void addSwitch(Set<Label> targets, Label dflt, List<SwitchCase> cases) {
        targets.add(dflt);
        for (SwitchCase c : cases) {
            targets.add(c.target());
        }
    }

    /** Inserts a marker at the method entry, after every branch-target label and after every conditional branch. */
    private static final class Markers implements CodeTransform {
        private final Set<Label> targets = new HashSet<>();
        private final boolean blocks;
        private final int base;
        private final int[] lines;
        private int used;
        private int line;
        private int pending = -1;

        Markers(CodeModel code, boolean blocks, String cls, MethodModel mm, List<Method> found) {
            this.blocks = blocks;
            int n = 1;
            if (blocks) {
                n += scan(code, targets) + targets.size();
            }
            this.base = reserve(n);
            this.lines = new int[n];
            found.add(new Method(cls, mm.methodName().stringValue(), mm.methodType().stringValue(), mm.flags().flagsMask(), base, lines));
        }

        private void mark(CodeBuilder b) {
            int i = used++;
            lines[i] = line;
            pending = i;
            int id = base + i;
            if (count) {
                b.getstatic(HITS, "C", INTS).loadConstant(id).dup2().iaload().iconst_1().iadd().iastore();
            } else {
                // read first: a store from every thread to the same cache line makes the server's worker threads crawl
                Label done = b.newLabel();
                b.getstatic(HITS, "F", BYTES).loadConstant(id).baload().ifne(done);
                b.getstatic(HITS, "F", BYTES).loadConstant(id).iconst_1().bastore();
                b.labelBinding(done);
            }
        }

        @Override
        public void atStart(CodeBuilder b) {
            mark(b);
        }

        @Override
        public void accept(CodeBuilder b, CodeElement e) {
            b.with(e);
            if (e instanceof LineNumber ln) {
                line = ln.line();
                // a block's marker carries the line of its first statement, which follows the label
                if (pending >= 0) {
                    lines[pending] = line;
                }
                pending = -1;
            } else if (e instanceof Instruction) {
                pending = -1;
                if (blocks && e instanceof BranchInstruction br && conditional(br)) {
                    mark(b);
                }
            } else if (blocks && e instanceof LabelTarget t && targets.contains(t.label())) {
                mark(b);
            }
        }
    }

    private static void control() {
        if (!Files.exists(controlFile)) {
            return;
        }
        try {
            String[] cmd = Files.readString(controlFile).trim().split("\\s+");
            if (cmd[0].equals("dump")) {
                dump(cmd.length > 1 ? Path.of(cmd[1]) : outFile);
            } else if (cmd[0].equals("reset")) {
                synchronized (LOCK) {
                    Arrays.fill(Hits.F, 0, next, (byte) 0);
                    if (count) {
                        Arrays.fill(Hits.C, 0, next, 0);
                    }
                }
            }
            // last: whoever wrote the file waits for it to disappear
            Files.deleteIfExists(controlFile);
        } catch (IOException e) {
            e.printStackTrace();
        }
    }

    private static void dumpQuietly(Path out) {
        try {
            dump(out);
        } catch (IOException e) {
            e.printStackTrace();
        }
    }

    /** Writes the current markers to {@code out} (replaced atomically). */
    public static void dump(Path out) throws IOException {
        List<Method> snapshot;
        List<String> skipped;
        int used;
        synchronized (LOCK) {
            snapshot = new ArrayList<>(METHODS);
            skipped = new ArrayList<>(SKIPPED);
            used = next;
        }
        VarHandle.fullFence();
        snapshot.sort(Comparator.comparing(Method::cls).thenComparing(Method::name).thenComparing(Method::desc));
        Path tmp = out.resolveSibling(out.getFileName() + ".tmp");
        try (BufferedWriter w = new BufferedWriter(new OutputStreamWriter(new GZIPOutputStream(Files.newOutputStream(tmp)), StandardCharsets.UTF_8), 1 << 16)) {
            w.write("#cov 1 mode=" + (count ? "count" : "flag") + " label=" + label + " prefix=" + prefix + " markers=" + used + "\n");
            for (String s : skipped) {
                w.write("#skipped\t" + s + "\n");
            }
            StringBuilder mask = new StringBuilder();
            for (Method m : snapshot) {
                int n = m.lines().length;
                mask.setLength(0);
                for (int i = 0; i < n; i += 4) {
                    int nib = 0;
                    for (int j = 0; j < 4 && i + j < n; j++) {
                        nib |= (Hits.F[m.base() + i + j] != 0 || count && Hits.C[m.base() + i + j] != 0 ? 1 : 0) << j;
                    }
                    mask.append(Character.forDigit(nib, 16));
                }
                w.write(m.cls() + "\t" + m.name() + "\t" + m.desc() + "\t" + m.access() + "\t" + n + "\t" + mask + "\t");
                for (int i = 0; i < n; i++) {
                    w.write((i == 0 ? "" : ",") + m.lines()[i]);
                }
                if (count) {
                    w.write("\t");
                    for (int i = 0; i < n; i++) {
                        w.write((i == 0 ? "" : ",") + Hits.C[m.base() + i]);
                    }
                }
                w.write("\n");
            }
        }
        Files.move(tmp, out, StandardCopyOption.REPLACE_EXISTING, StandardCopyOption.ATOMIC_MOVE);
    }
}
