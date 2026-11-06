import com.google.common.collect.ImmutableList;
import com.google.common.collect.ImmutableMap;
import com.google.common.collect.ImmutableSet;
import com.mojang.datafixers.util.Pair;
import java.lang.reflect.Field;
import java.util.*;
import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.util.RandomSource;
import net.minecraft.world.entity.EntitySpawnReason;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.ai.Brain;
import net.minecraft.world.entity.ai.ActivityData;
import net.minecraft.world.entity.ai.behavior.*;
import net.minecraft.world.entity.ai.memory.*;
import net.minecraft.world.entity.ai.sensing.Sensor;
import net.minecraft.world.entity.ai.sensing.SensorType;
import net.minecraft.world.entity.schedule.Activity;
import net.minecraft.world.level.levelgen.LegacyRandomSource;
import net.minecraft.world.level.storage.ServerLevelData;

/**
 * Brain oracle: a vanilla Brain with fake behaviors (log start/tick/stop) ticked on a real level with a seeded Level.random, plus the
 * sensor scan-rate pattern.
 * Usage: scripts/entity-oracle.sh crates/mc-game/tests/oracle/BrainOracle.java -o crates/mc-game-core/tests/fixtures/brain/brain.txt
 * Output: crates/mc-game-core/tests/fixtures/brain/brain.txt
 */
public class BrainOracle {
   static final List<String> LOG = new ArrayList<>();
   static long now;
   static ServerLevel level;

   static class Fake extends Behavior<LivingEntity> {
      final String name; final int runFor; final int period; int calls = 0; int ran = 0; final MemoryModuleType<Boolean> setOnStart; final MemoryModuleType<?> eraseOnStop;
      Fake(String name, Map<MemoryModuleType<?>, MemoryStatus> cond, int min, int max, int runFor, int period, MemoryModuleType<Boolean> setOnStart, MemoryModuleType<?> eraseOnStop) {
         super(cond, min, max);
         this.name = name; this.runFor = runFor; this.period = period; this.setOnStart = setOnStart; this.eraseOnStop = eraseOnStop;
      }
      protected boolean checkExtraStartConditions(ServerLevel l, LivingEntity body) { return calls++ % period == 0; }
      protected void start(ServerLevel l, LivingEntity body, long ts) { ran = 0; LOG.add(now + " " + name + " start"); if (setOnStart != null) body.getBrain().setMemoryWithExpiry(setOnStart, true, 4L); }
      protected boolean canStillUse(ServerLevel l, LivingEntity body, long ts) { return ran < runFor; }
      protected void tick(ServerLevel l, LivingEntity body, long ts) { ran++; LOG.add(now + " " + name + " tick"); }
      protected void stop(ServerLevel l, LivingEntity body, long ts) { LOG.add(now + " " + name + " stop"); if (eraseOnStop != null) body.getBrain().eraseMemory(eraseOnStop); }
   }

   static class TestBrain extends Brain<LivingEntity> {
      TestBrain(List<ActivityData<LivingEntity>> activities, RandomSource random) {
         super(List.of(MemoryModuleType.WALK_TARGET, MemoryModuleType.LOOK_TARGET, MemoryModuleType.ATTACK_COOLING_DOWN, MemoryModuleType.LONG_JUMP_COOLDOWN_TICKS,
            MemoryModuleType.HEARD_BELL_TIME, MemoryModuleType.IS_PANICKING), List.of(SensorType.DUMMY, SensorType.DUMMY), activities, MemoryMap.EMPTY, random);
      }
   }

   static Map<MemoryModuleType<?>, MemoryStatus> cond(Object... kv) {
      Map<MemoryModuleType<?>, MemoryStatus> m = new LinkedHashMap<>();
      for (int i = 0; i < kv.length; i += 2) m.put((MemoryModuleType<?>) kv[i], (MemoryStatus) kv[i + 1]);
      return m;
   }

   @SuppressWarnings({"unchecked", "rawtypes"})
   static ActivityData<LivingEntity> act(Activity a, Set<Pair<MemoryModuleType<?>, MemoryStatus>> conditions, Set<MemoryModuleType<?>> erase, Object... prioBehavior) {
      ImmutableList.Builder<Pair<Integer, ? extends BehaviorControl<? super LivingEntity>>> b = ImmutableList.builder();
      for (int i = 0; i < prioBehavior.length; i += 2) b.add(Pair.of((Integer) prioBehavior[i], (BehaviorControl<? super LivingEntity>) prioBehavior[i + 1]));
      return ActivityData.create(a, b.build(), conditions, erase);
   }

   static void reseedShuffle(GateBehavior<?> gate, long seed) throws Exception {
      Field f = GateBehavior.class.getDeclaredField("behaviors"); f.setAccessible(true);
      Object list = f.get(gate);
      Field r = ShufflingList.class.getDeclaredField("random"); r.setAccessible(true);
      sun.misc.Unsafe u; Field uf = sun.misc.Unsafe.class.getDeclaredField("theUnsafe"); uf.setAccessible(true); u = (sun.misc.Unsafe) uf.get(null);
      u.putObject(list, u.objectFieldOffset(r), new LegacyRandomSource(seed));
   }

   public static void main(String[] args) throws Exception {
      java.io.PrintStream out = VanillaLevel.OUT;
      VanillaLevel.Options o = new VanillaLevel.Options();
      VanillaLevel vl = VanillaLevel.boot(o);
      level = vl.level;
      level.getRandom().setSeed(424242L);
      LivingEntity pig = (LivingEntity) net.minecraft.world.entity.EntityTypes.PIG.create(level, EntitySpawnReason.COMMAND);
      LegacyRandomSource brainRandom = new LegacyRandomSource(99L);

      Fake coreA = new Fake("core_a", cond(), 3, 5, 4, 1, null, null);
      RunOne<LivingEntity> runOne = new RunOne<>(ImmutableList.of(
         Pair.of(new Fake("gate_a", cond(), 2, 4, 2, 1, null, null), 1),
         Pair.of(new Fake("gate_b", cond(), 2, 6, 3, 2, null, null), 2),
         Pair.of(new Fake("gate_c", cond(), 1, 3, 1, 1, null, null), 3)));
      reseedShuffle(runOne, 777L);
      Fake idleA = new Fake("idle_a", cond(), 4, 4, 5, 1, null, null);
      Fake idleB = new Fake("idle_b", cond(MemoryModuleType.LOOK_TARGET, MemoryStatus.VALUE_PRESENT), 2, 7, 3, 3, null, null);
      Fake workA = new Fake("work_a", cond(MemoryModuleType.ATTACK_COOLING_DOWN, MemoryStatus.VALUE_ABSENT), 3, 9, 6, 1, MemoryModuleType.ATTACK_COOLING_DOWN, null);
      Fake panicA = new Fake("panic_a", cond(), 2, 2, 8, 1, null, MemoryModuleType.IS_PANICKING);
      Fake panicB = new Fake("panic_b", cond(MemoryModuleType.IS_PANICKING, MemoryStatus.VALUE_PRESENT), 1, 5, 2, 1, null, null);
      Fake meetA = new Fake("meet_a", cond(), 3, 3, 3, 1, null, null);

      List<ActivityData<LivingEntity>> activities = new ArrayList<>();
      activities.add(act(Activity.CORE, ImmutableSet.of(), new HashSet<>(),
         0, coreA, 1, new DoNothing(2, 6), 2, SetWalkTargetFromLookTarget.create(1.0F, 2), 3, runOne));
      activities.add(act(Activity.IDLE, ImmutableSet.of(), new HashSet<>(), 10, idleA, 10, idleB));
      activities.add(act(Activity.WORK, ImmutableSet.of(Pair.of(MemoryModuleType.HEARD_BELL_TIME, MemoryStatus.VALUE_ABSENT)), new HashSet<>(), 10, workA));
      activities.add(act(Activity.PANIC, ImmutableSet.of(Pair.of(MemoryModuleType.IS_PANICKING, MemoryStatus.VALUE_PRESENT)), ImmutableSet.of(MemoryModuleType.IS_PANICKING), 4, panicA, 10, panicB));
      activities.add(act(Activity.MEET, ImmutableSet.of(), new HashSet<>(), 10, meetA));
      TestBrain brain = new TestBrain(activities, brainRandom);
      Field bf = LivingEntity.class.getDeclaredField("brain"); bf.setAccessible(true);
      bf.set(pig, brain);
      out.println("brain_random_after_construct " + brainRandom.nextInt(1000000));

      Brain<LivingEntity> b = (Brain<LivingEntity>) pig.getBrain();
      for (int t = 1; t <= 70; t++) {
         now = 1000 + t;
         ((ServerLevelData) level.getLevelData()).setGameTime(now);
         LOG.clear();
         switch (t) {
            case 3 -> b.setMemory(MemoryModuleType.LOOK_TARGET, new BlockPosTracker(new BlockPos(1, 2, 3)));
            case 5 -> b.setMemoryWithExpiry(MemoryModuleType.ATTACK_COOLING_DOWN, true, 6L);
            case 9 -> { b.setMemory(MemoryModuleType.IS_PANICKING, true); b.setActiveActivityIfPossible(Activity.PANIC); }
            case 15 -> b.setActiveActivityIfPossible(Activity.WORK);
            case 20 -> b.useDefaultActivity();
            case 25 -> b.setActiveActivityToFirstValid(List.of(Activity.PANIC, Activity.MEET, Activity.WORK));
            case 30 -> b.setMemoryWithExpiry(MemoryModuleType.LONG_JUMP_COOLDOWN_TICKS, 7, 3L);
            case 33 -> b.setMemory(MemoryModuleType.HEARD_BELL_TIME, 5L);
            case 34 -> b.setActiveActivityIfPossible(Activity.WORK);
            case 40 -> { b.setDefaultActivity(Activity.MEET); b.useDefaultActivity(); }
            case 50 -> b.eraseMemory(MemoryModuleType.LOOK_TARGET);
            case 55 -> b.stopAll(level, pig);
            default -> {}
         }
         b.tick(level, pig);
         StringBuilder sb = new StringBuilder();
         for (String l : LOG) out.println("ev " + l);
         List<String> act = new ArrayList<>();
         for (Activity a : b.getActiveActivities()) act.add(a.getName());
         Collections.sort(act);
         out.println("tick " + t + " active=" + act + " walk=" + b.hasMemoryValue(MemoryModuleType.WALK_TARGET) + " look=" + b.hasMemoryValue(MemoryModuleType.LOOK_TARGET)
            + " cool=" + b.hasMemoryValue(MemoryModuleType.ATTACK_COOLING_DOWN) + (b.hasMemoryValue(MemoryModuleType.ATTACK_COOLING_DOWN) ? "/" + b.getTimeUntilExpiry(MemoryModuleType.ATTACK_COOLING_DOWN) : "")
            + " jump=" + b.getMemory(MemoryModuleType.LONG_JUMP_COOLDOWN_TICKS).map(Object::toString).orElse("-") + " panic=" + b.hasMemoryValue(MemoryModuleType.IS_PANICKING)
            + " bell=" + b.hasMemoryValue(MemoryModuleType.HEARD_BELL_TIME) + " rand=" + level.getRandom().nextInt(1000000));
      }

      // sensor scan pattern: ticks (1-based) at which doTick fires
      for (int rate : new int[] {1, 2, 5, 20, 40, 200}) {
         for (long seed : new long[] {1L, 42L, 987654321L}) {
            int[] fired = new int[1];
            List<Integer> ticks = new ArrayList<>();
            int[] tk = new int[1];
            Sensor<LivingEntity> s = new Sensor<>(rate) {
               protected void doTick(ServerLevel l, LivingEntity body) { ticks.add(tk[0]); }
               public Set<MemoryModuleType<?>> requires() { return Set.of(); }
            };
            LegacyRandomSource r = new LegacyRandomSource(seed);
            s.randomlyDelayStart(r);
            for (int t = 1; t <= 500; t++) { tk[0] = t; s.tick(level, pig); }
            out.println("sensor " + rate + " " + seed + " next=" + r.nextInt(1000) + " " + ticks.subList(0, Math.min(ticks.size(), 12)));
         }
      }
      System.exit(0);
   }
}
