import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;
import net.minecraft.world.entity.AreaEffectCloud;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.OminousItemSpawner;
import net.minecraft.world.entity.animal.armadillo.Armadillo;
import net.minecraft.world.entity.animal.axolotl.Axolotl;
import net.minecraft.world.entity.animal.dolphin.Dolphin;
import net.minecraft.world.entity.animal.fish.Pufferfish;
import net.minecraft.world.entity.animal.fish.Salmon;
import net.minecraft.world.entity.animal.fish.TropicalFish;
import net.minecraft.world.entity.animal.frog.Frog;
import net.minecraft.world.entity.animal.frog.Tadpole;
import net.minecraft.world.entity.animal.squid.GlowSquid;
import net.minecraft.world.entity.animal.turtle.Turtle;
import net.minecraft.world.entity.animal.bee.Bee;
import net.minecraft.core.component.DataComponents;
import net.minecraft.world.entity.animal.equine.AbstractHorse;
import net.minecraft.world.entity.animal.equine.Llama;
import net.minecraft.world.entity.animal.equine.SkeletonHorse;
import net.minecraft.world.entity.animal.feline.Cat;
import net.minecraft.world.entity.animal.feline.Ocelot;
import net.minecraft.world.entity.animal.goat.Goat;
import net.minecraft.world.entity.animal.parrot.Parrot;
import net.minecraft.world.entity.animal.wolf.Wolf;
import net.minecraft.world.entity.monster.PatrollingMonster;
import net.minecraft.world.entity.monster.Ravager;
import net.minecraft.world.entity.monster.Witch;
import net.minecraft.world.entity.animal.sheep.Sheep;
import net.minecraft.world.entity.monster.Vex;
import net.minecraft.world.entity.monster.illager.Evoker;
import net.minecraft.world.entity.monster.illager.Pillager;
import net.minecraft.world.entity.monster.illager.SpellcasterIllager;
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
      ALL.add(new Extra(30, AbstractHorse.class, (d, e, out) -> {
         var owner = (net.minecraft.world.entity.EntityReference<?>)EntityOracleDriver.priv(e, AbstractHorse.class, "owner");
         out.add("owner", d.uuidName(owner == null ? null : owner.getUUID()));
         out.add("temper", EntityOracleDriver.priv(e, AbstractHorse.class, "temper"));
         out.add("eatc", EntityOracleDriver.priv(e, AbstractHorse.class, "eatingCounter"));
         out.add("mouthc", EntityOracleDriver.priv(e, AbstractHorse.class, "mouthCounter"));
         out.add("standc", EntityOracleDriver.priv(e, AbstractHorse.class, "standCounter"));
         out.add("tailc", ((AbstractHorse)e).tailCounter).add("sprintc", ((AbstractHorse)e).sprintCounter);
         out.add("jscale", EntityOracleDriver.f((Float)EntityOracleDriver.priv(e, AbstractHorse.class, "playerJumpPendingScale")));
         out.add("slide", EntityOracleDriver.bool((Boolean)EntityOracleDriver.priv(e, AbstractHorse.class, "allowStandSliding")));
         out.add("gallopc", EntityOracleDriver.priv(e, AbstractHorse.class, "gallopSoundCounter"));
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
      ALL.add(new Extra(30, Axolotl.class, (d, e, out) -> {
         Axolotl a = (Axolotl)e;
         out.add("variant", a.getVariant().getName()).add("playdead", EntityOracleDriver.bool(a.isPlayingDead()));
         out.add("bucket", EntityOracleDriver.bool(a.fromBucket()));
      }));
      ALL.add(new Extra(30, Bee.class, (d, e, out) -> {
         Bee b = (Bee)e;
         @SuppressWarnings("unchecked")
         var flags = (net.minecraft.network.syncher.EntityDataAccessor<Byte>)EntityOracleDriver.priv(null, Bee.class, "DATA_FLAGS_ID");
         out.add("flags", e.getEntityData().get(flags));
         out.add("hive", b.getHivePos() == null ? "-" : EntityOracleDriver.pos(b.getHivePos()));
         out.add("flower", b.getSavedFlowerPos() == null ? "-" : EntityOracleDriver.pos(b.getSavedFlowerPos()));
         out.add("hivecd", EntityOracleDriver.priv(e, Bee.class, "remainingCooldownBeforeLocatingNewHive"));
         out.add("flowercd", EntityOracleDriver.priv(e, Bee.class, "remainingCooldownBeforeLocatingNewFlower"));
         out.add("nonectar", EntityOracleDriver.priv(e, Bee.class, "ticksWithoutNectarSinceExitingHive"));
         out.add("stayout", EntityOracleDriver.priv(e, Bee.class, "stayOutOfHiveCountdown"));
         out.add("crops", EntityOracleDriver.priv(e, Bee.class, "numCropsGrownSincePollination"));
         out.add("sting", EntityOracleDriver.priv(e, Bee.class, "timeSinceSting"));
         out.add("wet", EntityOracleDriver.priv(e, Bee.class, "underWaterTicks"));
         out.add("roll", EntityOracleDriver.f((Float)EntityOracleDriver.priv(e, Bee.class, "rollAmount")));
         out.add("poll", EntityOracleDriver.bool((Boolean)EntityOracleDriver.priv(EntityOracleDriver.priv(e, Bee.class, "beePollinateGoal"), "pollinating")));
         var black = b.getBlacklistedHives();
         out.add("black", black.isEmpty() ? "-" : black.stream().map(EntityOracleDriver::pos).collect(java.util.stream.Collectors.joining(";")));
         var target = b.getPersistentAngerTarget();
         out.add("angerend", b.getPersistentAngerEndTime());
         out.add("angerat", d.uuidName(target == null ? null : target.getUUID()));
      }));
      ALL.add(new Extra(30, Dolphin.class, (d, e, out) -> {
         Dolphin dolphin = (Dolphin)e;
         var treasure = (net.minecraft.core.BlockPos)EntityOracleDriver.priv(e, Dolphin.class, "treasurePos");
         out.add("gotfish", EntityOracleDriver.bool(dolphin.gotFish())).add("moist", dolphin.getMoistnessLevel());
         out.add("treasure", treasure == null ? "-" : EntityOracleDriver.pos(treasure));
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
      ALL.add(new Extra(30, Frog.class, (d, e, out) -> {
         Frog f = (Frog)e;
         out.add("variant", f.getVariant().unwrapKey().map(k -> k.identifier().toString()).orElse("-"));
         out.add("tongue", d.entName(f.getTongueTarget().orElse(null)));
      }));
      ALL.add(new Extra(30, GlowSquid.class, (d, e, out) -> out.add("dark", ((GlowSquid)e).getDarkTicksRemaining())));
      ALL.add(new Extra(30, OminousItemSpawner.class, (d, e, out) -> {
         OminousItemSpawner s = (OminousItemSpawner)e;
         out.add("item", EntityOracleDriver.item(s.getItem())).add("ticks", EntityOracleDriver.priv(s, "spawnItemAfterTicks"));
      }));
      ALL.add(new Extra(30, Pufferfish.class, (d, e, out) -> {
         out.add("puff", ((Pufferfish)e).getPuffState());
         out.add("inflate", EntityOracleDriver.priv(e, Pufferfish.class, "inflateCounter"));
         out.add("deflate", EntityOracleDriver.priv(e, Pufferfish.class, "deflateTimer"));
      }));
      ALL.add(new Extra(30, Salmon.class, (d, e, out) -> out.add("size", ((Salmon)e).getVariant().getSerializedName())));
      ALL.add(new Extra(30, ShulkerBullet.class, (d, e, out) -> {
         var target = (net.minecraft.world.entity.EntityReference<?>)EntityOracleDriver.priv(e, "finalTarget");
         var dir = (net.minecraft.core.Direction)EntityOracleDriver.priv(e, "currentMoveDirection");
         out.add("steps", EntityOracleDriver.priv(e, "flightSteps")).add("dir", dir == null ? "-" : dir.getName());
         out.add("txd", EntityOracleDriver.f((Double)EntityOracleDriver.priv(e, "targetDeltaX")));
         out.add("tyd", EntityOracleDriver.f((Double)EntityOracleDriver.priv(e, "targetDeltaY")));
         out.add("tzd", EntityOracleDriver.f((Double)EntityOracleDriver.priv(e, "targetDeltaZ")));
         out.add("target", d.uuidName(target == null ? null : target.getUUID()));
      }));
      ALL.add(new Extra(30, Tadpole.class, (d, e, out) -> {
         Tadpole t = (Tadpole)e;
         out.add("tage", EntityOracleDriver.priv(e, Tadpole.class, "age")).add("locked", EntityOracleDriver.bool(t.isAgeLocked()));
         out.add("particle", EntityOracleDriver.priv(e, Tadpole.class, "ageLockParticleTimer"));
      }));
      ALL.add(new Extra(30, TropicalFish.class, (d, e, out) -> {
         TropicalFish f = (TropicalFish)e;
         out.add("pattern", f.getPattern().getSerializedName()).add("base", f.getBaseColor().getName()).add("pcolor", f.getPatternColor().getName());
      }));
      ALL.add(new Extra(30, Turtle.class, (d, e, out) -> {
         Turtle t = (Turtle)e;
         var travel = (net.minecraft.core.BlockPos)EntityOracleDriver.priv(e, Turtle.class, "travelPos");
         out.add("egg", EntityOracleDriver.bool(t.hasEgg())).add("laying", EntityOracleDriver.bool(t.isLayingEgg()));
         out.add("layegg", EntityOracleDriver.priv(e, Turtle.class, "layEggCounter"));
         out.add("home", EntityOracleDriver.pos(t.getHomePos())).add("travel", travel == null ? "-" : EntityOracleDriver.pos(travel));
         out.add("goinghome", EntityOracleDriver.bool((Boolean)EntityOracleDriver.priv(e, Turtle.class, "goingHome")));
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
      ALL.add(new Extra(31, Llama.class, (d, e, out) -> {
         out.add("spat", EntityOracleDriver.bool((Boolean)EntityOracleDriver.priv(e, Llama.class, "didSpit")));
         out.add("chead", d.entName((Entity)EntityOracleDriver.priv(e, Llama.class, "caravanHead")));
         out.add("ctail", d.entName((Entity)EntityOracleDriver.priv(e, Llama.class, "caravanTail")));
      }));
      ALL.add(new Extra(31, SkeletonHorse.class, (d, e, out) -> {
         out.add("trap", EntityOracleDriver.bool(((SkeletonHorse)e).isTrap()));
         out.add("traptime", EntityOracleDriver.priv(e, SkeletonHorse.class, "trapTime"));
      }));
      ALL.add(new Extra(31, Cat.class, (d, e, out) -> {
         Cat c = (Cat)e;
         out.add("variant", holderName(c.get(DataComponents.CAT_VARIANT))).add("svariant", holderName(c.get(DataComponents.CAT_SOUND_VARIANT)));
         out.add("collar", c.get(DataComponents.CAT_COLLAR).getSerializedName());
         out.add("lie", EntityOracleDriver.f((Float)EntityOracleDriver.priv(e, Cat.class, "lieDownAmount")));
         out.add("lietail", EntityOracleDriver.f((Float)EntityOracleDriver.priv(e, Cat.class, "lieDownAmountTail")));
         out.add("ontop", EntityOracleDriver.bool(c.isLyingOnTopOfSleepingPlayer()));
         out.add("relax", EntityOracleDriver.f((Float)EntityOracleDriver.priv(e, Cat.class, "relaxStateOneAmount")));
      }));
      ALL.add(new Extra(31, Ocelot.class, (d, e, out) -> out.add("trusting", EntityOracleDriver.bool(e.getEntityData().get((net.minecraft.network.syncher.EntityDataAccessor<Boolean>)EntityOracleDriver.priv(null, Ocelot.class, "DATA_TRUSTING"))))));
      ALL.add(new Extra(31, Parrot.class, (d, e, out) -> {
         Parrot p = (Parrot)e;
         var jukebox = (net.minecraft.core.BlockPos)EntityOracleDriver.priv(e, Parrot.class, "jukebox");
         out.add("variant", p.getVariant().getSerializedName());
         out.add("flap", EntityOracleDriver.f(p.flap)).add("flapspeed", EntityOracleDriver.f(p.flapSpeed));
         out.add("flapping", EntityOracleDriver.f((Float)EntityOracleDriver.priv(e, Parrot.class, "flapping")));
         out.add("nextflap", EntityOracleDriver.f((Float)EntityOracleDriver.priv(e, Parrot.class, "nextFlap")));
         out.add("party", EntityOracleDriver.bool(p.isPartyParrot()));
         out.add("jukebox", jukebox == null ? "-" : EntityOracleDriver.pos(jukebox));
      }));
      ALL.add(new Extra(31, Wolf.class, (d, e, out) -> {
         out.add("variant", holderName(e.get(DataComponents.WOLF_VARIANT))).add("svariant", holderName(e.get(DataComponents.WOLF_SOUND_VARIANT)));
         out.add("collar", e.get(DataComponents.WOLF_COLLAR).getSerializedName());
         out.add("wet", EntityOracleDriver.bool((Boolean)EntityOracleDriver.priv(e, Wolf.class, "isWet")));
         out.add("shaking", EntityOracleDriver.bool((Boolean)EntityOracleDriver.priv(e, Wolf.class, "isShaking")));
         out.add("shake", EntityOracleDriver.f((Float)EntityOracleDriver.priv(e, Wolf.class, "shakeAnim")));
         out.add("interest", EntityOracleDriver.f((Float)EntityOracleDriver.priv(e, Wolf.class, "interestedAngle")));
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
      ALL.add(new Extra(41, SpellcasterIllager.class, (d, e, out) -> {
         out.add("spell", ((Enum<?>)EntityOracleDriver.priv(e, SpellcasterIllager.class, "currentSpell")).name().toLowerCase(java.util.Locale.ROOT));
         out.add("spellticks", EntityOracleDriver.priv(e, SpellcasterIllager.class, "spellCastingTickCount"));
      }));
      ALL.add(new Extra(41, Vex.class, (d, e, out) -> {
         Vex v = (Vex)e;
         var bound = v.getBoundOrigin();
         out.add("charging", EntityOracleDriver.bool(v.isCharging())).add("owner", d.entName(v.getOwner()));
         out.add("bound", bound == null ? "-" : bound.getX() + "," + bound.getY() + "," + bound.getZ());
         out.add("life", (Boolean)EntityOracleDriver.priv(e, Vex.class, "hasLimitedLife") ? EntityOracleDriver.priv(e, Vex.class, "limitedLifeTicks") : "-");
      }));
      ALL.add(new Extra(41, Vindicator.class, (d, e, out) -> out.add("johnny", EntityOracleDriver.bool((Boolean)EntityOracleDriver.priv(e, Vindicator.class, "isJohnny")))));
      ALL.add(new Extra(41, Ravager.class, (d, e, out) -> out.add("attack", EntityOracleDriver.priv(e, Ravager.class, "attackTick")).add("stun", EntityOracleDriver.priv(e, Ravager.class, "stunnedTick")).add("roar", EntityOracleDriver.priv(e, Ravager.class, "roarTick"))));
      ALL.add(new Extra(41, Witch.class, (d, e, out) -> out.add("drinking", EntityOracleDriver.bool(((Witch)e).isDrinkingPotion())).add("usingtime", EntityOracleDriver.priv(e, Witch.class, "usingTime"))));
      ALL.add(new Extra(42, Evoker.class, (d, e, out) -> out.add("wololo", d.entName((Sheep)EntityOracleDriver.priv(e, Evoker.class, "wololoTarget")))));
      ALL.sort(Comparator.comparingInt(Extra::order));
   }

   /** Appends the fields of every registered class `e` is an instance of. */
   static String holderName(net.minecraft.core.Holder<?> h) {
      return h.unwrapKey().get().identifier().toString();
   }

   static void append(EntityOracleDriver d, StringBuilder b, Entity e) {
      Fields out = new Fields(b);
      for (Extra x : ALL) {
         if (x.type().isInstance(e)) {
            x.fill().fill(d, e, out);
         }
      }
   }
}
