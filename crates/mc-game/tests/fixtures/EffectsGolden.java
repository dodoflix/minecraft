import com.mojang.serialization.DataResult;
import java.util.*;
import net.minecraft.core.Holder;
import net.minecraft.core.particles.ParticleOptions;
import net.minecraft.core.particles.ParticleTypes;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.NbtOps;
import net.minecraft.nbt.Tag;
import net.minecraft.nbt.TagParser;
import net.minecraft.resources.Identifier;
import net.minecraft.world.effect.MobEffect;
import net.minecraft.world.effect.MobEffectInstance;
import net.minecraft.world.entity.EntityTypes;
import net.minecraft.world.entity.ai.attributes.*;
import net.minecraft.world.item.alchemy.PotionContents;
import net.minecraft.world.item.alchemy.Potion;

/**
 * Golden values of the effect / attribute data classes (no level needed):
 * Usage: scripts/golden-effects.sh
 * Output: crates/mc-game/tests/fixtures/effects_golden.txt (replayed by crates/mc-game-core/tests/golden/effects_golden.rs)
 * Sections (tab separated): INTERVAL (shouldApplyEffectTickThisTick bitmaps), TEMPLATE (createModifiers), PARTICLE,
 * POTION (getColor), MEI (MobEffectInstance scripts: update/tick/scale/NBT), ATTR (AttributeInstance scripts).
 */
public class EffectsGolden {
   static java.io.PrintStream out;
   static List<Holder.Reference<MobEffect>> effects = new ArrayList<>();

   static String state(MobEffectInstance i) {
      DataResult<Tag> r = MobEffectInstance.CODEC.encodeStart(NbtOps.INSTANCE, i);
      return r.result().get().toString();
   }

   static String bits(double d) {
      return Long.toHexString(Double.doubleToRawLongBits(d));
   }

   public static void main(String[] args) throws Exception {
      net.minecraft.SharedConstants.tryDetectVersion();
      net.minecraft.server.Bootstrap.bootStrap();
      out = new java.io.PrintStream(new java.io.FileOutputStream(args[0]), false, "UTF-8");
      BuiltInRegistries.MOB_EFFECT.listElements().forEach(effects::add);

      // INTERVAL: shouldApplyEffectTickThisTick for amplifier 0..8, tick 0..120 (+ isInstantaneous, category, color)
      for (Holder.Reference<MobEffect> h : effects) {
         MobEffect e = h.value();
         StringBuilder sb = new StringBuilder();
         for (int amp = 0; amp <= 8; amp++) {
            for (int t = 0; t <= 120; t++) {
               sb.append(e.shouldApplyEffectTickThisTick(t, amp) ? '1' : '0');
            }
            sb.append('|');
         }
         out.println("INTERVAL\t" + h.key().identifier() + "\t" + e.isInstantaneous() + "\t" + e.getCategory() + "\t" + e.getColor() + "\t" + sb);
      }

      // TEMPLATE: createModifiers
      for (Holder.Reference<MobEffect> h : effects) {
         for (int amp = 0; amp <= 4; amp++) {
            StringBuilder sb = new StringBuilder();
            h.value().createModifiers(amp, (attribute, m) -> sb.append(attribute.unwrapKey().get().identifier()).append(',').append(m.id()).append(',').append(bits(m.amount())).append(',').append(m.operation().getSerializedName()).append(';'));
            out.println("TEMPLATE\t" + h.key().identifier() + "\t" + amp + "\t" + sb);
         }
      }

      // PARTICLE: the particle options of every effect, ambient or not
      for (Holder.Reference<MobEffect> h : effects) {
         for (boolean ambient : new boolean[] {false, true}) {
            MobEffectInstance i = new MobEffectInstance(h, 100, 0, ambient, true);
            ParticleOptions p = i.getParticleOptions();
            Tag t = ParticleTypes.CODEC.encodeStart(NbtOps.INSTANCE, p).result().get();
            out.println("PARTICLE\t" + h.key().identifier() + "\t" + ambient + "\t" + t);
         }
      }

      // POTION: PotionContents colour and effect list
      BuiltInRegistries.POTION.listElements().forEach(h -> {
         PotionContents c = new PotionContents(h);
         StringBuilder sb = new StringBuilder();
         for (MobEffectInstance i : c.getAllEffects()) sb.append(state(i)).append(';');
         out.println("POTION\t" + h.key().identifier() + "\t" + c.getColor() + "\t" + c.hasEffects() + "\t" + sb);
      });
      for (int s = 0; s < 40; s++) {
         Random r = new Random(1000 + s);
         List<MobEffectInstance> list = new ArrayList<>();
         int n = 1 + r.nextInt(4);
         for (int k = 0; k < n; k++) list.add(randomInstance(r));
         PotionContents c = new PotionContents(Optional.empty(), Optional.empty(), list, Optional.empty());
         StringBuilder sb = new StringBuilder();
         for (MobEffectInstance i : list) sb.append(state(i)).append(';');
         out.println("POTIONCUSTOM\t" + c.getColor() + "\t" + c.getColorOr(7) + "\t" + sb);
      }

      for (int s = 0; s < 400; s++) meiScript(s);
      for (int s = 0; s < 120; s++) attrScript(s);
      out.close();
   }

   static final int[] DURATIONS = {1, 2, 3, 5, 20, 40, 100, 600, 1200, 9600, 32147, 40000, -1};

   static MobEffectInstance randomInstance(Random r) {
      Holder.Reference<MobEffect> e = effects.get(r.nextInt(effects.size()));
      return new MobEffectInstance(e, DURATIONS[r.nextInt(DURATIONS.length)], r.nextInt(4), r.nextInt(4) == 0, r.nextBoolean(), r.nextBoolean());
   }

   static void meiScript(int seed) {
      Random r = new Random(seed * 7919L + 1);
      Holder.Reference<MobEffect> e = effects.get(r.nextInt(effects.size()));
      MobEffectInstance cur = new MobEffectInstance(e, DURATIONS[r.nextInt(DURATIONS.length)], r.nextInt(4), r.nextInt(4) == 0, r.nextBoolean(), r.nextBoolean());
      out.println("MEI\t" + seed + "\t" + e.key().identifier() + "\t" + state(cur));
      int ops = 4 + r.nextInt(10);
      for (int k = 0; k < ops; k++) {
         int op = r.nextInt(6);
         if (op <= 1) {
            MobEffectInstance other = new MobEffectInstance(e, DURATIONS[r.nextInt(DURATIONS.length)], r.nextInt(4), r.nextInt(4) == 0, r.nextBoolean(), r.nextBoolean());
            boolean changed = cur.update(other);
            out.println("UPDATE\t" + state(other) + "\t" + changed + "\t" + state(cur));
         } else if (op <= 3) {
            int n = 1 + r.nextInt(60);
            for (int t = 0; t < n; t++) cur.tickClient();
            out.println("TICK\t" + n + "\t" + state(cur));
         } else if (op == 4) {
            float scale = new float[] {0.0f, 0.25f, 0.5f, 1.0f, 1.5f, 8.0f, 0.001f}[r.nextInt(7)];
            MobEffectInstance sc = cur.withScaledDuration(scale);
            out.println("SCALE\t" + Float.floatToRawIntBits(scale) + "\t" + state(sc));
         } else {
            MobEffectInstance other = randomInstance(r);
            int cmp = Integer.signum(cur.compareTo(other));
            int within = new int[] {0, 1, 20, 100, 600}[r.nextInt(5)];
            out.println("MISC\t" + state(other) + "\t" + cmp + "\t" + cur.equals(other) + "\t" + within + "\t" + cur.endsWithin(within) + "\t" + cur.isInfiniteDuration());
         }
      }
   }

   static void attrScript(int seed) {
      Random r = new Random(seed * 104729L + 3);
      Holder<Attribute> attribute = new Holder[] {Attributes.MOVEMENT_SPEED, Attributes.ATTACK_DAMAGE, Attributes.MAX_HEALTH, Attributes.ARMOR, Attributes.LUCK}[r.nextInt(5)];
      AttributeMap map = new AttributeMap(DefaultAttributes.getSupplier(EntityTypes.PLAYER));
      AttributeInstance inst = map.getInstance(attribute);
      int poolSize = new int[] {3, 8, 20, 40}[r.nextInt(4)];
      out.println("ATTR\t" + seed + "\t" + attribute.unwrapKey().get().identifier() + "\t" + poolSize + "\t" + bits(inst.getBaseValue()) + "\t" + bits(inst.getValue()));
      AttributeModifier.Operation[] ops = AttributeModifier.Operation.values();
      int n = 6 + r.nextInt(60);
      for (int k = 0; k < n; k++) {
         Identifier id = Identifier.withDefaultNamespace("m" + r.nextInt(poolSize));
         int op = r.nextInt(8);
         String desc;
         try {
            if (op <= 2) {
               AttributeModifier m = new AttributeModifier(id, (r.nextInt(2001) - 1000) / 100.0, ops[r.nextInt(3)]);
               desc = "add_transient\t" + id + "\t" + bits(m.amount()) + "\t" + m.operation().getSerializedName();
               inst.addTransientModifier(m);
            } else if (op == 3) {
               AttributeModifier m = new AttributeModifier(id, (r.nextInt(2001) - 1000) / 1000.0, ops[r.nextInt(3)]);
               desc = "add_permanent\t" + id + "\t" + bits(m.amount()) + "\t" + m.operation().getSerializedName();
               inst.addPermanentModifier(m);
            } else if (op == 4) {
               AttributeModifier m = new AttributeModifier(id, (r.nextInt(2001) - 1000) / 1000.0, ops[r.nextInt(3)]);
               desc = "replace_permanent\t" + id + "\t" + bits(m.amount()) + "\t" + m.operation().getSerializedName();
               inst.addOrReplacePermanentModifier(m);
            } else if (op == 5) {
               desc = "remove\t" + id;
               inst.removeModifier(id);
            } else if (op == 6) {
               double base = (r.nextInt(400) - 100) / 10.0;
               desc = "base\t" + bits(base);
               inst.setBaseValue(base);
            } else {
               desc = "remove_all";
               inst.removeModifiers();
            }
         } catch (IllegalArgumentException ex) {
            desc = "dup\t" + id;
         }
         StringBuilder mods = new StringBuilder();
         for (AttributeModifier m : inst.getModifiers()) mods.append(m.id()).append(':').append(bits(m.amount())).append(':').append(m.operation().ordinal()).append(',');
         StringBuilder perm = new StringBuilder();
         for (AttributeModifier m : inst.getPermanentModifiers()) perm.append(m.id()).append(',');
         StringBuilder ord = new StringBuilder();
         try {
            java.lang.reflect.Method gm = AttributeInstance.class.getDeclaredMethod("getModifiers", AttributeModifier.Operation.class);
            gm.setAccessible(true);
            for (AttributeModifier.Operation o : ops) {
               for (AttributeModifier m : ((Map<Identifier, AttributeModifier>) gm.invoke(inst, o)).values()) ord.append(m.id().getPath()).append(',');
               ord.append('|');
            }
         } catch (ReflectiveOperationException ex) {
            throw new RuntimeException(ex);
         }
         out.println("OP\t" + desc + "\t=\t" + bits(inst.getValue()) + "\t" + mods + "\t" + perm + "\t" + ord);
      }
      // replaceFrom into a fresh instance (re-inserts per-operation maps in iteration order)
      AttributeMap other = new AttributeMap(DefaultAttributes.getSupplier(EntityTypes.PLAYER));
      other.assignAllValues(map);
      AttributeInstance copy = other.getInstance(attribute);
      StringBuilder cm = new StringBuilder();
      for (AttributeModifier m : copy.getModifiers()) cm.append(m.id()).append(',');
      out.println("COPY\t" + bits(copy.getValue()) + "\t" + bits(copy.getBaseValue()) + "\t" + cm);
   }
}
