import java.io.PrintStream;
import java.util.*;
import java.util.function.Supplier;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.util.RandomSource;
import net.minecraft.world.damagesource.DamageSource;
import net.minecraft.world.damagesource.DamageSources;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntitySpawnReason;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.EquipmentSlot;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.enchantment.Enchantment;
import net.minecraft.world.item.enchantment.EnchantmentHelper;
import net.minecraft.world.item.enchantment.EnchantmentInstance;
import net.minecraft.world.level.GameType;
import net.minecraft.core.component.DataComponentType;
import net.minecraft.world.item.enchantment.EnchantmentEffectComponents;
import net.minecraft.world.level.storage.loot.LootContext;

/**
 * Enchantment golden generator (EnchantmentHelper on the real vanilla server): enchanting table maths, protection / immunity per
 * damage source, weapon damage / knockback / armor effectiveness / fall-based damage per victim type, durability (unbreaking) RNG
 * sequences, mending / block xp, equipment attribute modifiers, has / hasTag.
 * Usage: scripts/entity-oracle.sh crates/mc-game/tests/oracle/EnchantOracle.java -o crates/mc-game/tests/fixtures/enchant_golden.txt
 * Output: crates/mc-game/tests/fixtures/enchant_golden.txt
 */
public final class EnchantOracle {
   static PrintStream out;
   static VanillaLevel v;
   static ServerLevel level;
   static net.minecraft.core.RegistryAccess access;
   static net.minecraft.core.HolderLookup.RegistryLookup<Enchantment> enchants;

   static Holder<Enchantment> h(String name) {
      return enchants.getOrThrow(ResourceKey.create(Registries.ENCHANTMENT, Identifier.withDefaultNamespace(name)));
   }

   static List<String> enchantNames() {
      List<String> names = new ArrayList<>();
      enchants.listElementIds().forEach(k -> names.add(k.identifier().getPath()));
      Collections.sort(names);
      return names;
   }

   static ItemStack with(Item item, String enchant, int lvl) {
      ItemStack s = new ItemStack(item);
      if (enchant != null) s.enchant(h(enchant), lvl);
      return s;
   }

   static String bits(float f) {
      return Integer.toString(Float.floatToIntBits(f));
   }

   public static void main(String[] args) throws Exception {
      out = VanillaLevel.OUT;
      v = VanillaLevel.boot(new VanillaLevel.Options());
      level = v.level;
      access = v.server.registryAccess();
      enchants = access.lookupOrThrow(Registries.ENCHANTMENT);
      table();
      protection();
      weapons();
      durability();
      xp();
      attributes();
      has();
      out.flush();
      System.exit(0);
   }

   static String render(List<EnchantmentInstance> list) {
      List<String> parts = new ArrayList<>();
      for (EnchantmentInstance e : list) parts.add(e.enchantment().unwrapKey().get().identifier().getPath() + ":" + e.level());
      return String.join(",", parts);
   }

   // ---- enchanting table ---------------------------------------------------------------------------------------------
   static void table() {
      Item[] items = {Items.IRON_SWORD, Items.DIAMOND_PICKAXE, Items.BOW, Items.CROSSBOW, Items.TRIDENT, Items.MACE, Items.IRON_HELMET, Items.DIAMOND_BOOTS, Items.IRON_CHESTPLATE,
         Items.BOOK, Items.FISHING_ROD, Items.GOLDEN_AXE, Items.ELYTRA, Items.SHIELD, Items.WOODEN_SPEAR, Items.NETHERITE_LEGGINGS};
      for (Item item : items) {
         String name = BuiltInRegistries.ITEM.getKey(item).getPath();
         ItemStack stack = new ItemStack(item);
         for (int cost = 1; cost <= 60; cost++) {
            out.println("AV " + name + " " + cost + " " + render(EnchantmentHelper.getAvailableEnchantmentResults(cost, stack, enchants.listElements().map(e -> (Holder<Enchantment>)e))));
         }
         for (long seed = 1; seed <= 12; seed++) {
            for (int books = 0; books <= 15; books += 3) {
               StringBuilder sb = new StringBuilder();
               for (int slot = 0; slot < 3; slot++) {
                  RandomSource r = RandomSource.create(seed + slot);
                  sb.append(EnchantmentHelper.getEnchantmentCost(r, slot, books, stack)).append(' ');
               }
               out.println("COST " + name + " " + seed + " " + books + " " + sb.toString().trim());
            }
            for (int cost : new int[] {1, 5, 10, 15, 20, 30, 40, 50}) {
               RandomSource r = RandomSource.create(seed);
               ItemStack res = EnchantmentHelper.enchantItem(r, stack.copy(), cost, access, Optional.empty());
               List<String> parts = new ArrayList<>();
               EnchantmentHelper.getEnchantmentsForCrafting(res).entrySet().forEach(e -> parts.add(e.getKey().unwrapKey().get().identifier().getPath() + ":" + e.getIntValue()));
               Collections.sort(parts);
               out.println("ENCH " + name + " " + seed + " " + cost + " " + BuiltInRegistries.ITEM.getKey(res.getItem()).getPath() + " " + String.join(",", parts) + " next=" + r.nextInt());
            }
         }
      }
   }

   // ---- damage sources -------------------------------------------------------------------------------------------------
   static Map<String, java.util.function.Function<DamageSources, DamageSource>> sources() {
      Map<String, java.util.function.Function<DamageSources, DamageSource>> m = new LinkedHashMap<>();
      m.put("generic", DamageSources::generic);
      m.put("in_fire", DamageSources::inFire);
      m.put("on_fire", DamageSources::onFire);
      m.put("lava", DamageSources::lava);
      m.put("hot_floor", DamageSources::hotFloor);
      m.put("cactus", DamageSources::cactus);
      m.put("fall", DamageSources::fall);
      m.put("drown", DamageSources::drown);
      m.put("starve", DamageSources::starve);
      m.put("freeze", DamageSources::freeze);
      m.put("magic", DamageSources::magic);
      m.put("lightning_bolt", DamageSources::lightningBolt);
      m.put("fell_out_of_world", DamageSources::fellOutOfWorld);
      m.put("wither", DamageSources::wither);
      m.put("dragon_breath", DamageSources::dragonBreath);
      m.put("sweet_berry_bush", DamageSources::sweetBerryBush);
      m.put("stalagmite", DamageSources::stalagmite);
      m.put("explosion", s -> s.explosion(null, null));
      return m;
   }

   static void protection() throws Exception {
      ServerPlayer p = v.connect("Victim", GameType.SURVIVAL, 4.5, -60, 0.5, 0, 0, false).player;
      Item[] armor = {Items.IRON_HELMET, Items.IRON_CHESTPLATE, Items.IRON_LEGGINGS, Items.IRON_BOOTS};
      EquipmentSlot[] slots = {EquipmentSlot.HEAD, EquipmentSlot.CHEST, EquipmentSlot.LEGS, EquipmentSlot.FEET};
      for (String name : enchantNames()) {
         int max = h(name).value().getMaxLevel();
         for (int lvl = 1; lvl <= max; lvl++) {
            for (int i = 0; i < 4; i++) p.setItemSlot(slots[i], with(armor[i], name, lvl));
            for (var e : sources().entrySet()) {
               DamageSource src = e.getValue().apply(level.damageSources());
               out.println("PROT " + name + " " + lvl + " " + e.getKey() + " " + bits(EnchantmentHelper.getDamageProtection(level, p, src)) + " imm="
                  + EnchantmentHelper.isImmuneToDamage(level, p, src));
            }
         }
      }
      for (int i = 0; i < 4; i++) p.setItemSlot(slots[i], ItemStack.EMPTY);
   }

   // ---- weapons --------------------------------------------------------------------------------------------------------
   static void weapons() throws Exception {
      ServerPlayer attacker = v.connect("Attacker", GameType.SURVIVAL, 8.5, -60, 0.5, 0, 0, false).player;
      EntityType<?>[] types = {net.minecraft.world.entity.EntityTypes.ZOMBIE, net.minecraft.world.entity.EntityTypes.SPIDER, net.minecraft.world.entity.EntityTypes.PIG, net.minecraft.world.entity.EntityTypes.DROWNED, net.minecraft.world.entity.EntityTypes.GUARDIAN, net.minecraft.world.entity.EntityTypes.BLAZE, net.minecraft.world.entity.EntityTypes.CREEPER};
      Item[] weapons = {Items.DIAMOND_SWORD, Items.MACE, Items.TRIDENT, Items.DIAMOND_AXE};
      for (String name : enchantNames()) {
         int max = h(name).value().getMaxLevel();
         for (int lvl = 1; lvl <= max; lvl++) {
            for (Item w : weapons) {
               ItemStack weapon = with(w, name, lvl);
               attacker.setItemSlot(EquipmentSlot.MAINHAND, weapon);
               for (EntityType<?> t : types) {
                  Entity victim = t.create(level, EntitySpawnReason.COMMAND);
                  victim.snapTo(5.5, -60, 0.5, 0, 0);
                  for (float fall : new float[] {0f, 6f}) {
                     attacker.fallDistance = fall;
                     DamageSource src = level.damageSources().playerAttack(attacker);
                     out.println("DMG " + name + " " + lvl + " " + BuiltInRegistries.ITEM.getKey(w).getPath() + " " + BuiltInRegistries.ENTITY_TYPE.getKey(t).getPath() + " " + (int)fall
                        + " " + bits(EnchantmentHelper.modifyDamage(level, weapon, victim, src, 10.0F))
                        + " " + bits(EnchantmentHelper.modifyKnockback(level, weapon, victim, src, 0.4F))
                        + " " + bits(EnchantmentHelper.modifyArmorEffectiveness(level, weapon, victim, src, 1.0F))
                        + " " + bits(EnchantmentHelper.modifyFallBasedDamage(level, weapon, victim, src, 1.0F)));
                  }
                  victim.discard();
               }
            }
         }
      }
      attacker.fallDistance = 0;
      attacker.setItemSlot(EquipmentSlot.MAINHAND, ItemStack.EMPTY);
   }

   // ---- durability ---------------------------------------------------------------------------------------------------------
   static void durability() {
      Item[] items = {Items.DIAMOND_PICKAXE, Items.IRON_CHESTPLATE, Items.SHIELD};
      for (String name : new String[] {"unbreaking", "mending", "efficiency"}) {
         int max = h(name).value().getMaxLevel();
         for (int lvl = 1; lvl <= max; lvl++) {
            for (Item item : items) {
               ItemStack stack = with(item, name, lvl);
               for (long seed : new long[] {1, 2, 3}) {
                  level.getRandom().setSeed(seed);
                  StringBuilder sb = new StringBuilder();
                  for (int i = 0; i < 40; i++) sb.append(EnchantmentHelper.processDurabilityChange(level, stack, i % 5 == 4 ? 3 : 1)).append(',');
                  out.println("DUR " + name + " " + lvl + " " + BuiltInRegistries.ITEM.getKey(item).getPath() + " " + seed + " " + sb + " next=" + level.getRandom().nextInt());
               }
            }
         }
      }
   }

   // ---- xp --------------------------------------------------------------------------------------------------------------------
   static void xp() {
      for (String name : enchantNames()) {
         int max = h(name).value().getMaxLevel();
         for (int lvl = 1; lvl <= max; lvl++) {
            ItemStack stack = with(Items.DIAMOND_PICKAXE, name, lvl);
            StringBuilder sb = new StringBuilder();
            for (int amount : new int[] {1, 2, 5, 17, 100}) {
               level.getRandom().setSeed(7);
               sb.append(EnchantmentHelper.modifyDurabilityToRepairFromXp(level, stack, amount)).append('/').append(EnchantmentHelper.processBlockExperience(level, stack, amount)).append('/')
                  .append(EnchantmentHelper.processAmmoUse(level, stack, new ItemStack(Items.ARROW), amount)).append('/').append(EnchantmentHelper.getPiercingCount(level, stack, new ItemStack(Items.ARROW))).append(' ');
            }
            out.println("XP " + name + " " + lvl + " " + sb.toString().trim());
         }
      }
   }

   // ---- attribute modifiers ----------------------------------------------------------------------------------------------------
   static void attributes() {
      for (String name : enchantNames()) {
         int max = h(name).value().getMaxLevel();
         for (int lvl = 1; lvl <= max; lvl++) {
            ItemStack stack = with(Items.IRON_BOOTS, name, lvl);
            for (EquipmentSlot slot : EquipmentSlot.VALUES) {
               List<String> parts = new ArrayList<>();
               EnchantmentHelper.forEachModifier(stack, slot, (attr, mod) -> parts.add(attr.unwrapKey().get().identifier() + "|" + mod.id() + "|" + Double.doubleToLongBits(mod.amount()) + "|" + mod.operation().getSerializedName()));
               if (!parts.isEmpty()) out.println("ATTR " + name + " " + lvl + " " + slot.getName() + " " + String.join(";", parts));
            }
         }
      }
   }

   // ---- has / hasTag -------------------------------------------------------------------------------------------------------------
   static void has() throws Exception {
      List<DataComponentType<?>> comps = new ArrayList<>();
      BuiltInRegistries.ENCHANTMENT_EFFECT_COMPONENT_TYPE.forEach(comps::add);
      List<java.lang.reflect.Field> tagFields = new ArrayList<>(Arrays.asList(net.minecraft.tags.EnchantmentTags.class.getFields()));
      for (String name : enchantNames()) {
         ItemStack stack = with(Items.DIAMOND_SWORD, name, 1);
         List<String> parts = new ArrayList<>();
         for (DataComponentType<?> c : comps) if (EnchantmentHelper.has(stack, c)) parts.add(BuiltInRegistries.ENCHANTMENT_EFFECT_COMPONENT_TYPE.getKey(c).getPath());
         Collections.sort(parts);
         List<String> tags = new ArrayList<>();
         for (var f : tagFields) {
            @SuppressWarnings("unchecked") net.minecraft.tags.TagKey<Enchantment> tag = (net.minecraft.tags.TagKey<Enchantment>)f.get(null);
            if (EnchantmentHelper.hasTag(stack, tag)) tags.add(tag.location().getPath());
         }
         Collections.sort(tags);
         out.println("HAS " + name + " " + String.join(",", parts) + " | " + String.join(",", tags));
      }
   }
}
