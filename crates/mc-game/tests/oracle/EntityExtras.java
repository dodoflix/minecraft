import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;
import net.minecraft.world.entity.AreaEffectCloud;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.OminousItemSpawner;
import net.minecraft.world.entity.projectile.Projectile;

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
      ALL.add(new Extra(30, AreaEffectCloud.class, (d, e, out) -> {
         AreaEffectCloud c = (AreaEffectCloud)e;
         out.add("radius", EntityOracleDriver.f(c.getRadius())).add("waiting", EntityOracleDriver.bool(c.isWaiting()));
         out.add("duration", c.getDuration()).add("age", c.tickCount);
      }));
      ALL.add(new Extra(30, OminousItemSpawner.class, (d, e, out) -> {
         OminousItemSpawner s = (OminousItemSpawner)e;
         out.add("item", EntityOracleDriver.item(s.getItem())).add("ticks", EntityOracleDriver.priv(s, "spawnItemAfterTicks"));
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
