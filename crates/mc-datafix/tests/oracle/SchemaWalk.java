// Normalised description of a DFU type tree: the grammar the Rust side (mc_datafix::Type::description) writes identically.
//
// One line per node, two spaces of indent per depth, every line ends with LF. Strings are quoted with `\` and `"` escaped
// and control characters as \n \r \t or a four digit hex escape. Nodes (children follow one level deeper):
//   prim NAME                       Bool Byte Short Int Long Float Double String NamespacedString
//   empty | passthrough             EmptyPart | EmptyPartPassthrough
//   product | sum                   two children (first, second)
//   list                            one child
//   clist                           two children (key, value)
//   tag "NAME" | named "NAME"       one child
//   check "NAME" INDEX EXPECTED     one child
//   hook PRE POST                   one child; PRE and POST are `identity` (HookFunction.IDENTITY) or `custom` (any other
//                                   function: its behaviour is covered by the fix samples, not by the type description)
//   choice "NAME"                   the key type as first child, then per entry, sorted by key in String.compareTo order
//                                   (a Map.of in a schema iterates in a different order in every JVM): `case "KEY"` with one child
//   mu "FAMILY" INDEX               recursion point, never unfolded
//
// Usage: not run directly; DataFixerOracle schema-dump calls it (scripts/datafix-oracle.sh schema-dump).
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
import com.mojang.datafixers.types.templates.Tag;
import com.mojang.datafixers.types.templates.TaggedChoice;
import java.lang.reflect.Field;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;
import java.util.Map;

final class SchemaWalk {
  private SchemaWalk() {}

  static String describe(Type<?> type) {
    StringBuilder sb = new StringBuilder();
    node(type, 0, sb);
    return sb.toString();
  }

  private static void line(StringBuilder sb, int depth, String text) {
    for (int i = 0; i < depth; i++) sb.append("  ");
    sb.append(text).append('\n');
  }

  static String quote(String s) {
    StringBuilder sb = new StringBuilder("\"");
    for (int i = 0; i < s.length(); i++) {
      char c = s.charAt(i);
      switch (c) {
        case '\\' -> sb.append("\\\\");
        case '"' -> sb.append("\\\"");
        case '\n' -> sb.append("\\n");
        case '\r' -> sb.append("\\r");
        case '\t' -> sb.append("\\t");
        default -> {
          if (c < 0x20) sb.append(String.format("\\u%04x", (int) c));
          else sb.append(c);
        }
      }
    }
    return sb.append('"').toString();
  }

  static Object field(Object o, String name) {
    try {
      Field f = o.getClass().getDeclaredField(name);
      f.setAccessible(true);
      return f.get(o);
    } catch (ReflectiveOperationException e) {
      throw new IllegalStateException("cannot read " + o.getClass().getName() + "." + name, e);
    }
  }

  private static String hookKind(Object f) {
    return f == Hook.HookFunction.IDENTITY ? "identity" : "custom";
  }

  private static void node(Type<?> t, int depth, StringBuilder sb) {
    if (t instanceof Const.PrimitiveType<?> p) {
      line(sb, depth, "prim " + p.codec());
    } else if (t instanceof EmptyPart) {
      line(sb, depth, "empty");
    } else if (t instanceof EmptyPartPassthrough) {
      line(sb, depth, "passthrough");
    } else if (t instanceof Product.ProductType<?, ?> p) {
      line(sb, depth, "product");
      node(p.first(), depth + 1, sb);
      node(p.second(), depth + 1, sb);
    } else if (t instanceof Sum.SumType<?, ?> s) {
      line(sb, depth, "sum");
      node(s.first(), depth + 1, sb);
      node(s.second(), depth + 1, sb);
    } else if (t instanceof com.mojang.datafixers.types.templates.List.ListType<?> l) {
      line(sb, depth, "list");
      node(l.getElement(), depth + 1, sb);
    } else if (t instanceof CompoundList.CompoundListType<?, ?> c) {
      line(sb, depth, "clist");
      node(c.getKey(), depth + 1, sb);
      node(c.getElement(), depth + 1, sb);
    } else if (t instanceof Tag.TagType<?> g) {
      line(sb, depth, "tag " + quote(g.name()));
      node(g.element(), depth + 1, sb);
    } else if (t instanceof Named.NamedType<?> n) {
      line(sb, depth, "named " + quote(n.name()));
      node(n.element(), depth + 1, sb);
    } else if (t instanceof Check.CheckType<?> c) {
      line(sb, depth, "check " + quote((String) field(c, "name")) + " " + field(c, "index") + " " + field(c, "expectedIndex"));
      node((Type<?>) field(c, "delegate"), depth + 1, sb);
    } else if (t instanceof Hook.HookType<?> h) {
      line(sb, depth, "hook " + hookKind(field(h, "preRead")) + " " + hookKind(field(h, "postWrite")));
      node((Type<?>) field(h, "delegate"), depth + 1, sb);
    } else if (t instanceof TaggedChoice.TaggedChoiceType<?> c) {
      line(sb, depth, "choice " + quote(c.getName()));
      node(c.getKeyType(), depth + 1, sb);
      List<Map.Entry<?, Type<?>>> entries = new ArrayList<>(c.types().entrySet());
      entries.sort(Comparator.comparing(e -> String.valueOf(e.getKey())));
      for (Map.Entry<?, Type<?>> e : entries) {
        line(sb, depth + 1, "case " + quote(String.valueOf(e.getKey())));
        node(e.getValue(), depth + 2, sb);
      }
    } else if (t instanceof RecursivePoint.RecursivePointType<?> m) {
      line(sb, depth, "mu " + quote(m.family().name()) + " " + m.index());
    } else {
      throw new IllegalStateException("unknown DFU type class " + t.getClass().getName());
    }
  }
}
