import com.google.gson.JsonElement;
import com.google.gson.JsonParser;
import com.mojang.serialization.DataResult;
import com.mojang.serialization.JsonOps;
import java.nio.file.*;
import java.util.*;
import net.minecraft.advancements.predicates.DamagePredicate;
import net.minecraft.advancements.predicates.DamageSourcePredicate;
import net.minecraft.advancements.predicates.LocationPredicate;
import net.minecraft.advancements.predicates.entity.EntityPredicate;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.RegistryOps;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.damagesource.DamageSource;
import net.minecraft.world.damagesource.DamageType;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.entity.Pose;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.player.Input;
import net.minecraft.world.phys.Vec3;

/**
 * Vanilla oracle of the advancement predicates (`advancements::predicates` in mc-game): evaluates real
 * {@code EntityPredicate}/{@code DamageSourcePredicate}/{@code DamagePredicate}/{@code LocationPredicate} objects, decoded from
 * JSON with the real registry ops, over the real entities and blocks of the in-process vanilla level.
 *
 * Usage: scripts/predicate-oracle.sh [NAME...]
 * Output: crates/mc-game/tests/fixtures/predicate_oracle/NAME.txt
 * One scenario by hand: scripts/entity-oracle.sh crates/mc-game/tests/oracle/PredicateOracle.java -o crates/mc-game/tests/fixtures/predicate_oracle/NAME.txt crates/mc-game/tests/oracle/scenarios/predicates/NAME.txt
 *
 * <p>A scenario is an
 * {@code EntityOracleDriver} script (world/entities, see its header) extended by the commands below; the Rust test
 * {@code tests/golden/advancement_predicates_golden.rs} replays the same script against our level and entities.
 * <pre>
 * -- state setters (besides the driver commands) --
 * onground REF BOOL | pose REF POSE | sprint REF BOOL | swimming REF BOOL | fallflying REF BOOL | flying PLAYER BOOL
 * tickcount REF N | ride PASSENGER VEHICLE | target MOB TARGET | team_join REF TEAM | pinput PLAYER flags | known_movement PLAYER X Y Z
 * recipe PLAYER ID | ominous_banner MOB
 * -- evaluations (output: "R id true|false|ERR", "J id canonical-json" when the JSON decoded) --
 * pred_entity ID SUBJECT ORIGIN JSON            ORIGIN: none | x,y,z | @REF (the position of REF)
 * pred_location ID X Y Z JSON
 * pred_damage ID TYPE ORIGIN JSON [by=REF] [direct=REF]       DamageSourcePredicate.matches(level, origin, source)
 * pred_damage_taken ID PLAYER TYPE JSON DEALT TAKEN BLOCKED [by=REF] [direct=REF]    DamagePredicate.matches(player, ...)
 * </pre>
 */
public final class PredicateOracle {
   static EntityOracleDriver d;

   public static void main(String[] args) throws Exception {
      d = new EntityOracleDriver();
      List<String> lines = Files.readAllLines(Path.of(args[0]));
      try {
         for (int i = 0; i < lines.size(); i++) {
            d.lineNo = i + 1;
            d.line = lines.get(i).strip();
            if (d.line.isEmpty() || d.line.startsWith("#")) {
               continue;
            }
            List<String> a = EntityOracleDriver.tokens(d.line);
            if (!own(a)) {
               d.run(a);
            }
         }
      } catch (Throwable t) {
         VanillaLevel.ERR.println("scenario " + args[0] + ":" + d.lineNo + ": " + d.line);
         t.printStackTrace(VanillaLevel.ERR);
         d.out.flush();
         Runtime.getRuntime().halt(1);
      }
      d.out.flush();
      System.exit(0);
   }

   static RegistryOps<JsonElement> ops() {
      return RegistryOps.create(JsonOps.INSTANCE, d.v.server.registryAccess());
   }

   static Vec3 origin(String s) {
      if (s.equals("none")) {
         return null;
      }
      if (s.startsWith("@")) {
         return d.ref(s.substring(1)).position();
      }
      String[] p = s.split(",");
      return new Vec3(Double.parseDouble(p[0]), Double.parseDouble(p[1]), Double.parseDouble(p[2]));
   }

   static <T> T parse(String id, com.mojang.serialization.Codec<T> codec, String json) {
      DataResult<T> r = codec.parse(ops(), JsonParser.parseString(json));
      if (r.error().isPresent()) {
         d.out.println("R " + id + " ERR");
         return null;
      }
      return r.getOrThrow();
   }

   static <T> void result(String id, com.mojang.serialization.Codec<T> codec, T p, boolean matches) {
      d.out.println("R " + id + " " + matches);
      d.out.println("J " + id + " " + codec.encodeStart(ops(), p).getOrThrow());
   }

   static DamageSource source(String type, List<String> a, int from) {
      Holder<DamageType> holder = d.v.server.registryAccess().lookupOrThrow(Registries.DAMAGE_TYPE).getOrThrow(ResourceKey.create(Registries.DAMAGE_TYPE, EntityOracleDriver.id(type)));
      Map<String, String> kv = d.kv(a, from);
      Entity direct = kv.containsKey("direct") ? d.ref(kv.get("direct")) : null;
      Entity causing = kv.containsKey("by") ? d.ref(kv.get("by")) : null;
      return new DamageSource(holder, direct, causing);
   }

   static void shared(Entity e, int flag, boolean value) throws Exception {
      var m = Entity.class.getDeclaredMethod("setSharedFlag", int.class, boolean.class);
      m.setAccessible(true);
      m.invoke(e, flag, value);
   }

   static boolean own(List<String> a) throws Exception {
      String cmd = a.get(0);
      switch (cmd) {
         case "onground" -> d.ref(a.get(1)).setOnGround(Boolean.parseBoolean(a.get(2)));
         case "pose" -> d.ref(a.get(1)).setPose(Pose.valueOf(a.get(2).toUpperCase(Locale.ROOT)));
         case "sprint" -> d.ref(a.get(1)).setSprinting(Boolean.parseBoolean(a.get(2)));
         case "swimming" -> d.ref(a.get(1)).setSwimming(Boolean.parseBoolean(a.get(2)));
         case "fallflying" -> shared(d.ref(a.get(1)), 7, Boolean.parseBoolean(a.get(2)));
         case "flying" -> ((ServerPlayer)d.ref(a.get(1))).getAbilities().flying = Boolean.parseBoolean(a.get(2));
         case "tickcount" -> d.ref(a.get(1)).tickCount = Integer.parseInt(a.get(2));
         case "ride" -> {
            d.world();
            d.ref(a.get(1)).startRiding(d.ref(a.get(2)), true, false);
         }
         case "target" -> ((Mob)d.ref(a.get(1))).setTarget((LivingEntity)d.ref(a.get(2)));
         case "team_join" -> {
            d.world();
            d.v.command("team add " + a.get(2));
            d.v.command("team join " + a.get(2) + " " + d.ref(a.get(1)).getScoreboardName());
         }
         case "ominous_banner" -> {
            d.world();
            var e = (LivingEntity)d.ref(a.get(1));
            e.setItemSlot(net.minecraft.world.entity.EquipmentSlot.HEAD, net.minecraft.world.entity.raid.Raid.getOminousBannerInstance(d.v.server.registryAccess().lookupOrThrow(Registries.BANNER_PATTERN)));
         }
         case "pinput" -> {
            Set<String> fl = new HashSet<>(Arrays.asList(a.get(2).split(",")));
            ((ServerPlayer)d.ref(a.get(1))).setLastClientInput(new Input(fl.contains("forward"), fl.contains("back"), fl.contains("left"), fl.contains("right"), fl.contains("jump"), fl.contains("sneak"), fl.contains("sprint")));
         }
         case "known_movement" -> ((ServerPlayer)d.ref(a.get(1))).setKnownMovement(new Vec3(Double.parseDouble(a.get(2)), Double.parseDouble(a.get(3)), Double.parseDouble(a.get(4))));
         case "recipe" -> ((ServerPlayer)d.ref(a.get(1))).getRecipeBook().add(ResourceKey.create(Registries.RECIPE, EntityOracleDriver.id(a.get(2))));
         case "pred_entity" -> {
            d.world();
            EntityPredicate p = parse(a.get(1), EntityPredicate.CODEC, a.get(4));
            if (p != null) {
               result(a.get(1), EntityPredicate.CODEC, p, p.matches(d.v.level, origin(a.get(3)), d.ref(a.get(2))));
            }
         }
         case "pred_location" -> {
            d.world();
            LocationPredicate p = parse(a.get(1), LocationPredicate.CODEC, a.get(5));
            if (p != null) {
               result(a.get(1), LocationPredicate.CODEC, p, p.matches(d.v.level, Double.parseDouble(a.get(2)), Double.parseDouble(a.get(3)), Double.parseDouble(a.get(4))));
            }
         }
         case "pred_damage" -> {
            d.world();
            DamageSourcePredicate p = parse(a.get(1), DamageSourcePredicate.CODEC, a.get(4));
            if (p != null) {
               result(a.get(1), DamageSourcePredicate.CODEC, p, p.matches(d.v.level, origin(a.get(3)), source(a.get(2), a, 5)));
            }
         }
         case "pred_damage_taken" -> {
            d.world();
            DamagePredicate p = parse(a.get(1), DamagePredicate.CODEC, a.get(4));
            if (p != null) {
               result(a.get(1), DamagePredicate.CODEC, p, p.matches((ServerPlayer)d.ref(a.get(2)), source(a.get(3), a, 8), Float.parseFloat(a.get(5)), Float.parseFloat(a.get(6)), Boolean.parseBoolean(a.get(7))));
            }
         }
         default -> {
            return false;
         }
      }
      return true;
   }
}
