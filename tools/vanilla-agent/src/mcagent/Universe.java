package mcagent;

import java.io.BufferedWriter;
import java.io.IOException;
import java.io.OutputStreamWriter;
import java.lang.classfile.ClassFile;
import java.lang.classfile.ClassModel;
import java.lang.classfile.CodeModel;
import java.lang.classfile.Label;
import java.lang.classfile.MethodModel;
import java.lang.classfile.instruction.FieldInstruction;
import java.lang.classfile.instruction.InvokeInstruction;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.HashMap;
import java.util.HashSet;
import java.util.Map;
import java.util.Set;
import java.util.TreeMap;
import java.util.zip.GZIPOutputStream;
import java.util.zip.ZipFile;

/**
 * Every method of the classes of a jar under a name prefix, with what a coverage report needs to explain an unreached one.
 *
 * <p>Usage: {@code java -cp agent.jar mcagent.Universe SERVER.jar OUT.tsv.gz [PREFIX]} (PREFIX defaults to
 * {@code net/minecraft/}).
 *
 * <p>Output: one row per method with code, {@code class name desc access nMarkers callers debugOnly}. {@code nMarkers} is the
 * number of markers the agent places in it; {@code callers} the number of methods of the jar that call it by name;
 * {@code debugOnly} is 1 when it has callers and each of them reads a {@code SharedConstants.DEBUG_*} flag or is itself
 * {@code debugOnly}, i.e. the method is reachable only behind a debug switch.
 */
public final class Universe {
    private record Id(String cls, String name, String desc) {}

    private Universe() {}

    public static void main(String[] args) throws IOException {
        String prefix = args.length > 2 ? args[2] : "net/minecraft/";
        Map<Id, Integer> methods = new TreeMap<>((a, b) -> {
            int c = a.cls().compareTo(b.cls());
            c = c != 0 ? c : a.name().compareTo(b.name());
            return c != 0 ? c : a.desc().compareTo(b.desc());
        });
        Map<Id, Set<Id>> callers = new HashMap<>();
        Map<Id, Integer> access = new HashMap<>();
        Set<Id> flagReaders = new HashSet<>();
        ClassFile cf = ClassFile.of();
        try (ZipFile zip = new ZipFile(args[0])) {
            for (var entries = zip.entries(); entries.hasMoreElements(); ) {
                var e = entries.nextElement();
                if (!e.getName().endsWith(".class") || !e.getName().startsWith(prefix)) {
                    continue;
                }
                ClassModel cm = cf.parse(zip.getInputStream(e).readAllBytes());
                String cls = cm.thisClass().asInternalName();
                for (MethodModel mm : cm.methods()) {
                    CodeModel code = mm.code().orElse(null);
                    if (code == null) {
                        continue;
                    }
                    Id self = new Id(cls, mm.methodName().stringValue(), mm.methodType().stringValue());
                    access.put(self, mm.flags().flagsMask());
                    // the agent's count: entry, block starts, conditional fall-throughs
                    Set<Label> targets = new HashSet<>();
                    methods.put(self, 1 + Agent.scan(code, targets) + targets.size());
                    for (var el : code.elementList()) {
                        if (el instanceof InvokeInstruction inv) {
                            Id callee = new Id(inv.owner().asInternalName(), inv.name().stringValue(), inv.type().stringValue());
                            callers.computeIfAbsent(callee, k -> new HashSet<>()).add(self);
                        } else if (el instanceof FieldInstruction f && f.opcode().bytecode() == 178
                                && f.owner().asInternalName().equals("net/minecraft/SharedConstants")
                                && f.name().stringValue().startsWith("DEBUG_")) {
                            flagReaders.add(self);
                        }
                    }
                }
            }
        }
        // least fixpoint: a method is debug-only when every caller reads a flag or is debug-only
        Set<Id> debugOnly = new HashSet<>();
        boolean changed = true;
        while (changed) {
            changed = false;
            for (Id m : methods.keySet()) {
                Set<Id> cs = callers.get(m);
                if (cs == null || debugOnly.contains(m)) {
                    continue;
                }
                boolean all = true;
                boolean any = false;
                for (Id c : cs) {
                    if (c.equals(m)) {
                        continue; // recursion says nothing about who reaches the method
                    }
                    any = true;
                    if (!flagReaders.contains(c) && !debugOnly.contains(c)) {
                        all = false;
                        break;
                    }
                }
                all &= any;
                if (all) {
                    debugOnly.add(m);
                    changed = true;
                }
            }
        }
        try (BufferedWriter w = new BufferedWriter(new OutputStreamWriter(new GZIPOutputStream(Files.newOutputStream(Path.of(args[1]))), StandardCharsets.UTF_8))) {
            for (var m : methods.entrySet()) {
                Id id = m.getKey();
                Set<Id> cs = callers.getOrDefault(id, Set.of());
                w.write(id.cls() + "\t" + id.name() + "\t" + id.desc() + "\t" + access.get(id) + "\t" + m.getValue() + "\t" + cs.size() + "\t" + (debugOnly.contains(id) ? 1 : 0) + "\n");
            }
        }
    }
}
