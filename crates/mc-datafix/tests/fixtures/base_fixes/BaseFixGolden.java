import com.google.common.collect.Maps;
import com.mojang.datafixers.DSL;
import com.mojang.datafixers.DataFix;
import com.mojang.datafixers.DataFixer;
import com.mojang.datafixers.DataFixerBuilder;
import com.mojang.datafixers.Typed;
import com.mojang.datafixers.schemas.Schema;
import com.mojang.datafixers.types.templates.TypeTemplate;
import com.mojang.datafixers.util.Pair;
import com.mojang.serialization.Dynamic;
import java.nio.file.Files;
import java.nio.file.Path;
import java.lang.reflect.Method;
import java.util.Arrays;
import java.util.HashSet;
import java.util.Map;
import java.util.Set;
import java.util.function.Supplier;
import java.util.function.UnaryOperator;
import java.util.stream.Stream;
import net.minecraft.SharedConstants;
import net.minecraft.nbt.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.util.datafix.fixes.*;
import net.minecraft.util.datafix.schemas.NamespacedSchema;

/**
 * Golden generator for the abstract and family fix classes of util/datafix/fixes: each case builds a two-schema chain
 * (V100 -> V101, both NamespacedSchema, the structure of V101 changed by flags), registers one fix at V101, updates one
 * NBT value with the real DataFixerUpper and records the dumped result or the exception.
 * Line: fix TAB flags TAB type TAB input TAB result (SNBT, no tabs or newlines inside the fields).
 * Usage: crates/mc-datafix/tests/fixtures/base_fixes/gen.sh
 * Output: crates/mc-datafix/tests/fixtures/base_fixes.tsv
 */
public class BaseFixGolden {
   static final StringBuilder OUT = new StringBuilder();
   static final ThreadLocal<Set<String>> FLAGS = new ThreadLocal<>();

   static boolean flag(String f) {
      return FLAGS.get().contains(f);
   }

   // ---- dump (same format as LibraryGolden) ----
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

   // ---- the mini schemas ----
   static TypeTemplate nameType() {
      return DSL.constType(flag("badnames") ? DSL.string() : NamespacedSchema.namespacedString());
   }

   static class S100 extends NamespacedSchema {
      S100(int key, Schema parent) {
         super(key, parent);
      }

      @Override
      public Map<String, Supplier<TypeTemplate>> registerEntities(final Schema schema) {
         Map<String, Supplier<TypeTemplate>> map = Maps.newHashMap();
         schema.registerSimple(map, "minecraft:zombie");
         schema.registerSimple(map, "minecraft:pig");
         schema.registerSimple(map, "minecraft:cow");
         schema.register(map, "minecraft:item", () -> DSL.optionalFields("Item", References.ITEM_STACK.in(schema)));
         return map;
      }

      @Override
      public Map<String, Supplier<TypeTemplate>> registerBlockEntities(final Schema schema) {
         Map<String, Supplier<TypeTemplate>> map = Maps.newHashMap();
         schema.register(map, "minecraft:chest", () -> DSL.optionalFields("Items", DSL.list(References.ITEM_STACK.in(schema))));
         schema.registerSimple(map, "minecraft:sign");
         return map;
      }

      @Override
      public void registerTypes(final Schema schema, final Map<String, Supplier<TypeTemplate>> entityTypes, final Map<String, Supplier<TypeTemplate>> blockEntityTypes) {
         schema.registerType(false, References.ITEM_NAME, BaseFixGolden::nameType);
         schema.registerType(false, References.BLOCK_NAME, BaseFixGolden::nameType);
         schema.registerType(false, References.ENTITY_NAME, BaseFixGolden::nameType);
         schema.registerType(false, References.BIOME, BaseFixGolden::nameType);
         schema.registerType(
            true,
            References.ITEM_STACK,
            () -> DSL.optionalFields("id", References.ITEM_NAME.in(schema), "tag", DSL.optionalFields("Items", DSL.list(References.ITEM_STACK.in(schema))))
         );
         schema.registerType(
            true, References.ENTITY_TREE, () -> DSL.optionalFields("Passengers", DSL.list(References.ENTITY_TREE.in(schema)), References.ENTITY.in(schema))
         );
         schema.registerType(true, References.ENTITY, () -> DSL.taggedChoiceLazy("id", NamespacedSchema.namespacedString(), entityTypes));
         schema.registerType(false, References.BLOCK_ENTITY, () -> DSL.taggedChoiceLazy("id", NamespacedSchema.namespacedString(), blockEntityTypes));
         schema.registerType(false, References.BLOCK_STATE, DSL::remainder);
         schema.registerType(false, References.FLAT_BLOCK_STATE, DSL::remainder);
         schema.registerType(false, References.DATA_COMPONENTS, DSL::remainder);
         schema.registerType(false, References.PLAYER, () -> DSL.optionalFields("Inventory", DSL.list(References.ITEM_STACK.in(schema))));
         schema.registerType(false, References.ADVANCEMENTS, DSL::remainder);
         schema.registerType(false, References.POI_CHUNK, DSL::remainder);
      }
   }

   static class S101 extends NamespacedSchema {
      S101(int key, Schema parent) {
         super(key, parent);
      }

      @Override
      public Map<String, Supplier<TypeTemplate>> registerEntities(final Schema schema) {
         Map<String, Supplier<TypeTemplate>> map = super.registerEntities(schema);
         if (flag("newentities")) {
            schema.registerSimple(map, "minecraft:cattle");
            schema.registerSimple(map, "minecraft:sheep");
         }
         if (flag("dropcow")) {
            map.remove("minecraft:cow");
         }
         if (flag("pigsaddle")) {
            schema.register(map, "minecraft:pig", () -> DSL.optionalFields("Saddle", References.ITEM_STACK.in(schema)));
         }
         if (flag("itemfield")) {
            schema.register(map, "minecraft:item", () -> DSL.optionalFields("ItemStack", References.ITEM_STACK.in(schema)));
         }
         return map;
      }

      @Override
      public Map<String, Supplier<TypeTemplate>> registerBlockEntities(final Schema schema) {
         Map<String, Supplier<TypeTemplate>> map = super.registerBlockEntities(schema);
         if (flag("newbes")) {
            schema.registerSimple(map, "minecraft:hanging_sign");
         }
         if (flag("dropsign")) {
            map.remove("minecraft:sign");
         }
         return map;
      }

      @Override
      public void registerTypes(final Schema schema, final Map<String, Supplier<TypeTemplate>> entityTypes, final Map<String, Supplier<TypeTemplate>> blockEntityTypes) {
         super.registerTypes(schema, entityTypes, blockEntityTypes);
         if (flag("bekey")) {
            schema.registerType(false, References.BLOCK_ENTITY, () -> DSL.taggedChoiceLazy("id", DSL.string(), blockEntityTypes));
         }
         if (flag("badentityname")) {
            schema.registerType(false, References.ENTITY_NAME, () -> DSL.constType(DSL.string()));
         }
         if (flag("badpoi")) {
            schema.registerType(false, References.POI_CHUNK, () -> DSL.optionalFields("Sections", DSL.remainder()));
         }
      }
   }

   // ---- the fixes ----
   static Dynamic<?> mark(Dynamic<?> d) {
      return d.set("fixed", d.createByte((byte) 1));
   }

   static DataFix make(String fix, Schema v101) {
      switch (fix) {
         case "named_entity":
            return new NamedEntityFix(v101, false, "NamedZombie", References.ENTITY, "minecraft:zombie") {
               @Override
               protected Typed<?> fix(Typed<?> entity) {
                  return entity.update(DSL.remainderFinder(), BaseFixGolden::mark);
               }
            };
         case "named_entity_be":
            return new NamedEntityFix(v101, false, "NamedChest", References.BLOCK_ENTITY, "minecraft:chest") {
               @Override
               protected Typed<?> fix(Typed<?> entity) {
                  return entity.update(DSL.remainderFinder(), BaseFixGolden::mark);
               }
            };
         case "named_entity_missing":
            return new NamedEntityFix(v101, false, "NamedGhost", References.ENTITY, "minecraft:ghost") {
               @Override
               protected Typed<?> fix(Typed<?> entity) {
                  return entity;
               }
            };
         case "named_entity_write_read":
            return new NamedEntityWriteReadFix(v101, true, "WriteReadPig", References.ENTITY, "minecraft:pig") {
               @Override
               protected <T> Dynamic<T> fix(Dynamic<T> input) {
                  return input.set("wr", input.createInt(2));
               }
            };
         case "named_entity_convert":
            return new NamedEntityConvertUncheckedFix(v101, "ConvertItem", References.ENTITY, "minecraft:item");
         case "component_inc":
            return new DataComponentRemainderFix(v101, "Inc", "minecraft:foo") {
               @Override
               protected <T> Dynamic<T> fixComponent(Dynamic<T> input) {
                  return input.asNumber().result().map(n -> input.createInt(n.intValue() + 1)).orElse(null);
               }
            };
         case "component_rename":
            return new DataComponentRemainderFix(v101, "IncRename", "minecraft:foo", "minecraft:bar") {
               @Override
               protected <T> Dynamic<T> fixComponent(Dynamic<T> input) {
                  return input.asNumber().result().map(n -> input.createInt(n.intValue() + 1)).orElse(null);
               }
            };
         case "item_stack_tag":
            return new ItemStackTagFix(v101, "TagStone", id -> id.equals("minecraft:stone")) {
               @Override
               protected Typed<?> fixItemStackTag(Typed<?> tag) {
                  return tag.update(DSL.remainderFinder(), BaseFixGolden::mark);
               }
            };
         case "item_stack_tag_remainder":
            return new ItemStackTagRemainderFix(v101, "TagS", id -> id.startsWith("minecraft:s")) {
               @Override
               protected <T> Dynamic<T> fixItemStackTag(Dynamic<T> tag) {
                  return tag.set("rem", tag.createString("y"));
               }
            };
         case "uuid":
            return new AbstractUUIDFix(v101, References.ENTITY) {
               @Override
               protected com.mojang.datafixers.TypeRewriteRule makeRule() {
                  return this.fixTypeEverywhereTyped(
                     "UuidTest",
                     this.getInputSchema().getType(References.ENTITY),
                     typed -> this.updateNamedChoice(typed, "minecraft:zombie", d -> {
                        Dynamic<?> out = d;
                        out = replaceUUIDString(out, "S", "S2").orElse(out);
                        out = replaceUUIDMLTag(out, "ML", "ML2").orElse(out);
                        out = replaceUUIDLeastMost(out, "LM", "LM2").orElse(out);
                        return out;
                     })
                  );
               }
            };
         case "poi":
            return new AbstractPoiSectionFix(v101, "PoiFilter") {
               @Override
               protected <T> Stream<Dynamic<T>> processRecords(Stream<Dynamic<T>> records) {
                  return records.filter(r -> !r.get("type").asString("").equals("drop")).map(r -> r.set("seen", r.createByte((byte) 1)));
               }
            };
         case "entity_rename":
            return new EntityRenameFix("RenameCow", v101, true) {
               @Override
               protected Pair<String, Typed<?>> fix(String name, Typed<?> entity) {
                  return Pair.of(name.equals("minecraft:cow") ? "minecraft:cattle" : name, entity);
               }
            };
         case "entity_rename_pig":
            return new EntityRenameFix("RenameCowToPig", v101, true) {
               @Override
               protected Pair<String, Typed<?>> fix(String name, Typed<?> entity) {
                  return Pair.of(name.equals("minecraft:cow") ? "minecraft:pig" : name, entity);
               }
            };
         case "entity_rename_ghost":
            return new EntityRenameFix("RenameCowToGhost", v101, true) {
               @Override
               protected Pair<String, Typed<?>> fix(String name, Typed<?> entity) {
                  return Pair.of(name.equals("minecraft:cow") ? "minecraft:ghost" : name, entity);
               }
            };
         case "simple_entity_rename":
            return new SimpleEntityRenameFix("SimpleRename", v101, true) {
               @Override
               protected Pair<String, Dynamic<?>> getNewNameAndTag(String name, Dynamic<?> tag) {
                  return name.equals("minecraft:cow")
                     ? Pair.of("minecraft:cattle", tag.set("renamed", tag.createByte((byte) 1)))
                     : Pair.of(name, tag);
               }
            };
         case "simplest_entity_rename":
            return new SimplestEntityRenameFix("SimplestRename", v101, true) {
               @Override
               protected String rename(String name) {
                  return name.equals("minecraft:cow") ? "minecraft:cattle" : name;
               }
            };
         case "simplest_entity_rename_ghost":
            return new SimplestEntityRenameFix("SimplestRenameGhost", v101, true) {
               @Override
               protected String rename(String name) {
                  return name.equals("minecraft:cow") ? "minecraft:ghost" : name;
               }
            };
         case "attributes_rename":
            return new AttributesRenameFix(v101, "AttrRename", id -> id.equals("minecraft:generic.foo") ? "minecraft:foo" : id);
         case "attributes_rename_old":
            return new AttributesRenameFix(v101, "AttrRenameOld", id -> id.equals("minecraft:generic.foo") ? "minecraft:foo" : id, true);
         case "legacy_block_property":
            return new LegacyAbstractBlockPropertyFix(v101, "BlockProp") {
               @Override
               protected boolean shouldFix(String blockId) {
                  return blockId.equals("minecraft:stone");
               }

               @Override
               protected <T> Dynamic<T> fixProperties(String blockId, Dynamic<T> properties) {
                  return properties.set("fixed", properties.createString(blockId));
               }
            };
         case "item_rename":
            return ItemRenameFix.create(v101, "ItemRename", s -> s.equals("minecraft:old_item") ? "minecraft:new_item" : s);
         case "legacy_block_rename":
            return LegacyBlockRenameFix.create(v101, "BlockRename", s -> s.equals("minecraft:old") ? "minecraft:new" : s);
         case "namespaced_type_rename":
            return new NamespacedTypeRenameFix(v101, "BiomeRename", References.BIOME, s -> s.equals("minecraft:old_biome") ? "minecraft:new_biome" : s);
         case "block_entity_rename":
            return BlockEntityRenameFix.create(v101, "BlockEntityRename", s -> s.equals("minecraft:sign") ? "minecraft:hanging_sign" : s);
         case "advancements_rename":
            return new AdvancementsRenameFix(v101, false, "AdvRename", s -> s.equals("minecraft:a") ? "minecraft:b" : s);
         case "advancements_rename_changes":
            return new AdvancementsRenameFix(v101, true, "AdvRenameChanges", s -> s.equals("minecraft:a") ? "minecraft:b" : s);
         case "advancements_rename_all":
            return new AdvancementsRenameFix(v101, false, "AdvRenameAll", s -> "minecraft:z");
         case "write_and_read":
            return new WriteAndReadFix(v101, "WriteAndRead", References.ENTITY);
         case "add_new_choices_entity":
            return new AddNewChoices(v101, "AddChoices", References.ENTITY);
         case "add_new_choices_block_entity":
            return new AddNewChoices(v101, "AddChoices", References.BLOCK_ENTITY);
         case "add_field":
            return new AddFieldFix(v101, References.PLAYER, "added", d -> d.createInt(7));
         case "add_field_path":
            return new AddFieldFix(v101, References.PLAYER, "added", d -> d.createInt(7), "Inner", "Deep");
         case "add_flag":
            return new AddFlagIfNotPresentFix(v101, References.PLAYER, "flag", true);
         default:
            throw new IllegalArgumentException(fix);
      }
   }

   static void line(String fix, String flags, com.mojang.datafixers.DSL.TypeReference ref, String input) throws Exception {
      FLAGS.set(new HashSet<>(Arrays.asList(flags.split(","))));
      String result;
      try {
         DataFixerBuilder builder = new DataFixerBuilder(5023);
         builder.addSchema(100, S100::new);
         Schema v101 = builder.addSchema(101, S101::new);
         builder.addFixer(make(fix, v101));
         DataFixer fixer = builder.build().fixer();
         Dynamic<Tag> in = new Dynamic<>(NbtOps.INSTANCE, TagParser.create(NbtOps.INSTANCE).parseFully(input));
         result = dump((Tag) fixer.update(ref, in, 100, 101).getValue());
      } catch (Throwable t) {
         result = "EX:" + t.getClass().getSimpleName() + ":" + t.getMessage();
      }
      OUT.append(fix).append('\t').append(flags).append('\t').append(ref.typeName()).append('\t').append(input).append('\t').append(result).append('\n');
   }

   static void cases(String fix, String flags, com.mojang.datafixers.DSL.TypeReference ref, String... inputs) throws Exception {
      for (String input : inputs) {
         line(fix, flags, ref, input);
      }
   }

   // the private DataFixers.createRenamer* helpers; line: renamer_<kind> TAB - TAB - TAB id TAB result
   @SuppressWarnings("unchecked")
   static void renamers() throws Exception {
      SharedConstants.tryDetectVersion();
      Bootstrap.bootStrap();
      Class<?> fixers = Class.forName("net.minecraft.util.datafix.DataFixers");
      Method byMap = fixers.getDeclaredMethod("createRenamer", Map.class);
      Method noNamespace = fixers.getDeclaredMethod("createRenamerNoNamespace", Map.class);
      Method pair = fixers.getDeclaredMethod("createRenamer", String.class, String.class);
      byMap.setAccessible(true);
      noNamespace.setAccessible(true);
      pair.setAccessible(true);
      Map<String, String> map = Map.of("minecraft:a", "minecraft:x", "b", "y");
      UnaryOperator<String> byMapRenamer = (UnaryOperator<String>) byMap.invoke(null, map);
      UnaryOperator<String> noNamespaceRenamer = (UnaryOperator<String>) noNamespace.invoke(null, map);
      UnaryOperator<String> pairRenamer = (UnaryOperator<String>) pair.invoke(null, "minecraft:a", "minecraft:x");
      for (String id : new String[] {"minecraft:a", "a", "b", "minecraft:b", "Bad Id", "", "minecraft:a ", "MINECRAFT:a", "x:a"}) {
         OUT.append("renamer_map\t-\t-\t").append(id).append('\t').append(byMapRenamer.apply(id)).append('\n');
         OUT.append("renamer_no_namespace\t-\t-\t").append(id).append('\t').append(noNamespaceRenamer.apply(id)).append('\n');
         OUT.append("renamer_pair\t-\t-\t").append(id).append('\t').append(pairRenamer.apply(id)).append('\n');
      }
   }

   public static void main(String[] args) throws Exception {
      String none = "-";
      cases("named_entity", none, References.ENTITY, "{id:'minecraft:zombie',Health:20}", "{id:'zombie',Health:20}", "{id:'minecraft:pig',Health:10}", "{Health:1}", "{id:'minecraft:cow'}", "{id:'minecraft:ghost'}");
      cases("named_entity", none, References.ENTITY_TREE, "{id:'minecraft:zombie',Passengers:[{id:'minecraft:zombie'},{id:'minecraft:pig',Passengers:[{id:'minecraft:zombie',Health:2}]}]}");
      cases("named_entity_be", none, References.BLOCK_ENTITY, "{id:'minecraft:chest',Items:[{id:'stone',Count:1b}]}", "{id:'minecraft:sign'}");
      cases("named_entity_missing", none, References.ENTITY, "{id:'minecraft:zombie'}");
      cases("named_entity_write_read", "pigsaddle", References.ENTITY, "{id:'minecraft:pig',Health:3}", "{id:'minecraft:pig',Health:3,Saddle:{id:'stone',Count:1b}}", "{id:'minecraft:zombie'}");
      cases("named_entity_convert", "itemfield", References.ENTITY, "{id:'minecraft:item',Item:{id:'stone',Count:1b}}", "{id:'minecraft:item'}", "{id:'minecraft:zombie'}");
      for (String fix : new String[] {"component_inc", "component_rename"}) {
         cases(fix, none, References.DATA_COMPONENTS, "{'minecraft:foo':1}", "{'minecraft:foo':'x'}", "{}", "{'minecraft:other':1}", "{'minecraft:foo':1,'minecraft:bar':9,z:1}");
      }
      cases("item_stack_tag", none, References.ITEM_STACK, "{id:'stone',Count:1b,tag:{a:1}}", "{id:'minecraft:dirt',tag:{a:1}}", "{id:'stone'}", "{id:'stone',tag:{Items:[{id:'stone',Count:1b,tag:{}},{id:'dirt',tag:{b:2}}]}}", "{Count:1b}");
      cases("item_stack_tag_remainder", none, References.ITEM_STACK, "{id:'sand',tag:{}}", "{id:'dirt',tag:{}}", "{id:'minecraft:stone',tag:{k:1}}");
      cases("uuid", none, References.ENTITY,
         "{id:'minecraft:zombie',S:'123e4567-e89b-12d3-a456-426614174000',ML:{M:1L,L:2L},LMMost:3L,LMLeast:4L}",
         "{id:'minecraft:zombie',S:'abc'}", "{id:'minecraft:zombie',S:'1-2-3-4-5'}", "{id:'minecraft:zombie',S:5}",
         "{id:'minecraft:zombie',S:'123e4567-e89b-12d3-a456-4266141740001'}", "{id:'minecraft:zombie',S:'ABCDEF01-e89b-12d3-a456-426614174000'}",
         "{id:'minecraft:zombie',ML:{M:0L,L:2L}}", "{id:'minecraft:zombie',ML:{M:-1L,L:-2L}}", "{id:'minecraft:zombie',LMMost:3L}", "{id:'minecraft:zombie',LMMost:3L,LMLeast:0L}",
         "{id:'minecraft:zombie',LMMost:-9223372036854775808L,LMLeast:9223372036854775807L}", "{id:'minecraft:zombie',ML:5}", "{id:'minecraft:zombie'}", "{id:'minecraft:pig',S:'123e4567-e89b-12d3-a456-426614174000'}",
         "{id:'minecraft:zombie',S:'-1-2-3-4'}", "{id:'minecraft:zombie',S:'+1-+2-+3-+4-+5'}", "{id:'minecraft:zombie',S:'ffffffffffffffffff-1-2-3-4'}");
      cases("poi", none, References.POI_CHUNK,
         "{Sections:{'0':{Records:[{type:'a'},{type:'drop'},{type:'b'}],Valid:1b},'1':{Records:5},'2':{},'10':{Records:[]}},DataVersion:7}", "{}", "{Sections:5}", "{Sections:{'0':5}}");
      cases("poi", "badpoi", References.POI_CHUNK, "{Sections:{}}");
      cases("entity_rename", "newentities", References.ENTITY, "{id:'minecraft:cow',Health:4}", "{id:'minecraft:zombie'}", "{id:'minecraft:item',Item:{id:'stone',Count:1b}}", "{id:'minecraft:pig'}");
      cases("entity_rename", "newentities", References.ENTITY_TREE, "{id:'minecraft:cow',Passengers:[{id:'minecraft:cow'},{id:'minecraft:pig'}]}");
      cases("entity_rename_pig", "newentities,pigsaddle", References.ENTITY, "{id:'minecraft:cow'}", "{id:'minecraft:zombie'}");
      cases("entity_rename_ghost", "newentities", References.ENTITY, "{id:'minecraft:cow'}", "{id:'minecraft:zombie'}");
      cases("simple_entity_rename", "newentities", References.ENTITY, "{id:'minecraft:cow',Health:4}", "{id:'minecraft:zombie',Health:4}", "{id:'minecraft:cow'}");
      cases("simplest_entity_rename", "newentities", References.ENTITY, "{id:'minecraft:cow',Health:4}", "{id:'minecraft:zombie'}", "{id:'minecraft:pig'}");
      cases("simplest_entity_rename", "newentities", References.ENTITY_NAME, "'cow'", "'minecraft:cow'", "'minecraft:pig'", "'Bad Id'");
      cases("simplest_entity_rename", "newentities,pigsaddle", References.ENTITY, "{id:'minecraft:pig'}", "{id:'minecraft:zombie'}");
      cases("simplest_entity_rename", "newentities,badentityname", References.ENTITY, "{id:'minecraft:cow'}");
      cases("simplest_entity_rename_ghost", "newentities", References.ENTITY, "{id:'minecraft:cow'}", "{id:'minecraft:zombie'}");
      cases("attributes_rename", none, References.DATA_COMPONENTS,
         "{'minecraft:attribute_modifiers':[{type:'minecraft:generic.foo',amount:1.0d,id:'x'},{type:'minecraft:other'},{type:5},7]}", "{'minecraft:attribute_modifiers':5}", "{'minecraft:attribute_modifiers':{modifiers:[{type:'minecraft:generic.foo'}]}}", "{}");
      cases("attributes_rename", none, References.ENTITY, "{id:'minecraft:zombie',attributes:[{id:'minecraft:generic.foo',base:1.0d},{id:'minecraft:x'},{id:3}]}", "{id:'minecraft:zombie',attributes:5}", "{id:'minecraft:zombie'}");
      cases("attributes_rename", none, References.PLAYER, "{Inventory:[],attributes:[{id:'minecraft:generic.foo'}]}", "{attributes:[]}");
      cases("attributes_rename_old", none, References.DATA_COMPONENTS,
         "{'minecraft:attribute_modifiers':{modifiers:[{type:'minecraft:generic.foo'},{type:'q'}],show_in_tooltip:1b}}", "{'minecraft:attribute_modifiers':[{type:'minecraft:generic.foo'}]}", "{'minecraft:attribute_modifiers':{modifiers:5}}");
      cases("legacy_block_property", none, References.BLOCK_STATE, "{Name:'stone',Properties:{a:'b'}}", "{Name:'minecraft:dirt',Properties:{}}", "{Name:'stone'}", "{Name:5}", "{}", "{Name:'minecraft:stone',Properties:5}");
      cases("item_rename", none, References.ITEM_NAME, "'old_item'", "'minecraft:other'", "'Bad Id'", "'minecraft:old_item'");
      cases("item_rename", none, References.ITEM_STACK, "{id:'old_item',Count:1b,tag:{Items:[{id:'old_item',Count:2b}]}}", "{Count:1b}");
      cases("item_rename", "badnames", References.ITEM_NAME, "'old_item'");
      cases("legacy_block_rename", none, References.BLOCK_NAME, "'old'", "'minecraft:other'", "'Bad Id'");
      cases("legacy_block_rename", none, References.BLOCK_STATE, "{Name:'old',Properties:{a:'b'}}", "{Name:5}", "{}", "{Name:'minecraft:old'}");
      cases("legacy_block_rename", none, References.FLAT_BLOCK_STATE,
         "'minecraft:old[a=b]'", "'old{x:1}'", "'minecraft:old{x:1}[a=b]'", "'[x]'", "'{x}'", "'minecraft:old'", "5", "'minecraft:old[a=b]{x:1}'", "''", "'old'", "'a[b[c'");
      cases("legacy_block_rename", "badnames", References.BLOCK_NAME, "'old'");
      cases("namespaced_type_rename", none, References.BIOME, "'old_biome'", "'minecraft:other'", "'Bad Id'");
      cases("namespaced_type_rename", "badnames", References.BIOME, "'old_biome'");
      cases("block_entity_rename", "newbes,dropsign", References.BLOCK_ENTITY, "{id:'minecraft:sign',Text1:'x'}", "{id:'minecraft:chest',Items:[]}", "{id:'sign'}");
      for (String fix : new String[] {"advancements_rename", "advancements_rename_changes"}) {
         cases(fix, none, References.ADVANCEMENTS, "{'minecraft:a':{done:1b},'minecraft:c':2,DataVersion:7}", "{}", "{'a':1}", "5");
      }
      cases("advancements_rename_all", none, References.ADVANCEMENTS, "{'minecraft:a':{done:1b},'minecraft:c':2}", "{'minecraft:z':1}");
      cases("write_and_read", "pigsaddle", References.ENTITY, "{id:'minecraft:pig',Health:3}", "{id:'minecraft:pig',Saddle:{id:'stone',Count:1b}}", "{id:'minecraft:zombie'}", "{id:'minecraft:pig',Saddle:5}");
      cases("add_new_choices_entity", "newentities", References.ENTITY, "{id:'minecraft:cow'}", "{id:'minecraft:zombie',Health:1}");
      cases("add_new_choices_entity", "dropcow", References.ENTITY, "{id:'minecraft:cow'}", "{id:'minecraft:zombie'}");
      cases("add_new_choices_block_entity", "newbes", References.BLOCK_ENTITY, "{id:'minecraft:sign'}");
      cases("add_new_choices_block_entity", "bekey", References.BLOCK_ENTITY, "{id:'minecraft:sign'}");
      cases("add_field", none, References.PLAYER, "{Inventory:[],x:1}", "{}", "{added:1}");
      cases("add_field_path", none, References.PLAYER, "{Inner:{Deep:{a:1}}}", "{Inner:{}}", "{}", "{Inner:5}", "{Inner:{Deep:5}}");
      cases("add_flag", none, References.PLAYER, "{}", "{flag:0b}", "{flag:5}", "{Inventory:[{id:'stone'}]}");
      renamers();
      Files.writeString(Path.of(args[0]), OUT.toString());
   }
}
