// Lists every class of a jar with its kind, nesting, supertypes and source line span (used by scripts/class-map.py).
//
//   java scripts/ClassDump.java JAR   (tab-separated rows on stdout)
//
// Columns: name (binary, `/`-separated), kind (class|interface|enum|record|annotation|module-info|package-info),
// nest (top|member|local|anon), outer (enclosing class or empty), super, interfaces (comma separated),
import java.lang.classfile.Attributes;
import java.lang.classfile.ClassFile;
import java.lang.classfile.ClassModel;
import java.lang.classfile.MethodModel;
import java.lang.classfile.attribute.InnerClassInfo;
import java.lang.constant.ClassDesc;
import java.lang.reflect.AccessFlag;
import java.util.ArrayList;
import java.util.List;
import java.util.stream.Collectors;
import java.util.zip.ZipEntry;
import java.util.zip.ZipFile;

public class ClassDump {
    public static void main(String[] args) throws Exception {
        List<String> rows = new ArrayList<>();
        try (ZipFile zip = new ZipFile(args[0])) {
            for (var e = zip.entries(); e.hasMoreElements(); ) {
                ZipEntry entry = e.nextElement();
                if (!entry.getName().endsWith(".class")) {
                    continue;
                }
                ClassModel cm = ClassFile.of().parse(zip.getInputStream(entry).readAllBytes());
                rows.add(row(cm));
            }
        }
        rows.sort(null);
        rows.forEach(System.out::println);
    }

    static String row(ClassModel cm) {
        String name = cm.thisClass().asInternalName();
        var flags = cm.flags().flags();
        String kind = name.endsWith("package-info") ? "package-info"
            : name.equals("module-info") ? "module-info"
            : flags.contains(AccessFlag.ANNOTATION) ? "annotation"
            : flags.contains(AccessFlag.INTERFACE) ? "interface"
            : flags.contains(AccessFlag.ENUM) ? "enum"
            : cm.superclass().map(s -> s.asInternalName().equals("java/lang/Record")).orElse(false) ? "record"
            : "class";
        String nest = "top";
        String outer = "";
        var inner = cm.findAttribute(Attributes.innerClasses());
        if (inner.isPresent()) {
            for (InnerClassInfo i : inner.get().classes()) {
                if (i.innerClass().asInternalName().equals(name)) {
                    if (i.innerName().isEmpty()) {
                        nest = "anon";
                    } else {
                        nest = i.outerClass().isPresent() ? "member" : "local";
                    }
                    outer = i.outerClass().map(c -> c.asInternalName()).orElse("");
                }
            }
        }
        if (nest.equals("anon") || nest.equals("local")) {
            outer = cm.findAttribute(Attributes.enclosingMethod()).map(a -> a.enclosingClass().asInternalName()).orElse(outer);
        }
        int lo = Integer.MAX_VALUE;
        int hi = 0;
        int methods = 0;
        for (MethodModel m : cm.methods()) {
            methods++;
            var code = m.code();
            if (code.isEmpty()) {
                continue;
            }
            var table = code.get().findAttribute(Attributes.lineNumberTable());
            if (table.isEmpty()) {
                continue;
            }
            for (var l : table.get().lineNumbers()) {
                lo = Math.min(lo, l.lineNumber());
                hi = Math.max(hi, l.lineNumber());
            }
        }
        if (hi == 0) {
            lo = 0;
        }
        boolean record = kind.equals("record");
        var comps = new java.util.HashSet<String>();
        if (record) {
            cm.findAttribute(Attributes.record()).ifPresent(a -> a.components().forEach(c -> comps.add(c.name().stringValue())));
        }
        int own = 0;
        for (MethodModel m : cm.methods()) {
            String n = m.methodName().stringValue();
            var f = m.flags().flags();
            if (n.equals("<init>") || n.equals("<clinit>") || f.contains(AccessFlag.SYNTHETIC) || f.contains(AccessFlag.BRIDGE)
                || n.startsWith("lambda$") || n.startsWith("$values") || n.startsWith("access$")) {
                continue;
            }
            boolean noArgs = m.methodTypeSymbol().parameterCount() == 0;
            if (kind.equals("enum") && (n.equals("values") && noArgs || n.equals("valueOf") && m.methodTypeSymbol().parameterCount() == 1)) {
                continue;
            }
            if (record && (noArgs && (comps.contains(n) || n.equals("toString") || n.equals("hashCode")) || n.equals("equals"))) {
                continue;
            }
            own++;
        }
        int fields = 0;
        if (!record) {
            for (var fm : cm.fields()) {
                var f = fm.flags().flags();
                if (!f.contains(AccessFlag.STATIC) && !f.contains(AccessFlag.SYNTHETIC)) {
                    fields++;
                }
            }
        }
        String sup = cm.superclass().map(s -> s.asInternalName()).orElse("");
        String ifaces = cm.interfaces().stream().map(i -> i.asInternalName()).collect(Collectors.joining(","));
        return String.join("\t", name, kind, nest, outer, sup, ifaces, "" + lo, "" + hi, "" + methods, "" + own, "" + fields);
    }
}
