import com.mojang.brigadier.arguments.ArgumentType;
import java.util.Optional;
import net.minecraft.commands.Commands;
import net.minecraft.commands.arguments.*;
import net.minecraft.commands.arguments.blocks.*;
import net.minecraft.commands.arguments.coordinates.*;
import net.minecraft.commands.arguments.item.ItemArgument;
import net.minecraft.server.permissions.PermissionSet;
import net.minecraft.util.StringRepresentable;
import net.minecraft.world.phys.Vec2;
import net.minecraft.world.phys.Vec3;

/** The argument types under test with their seeds and value renderings (see ArgsGolden). */
final class ArgsSpecs {
   /** The source every value rendering that needs one uses (mirrored by args_common::render_source). */
   static net.minecraft.commands.CommandSourceStack renderSource() {
      return Commands.createCompilationContext(PermissionSet.ALL_PERMISSIONS).withPosition(new Vec3(1.5, 64.25, -2.75)).withRotation(new Vec2(12.5f, 40.0f));
   }

   static String opt(Optional<?> o) {
      return o.map(String::valueOf).orElse("-");
   }

   static void add(String name, java.util.function.Supplier<ArgumentType<?>> type, java.util.function.Function<Object, String> render, String... seeds) {
      ArgsGolden.add(new ArgsGolden.Spec(name, type, seeds).render(render));
   }

   static String ser(Object o) {
      return ((StringRepresentable) o).getSerializedName();
   }


   static String coords(Object v) {
      Coordinates c = (Coordinates) v;
      var s = renderSource();
      Vec3 p = c.getPosition(s);
      Vec2 r = c.getRotation(s);
      var b = c.getBlockPos(s);
      return (c.isXRelative() ? "~" : "a") + (c.isYRelative() ? "~" : "a") + (c.isZRelative() ? "~" : "a") + " pos=" + p.x + "," + p.y + "," + p.z + " rot=" + r.x + "," + r.y + " block=" + b.getX() + "," + b.getY() + "," + b.getZ();
   }

   static String bounds(Object o) {
      var b = (net.minecraft.advancements.predicates.MinMaxBounds<?>) o;
      return opt(b.min()) + ".." + opt(b.max());
   }

   static final String[] POS3 = {"1 2 3", "~ ~ ~", "~1 ~-2 ~3", "^ ^ ^", "^1 ^2 ^3", "^1 ~ ^", "1 ~ ^", "^ 2 3", "~1.5 ~ ~", ".5 .5 .5", "1e3 2 3", "NaN 0 0", "1 2", "1", "~ ~", "0.1 -0.5 .9",
      "1 2 3 4", "-0 -0 -0", "1.0000000001 2 3", "999999999999 0 0", "1e400 0 0", "~-1 ~ ~", "~--1 ~ ~", "^-.5 ^ ^", " ", "x y z", "~x ~ ~", "0 0 0", "1  2 3", "1 2 3 ", "~ ~ ^", "^ ~ ~", "-5 -64 5", "3.5 70 -3.5",
      "30000000 0 30000000", "-30000001 0 0", "1.5 2.5 3.5", "~0.5 ~1 ~-5", "^1 ^ ^-5", "~\u00e9 ~ ~", "+1 +2 +3", "1d 2 3", "1f 2 3", "0x10 2 3"};
   static final String[] POS2 = {"1 2", "~ ~", "~1 ~-2", "^ ^", "^1 ^2", "^1 ~", "1 ^", "0.1 -0.5", ".5 .5", "1", "~", "1 2 3", "NaN 1", "1e400 1", " ", "x y", "-0 0", "30000000 30000000", "1.5 2.5", "~0.5 ~-5", "1  2", "^ ^ ^", "~ ~ ~"};

   static void coordSpec(String name, java.util.function.Supplier<ArgumentType<?>> t, String... seeds) {
      add(name, t, ArgsSpecs::coords, seeds);
   }

   static void registerCoordinates() {
      coordSpec("vec3", () -> Vec3Argument.vec3(), POS3);
      coordSpec("vec3_uncentered", () -> Vec3Argument.vec3(false), POS3);
      coordSpec("vec2", () -> Vec2Argument.vec2(), POS2);
      coordSpec("vec2_uncentered", () -> Vec2Argument.vec2(false), POS2);
      coordSpec("block_pos", () -> BlockPosArgument.blockPos(), POS3);
      coordSpec("column_pos", () -> ColumnPosArgument.columnPos(), POS2);
      coordSpec("rotation", () -> RotationArgument.rotation(), POS2);
      add("swizzle", () -> SwizzleArgument.swizzle(), v -> v.toString(), "xyz", "x", "y", "z", "xz", "zx", "yx", "xx", "xyzx", "", "w", "X", "x y", "xy ", "\u00e9", "xyz!");
   }

   static void registerScoreboard() {
      add("operation", () -> OperationArgument.operation(), v -> {
         var op = (OperationArgument.Operation) v;
         StringBuilder out = new StringBuilder();
         for (int[] ab : new int[][] {{17, 5}, {-7, 3}, {5, 0}, {Integer.MAX_VALUE, 1}, {Integer.MIN_VALUE, -1}, {0, 0}}) {
            int[] cell = {ab[0], ab[1]};
            var a = scoreAccess(cell, 0);
            var b = scoreAccess(cell, 1);
            try {
               op.apply(a, b);
               out.append(cell[0]).append(',').append(cell[1]);
            } catch (com.mojang.brigadier.exceptions.CommandSyntaxException e) {
               out.append(e.getMessage());
            }
            out.append(';');
         }
         return out.toString();
      }, "=", "+=", "-=", "*=", "/=", "%=", "<", ">", "><", "==", "+", "-", "*", "/", "%", "<=", ">=", ">>", "", "a", "=>", "\u00e9", "+= ", "= =");
      add("int_range", () -> RangeArgument.intRange(), ArgsSpecs::bounds,
         "0..5", "0", "-5", "-100..", "..100", "..", "1..1", "5..1", "1...2", "1.5", "1.5..2", "..-1", "-..", "a", "1..a", "2147483647", "2147483648", "-2147483648..2147483647", "+1", "1e2", "--1", ".", "...", "1 ", "0..0");
      add("float_range", () -> RangeArgument.floatRange(), v -> {
         var d = (net.minecraft.advancements.predicates.MinMaxBounds.Doubles) v;
         return bounds(v) + " sq=" + opt(d.boundsSqr().min()) + ".." + opt(d.boundsSqr().max());
      }, "0..5.2", "0", "-5.4", "-100.76..", "..100", "..", "1.5..1.5", "5..1", "1.5", ".5", "5.", "..-1.5", "1e2", "1e400", "NaN", "a", "1..a", "0.1..0.2", "-0..0", "1.0E10", "1 ");
      add("objective_criteria", () -> ObjectiveCriteriaArgument.criteria(), v -> {
         var c = (net.minecraft.world.scores.criteria.ObjectiveCriteria) v;
         return c.getName() + "|" + c.isReadOnly() + "|" + c.getDefaultRenderType().getId();
      }, "dummy", "trigger", "deathCount", "playerKillCount", "totalKillCount", "health", "food", "air", "armor", "xp", "level", "teamkill.red", "killedByTeam.dark_blue", "teamkill.", "teamkill.pink",
         "minecraft.mined:minecraft.stone", "minecraft.crafted:minecraft.stick", "minecraft.custom:minecraft.jump", "minecraft.killed:minecraft.zombie", "minecraft.killed_by:minecraft.creeper", "minecraft.used:minecraft.bow",
         "minecraft.broken:minecraft.diamond_pickaxe", "minecraft.picked_up:minecraft.diamond", "minecraft.dropped:minecraft.dirt", "minecraft.mined:minecraft.nope", "minecraft.mined:stone", "minecraft:mined:minecraft:stone", "Dummy", "foo", "foo bar", "", "minecraft.", "minecraft.custom:", "minecraft.mined:minecraft.stone ", "minecraft.mined:minecraft.");
      add("scoreboard_slot", () -> ScoreboardSlotArgument.displaySlot(), ArgsSpecs::ser,
         "list", "sidebar", "below_name", "sidebar.team.red", "sidebar.team.dark_blue", "sidebar.team.", "sidebar.team.pink", "team.red", "foo", "Sidebar", "", "side", "belowName", "sidebar.team.white", "sidebar.team.reset", "sidebar ");
      add("team", () -> TeamArgument.team(), Object::toString, "red", "blue", "Team Name!", "foo", "", "r", "\u00e9quipe", "a b", "\"red\"", "red ", "*", "@a", "#x", "\ud83d\ude00");
      add("objective", () -> ObjectiveArgument.objective(), Object::toString, "kills", "deaths", "Obj 1", "foo", "", "k", "\u00e9", "health_bar", "kills ", "*", "@a", "\ud83d\ude00", "A.B-c_d+e");
   }

   static net.minecraft.world.scores.ScoreAccess scoreAccess(int[] cell, int i) {
      return (net.minecraft.world.scores.ScoreAccess) java.lang.reflect.Proxy.newProxyInstance(ArgsSpecs.class.getClassLoader(), new Class<?>[] {net.minecraft.world.scores.ScoreAccess.class}, (p, m, a) -> {
         if (m.getName().equals("get")) return cell[i];
         if (m.getName().equals("set")) { cell[i] = (Integer) a[0]; return null; }
         throw new UnsupportedOperationException(m.getName());
      });
   }


   // ------------------------------------------------------------------------------------------------ registry arguments

   static String refId(Object o) {
      return ((net.minecraft.core.Holder.Reference<?>) o).key().identifier().toString();
   }

   static String hash(java.util.List<String> ids) {
      return ids.size() + ":" + String.join(",", ids.subList(0, Math.min(5, ids.size()))) + ":" + String.join("\t", ids).hashCode();
   }

   @SuppressWarnings({"unchecked", "rawtypes"})
   static void registerRegistries() {
      String[] ITEM = {"minecraft:stone", "stone", "minecraft:Stone", "minecraft:", ":stone", "minecraft:diamond_sword", "diamond", "minecraft:nope", "foo:bar", "minecraft:stone ", "#minecraft:logs", "minecraft:stick", "minecraft:oak_log",
         "a b", "m\u00e9", "minecraft:\u00e9", "minecraft:s", "sto", "minecraft:dia", "_", "minecraft:white_wool", "wool", "minecraft:/", "minecraft:a/b", "minecraft:air", "#", "#minecraft:", "#minecraft:nope", "#foo", "#minecraft:planks", "#minecraft:Logs"};
      String[] BLOCK = {"minecraft:stone", "oak_planks", "minecraft:grass_block", "minecraft:water", "minecraft:fire", "#minecraft:logs", "#minecraft:mineable/pickaxe", "#minecraft:leaves", "#minecraft:nope", "minecraft:nope", "minecraft:", "", "#", "minecraft:air", "lava", "foo:stone", "minecraft:oak_s", "#minecraft:mineable/", "minecraft:stone[", "#minecraft:logs "};
      String[] BIOME = {"minecraft:plains", "plains", "minecraft:the_void", "minecraft:desert", "#minecraft:is_forest", "#minecraft:is_overworld", "#minecraft:is_nether", "#minecraft:nope", "minecraft:nope", "minecraft:", "#minecraft:", "", "#", "minecraft:old_growth_pine_taiga", "badlands", "minecraft:cherry_grove", "forest", "minecraft:Plains", "minecraft:plains ", "#minecraft:has_structure/village_plains"};
      String[] OTHER = {"minecraft:zombie", "zombie", "minecraft:player", "minecraft:nope", "minecraft:", "", "#minecraft:skeletons", "#minecraft:nope", "minecraft:speed", "minecraft:sharpness", "minecraft:fortune", "minecraft:village_plains", "minecraft:mineshaft", "minecraft:ancient_city", "foo:bar", "ZOMBIE", "minecraft:armor", "minecraft:max_health", "minecraft:generic.max_health"};
      var reg = new java.util.LinkedHashMap<String, net.minecraft.resources.ResourceKey>();
      reg.put("item", net.minecraft.core.registries.Registries.ITEM);
      reg.put("block", net.minecraft.core.registries.Registries.BLOCK);
      reg.put("entity_type", net.minecraft.core.registries.Registries.ENTITY_TYPE);
      reg.put("mob_effect", net.minecraft.core.registries.Registries.MOB_EFFECT);
      reg.put("biome", net.minecraft.core.registries.Registries.BIOME);
      reg.put("enchantment", net.minecraft.core.registries.Registries.ENCHANTMENT);
      reg.put("structure", net.minecraft.core.registries.Registries.STRUCTURE);
      reg.put("attribute", net.minecraft.core.registries.Registries.ATTRIBUTE);
      java.util.function.Function<String, String[]> seedsOf = r -> switch (r) {
         case "item" -> ITEM; case "block" -> BLOCK; case "biome" -> BIOME; default -> OTHER; };
      for (var e : reg.entrySet()) {
         String r = e.getKey();
         var key = e.getValue();
         add("resource_" + r, () -> ResourceArgument.resource(ArgsGolden.ctx, key), ArgsSpecs::refId, seedsOf.apply(r));
         add("resource_key_" + r, () -> ResourceKeyArgument.key(key), v -> {
            var k = (net.minecraft.resources.ResourceKey<?>) v;
            return k.registry() + "|" + k.identifier();
         }, seedsOf.apply(r));
         add("resource_or_tag_" + r, () -> ResourceOrTagArgument.resourceOrTag(ArgsGolden.ctx, key), v -> {
            var res = (ResourceOrTagArgument.Result<?>) v;
            var u = res.unwrap();
            return res.asPrintable() + "|" + u.map(h -> "resource " + h.key().registry(), t -> "tag " + t.key().registry() + " n=" + t.size());
         }, seedsOf.apply(r));
         add("resource_or_tag_key_" + r, () -> ResourceOrTagKeyArgument.resourceOrTagKey(key), v -> {
            var res = (ResourceOrTagKeyArgument.Result<?>) v;
            return res.asPrintable() + "|" + res.unwrap().map(k -> "resource " + k.registry(), t -> "tag " + t.registry());
         }, seedsOf.apply(r));
         add("resource_selector_" + r, () -> ResourceSelectorArgument.resourceSelector(ArgsGolden.ctx, key), v -> {
            var l = ((java.util.Collection<net.minecraft.core.Holder.Reference<?>>) v).stream().map(h -> h.key().identifier().toString()).toList();
            return hash(l);
         }, "*", "minecraft:*", "*:asset", "?", "stone", "minecraft:s*", "*_log", "*stone*", "minecraft:oak_*", "z*", "**", "*?", "minecraft:", ":*", "foo:*", "*:*", "minecraft:zombie*", "plains", "minecraft:?????", "*e", "a*b*c", "Stone", "*\u00e9", "# ", "*/*", "minecraft:*/*", "m*", "mine*", "*effect", "*sharp*", "*dirt*");
      }
   }


   // ------------------------------------------------------------------------------------------------ entity selectors

   static Object field(Object o, String name) {
      try {
         var f = o.getClass().getDeclaredField(name);
         f.setAccessible(true);
         return f.get(o);
      } catch (ReflectiveOperationException e) {
         throw new RuntimeException(e);
      }
   }

   /** Rendering mirrored by `EntitySelector::describe` in Rust. */
   static String selector(Object v) {
      var sel = (net.minecraft.commands.arguments.selector.EntitySelector) v;
      Object order = field(sel, "order");
      String orderName = order == net.minecraft.commands.arguments.selector.EntitySelector.ORDER_ARBITRARY ? "arbitrary"
         : order == net.minecraft.commands.arguments.selector.EntitySelectorParser.ORDER_NEAREST ? "nearest"
         : order == net.minecraft.commands.arguments.selector.EntitySelectorParser.ORDER_FURTHEST ? "furthest"
         : order == net.minecraft.commands.arguments.selector.EntitySelectorParser.ORDER_RANDOM ? "random" : "unknown";
      @SuppressWarnings("unchecked")
      var position = (java.util.function.Function<Vec3, Vec3>) field(sel, "position");
      Vec3 moved = position.apply(new Vec3(10.5, 20.25, 30.125));
      var range = (net.minecraft.advancements.predicates.MinMaxBounds.Doubles) field(sel, "range");
      var aabb = (net.minecraft.world.phys.AABB) field(sel, "aabb");
      Object type = field(sel, "type");
      String typeName = type instanceof net.minecraft.world.entity.EntityType<?> t ? net.minecraft.world.entity.EntityType.getKey(t).toString() : "any";
      return "max=" + sel.getMaxResults() + " ent=" + sel.includesEntities() + " wl=" + sel.isWorldLimited() + " us=" + sel.usesSelector() + " cur=" + sel.isSelfSelector()
         + " name=" + field(sel, "playerName") + " uuid=" + field(sel, "entityUUID") + " type=" + typeName
         + " range=" + (range == null ? "-" : bounds(range) + " sq=" + opt(range.boundsSqr().min()) + ".." + opt(range.boundsSqr().max()))
         + " pos=" + moved.x + "," + moved.y + "," + moved.z
         + " aabb=" + (aabb == null ? "-" : aabb.minX + "," + aabb.minY + "," + aabb.minZ + "," + aabb.maxX + "," + aabb.maxY + "," + aabb.maxZ)
         + " order=" + orderName + " preds=" + ((java.util.List<?>) field(sel, "contextFreePredicates")).size();
   }

   static final String[] SELECTORS = {
      "@e", "@a", "@p", "@r", "@s", "@n", "@x", "@", "@E", "@e ", "@e[", "@e[]", "@e[ ]", "@e[,]", "@e[type=zombie]", "@e[type=minecraft:zombie]", "@e[type=!zombie]", "@e[type=#minecraft:skeletons]", "@e[type=!#minecraft:skeletons]",
      "@e[type=!#minecraft:skeletons,type=!zombie]", "@e[type=zombie,type=skeleton]", "@e[type=!zombie,type=zombie]", "@e[type=player]", "@e[type=!player]", "@e[type=foo]", "@e[type=#foo]", "@e[type=#]", "@e[type=]", "@e[type=minecraft:]", "@a[type=zombie]", "@a[type=player]", "@p[type=player]", "@r[type=zombie]",
      "@s[type=zombie]", "@e[type=item]", "@e[type=!item]", "@e[type=marker]", "@e[type=ender_dragon]", "@e[type=Zombie]", "@e[type=zombie,]", "@e[type=zombie", "@e[type=zombie,type", "@e[type", "@e[=", "@e[=zombie]", "@e[type=zombie]]", "@e[type=zombie]x", "@e[type=zombie] ",
      "@e[name=Foo]", "@e[name=!Foo]", "@e[name=\"Foo Bar\"]", "@e[name='Foo Bar']", "@e[name=Foo,name=Bar]", "@e[name=Foo,name=!Foo]", "@e[name=!Foo,name=!Bar]", "@e[name=!Foo,name=Foo]", "@e[name=]", "@e[name=!]", "@e[name=\"\"]", "@e[name=\u00e9]", "@e[name=\ud83d\ude00]",
      "@e[distance=..5]", "@e[distance=5]", "@e[distance=1..2]", "@e[distance=-1]", "@e[distance=-1..]", "@e[distance=..-1]", "@e[distance=1..2,distance=3]", "@e[distance=5..1]", "@e[distance=a]", "@e[distance=]", "@e[distance=1.5..2.5]", "@e[distance=1e3]", "@e[distance=..]", "@e[distance=0]", "@e[distance=0..0]",
      "@e[level=5]", "@e[level=..5]", "@e[level=1..]", "@e[level=-1]", "@e[level=1.5]", "@e[level=1,level=2]", "@a[level=3]", "@e[level=]", "@e[level=a]", "@e[level=5..1]", "@e[level=2147483647]", "@e[level=2147483648]",
      "@e[x=1,y=2,z=3]", "@e[x=1]", "@e[y=2]", "@e[z=3]", "@e[x=~1]", "@e[x=1.5,x=2]", "@e[x=a]", "@e[x=]", "@e[x=1e3]", "@e[x=-0]", "@e[x=NaN]", "@e[y=1e400]", "@e[x=1,y=2,z=3,distance=..5]",
      "@e[dx=1,dy=2,dz=3]", "@e[dx=1]", "@e[dy=1]", "@e[dz=1]", "@e[dx=-1.5,dy=0,dz=0]", "@e[dx=0]", "@e[dx=0,dy=0,dz=0]", "@e[dx=1,dx=2]", "@e[dx=a]", "@e[dx=]", "@e[x=1,y=2,z=3,dx=4,dy=5,dz=6]", "@e[dx=-0.0]", "@e[dx=1e400]",
      "@e[x_rotation=..90]", "@e[x_rotation=-45..45]", "@e[x_rotation=5]", "@e[y_rotation=5]", "@e[y_rotation=-180..180]", "@e[y_rotation=1..2,y_rotation=3]", "@e[x_rotation=a]", "@e[x_rotation=]", "@e[x_rotation=5..1]", "@e[y_rotation=370]", "@e[x_rotation=..]",
      "@e[scores={a=1}]", "@e[scores={a=1..2,b=..3}]", "@e[scores={}]", "@e[scores={a=}]", "@e[scores={a}]", "@e[scores={a=1}", "@e[scores={a=1,}]", "@e[scores={a=1,b=2}]", "@e[scores={a=1,a=2}]", "@e[scores=a]", "@e[scores=]", "@e[scores={=1}]", "@e[scores={a=1},scores={b=2}]",
      "@e[scores={a=-1}]", "@e[scores={a=1.5}]", "@e[scores={a=..}]", "@e[scores={a=5..1}]", "@e[scores={a=1 }]", "@e[scores={ a=1 }]", "@e[scores={\u00e9=1}]", "@e[scores={Obj 1=1}]", "@e[scores={\"a b\"=1}]",
      "@e[tag=foo]", "@e[tag=!foo]", "@e[tag=]", "@e[tag=!]", "@e[tag=foo,tag=bar]", "@e[tag=foo,tag=!bar]", "@e[tag=!foo,tag=foo]", "@e[tag=\"a b\"]", "@e[tag=\u00e9]", "@e[tag=a.b-c_d+e]", "@e[tag=foo,tag=foo]",
      "@e[team=red]", "@e[team=!red]", "@e[team=]", "@e[team=!]", "@e[team=red,team=blue]", "@e[team=!red,team=!blue]", "@e[team=red,team=!blue]", "@e[team=!red,team=red]", "@e[team=\"Team Name!\"]", "@e[team=nope]",
      "@e[sort=nearest]", "@e[sort=furthest]", "@e[sort=random]", "@e[sort=arbitrary]", "@e[sort=foo]", "@e[sort=]", "@e[sort=nearest,sort=furthest]", "@e[sort=Nearest]", "@a[sort=nearest]", "@p[sort=random]", "@r[sort=nearest]", "@s[sort=nearest]", "@n[sort=furthest]", "@e[sort=nea",
      "@e[limit=1]", "@e[limit=0]", "@e[limit=-1]", "@e[limit=1,limit=2]", "@e[limit=a]", "@e[limit=]", "@e[limit=1.5]", "@e[limit=2147483647]", "@e[limit=2147483648]", "@p[limit=2]", "@r[limit=2]", "@s[limit=1]", "@s[limit=2]", "@a[limit=1]", "@n[limit=3]", "@e[limit=1e2]", "@e[limit=+1]",
      "@e[gamemode=creative]", "@e[gamemode=!creative]", "@e[gamemode=foo]", "@e[gamemode=]", "@e[gamemode=survival,gamemode=creative]", "@e[gamemode=!survival,gamemode=!creative]", "@e[gamemode=survival,gamemode=!creative]", "@e[gamemode=!creative,gamemode=creative]", "@a[gamemode=spectator]", "@e[gamemode=adventure]", "@e[gamemode=Creative]",
      "@e[nbt={a:1}]", "@e[nbt=!{a:1}]", "@e[nbt={}]", "@e[nbt={a:1},nbt={b:2}]", "@e[nbt={a:1},nbt=!{b:2}]", "@e[nbt=]", "@e[nbt=a]", "@e[nbt={a:}]", "@e[nbt={a:1]", "@e[nbt={a:{b:[1,2]}}]", "@e[nbt={Tags:[\"x\"]}]", "@e[nbt={a:'x]'}]", "@e[nbt=[]]", "@e[nbt=1]", "@e[nbt=!]",
      "@e[advancements={minecraft:story/root=true}]", "@e[advancements={story/root=false}]", "@e[advancements={minecraft:story/root={a=true}}]", "@e[advancements={}]", "@e[advancements={a=}]", "@e[advancements={a=maybe}]", "@e[advancements={a=true,b=false}]", "@e[advancements={a={b=true,c=false}}]", "@e[advancements={a={}}]", "@e[advancements={a={b=}}]",
      "@e[advancements={a=true},advancements={b=true}]", "@e[advancements=a]", "@e[advancements=]", "@e[advancements={a=true", "@e[advancements={a={b=true]", "@e[advancements={:=true}]", "@e[advancements={A=true}]",
      "@e[predicate=foo:bar]", "@e[predicate=!foo:bar]", "@e[predicate=foo]", "@e[predicate=]", "@e[predicate=foo:bar,predicate=foo:baz]", "@e[predicate=!foo:bar,predicate=foo:bar]", "@e[predicate=Foo:Bar]", "@e[predicate=!]",
      "@e[foo=1]", "@e[Type=zombie]", "@e[limit=1,foo=1]", "@e[ type=zombie]", "@e[type =zombie]", "@e[type= zombie]", "@e[type=zombie ]", "@e[ type = zombie ]", "@e[type=zombie, limit=1]", "@e[type=zombie ,limit=1]",
      "@e[distance=..5,limit=1,sort=nearest]", "@a[gamemode=survival,distance=..10,limit=3]", "@e[type=!player,tag=boss,scores={hp=1..},team=!red,limit=5,sort=random,name=!X,level=1..,x_rotation=..0,y_rotation=0..,x=1,y=2,z=3,dx=4,dy=5,dz=6]",
      "@e[x=1,y=2,z=3,dx=4,dy=5,dz=6,distance=1..]", "@p[x=1,y=2,z=3]", "@r[distance=..5]", "@e[limit=1,sort=furthest]", "@s[tag=a]", "@n", "@n[type=zombie]", "@n[limit=1]", "@n[sort=random]", "@e[sort=random,limit=1]",
      "Steve", "Steve Jr", "012", "dd12be42-52a9-4a91-a8a1-11c01849e498", "DD12BE42-52A9-4A91-A8A1-11C01849E498", "dd12be42-52a9-4a91-a8a1-11c01849e49", "dd12be42-52a9-4a91-a8a1-11c01849e498x", "1-1-1-1-1", "\u00e9", "\ud83d\ude00", "A_B-c.d", "abcdefghijklmnopqrstuvwxyz0123456789", "a,b", "a[b]", "a=b", "x@y", "#", "~", "\"a\"",
      "@e[type=zombie][limit=1]", "@e[limit=1][limit=1]", "@e[type=zombie],", "@e,", "@e@e", "@e[type=zombie]@e",
   };

   static void registerEntities() {
      for (var e : new Object[][] {{"entity", (java.util.function.Supplier<ArgumentType<?>>) () -> EntityArgument.entity()}, {"entities", (java.util.function.Supplier<ArgumentType<?>>) () -> EntityArgument.entities()},
         {"player", (java.util.function.Supplier<ArgumentType<?>>) () -> EntityArgument.player()}, {"players", (java.util.function.Supplier<ArgumentType<?>>) () -> EntityArgument.players()}}) {
         @SuppressWarnings("unchecked")
         var sup = (java.util.function.Supplier<ArgumentType<?>>) e[1];
         var spec = new ArgsGolden.Spec("entity_" + e[0], sup, SELECTORS).render(ArgsSpecs::selector);
         if (e[0].equals("entities")) spec.variants("all", "none").target(2200); else spec.noPrefixes().target(700);
         ArgsGolden.add(spec);
      }
   }


   // ------------------------------------------------------------------------------------------------ messages, components, NBT

   static String canonJson(com.google.gson.JsonElement e) {
      if (e.isJsonObject()) {
         var keys = new java.util.TreeSet<>(e.getAsJsonObject().keySet());
         StringBuilder b = new StringBuilder("{");
         for (String k : keys) b.append(b.length() > 1 ? "," : "").append('"').append(k).append("\":").append(canonJson(e.getAsJsonObject().get(k)));
         return b.append('}').toString();
      }
      if (e.isJsonArray()) {
         StringBuilder b = new StringBuilder("[");
         for (var x : e.getAsJsonArray()) b.append(b.length() > 1 ? "," : "").append(canonJson(x));
         return b.append(']').toString();
      }
      return e.toString();
   }

   static String message(Object v) {
      var m = (MessageArgument.Message) v;
      StringBuilder b = new StringBuilder(m.text());
      b.append('|');
      for (var p : m.parts()) b.append(p.start()).append('-').append(p.end()).append(':').append(selector(p.selector())).append(';');
      return b.toString();
   }

   static final String[] NBT_SEEDS = {"0", "0b", "0B", "0s", "0l", "0L", "0.0", "0f", "0d", "1.5e3", "-1", "+1", "1e5", ".5", "5.", "0x10", "0b101", "1_000", "128b", "-129b", "true", "false", "True", "\"foo\"", "'foo'", "foo", "\"a\\\"b\"", "'a\\'b'", "\"\\u00e9\"",
      "\"\\n\"", "\"\\x41\"", "\"\\N{LATIN SMALL LETTER A}\"", "\"\\q\"", "{}", "{foo=bar}", "{foo:bar}", "{foo:1b,bar:[1,2]}", "{\"a b\":1}", "{a:{b:{c:1}}}", "{a:1,}", "{,}", "{a:1,a:2}", "{a:}", "{:1}", "{a 1}", "{a:1 b:2}", "{ a : 1 }", "[]", "[0]", "[B;]", "[B;1b,2b]", "[I;1,2,3]", "[L;1l]", "[B;1]",
      "[I;1b]", "[1,2,3]", "[1,\"a\"]", "[[1],[2]]", "[{a:1},{b:2}]", "[1,]", "[,]", "[I;,]", "[X;1]", "[B;", "[0", "{a:1", "\"abc", "'abc", "0 ", " 0", "0 1", "1.0f", "1.0F", "Infinity", "NaN", "-Infinity", "1e400", "1e-400", "99999999999", "9223372036854775807L", "9223372036854775808L", "-0", "-0.0", "0.1f", "1d", "1D", "1s", "32768s", "\u00e9", "\ud83d\ude00",
      "{a:\"\\ud83d\\ude00\"}", "[;]", "I;1", "@", "#", "{a:[B;1b,2b,3b]}", "{a:[I;1,2],b:[L;1l,2l]}", "{a:true,b:false}", "{a:1.5f,b:2.5d}", "{a:\"x\",b:'y'}", "{a:{}}", "{a:[]}", "[[]]", "[{}]", "{1:2}", "{1.5:2}", "{true:1}", "{a.b:1}", "{a-b:1}", "{a+b:1}", "{a_b:1}"};

   static void registerNbt() {
      add("nbt_tag", () -> NbtTagArgument.nbtTag(), Object::toString, NBT_SEEDS);
      add("compound_tag", () -> CompoundTagArgument.compoundTag(), Object::toString, NBT_SEEDS);
      add("component", () -> ComponentArgument.textComponent(ArgsGolden.ctx), v -> {
         var json = net.minecraft.network.chat.ComponentSerialization.CODEC.encodeStart(com.mojang.serialization.JsonOps.INSTANCE, (net.minecraft.network.chat.Component) v).getOrThrow();
         return canonJson(json);
      }, "\"hello world\"", "'hello world'", "\"\"", "{text:\"hello world\"}", "[\"\"]", "{text:\"a\",bold:true}", "{text:\"a\",color:\"red\"}", "{text:\"a\",color:\"#ff0000\"}", "{text:\"a\",color:\"nope\"}", "{translate:\"chat.type.text\"}", "{translate:\"a\",with:[1,\"b\"]}",
         "{score:{name:\"@s\",objective:\"a\"}}", "{selector:\"@a\"}", "{keybind:\"key.jump\"}", "{nbt:\"a\",entity:\"@s\"}", "{nbt:\"a\",block:\"1 2 3\"}", "{nbt:\"a\",storage:\"a:b\"}", "{text:\"a\",extra:[\"b\",{text:\"c\"}]}", "{text:\"a\",italic:false,underlined:true,strikethrough:true,obfuscated:true}",
         "{text:\"a\",font:\"minecraft:uniform\"}", "{text:\"a\",click_event:{action:\"open_url\",url:\"https://a.b\"}}", "{text:\"a\",hover_event:{action:\"show_text\",value:\"x\"}}", "{text:\"a\",insertion:\"x\"}", "{text:\"a\",shadow_color:-1}", "1", "1.5", "true", "[]", "[1,2]", "[\"a\",\"b\"]", "{}", "{foo:1}", "{text:1}", "{text:}", "{text:\"a\"", "[\"a\"", "\"a", "abc", "{text:\"a\",bold:\"x\"}",
         "{type:\"text\",text:\"a\"}", "{type:\"translatable\",translate:\"a\"}", "{type:\"nope\"}", "{text:\"a\",color:\"dark_red\"}", "{object:\"atlas\",sprite:\"a\"}", "{object:\"player\",player:{name:\"a\"}}", "{text:\"\\u00e9\"}", "{text:\"\\ud83d\\ude00\"}", "\"\\u00a7a\"");
      add("style", () -> StyleArgument.style(ArgsGolden.ctx), v -> {
         var json = net.minecraft.network.chat.Style.Serializer.CODEC.encodeStart(com.mojang.serialization.JsonOps.INSTANCE, (net.minecraft.network.chat.Style) v).getOrThrow();
         return canonJson(json);
      }, "{bold: true}", "{color: 'red'}", "{}", "{bold:false}", "{italic:true,underlined:true}", "{color:\"#abcdef\"}", "{color:\"nope\"}", "{color:\"#abcdeg\"}", "{color:\"#abc\"}", "{font:\"minecraft:default\"}", "{insertion:\"x\"}", "{shadow_color:[1.0,0.5,0.25,1.0]}", "{shadow_color:16777215}",
         "{click_event:{action:\"run_command\",command:\"/say hi\"}}", "{click_event:{action:\"nope\"}}", "{hover_event:{action:\"show_text\",value:\"hi\"}}", "{hover_event:{action:\"show_item\",id:\"stone\"}}", "{hover_event:{action:\"show_entity\",id:\"zombie\",uuid:[I;1,2,3,4]}}", "{bold:1}", "{bold:\"true\"}", "{unknown:1}", "{bold:true", "[]", "\"a\"", "1", "bold", "{obfuscated:true,strikethrough:true}", "{color:'dark_blue',bold:true}", "{color:16711680}", "{color:-1}");
      add("message", () -> MessageArgument.message(), ArgsSpecs::message, "Hello world!", "foo", "@e", "Hello @p :)", "@a", "@e[type=zombie] hi", "hi @e[type=zombie]", "a @x b", "a @ b", "@", "@@e", "@e@e", "@e[", "hi @e[type=zombie", "@e[type=zombie]]", "x @s y @r z", "\u00e9 @p", "\ud83d\ude00 @a",
         "@p[limit=2]", "@e[foo=1]", "@e[type=foo]", "@P", "mail@foo.bar", "@e[name=\"a b\"] and @a[tag=x]", "@n[type=zombie]", "100@", "@e[distance=..5]", "", " ", "  @a", "@a ", "x".repeat(255), "x".repeat(256), "x".repeat(257), "@a".repeat(100), "@a".repeat(200));
      ArgsGolden.add(new ArgsGolden.Spec("message_noselectors", () -> MessageArgument.message(), "Hello world!", "@e", "Hello @p :)", "@e[type=zombie] hi", "a @x b", "@", "x".repeat(257), "", " ").render(ArgsSpecs::message).variants("none").target(300));
   }


   static String scoreHolder(Object v) {
      var r = (ScoreHolderArgument.Result) v;
      if (r instanceof ScoreHolderArgument.SelectorResult sr) return "selector " + selector(field(sr, "selector"));
      try {
         var names = r.getNames(ArgsGolden.currentSrc, () -> java.util.List.of(net.minecraft.world.scores.ScoreHolder.forNameOnly("w1"), net.minecraft.world.scores.ScoreHolder.forNameOnly("w2")));
         return "names " + names.stream().map(net.minecraft.world.scores.ScoreHolder::getScoreboardName).toList();
      } catch (com.mojang.brigadier.exceptions.CommandSyntaxException e) {
         return "error " + e.getMessage();
      }
   }

   static void registerScoreHolders() {
      String[] seeds = {"Player", "0123", "*", "@e", "@a", "@p", "@s", "@r", "@n", "@e[type=zombie]", "@a[limit=1]", "@p[limit=2]", "@e[limit=1]", "@e[", "@x", "@", "#fake", "#", "#a b", "dd12be42-52a9-4a91-a8a1-11c01849e498", "1-1-1-1-1", "Steve Jr", "a b", "", " ", "\u00e9", "\ud83d\ude00",
         "*x", "x*", "**", "@e[type=!player,limit=1]", "@a[scores={a=1}]", "@s[limit=2]", "name with spaces and a very long length over sixteen", "Player ", "Player\tTab"};
      ArgsGolden.add(new ArgsGolden.Spec("score_holder", () -> ScoreHolderArgument.scoreHolder(), seeds).render(ArgsSpecs::scoreHolder).variants("all", "none").target(700).suggests(ScoreHolderArgument.SUGGEST_SCORE_HOLDERS));
      ArgsGolden.add(new ArgsGolden.Spec("score_holders", () -> ScoreHolderArgument.scoreHolders(), seeds).render(ArgsSpecs::scoreHolder).target(700).suggests(ScoreHolderArgument.SUGGEST_SCORE_HOLDERS));
   }


   static String slotRange(Object v) {
      var r = (net.minecraft.world.inventory.SlotRange) v;
      return r.getSerializedName() + "|" + r.slots();
   }

   static void registerSlots() {
      String[] seeds = {"container.0", "container.5", "container.53", "container.54", "container.*", "weapon", "weapon.mainhand", "weapon.offhand", "weapon.*", "armor.head", "armor.chest", "armor.legs", "armor.feet", "armor.body", "armor.*", "hotbar.0", "hotbar.8", "hotbar.9", "hotbar.*",
         "inventory.0", "inventory.26", "inventory.27", "inventory.*", "enderchest.5", "enderchest.*", "horse.chest.5", "horse.saddle", "horse.armor", "horse.*", "villager.7", "villager.*", "player.crafting.2", "player.crafting.*", "player.cursor", "contents", "saddle", "mount.*", "container.-1",
         "container.01", "container.", "container", "foo", "weapon.mainhand ", "", "Weapon", "\u00e9", "armor.head.x", "container.5.6", "container. 5", "bundle.0", "bundle.*", "player.*"};
      ArgsGolden.add(new ArgsGolden.Spec("slot", () -> SlotArgument.slot(), seeds).render(Object::toString).target(400));
      ArgsGolden.add(new ArgsGolden.Spec("slots", () -> SlotsArgument.slots(), seeds).render(ArgsSpecs::slotRange).target(400));
      ArgsGolden.add(new ArgsGolden.Spec("game_profile", () -> GameProfileArgument.gameProfile(), "Player", "0123", "dd12be42-52a9-4a91-a8a1-11c01849e498", "@e", "@a", "@p", "@s", "@r", "@n", "@a[limit=1]", "@e[", "@e[type=player]", "@a[type=zombie]", "@x", "@", "", " ", "\u00e9", "a b", "Steve", "@a[name=Steve]", "@s[limit=2]", "x".repeat(40), "#x")
         .render(v -> v instanceof GameProfileArgument.SelectorResult sr ? "selector " + selector(field(sr, "selector")) : "plain").variants("all", "none").target(500));
   }


   // ------------------------------------------------------------------------------------------------ blocks and items

   static String stateString(net.minecraft.world.level.block.state.BlockState st) {
      var props = new java.util.TreeMap<String, String>();
      for (var p : st.getProperties()) props.put(p.getName(), net.minecraft.world.level.block.state.properties.Property.class.cast(p).getName(st.getValue((net.minecraft.world.level.block.state.properties.Property) p)));
      StringBuilder b = new StringBuilder(net.minecraft.core.registries.BuiltInRegistries.BLOCK.getKey(st.getBlock()).toString()).append('[');
      props.forEach((k, v) -> b.append(k).append('=').append(v).append(','));
      return b.append(']').toString();
   }

   static String propNames(java.util.Collection<? extends net.minecraft.world.level.block.state.properties.Property<?>> props) {
      return props.stream().map(net.minecraft.world.level.block.state.properties.Property::getName).sorted().toList().toString();
   }

   @SuppressWarnings("unchecked")
   static void registerBlocks() {
      String[] seeds = {"stone", "minecraft:stone", "stone[foo=bar]", "oak_stairs[facing=east,half=top]", "oak_stairs[facing=east", "oak_stairs[facing=up]", "oak_stairs[facing=east,facing=west]", "oak_stairs[nope=1]", "oak_stairs[]", "oak_stairs[ facing = east ]", "oak_stairs[facing=]", "oak_stairs[=east]", "oak_stairs[facing=east,]",
         "oak_stairs[facing=east]x", "oak_stairs[facing=east] ", "chest{Items:[]}", "chest[facing=north]{Items:[]}", "chest{Items:[]}[facing=north]", "stone{a:1}", "stone{}", "stone[]{}", "stone[", "stone{", "stone{a:", "foo{bar=baz}", "water[level=3]", "water[level=8]", "redstone_wire[power=15]", "redstone_wire[power=16]", "redstone_wire[power=-1]",
         "note_block[note=24,instrument=harp]", "note_block[note=25]", "lever[face=wall,powered=true]", "lever[powered=TRUE]", "oak_sign{front_text:{}}", "air", "cave_air", "#logs", "#minecraft:logs", "#minecraft:logs[axis=x]", "#minecraft:logs{a:1}", "#minecraft:logs[nope=1]", "#minecraft:logs[axis=nope]", "#minecraft:nope", "#", "#minecraft:", "#minecraft:logs[", "#minecraft:logs[axis",
         "#minecraft:logs[axis=", "#minecraft:logs[axis=x", "#minecraft:logs[axis=x,", "#minecraft:logs[axis=x,axis=y]", "#minecraft:logs[axis=x]{", "#minecraft:logs[axis=x]{a:1}", "minecraft:", "nope", "minecraft:nope", "STONE", "", " ", "stone ", "\u00e9", "stone[\u00e9=1]", "oak_door[half=lower,hinge=left,open=true,powered=false,facing=north]", "oak_door[half=lower,half=upper]",
         "piston[extended=true,facing=down]", "bed[part=head]", "red_bed[part=head,occupied=true]", "barrel[open=true]{Items:[{Slot:0b,id:\"stone\",count:1}]}", "command_block{Command:\"say hi\"}", "oak_leaves[distance=7,persistent=true,waterlogged=true]", "oak_leaves[distance=8]", "oak_leaves[distance=0]",
         "campfire[lit=true]", "stone[lit=true]", "stone[waterlogged=false]", "oak_stairs[waterlogged=maybe]", "oak_stairs[shape=inner_left]", "oak_stairs[shape=INNER_LEFT]", "cactus[age=15]", "wheat[age=7]", "wheat[age=08]", "wheat[age=+1]", "wheat[age=1.0]", "scaffold", "scaffolding[bottom=true,distance=3]", "lectern[has_book=true]{Book:{}}", "jigsaw[orientation=up_north]", "jigsaw[orientation=up_foo]"};
      ArgsGolden.add(new ArgsGolden.Spec("block_state", () -> BlockStateArgument.block(ArgsGolden.ctx), seeds).render(v -> {
         var b = (net.minecraft.commands.arguments.blocks.BlockInput) v;
         return stateString(b.getState()) + "|" + propNames((java.util.Collection) field(b, "properties")) + "|" + field(b, "tag");
      }).target(1400));
      ArgsGolden.add(new ArgsGolden.Spec("block_predicate", () -> BlockPredicateArgument.blockPredicate(ArgsGolden.ctx), seeds).render(v -> {
         if (v.getClass().getSimpleName().equals("BlockPredicate")) {
            return "block " + stateString((net.minecraft.world.level.block.state.BlockState) field(v, "state")) + "|" + propNames((java.util.Collection) field(v, "properties")) + "|" + field(v, "nbt");
         }
         var vague = new java.util.TreeMap<>((java.util.Map<String, String>) field(v, "vagueProperties"));
         return "tag n=" + ((net.minecraft.core.HolderSet<?>) field(v, "tag")).size() + "|" + vague + "|" + field(v, "nbt");
      }).target(1500));
   }

   static void registerItems() {
      String[] seeds = {"stick", "minecraft:stick", "stick{foo=bar}", "stick[]", "stick[damage=5]", "stick[!damage]", "stick[damage=5,!damage]", "stick[!damage,damage=5]", "stick[damage=5,damage=6]", "stick[!damage,!damage]", "diamond_sword[damage=5,enchantments={\"minecraft:sharpness\":5}]", "diamond_sword[enchantments={sharpness:5}]",
         "stick[custom_name='x']", "stick[custom_name={text:'x'}]", "stick[custom_name={text:}]", "stick[nope=1]", "stick[minecraft:nope=1]", "stick[damage=]", "stick[damage=a]", "stick[damage=-1]", "stick[damage=1.5]", "stick[minecraft:damage=5]", "stick[max_stack_size=64]", "stick[max_stack_size=0]", "stick[max_stack_size=100]", "stick[unbreakable={}]", "stick[unbreakable=true]",
         "apple[food={nutrition:2,saturation:1}]", "stick[custom_data={a:1}]", "stick[custom_data={a:1},custom_data={b:2}]", "stick[lore=['a','b']]", "stick[lore=[]]", "stick[damage=5", "stick[damage=5,", "stick[,]", "stick[!]", "stick[!nope]", "stick[ damage = 5 ]", "stick[damage=5]x", "stick[damage=5] ", "#stick", "#minecraft:logs", "nope", "minecraft:nope", "air", "minecraft:air",
         "", " ", "stick ", "STICK", "\u00e9", "stick[", "stick[d", "stick[damage", "stick[damage=5,!", "stick[damage=5,!d", "stick[damage=5,m", "stick{", "stick{}", "stick[]{}", "{}", "[]", "stick[enchantments={}]", "stick[enchantments={nope:1}]", "stick[enchantments={sharpness:0}]", "stick[enchantments={sharpness:255}]", "stick[enchantments={sharpness:256}]",
         "stick[enchantment_glint_override=true]", "stick[rarity=epic]", "stick[rarity=nope]", "stick[repair_cost=5]", "stick[repair_cost=-5]", "stick[item_name={text:'x'}]", "stick[dyed_color=16711680]", "stick[dyed_color={rgb:1}]", "stick[potion_contents={potion:'minecraft:water'}]", "stick[potion_contents={potion:'nope'}]", "stick[profile='Steve']", "stick[profile={name:'Steve'}]",
         "stick[container=[]]", "stick[container=[{slot:0,item:{id:'stone',count:1}}]]", "stick[bundle_contents=[]]", "stick[attribute_modifiers=[]]", "stick[tool={rules:[],default_mining_speed:1.0,damage_per_block:1}]", "stick[block_state={facing:'north'}]", "stick[can_break={blocks:'stone'}]", "stick[charged_projectiles=[]]", "stick[trim={material:'copper',pattern:'sentry'}]",
         "stick[firework_explosion={shape:'small_ball'}]", "stick[map_color=5]", "stick[entity_data={id:'zombie'}]", "stick[bees=[]]", "stick[lock='x']", "stick[note_block_sound='a:b']", "stick[instrument='minecraft:ponder_goat_horn']", "stick[jukebox_playable='minecraft:cat']", "stick[death_protection={}]", "stick[equippable={slot:'head'}]", "stick[damage_resistant={types:'#minecraft:is_fire'}]",
         "stick[use_cooldown={seconds:1.0}]", "stick[consumable={}]", "stick[weapon={}]", "stick[minimum_attack_charge=0.5]", "stick[tooltip_display={hide_tooltip:true}]", "stick[glider={}]", "stick[break_sound='minecraft:entity.item.break']"};
      ArgsGolden.add(new ArgsGolden.Spec("item", () -> ItemArgument.item(ArgsGolden.ctx), seeds).render(v -> {
         var i = (net.minecraft.commands.arguments.item.ItemInput) v;
         String patch;
         try {
            patch = net.minecraft.core.component.DataComponentPatch.CODEC.encodeStart(ArgsGolden.ctx.createSerializationContext(net.minecraft.nbt.NbtOps.INSTANCE), i.components()).getOrThrow().toString();
         } catch (RuntimeException e) {
            patch = "<encode failed>";
         }
         return i.item().getRegisteredName() + "|" + patch;
      }).target(1500));
   }

   static void register() {
      registerBlocks();
      registerItems();
      registerSlots();
      registerScoreHolders();
      registerNbt();
      registerEntities();
      registerRegistries();
      registerCoordinates();
      registerScoreboard();
      add("time", () -> TimeArgument.time(), Object::toString,
         "0", "1", "1d", "2s", "3t", "1.5s", "0.5d", "-1", "-1t", "1x", "d", "1e3t", "1e30", "NaN", "Infinity", "1_0", ".5", "5.", "99999999999d", "2147483647t", "0.5t", "1.5t", "-0.5t");
      ArgsGolden.add(new ArgsGolden.Spec("time_min", () -> TimeArgument.time(20), "0", "19t", "20", "1s", "1.5s", "-3", "1d").target(220));
      add("angle", () -> AngleArgument.angle(), v -> {
         var s = ArgsSpecs.renderSource();
         return String.valueOf(((AngleArgument.SingleAngle) v).getAngle(s));
      }, "0", "~", "~-5", "~5", "90", "180", "-180", "540", "-.5", "~.5", "1e3", "NaN", "Infinity", "~NaN", "5 ", "~ ", "-", "~-", "1.5.2");
      add("hex_color", () -> HexColorArgument.hexColor(), Object::toString,
         "F00", "FF0000", "abc", "ABCDEF", "123456", "12345", "1234567", "GGG", "00000", "000000", "#fff", "fFf", "-1", " ", "ff00ff", "0g0", "ŕ", "000", "ZZZZZZ");
      add("identifier", () -> IdentifierArgument.id(), Object::toString,
         "minecraft:stone", "stone", ":stone", "minecraft:", "Minecraft:Stone", "a/b/c", "a:b:c", "a b", "foo.bar_baz-1", "foo:bar/baz", "a%b", "é", "minecraft:é", "😀", "minecraft:stone ", "_:_", "-:-", ".:.", "/", "..", "a:/b", "UPPER");
      add("dimension", () -> DimensionArgument.dimension(), Object::toString,
         "minecraft:overworld", "overworld", "minecraft:the_nether", "the_end", "foo:bar", "Bad:Id", "", "minecraft:");
      add("game_mode", () -> GameModeArgument.gameMode(), v -> ((net.minecraft.world.level.GameType) v).getName(),
         "survival", "creative", "adventure", "spectator", "Survival", "s", "0", "1", "", "creative ", "survival_", "hardcore", "spec", "é");
      add("heightmap_type", () -> HeightmapTypeArgument.heightmap(), ArgsSpecs::ser,
         "world_surface_wg", "world_surface", "ocean_floor_wg", "ocean_floor", "motion_blocking", "motion_blocking_no_leaves", "WORLD_SURFACE", "foo", "ocean", "", "motion");
      add("template_mirror", () -> TemplateMirrorArgument.templateMirror(), ArgsSpecs::ser, "none", "left_right", "front_back", "NONE", "left", "x", "", "front_back ");
      add("template_rotation", () -> TemplateRotationArgument.templateRotation(), ArgsSpecs::ser, "none", "clockwise_90", "180", "counterclockwise_90", "clockwise_180", "CLOCKWISE_90", "x", "", "clock");
      add("team_color", () -> TeamColorArgument.teamColor(), ArgsSpecs::ser,
         "black", "dark_blue", "dark_green", "dark_aqua", "dark_red", "dark_purple", "gold", "gray", "dark_gray", "blue", "green", "aqua", "red", "light_purple", "yellow", "white", "reset", "bold", "Red", "pink", "", "re", "dark_");
      add("swing_animation", () -> SwingAnimationArgument.swingAnimationType(), ArgsSpecs::ser, "none", "whack", "stab", "Whack", "slash", "", "w", "stab ");
      add("entity_anchor", () -> EntityAnchorArgument.anchor(), v -> ((Enum<?>) v).name(), "feet", "eyes", "Feet", "head", "", "e", "feet ", "eyes_");
      add("uuid", () -> UuidArgument.uuid(), Object::toString,
         "dd12be42-52a9-4a91-a8a1-11c01849e498", "00000000-0000-0000-0000-000000000000", "DD12BE42-52A9-4A91-A8A1-11C01849E498", "dd12be42-52a9-4a91-a8a1-11c01849e49", "dd12be42-52a9-4a91-a8a1-11c01849e4980",
         "1-2-3-4-5", "1-1-1-1-1", "-", "--", "a", "---", "1-2-3-4", "-1-2-3-4-5", "ffffffff-ffff-ffff-ffff-ffffffffffff", "g", "dd12be42-52a9-4a91-a8a1-11c01849e498x", "dd12be42-52a9-4a91-a8a1-11c01849e498 ", "0-0-0-0-0", "1-1-1-1-ffffffffffff1");
   }
}
