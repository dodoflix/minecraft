// A small schema that uses every node kind a schema type can contain (`check` never survives Schema.getType), built with the real DSL. The Rust test
// `grammar_sample_matches_the_oracle` builds the same schema with the ported DSL and compares the normalised descriptions
// line by line: that proves the two normalisers write one grammar.
//
// Usage: scripts/datafix-oracle.sh grammar-sample
// Output: for every type of the schema in name order, `@@ NAME` and then its description.
import com.google.common.collect.Maps;
import com.mojang.datafixers.DSL;
import com.mojang.datafixers.schemas.Schema;
import com.mojang.datafixers.types.templates.Hook;
import com.mojang.datafixers.types.templates.TypeTemplate;
import java.util.Map;
import java.util.TreeSet;
import java.util.function.Supplier;
import net.minecraft.util.datafix.fixes.References;
import net.minecraft.util.datafix.schemas.NamespacedSchema;

final class GrammarSample extends Schema {
  GrammarSample() {
    super(990, null);
  }

  @Override
  public Map<String, Supplier<TypeTemplate>> registerEntities(Schema schema) {
    Map<String, Supplier<TypeTemplate>> map = Maps.newHashMap();
    schema.register(map, "Pig", name -> DSL.optionalFields("Saddle", DSL.constType(DSL.bool()), "Item", References.ITEM_STACK.in(schema)));
    schema.registerSimple(map, "Cow");
    return map;
  }

  @Override
  public Map<String, Supplier<TypeTemplate>> registerBlockEntities(Schema schema) {
    Map<String, Supplier<TypeTemplate>> map = Maps.newHashMap();
    schema.register(map, "Chest", name -> DSL.optionalFields("Items", DSL.list(References.ITEM_STACK.in(schema))));
    return map;
  }

  @Override
  public void registerTypes(Schema schema, Map<String, Supplier<TypeTemplate>> entityTypes, Map<String, Supplier<TypeTemplate>> blockEntityTypes) {
    schema.registerType(false, References.ITEM_NAME, () -> DSL.constType(NamespacedSchema.namespacedString()));
    schema.registerType(
        true,
        References.ITEM_STACK,
        () -> DSL.optionalFields(
            "id", References.ITEM_NAME.in(schema),
            "Count", DSL.constType(DSL.byteType()),
            "Damage", DSL.constType(DSL.shortType()),
            "Hooked", DSL.hook(DSL.optionalFields("A", DSL.constType(DSL.intType())), Hook.HookFunction.IDENTITY, Hook.HookFunction.IDENTITY)));
    schema.registerType(true, References.ENTITY, () -> DSL.taggedChoiceLazy("id", DSL.string(), entityTypes));
    schema.registerType(false, References.BLOCK_ENTITY, () -> DSL.taggedChoiceLazy("id", DSL.string(), blockEntityTypes));
    schema.registerType(
        true,
        References.ENTITY_TREE,
        () -> DSL.optionalFields("Passengers", DSL.list(References.ENTITY_TREE.in(schema)), References.ENTITY.in(schema)));
    schema.registerType(
        false,
        References.CHUNK,
        () -> DSL.optionalFields(
            "Level",
            DSL.optionalFields(
                "xPos", DSL.constType(DSL.longType()),
                "yPos", DSL.constType(DSL.floatType()),
                "zPos", DSL.constType(DSL.doubleType()),
                "Name", DSL.constType(DSL.string()),
                "Palette", DSL.compoundList(References.ITEM_STACK.in(schema))),
            "Extra",
            DSL.optionalFields("Flag", DSL.constType(DSL.bool()), "TileEntities", DSL.list(References.BLOCK_ENTITY.in(schema)))));
  }

  static String dump() {
    Schema schema = new GrammarSample();
    StringBuilder sb = new StringBuilder();
    for (String name : new TreeSet<>(schema.types())) {
      sb.append("@@ ").append(name).append('\n').append(SchemaWalk.describe(schema.getType(() -> name)));
    }
    return sb.toString();
  }
}
