import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;
import net.minecraft.world.entity.AreaEffectCloud;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.OminousItemSpawner;
import net.minecraft.world.entity.projectile.EvokerFangs;
import net.minecraft.world.entity.projectile.FireworkRocketEntity;
import net.minecraft.world.entity.projectile.Projectile;
import net.minecraft.world.entity.projectile.ShulkerBullet;
import net.minecraft.world.entity.vehicle.minecart.AbstractMinecart;

/**
 * Per-class fields of the rich `E` lines (`dump xentity:REF`, `xentities`, `mob:REF`, `mobs`): the Java twin of the Rust registry
 * `SCENARIO_DUMP_EXTRA` (`mc-game-core/src/world/entity/scenario_dump.rs`). An entry appends ` key=value` pairs to the line of every
 * entity of its class, in ascending `order` (ties: list order); the plain `entity:`/`entities` lines never carry them, so older
 * fixtures stay unchanged. A class task adds one entry here (keep the list sorted by `order`, then by class name) and the same
 * fields, in the same order, to its Rust registration. Values follow the driver's grammar: booleans `0`/`1`, floats via `f`,
 * entity references through `uuidName`/`entName`, `-` for none, no spaces.
 *
 * <p>Order 20 = {@link Projectile}; concrete classes use >= 30.
 */
final class EntityExtras {
   interface Fill {
      void fill(EntityOracleDriver d, Entity e, Fields out);
   }

   record Extra(int order, Class<? extends Entity> type, Fill fill) {
   }

   static final class Fields {
      final StringBuilder b;

      Fields(StringBuilder b) {
         this.b = b;
      }

      Fields add(String key, Object value) {
         b.append(' ').append(key).append('=').append(value);
         return this;
      }
   }

   static final List<Extra> ALL = new ArrayList<>();

   static {
      ALL.add(new Extra(20, Projectile.class, (d, e, out) -> {
         var owner = (net.minecraft.world.entity.EntityReference<?>)EntityOracleDriver.priv(e, Projectile.class, "owner");
         out.add("owner", d.uuidName(owner == null ? null : owner.getUUID()));
         out.add("leftowner", EntityOracleDriver.bool((Boolean)EntityOracleDriver.priv(e, Projectile.class, "leftOwner")));
         out.add("shot", EntityOracleDriver.bool((Boolean)EntityOracleDriver.priv(e, Projectile.class, "hasBeenShot")));
         out.add("canbreak", EntityOracleDriver.bool(EntityOracleDriver.priv(e, Projectile.class, "canBreak") != null));
      }));
      ALL.add(new Extra(30, AbstractMinecart.class, (d, e, out) -> {
         AbstractMinecart c = (AbstractMinecart)e;
         out.add("rails", EntityOracleDriver.bool(c.isOnRails())).add("flipped", EntityOracleDriver.bool(c.isFlipped()));
         out.add("vhurt", c.getHurtTime() + "/" + c.getHurtDir() + "/" + EntityOracleDriver.f(c.getDamage()));
         out.add("mdir", c.getMotionDirection().getSerializedName()).add("rider", d.entName(c.getFirstPassenger()));
      }));
      ALL.add(new Extra(30, AreaEffectCloud.class, (d, e, out) -> {
         AreaEffectCloud c = (AreaEffectCloud)e;
         out.add("radius", EntityOracleDriver.f(c.getRadius())).add("waiting", EntityOracleDriver.bool(c.isWaiting()));
         out.add("duration", c.getDuration()).add("age", c.tickCount);
      }));
      ALL.add(new Extra(30, EvokerFangs.class, (d, e, out) -> {
         var owner = (net.minecraft.world.entity.EntityReference<?>)EntityOracleDriver.priv(e, "owner");
         out.add("warmup", EntityOracleDriver.priv(e, "warmupDelayTicks"));
         out.add("spike", EntityOracleDriver.bool((Boolean)EntityOracleDriver.priv(e, "sentSpikeEvent")));
         out.add("life", EntityOracleDriver.priv(e, "lifeTicks"));
         out.add("owner", d.uuidName(owner == null ? null : owner.getUUID()));
      }));
      ALL.add(new Extra(30, FireworkRocketEntity.class, (d, e, out) -> {
         out.add("life", EntityOracleDriver.priv(e, "life")).add("lifetime", EntityOracleDriver.priv(e, "lifetime"));
         out.add("attached", d.entName((Entity)EntityOracleDriver.priv(e, "attachedToEntity")));
         out.add("angle", EntityOracleDriver.bool(((FireworkRocketEntity)e).isShotAtAngle()));
      }));
      ALL.add(new Extra(30, OminousItemSpawner.class, (d, e, out) -> {
         OminousItemSpawner s = (OminousItemSpawner)e;
         out.add("item", EntityOracleDriver.item(s.getItem())).add("ticks", EntityOracleDriver.priv(s, "spawnItemAfterTicks"));
      }));
      ALL.add(new Extra(30, ShulkerBullet.class, (d, e, out) -> {
         var target = (net.minecraft.world.entity.EntityReference<?>)EntityOracleDriver.priv(e, "finalTarget");
         var dir = (net.minecraft.core.Direction)EntityOracleDriver.priv(e, "currentMoveDirection");
         out.add("steps", EntityOracleDriver.priv(e, "flightSteps")).add("dir", dir == null ? "-" : dir.getName());
         out.add("txd", EntityOracleDriver.f((Double)EntityOracleDriver.priv(e, "targetDeltaX")));
         out.add("tyd", EntityOracleDriver.f((Double)EntityOracleDriver.priv(e, "targetDeltaY")));
         out.add("tzd", EntityOracleDriver.f((Double)EntityOracleDriver.priv(e, "targetDeltaZ")));
         out.add("target", d.uuidName(target == null ? null : target.getUUID()));
      }));
      ALL.sort(Comparator.comparingInt(Extra::order));
   }

   /** Appends the fields of every registered class `e` is an instance of. */
   static void append(EntityOracleDriver d, StringBuilder b, Entity e) {
      Fields out = new Fields(b);
      for (Extra x : ALL) {
         if (x.type().isInstance(e)) {
            x.fill().fill(d, e, out);
         }
      }
   }
}
