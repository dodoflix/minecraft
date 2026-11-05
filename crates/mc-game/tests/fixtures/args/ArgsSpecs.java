import com.mojang.brigadier.arguments.ArgumentType;
import java.util.Optional;
import net.minecraft.commands.Commands;
import net.minecraft.commands.arguments.*;
import net.minecraft.commands.arguments.coordinates.*;
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

   static void register() {
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
