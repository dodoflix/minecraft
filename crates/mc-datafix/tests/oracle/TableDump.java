// The lookup tables of vanilla's data fixes as data: the static Map/Collection/array/BitSet/Dynamic fields of the table
// classes, read by reflection after class initialisation, and the results of the public accessors over them.
//
// `tables DIR` writes DIR/<snake_case class>.tsv for every class of CLASSES. Line formats, columns tab separated, one
// header line `# <class name>` first; every value is typed SNBT (no raw tab or newline can occur in it):
//   FIELD  map    KEY VALUE     one entry of a Map (Java iteration order)
//   FIELD  set    VALUE         one element of a Set
//   FIELD  list   VALUE         one element of a List or array; an empty column is a null element
//   FIELD  bits   INDEX         one set bit of a BitSet
//   FIELD  value  VALUE         a Dynamic field
//   FIELD  default VALUE        first line of a fastutil map with a defaultReturnValue (not null): what get returns for a missing key
// Encoding of a Java value as a tag: String -> string, Integer/Long/Short/Byte/Float/Double -> the numeric tag,
// Boolean -> byte, a record -> a compound of its components, UUID/Identifier -> their string form, Dynamic -> its NbtOps value, Pair -> {l:..,r:..}, a nested Map -> list of {k:..,v:..} sorted by text, nested
// Collection/array -> list. Fields come in name order. Collections whose iteration order is randomised per JVM run
// (Map.of/Set.of, anything keyed by a Dynamic or a Pair of them) are written sorted by their line text; all others keep the Java order.
//
// `table-accessors` prints `ACCESSOR<TAB>ARGUMENT...<TAB>RESULT` rows of the public accessors (BlockStateData.getTag,
// upgradeBlock, upgradeBlockStateTag, ItemStackTheFlatteningFix.updateItem, ItemIdFix.getItem,
// EntityBlockStateFix.getBlockId) over every key of their tables, the ids around the table bounds and unknown inputs;
// `-` stands for null.
import com.mojang.datafixers.util.Pair;
import com.mojang.serialization.Dynamic;
import java.lang.reflect.Array;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.lang.reflect.Modifier;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.*;
import net.minecraft.nbt.*;

public class TableDump {
  static final String FIXES = "net.minecraft.util.datafix.fixes.";
  static final String SCHEMAS = "net.minecraft.util.datafix.schemas.";

  /** Classes whose tables are data: the table classes, the table constants of three schemas, and every other class with a table of 16 or more entries. */
  static final String[] CLASSES = {
    FIXES + "AdvancementsFix",
    FIXES + "AttributeModifierIdFix",
    FIXES + "BannerPatternFormatFix",
    FIXES + "BiomeFix",
    FIXES + "BlockEntityIdFix",
    FIXES + "BlockStateData",
    FIXES + "CavesAndCliffsRenames",
    FIXES + "ChunkHeightAndBiomeFix",
    FIXES + "ChunkPalettedStorageFix$MappingConstants",
    FIXES + "ChunkStructuresTemplateRenameFix",
    FIXES + "DyeItemRenameFix",
    FIXES + "EntityBlockStateFix",
    FIXES + "EntityHealthFix",
    FIXES + "EntityIdFix",
    FIXES + "EntityTheRenameningFix",
    FIXES + "EntityUUIDFix",
    FIXES + "ItemIdFix",
    FIXES + "ItemPotionFix",
    FIXES + "ItemShulkerBoxColorFix",
    FIXES + "ItemSpawnEggFix",
    FIXES + "ItemStackComponentizationFix",
    FIXES + "ItemStackEnchantmentNamesFix",
    FIXES + "ItemStackSpawnEggFix",
    FIXES + "ItemStackTheFlatteningFix",
    FIXES + "LeavesFix",
    FIXES + "LevelDataGeneratorOptionsFix",
    FIXES + "MobEffectIdFix",
    FIXES + "OptionsKeyLwjgl3Fix",
    FIXES + "RecipesFix",
    FIXES + "RecipesRenameningFix",
    FIXES + "RenamedCoralFansFix",
    FIXES + "RenamedCoralFix",
    FIXES + "ScoreboardDisplaySlotFix",
    FIXES + "StatsCounterFix",
    FIXES + "StonecutterRecipeRenameFix",
    FIXES + "StructuresBecomeConfiguredFix",
    FIXES + "TooltipDisplayComponentFix",
    FIXES + "TrialSpawnerConfigInRegistryFix$VanillaTrialChambers",
    SCHEMAS + "V704",
    SCHEMAS + "V705",
    SCHEMAS + "V99",
  };

  static String snake(String className) {
    String simple = className.substring(className.lastIndexOf('.') + 1).replace('$', '_');
    return simple.replaceAll("([a-z0-9])([A-Z])", "$1_$2").replaceAll("([A-Z]+)([A-Z][a-z])", "$1_$2").toLowerCase(Locale.ROOT);
  }

  // ---- dump ---------------------------------------------------------------------------------------------------

  static void dump(Path dir) throws Exception {
    Files.createDirectories(dir);
    Set<String> seen = new HashSet<>();
    for (String name : CLASSES) {
      String file = snake(name) + ".tsv";
      if (!seen.add(file)) throw new IllegalStateException("two classes map to " + file);
      Files.writeString(dir.resolve(file), dumpClass(Class.forName(name, true, TableDump.class.getClassLoader())), StandardCharsets.UTF_8);
    }
  }

  static String dumpClass(Class<?> c) throws Exception {
    StringBuilder out = new StringBuilder("# ").append(c.getName()).append('\n');
    Field[] fields = c.getDeclaredFields();
    Arrays.sort(fields, Comparator.comparing(Field::getName));
    int dumped = 0;
    for (Field f : fields) {
      if (!Modifier.isStatic(f.getModifiers()) || f.isSynthetic()) continue;
      Class<?> t = f.getType();
      boolean table = Map.class.isAssignableFrom(t) || Collection.class.isAssignableFrom(t) || t == BitSet.class || t.isArray()
          || Dynamic.class.isAssignableFrom(t);
      if (!table) continue;
      f.setAccessible(true);
      Object v = f.get(null);
      if (v == null) throw new IllegalStateException(c.getName() + "." + f.getName() + " is null");
      dumpField(out, f.getName(), v);
      if (c.getSimpleName().equals("BlockStateData") && f.getName().equals("MAP")) {
        // MAP_FIRST: per slot the lowest slot holding the identical object (-1 for a null slot); ChunkPalettedStorageFix compares states by identity
        Object[] slots = (Object[]) v;
        Map<Object, Integer> first = new IdentityHashMap<>();
        int[] shared = new int[slots.length];
        for (int i = 0; i < slots.length; i++) {
          Integer j = slots[i] == null ? null : first.putIfAbsent(slots[i], i);
          shared[i] = slots[i] == null ? -1 : j == null ? i : j;
        }
        dumpField(out, "MAP_FIRST", shared);
      }
      dumped++;
    }
    if (dumped == 0) throw new IllegalStateException("no table field in " + c.getName());
    return out.toString();
  }

  static void dumpField(StringBuilder out, String name, Object v) {
    List<String> lines = new ArrayList<>();
    if (v instanceof Map<?, ?> m) {
      for (Map.Entry<?, ?> e : m.entrySet()) lines.add(name + "\tmap\t" + text(enc(e.getKey())) + "\t" + text(enc(e.getValue())));
      if (randomOrder(v)) Collections.sort(lines);
      try {
        Object d = v.getClass().getMethod("defaultReturnValue").invoke(v);
        if (d != null) lines.add(0, name + "\tdefault\t" + text(enc(d)));
      } catch (NoSuchMethodException e) {
        // not a fastutil map
      } catch (ReflectiveOperationException e) {
        throw new IllegalStateException(e);
      }
    } else if (v instanceof BitSet b) {
      b.stream().forEach(i -> lines.add(name + "\tbits\t" + i));
    } else if (v instanceof Dynamic<?>) {
      lines.add(name + "\tvalue\t" + text(enc(v)));
    } else {
      boolean set = v instanceof Set<?>;
      Iterable<?> items = v instanceof Collection<?> c ? c : arrayList(v);
      for (Object o : items) lines.add(name + (set ? "\tset\t" : "\tlist\t") + (o == null ? "" : text(enc(o))));
      if (set && randomOrder(v)) Collections.sort(lines);
    }
    for (String l : lines) out.append(l).append('\n');
  }

  static List<Object> arrayList(Object array) {
    List<Object> r = new ArrayList<>();
    for (int i = 0; i < Array.getLength(array); i++) r.add(Array.get(array, i));
    return r;
  }

  /** Map.of/Set.of iterate in an order that changes with every JVM run; so does anything hashed by a Dynamic (identity hash of its ops); pairs hash their members. */
  static boolean randomOrder(Object v) {
    if (v.getClass().getName().startsWith("java.util.ImmutableCollections$")) return true;
    Collection<?> keys = v instanceof Map<?, ?> m ? m.keySet() : (Collection<?>) v;
    for (Object k : keys) return k instanceof Dynamic<?> || k instanceof Pair<?, ?>;
    return false;
  }

  static String text(Tag tag) {
    String s = tag.toString();
    if (s.indexOf('\t') >= 0 || s.indexOf('\n') >= 0 || s.indexOf('\r') >= 0) throw new IllegalStateException("raw control character in " + s);
    return s;
  }

  static Tag enc(Object o) {
    if (o == null) throw new IllegalArgumentException("null inside a table value");
    if (o instanceof String s) return StringTag.valueOf(s);
    if (o instanceof Integer i) return IntTag.valueOf(i);
    if (o instanceof Long l) return LongTag.valueOf(l);
    if (o instanceof Short s) return ShortTag.valueOf(s);
    if (o instanceof Byte b) return ByteTag.valueOf(b);
    if (o instanceof Boolean b) return ByteTag.valueOf(b);
    if (o instanceof Float f) return FloatTag.valueOf(f);
    if (o instanceof Double d) return DoubleTag.valueOf(d);
    if (o instanceof UUID || o instanceof net.minecraft.resources.Identifier) return StringTag.valueOf(o.toString());
    if (o instanceof Dynamic<?> d) return (Tag) d.convert(NbtOps.INSTANCE).getValue();
    if (o instanceof Pair<?, ?> p) {
      CompoundTag t = new CompoundTag();
      t.put("l", enc(p.getFirst()));
      t.put("r", enc(p.getSecond()));
      return t;
    }
    if (o.getClass().isRecord()) {
      CompoundTag t = new CompoundTag();
      try {
        for (var rc : o.getClass().getRecordComponents()) {
          rc.getAccessor().setAccessible(true);
          t.put(rc.getName(), enc(rc.getAccessor().invoke(o)));
        }
      } catch (ReflectiveOperationException e) {
        throw new IllegalStateException(e);
      }
      return t;
    }
    if (o instanceof Map<?, ?> m) {
      List<Tag> items = new ArrayList<>();
      for (Map.Entry<?, ?> e : m.entrySet()) {
        CompoundTag t = new CompoundTag();
        t.put("k", enc(e.getKey()));
        t.put("v", enc(e.getValue()));
        items.add(t);
      }
      // a map nested in a value can be built from a randomly ordered one (StructuresBecomeConfiguredFix): always sorted
      return list(items, true);
    }
    if (o instanceof Collection<?> c) {
      List<Tag> items = new ArrayList<>();
      for (Object e : c) items.add(enc(e));
      return list(items, o instanceof Set<?> && randomOrder(o));
    }
    if (o.getClass().isArray()) {
      List<Tag> items = new ArrayList<>();
      for (Object e : arrayList(o)) items.add(enc(e));
      return list(items, false);
    }
    throw new IllegalArgumentException("cannot encode " + o.getClass().getName());
  }

  static ListTag list(List<Tag> items, boolean sort) {
    if (sort) items.sort(Comparator.comparing(Tag::toString));
    ListTag l = new ListTag();
    l.addAll(items);
    return l;
  }

  // ---- accessors ----------------------------------------------------------------------------------------------

  static Object staticField(Class<?> c, String name) throws Exception {
    Field f = c.getDeclaredField(name);
    f.setAccessible(true);
    return f.get(null);
  }

  static String show(Object o) {
    if (o == null) return "-";
    if (o instanceof Dynamic<?> d) return text((Tag) d.convert(NbtOps.INSTANCE).getValue());
    return o.toString();
  }

  static void accessors(java.io.PrintStream out) throws Exception {
    ClassLoader cl = TableDump.class.getClassLoader();
    Class<?> bsd = Class.forName(FIXES + "BlockStateData", true, cl);
    Method getTag = bsd.getMethod("getTag", int.class);
    Method upgradeId = bsd.getMethod("upgradeBlock", int.class);
    Method upgradeName = bsd.getMethod("upgradeBlock", String.class);
    Method upgradeTag = bsd.getMethod("upgradeBlockStateTag", Dynamic.class);
    for (int id = -2; id <= 4098; id++) {
      out.println("BlockStateData.getTag\t" + id + "\t" + show(getTag.invoke(null, id)));
      out.println("BlockStateData.upgradeBlock(int)\t" + id + "\t" + show(upgradeId.invoke(null, id)));
    }
    out.println("BlockStateData.getTag\t100000\t" + show(getTag.invoke(null, 100000)));
    out.println("BlockStateData.upgradeBlock(int)\t100000\t" + show(upgradeId.invoke(null, 100000)));
    Map<?, ?> byName = (Map<?, ?>) staticField(bsd, "ID_BY_OLD_NAME");
    List<String> names = new ArrayList<>();
    for (Object k : byName.keySet()) names.add((String) k);
    Collections.sort(names);
    names.addAll(List.of("", "minecraft:nope", "stone", "minecraft:stone_slab2"));
    for (String n : names) out.println("BlockStateData.upgradeBlock(String)\t" + text(StringTag.valueOf(n)) + "\t" + show(upgradeName.invoke(null, n)));
    Map<?, ?> byOld = (Map<?, ?>) staticField(bsd, "ID_BY_OLD");
    List<String> olds = new ArrayList<>();
    for (Object k : byOld.keySet()) olds.add(text((Tag) ((Dynamic<?>) k).convert(NbtOps.INSTANCE).getValue()));
    Collections.sort(olds);
    List<String> cases = new ArrayList<>(olds);
    cases.addAll(List.of(
        "{}",
        "\"minecraft:stone\"",
        "1",
        "[]",
        "{Name:\"minecraft:stone\",Extra:1b}",
        "{Name:\"minecraft:stone\",Properties:{variant:1}}",
        "{Name:\"minecraft:stone\",Properties:{variant:\"stone\",other:\"x\"}}",
        "{Name:\"minecraft:stone\",Properties:{}}",
        "{Name:\"minecraft:stone\",Properties:{variant:\"nope\"}}",
        "{Name:\"minecraft:nope\"}",
        "{Name:\"minecraft:log\"}",
        "{Name:3}"));
    for (String p : cases) {
      Dynamic<Tag> d = new Dynamic<>(NbtOps.INSTANCE, parse(p));
      out.println("BlockStateData.upgradeBlockStateTag\t" + p + "\t" + show(upgradeTag.invoke(null, d)));
    }

    Class<?> flat = Class.forName(FIXES + "ItemStackTheFlatteningFix", true, cl);
    Method update = flat.getMethod("updateItem", String.class, int.class);
    Map<?, ?> flatMap = (Map<?, ?>) staticField(flat, "MAP");
    SortedSet<String> keys = new TreeSet<>();
    for (Object k : flatMap.keySet()) keys.add((String) k);
    SortedSet<String> flatNames = new TreeSet<>(List.of("minecraft:nope", "", "minecraft:stone."));
    for (String k : keys) flatNames.add(k.substring(0, k.indexOf('.')));
    for (String n : flatNames) {
      SortedSet<Integer> datas = new TreeSet<>(List.of(-1, 0, 1, 15, 16, 32767, 65535));
      for (String k : keys) if (k.startsWith(n + ".")) datas.add(Integer.parseInt(k.substring(n.length() + 1)));
      for (int d : datas) out.println("ItemStackTheFlatteningFix.updateItem\t" + n + "\t" + d + "\t" + show(update.invoke(null, n, d)));
    }

    Class<?> itemId = Class.forName(FIXES + "ItemIdFix", true, cl);
    Method getItem = itemId.getMethod("getItem", int.class);
    for (int id = -1; id <= 4100; id++) out.println("ItemIdFix.getItem\t" + id + "\t" + show(getItem.invoke(null, id)));

    Class<?> ebs = Class.forName(FIXES + "EntityBlockStateFix", true, cl);
    Method getBlockId = ebs.getMethod("getBlockId", String.class);
    SortedSet<String> blocks = new TreeSet<>(List.of("", "minecraft:nope", "stone"));
    for (Object k : ((Map<?, ?>) staticField(ebs, "MAP")).keySet()) blocks.add((String) k);
    for (String b : blocks) out.println("EntityBlockStateFix.getBlockId\t" + text(StringTag.valueOf(b)) + "\t" + getBlockId.invoke(null, b));
  }

  static Tag parse(String snbt) throws Exception {
    return TagParser.create(NbtOps.INSTANCE).parseFully(snbt);
  }
}
