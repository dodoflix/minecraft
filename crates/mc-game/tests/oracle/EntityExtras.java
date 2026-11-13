import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;
import net.minecraft.world.entity.AreaEffectCloud;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.OminousItemSpawner;
import net.minecraft.world.entity.animal.armadillo.Armadillo;
import net.minecraft.world.entity.animal.goat.Goat;
import net.minecraft.world.entity.monster.PatrollingMonster;
import net.minecraft.world.entity.monster.illager.Pillager;
import net.minecraft.world.entity.monster.illager.Vindicator;
import net.minecraft.world.entity.projectile.EvokerFangs;
import net.minecraft.world.entity.projectile.EyeOfEnder;
import net.minecraft.world.entity.projectile.FireworkRocketEntity;
import net.minecraft.world.entity.projectile.FishingHook;
import net.minecraft.world.entity.projectile.Projectile;
import net.minecraft.world.entity.projectile.ShulkerBullet;
import net.minecraft.world.entity.raid.Raid;
import net.minecraft.world.entity.raid.Raider;
import net.minecraft.world.entity.vehicle.boat.AbstractBoat;
import net.minecraft.world.entity.vehicle.boat.AbstractChestBoat;
import net.minecraft.world.entity.vehicle.minecart.AbstractMinecart;
import net.minecraft.world.entity.vehicle.minecart.AbstractMinecartContainer;
import net.minecraft.world.entity.vehicle.minecart.MinecartFurnace;
import net.minecraft.world.entity.vehicle.minecart.MinecartHopper;
import net.minecraft.world.entity.vehicle.minecart.MinecartTNT;

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
      ALL.add(new Extra(20, Armadillo.class, (d, e, out) -> {
         out.add("astate", ((Armadillo)e).getState().getSerializedName());
         out.add("astateticks", EntityOracleDriver.priv(e, Armadillo.class, "inStateTicks"));
         out.add("scute", EntityOracleDriver.priv(e, Armadillo.class, "scuteTime"));
      }));
      ALL.add(new Extra(20, Projectile.class, (d, e, out) -> {
         var owner = (net.minecraft.world.entity.EntityReference<?>)EntityOracleDriver.priv(e, Projectile.class, "owner");
         out.add("owner", d.uuidName(owner == null ? null : owner.getUUID()));
         out.add("leftowner", EntityOracleDriver.bool((Boolean)EntityOracleDriver.priv(e, Projectile.class, "leftOwner")));
         out.add("shot", EntityOracleDriver.bool((Boolean)EntityOracleDriver.priv(e, Projectile.class, "hasBeenShot")));
         out.add("canbreak", EntityOracleDriver.bool(EntityOracleDriver.priv(e, Projectile.class, "canBreak") != null));
      }));
      ALL.add(new Extra(25, Goat.class, (d, e, out) -> {
         Goat g = (Goat)e;
         out.add("scream", EntityOracleDriver.bool(g.isScreamingGoat()));
         out.add("lhorn", EntityOracleDriver.bool(g.hasLeftHorn())).add("rhorn", EntityOracleDriver.bool(g.hasRightHorn()));
      }));
      ALL.add(new Extra(30, AbstractMinecart.class, (d, e, out) -> {
         AbstractMinecart c = (AbstractMinecart)e;
         out.add("rails", EntityOracleDriver.bool(c.isOnRails())).add("flipped", EntityOracleDriver.bool(c.isFlipped()));
         out.add("vhurt", c.getHurtTime() + "/" + c.getHurtDir() + "/" + EntityOracleDriver.f(c.getDamage()));
         out.add("mdir", c.getMotionDirection().getSerializedName()).add("rider", d.entName(c.getFirstPassenger()));
      }));
      ALL.add(new Extra(30, AbstractBoat.class, (d, e, out) -> {
         AbstractBoat b = (AbstractBoat)e;
         out.add("mdir", b.getMotionDirection().getSerializedName()).add("leash", d.entName(b.getLeashHolder()));
         out.add("ooc", EntityOracleDriver.f((Float)EntityOracleDriver.priv(b, AbstractBoat.class, "outOfControlTicks")));
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
      ALL.add(new Extra(30, EyeOfEnder.class, (d, e, out) -> {
         var target = (net.minecraft.world.phys.Vec3)EntityOracleDriver.priv(e, "target");
         out.add("target", target == null ? "-" : EntityOracleDriver.f(target.x) + "," + EntityOracleDriver.f(target.y) + "," + EntityOracleDriver.f(target.z));
         out.add("life", EntityOracleDriver.priv(e, "life"));
         out.add("survive", EntityOracleDriver.bool((Boolean)EntityOracleDriver.priv(e, "surviveAfterDeath")));
      }));
      ALL.add(new Extra(30, FireworkRocketEntity.class, (d, e, out) -> {
         out.add("life", EntityOracleDriver.priv(e, "life")).add("lifetime", EntityOracleDriver.priv(e, "lifetime"));
         out.add("attached", d.entName((Entity)EntityOracleDriver.priv(e, "attachedToEntity")));
         out.add("angle", EntityOracleDriver.bool(((FireworkRocketEntity)e).isShotAtAngle()));
      }));
      ALL.add(new Extra(30, FishingHook.class, (d, e, out) -> {
         out.add("state", EntityOracleDriver.priv(e, "currentState").toString().toLowerCase());
         out.add("life", EntityOracleDriver.priv(e, "life")).add("nibble", EntityOracleDriver.priv(e, "nibble"));
         out.add("lured", EntityOracleDriver.priv(e, "timeUntilLured")).add("hooked", EntityOracleDriver.priv(e, "timeUntilHooked"));
         out.add("angle", EntityOracleDriver.f((float)(Float)EntityOracleDriver.priv(e, "fishAngle")));
         out.add("biting", EntityOracleDriver.bool((Boolean)EntityOracleDriver.priv(e, "biting")));
         out.add("openwater", EntityOracleDriver.bool(((FishingHook)e).isOpenWaterFishing()));
         out.add("out", EntityOracleDriver.priv(e, "outOfWaterTime")).add("hookedin", d.entName(((FishingHook)e).getHookedIn()));
         out.add("luck", EntityOracleDriver.priv(e, "luck")).add("lure", EntityOracleDriver.priv(e, "lureSpeed"));
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
      ALL.add(new Extra(31, AbstractChestBoat.class, (d, e, out) -> {
         AbstractChestBoat c = (AbstractChestBoat)e;
         out.add("loot", c.getContainerLootTable() == null ? "-" : c.getContainerLootTable().identifier());
         out.add("seed", c.getContainerLootTableSeed());
      }));
      ALL.add(new Extra(31, AbstractMinecartContainer.class, (d, e, out) -> {
         AbstractMinecartContainer c = (AbstractMinecartContainer)e;
         // the raw list: reading it through getItem would unpack a pending loot table
         StringBuilder items = new StringBuilder();
         var stacks = c.getItemStacks();
         for (int i = 0; i < stacks.size(); i++) {
            if (!stacks.get(i).isEmpty()) {
               items.append(items.length() > 0 ? ";" : "").append(i).append(':').append(d.itemFull(stacks.get(i)));
            }
         }
         out.add("items", items.length() == 0 ? "-" : items.toString());
         out.add("loot", c.getContainerLootTable() == null ? "-" : c.getContainerLootTable().identifier());
         out.add("seed", c.getContainerLootTableSeed());
      }));
      ALL.add(new Extra(31, MinecartFurnace.class, (d, e, out) -> {
         MinecartFurnace c = (MinecartFurnace)e;
         out.add("fuel", EntityOracleDriver.priv(e, MinecartFurnace.class, "fuel"));
         out.add("push", EntityOracleDriver.f(c.push.x) + "/" + EntityOracleDriver.f(c.push.z));
      }));
      ALL.add(new Extra(31, MinecartTNT.class, (d, e, out) -> {
         out.add("fuse", EntityOracleDriver.priv(e, MinecartTNT.class, "fuse"));
      }));
      ALL.add(new Extra(32, MinecartHopper.class, (d, e, out) -> out.add("enabled", EntityOracleDriver.bool(((MinecartHopper)e).isEnabled()))));
      ALL.add(new Extra(40, Raider.class, (d, e, out) -> {
         Raider r = (Raider)e;
         Raid raid = (Raid)EntityOracleDriver.priv(e, Raider.class, "raid");
         out.add("raid", raid == null ? "-" : "1").add("wave", EntityOracleDriver.priv(e, Raider.class, "wave"));
         out.add("canjoin", EntityOracleDriver.bool((Boolean)EntityOracleDriver.priv(e, Raider.class, "canJoinRaid")));
         out.add("outside", EntityOracleDriver.priv(e, Raider.class, "ticksOutsideRaid"));
         var target = (net.minecraft.core.BlockPos)EntityOracleDriver.priv(e, PatrollingMonster.class, "patrolTarget");
         out.add("ptarget", target == null ? "-" : target.getX() + "," + target.getY() + "," + target.getZ());
         out.add("leader", EntityOracleDriver.bool(r.isPatrolLeader()));
         out.add("patrolling", EntityOracleDriver.bool((Boolean)EntityOracleDriver.priv(e, PatrollingMonster.class, "patrolling")));
         out.add("celebrating", EntityOracleDriver.bool(r.isCelebrating()));
      }));
      ALL.add(new Extra(41, Pillager.class, (d, e, out) -> {
         Pillager p = (Pillager)e;
         StringBuilder items = new StringBuilder();
         for (int i = 0; i < p.getInventory().getContainerSize(); i++) {
            if (!p.getInventory().getItem(i).isEmpty()) {
               items.append(items.length() > 0 ? ";" : "").append(i).append(':').append(d.itemFull(p.getInventory().getItem(i)));
            }
         }
         out.add("charging", EntityOracleDriver.bool(p.isChargingCrossbow())).add("inventory", items.length() == 0 ? "-" : items.toString());
      }));
      ALL.add(new Extra(41, Vindicator.class, (d, e, out) -> out.add("johnny", EntityOracleDriver.bool((Boolean)EntityOracleDriver.priv(e, Vindicator.class, "isJohnny")))));
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
