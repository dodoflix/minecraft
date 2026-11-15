import com.google.gson.JsonElement;
import com.google.gson.JsonParser;
import com.mojang.serialization.Codec;
import com.mojang.serialization.DataResult;
import com.mojang.serialization.JsonOps;
import java.io.PrintStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.*;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.registries.Registries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.NbtOps;
import net.minecraft.nbt.Tag;
import net.minecraft.nbt.TagParser;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.RegistryOps;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.util.ProblemReporter;
import net.minecraft.util.context.ContextKeySet;
import net.minecraft.world.damagesource.DamageSource;
import net.minecraft.world.entity.EquipmentSlot;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.item.enchantment.Enchantments;
import net.minecraft.world.item.slot.SlotCollection;
import net.minecraft.world.item.slot.SlotSource;
import net.minecraft.world.item.slot.SlotSources;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.ChestBlock;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.ChestType;
import net.minecraft.world.level.storage.loot.LootContext;
import net.minecraft.world.level.storage.loot.LootParams;
import net.minecraft.world.level.storage.loot.LootTable;
import net.minecraft.world.level.storage.loot.Validatable;
import net.minecraft.world.level.storage.loot.ValidationContext;
import net.minecraft.world.level.storage.loot.ValidationContextSource;
import net.minecraft.world.level.storage.loot.functions.LootItemFunction;
import net.minecraft.world.level.storage.loot.functions.LootItemFunctions;
import net.minecraft.world.level.storage.loot.parameters.LootContextParamSets;
import net.minecraft.world.level.storage.loot.parameters.LootContextParams;
import net.minecraft.world.level.storage.loot.predicates.LootItemCondition;
import net.minecraft.world.level.storage.loot.providers.number.floats.ContextFloatProvider;
import net.minecraft.world.level.storage.loot.providers.number.floats.ContextFloatProviders;
import net.minecraft.world.level.storage.loot.providers.number.ints.ContextIntProvider;
import net.minecraft.world.level.storage.loot.providers.number.ints.ContextIntProviders;
import net.minecraft.world.phys.Vec3;
import net.minecraft.world.scores.Objective;
import net.minecraft.world.scores.criteria.ObjectiveCriteria;

/**
 * Loot engine golden over synthetic cases (everything the vanilla data never uses): the real vanilla codecs decode one JSON
 * definition per case, validate it and roll/apply/evaluate it. Cases are read from the files named on the command line.
 *
 * <p>Case line: {@code kind || id || profile || input || json}. {@code kind}: {@code table} (LootTable.DIRECT_CODEC, rolled with
 * seeds 1, 2, 3), {@code modifier} (LootItemFunctions.DIRECT_CODEC applied to the stack {@code input}), {@code predicate}
 * (LootItemCondition.DIRECT_CODEC), {@code int} / {@code float} (context number providers), {@code slots}
 * (SlotSources.DIRECT_CODEC over the profile's container). {@code input}: SNBT of an {@code ItemStack.OPTIONAL_CODEC} value or
 * {@code -}. {@code profile}: {@code empty} (no parameters), {@code origin}, {@code command} (origin + this_entity), {@code block}
 * (block state, tool, block entity, explosion radius), {@code kill} (entity set with a player attacker holding a looting sword),
 * {@code chest} (luck 2), {@code enchloc} (enchantment level 2, active), {@code enchitem} (tool + enchantment level 3),
 * {@code slots} (command slot source set, the player as container). The world has two players, Looter (0.5, 64, 0.5) and Victim,
 * the objective {@code synth} (Looter 5, Victim 12) and the command storage {@code synth:data}.
 *
 * <p>Output per case: {@code C} and the case line (the fixture is self-contained), then {@code E message} when the codec rejects the JSON, otherwise {@code V set path | problem}
 * for each validation problem and, per seed {@code S seed}, {@code I snbt} (stacks) / {@code B bool} / {@code N unsafe safe};
 * {@code X class} ends a case whose evaluation threw (a case should avoid that).
 *
 * <p>Usage: scripts/loot-synthetic-oracle.sh
 * Output: crates/mc-game/tests/fixtures/loot_synthetic.txt.gz
 */
public final class LootSyntheticOracle {
   static VanillaLevel v;
   static PrintStream out;
   static RegistryOps<JsonElement> jsonOps;
   static RegistryOps<Tag> nbtOps;
   static ServerPlayer looter;
   static ServerPlayer victim;
   static BlockEntity chestEntity;
   static BlockState chestState;
   static ItemStack pickaxe;
   static final BlockPos POS = new BlockPos(0, 64, 0);
   static final long[] SEEDS = {1L, 2L, 3L};

   public static void main(String[] args) {
      try {
         main0(args);
      } catch (Throwable t) {
         VanillaLevel.ERR.println("oracle failed: " + t.getClass() + " " + t.getMessage());
         for (var e : t.getStackTrace()) VanillaLevel.ERR.println("  at " + e);
         Runtime.getRuntime().halt(1);
      }
   }

   static void main0(String[] args) throws Exception {
      out = VanillaLevel.OUT;
      VanillaLevel.Options options = new VanillaLevel.Options();
      options.levelType = "void";
      v = VanillaLevel.boot(options);
      var access = v.server.registryAccess();
      jsonOps = RegistryOps.create(JsonOps.INSTANCE, v.server.reloadableRegistries().lookup());
      nbtOps = access.createSerializationContext(NbtOps.INSTANCE);
      looter = v.connect("Looter", GameType.SURVIVAL, 0.5, 64, 0.5, 0, 0, false).player;
      victim = v.connect("Victim", GameType.SURVIVAL, 2.5, 64, 0.5, 0, 0, false).player;
      try {
         setup();
      } catch (Throwable t) {
         VanillaLevel.ERR.println("setup failed: " + t.getClass() + " " + t.getMessage());
         for (var e : t.getStackTrace()) VanillaLevel.ERR.println("  at " + e);
         Runtime.getRuntime().halt(1);
      }
      for (String file : args) {
         for (String line : Files.readAllLines(Path.of(file))) {
            if (line.isBlank() || line.startsWith("#")) continue;
            String[] f = line.split(" \\|\\| ", -1);
            if (f.length != 5) throw new IllegalStateException("bad case line: " + line);
            out.println("C " + line);
            try {
               run(f[0], f[1], f[2], f[3], f[4]);
            } catch (Throwable t) {
               VanillaLevel.ERR.println("case " + f[1] + " threw " + t);
               out.println("X " + t.getClass().getSimpleName());
            }
         }
      }
      out.flush();
      System.exit(0);
   }

   static void setup() throws Exception {
      var access = v.server.registryAccess();
      var enchants = access.lookupOrThrow(Registries.ENCHANTMENT);
      ItemStack sword = new ItemStack(Items.IRON_SWORD);
      sword.enchant(enchants.getOrThrow(Enchantments.LOOTING), 3);
      looter.setItemSlot(EquipmentSlot.MAINHAND, sword);
      looter.setItemSlot(EquipmentSlot.HEAD, new ItemStack(Items.IRON_HELMET));
      looter.getInventory().setItem(1, new ItemStack(Items.COBBLESTONE, 16));
      ItemStack gem = new ItemStack(Items.DIAMOND);
      gem.set(DataComponents.CUSTOM_NAME, Component.literal("Gem"));
      looter.getInventory().setItem(2, gem);
      looter.getInventory().setItem(3, new ItemStack(Items.APPLE, 5));
      looter.getInventory().setItem(9, new ItemStack(Items.STICK, 3));
      looter.getInventory().setItem(10, new ItemStack(Items.DIRT, 64));
      var sb = v.server.getScoreboard();
      Objective synth = sb.addObjective("synth", ObjectiveCriteria.DUMMY, Component.literal("synth"), ObjectiveCriteria.RenderType.INTEGER, false, null);
      sb.getOrCreatePlayerScore(looter, synth).set(5);
      sb.getOrCreatePlayerScore(victim, synth).set(12);
      CompoundTag data = TagParser.parseCompoundFully("{n:7,f:1.5d,s:\"text\",l:[1,2,3],c:{a:1}}");
      v.server.getCommandStorage().set(Identifier.parse("synth:data"), data);
      pickaxe = new ItemStack(Items.DIAMOND_PICKAXE);
      pickaxe.enchant(enchants.getOrThrow(Enchantments.FORTUNE), 2);
      chestState = Blocks.CHEST.defaultBlockState().setValue(ChestBlock.FACING, Direction.EAST).setValue(ChestBlock.TYPE, ChestType.LEFT)
         .setValue(ChestBlock.WATERLOGGED, true);
      chestEntity = ((net.minecraft.world.level.block.EntityBlock) Blocks.CHEST).newBlockEntity(POS, chestState);
      ItemStack named = new ItemStack(Items.STONE);
      named.set(DataComponents.CUSTOM_NAME, Component.literal("Named"));
      chestEntity.setLevel(v.level);
      chestEntity.applyComponentsFromItemStack(named);
   }

   static ContextKeySet set(String profile) {
      return switch (profile) {
         case "empty" -> LootContextParamSets.EMPTY;
         case "origin", "command" -> LootContextParamSets.COMMAND;
         case "block" -> LootContextParamSets.BLOCK;
         case "kill" -> LootContextParamSets.ENTITY;
         case "chest" -> LootContextParamSets.CHEST;
         case "enchloc" -> LootContextParamSets.ENCHANTED_LOCATION;
         case "enchitem" -> LootContextParamSets.ENCHANTED_ITEM;
         case "slots" -> LootContextParamSets.COMMAND_SLOT_SOURCE;
         default -> throw new IllegalArgumentException("profile " + profile);
      };
   }

   static LootParams params(String profile, ContextKeySet set) {
      LootParams.Builder b = new LootParams.Builder(v.level);
      Vec3 origin = Vec3.atCenterOf(POS);
      switch (profile) {
         case "empty" -> {
         }
         case "origin" -> b.withParameter(LootContextParams.ORIGIN, origin);
         case "command" -> b.withParameter(LootContextParams.ORIGIN, origin).withParameter(LootContextParams.THIS_ENTITY, looter);
         case "block" -> b.withParameter(LootContextParams.ORIGIN, origin).withParameter(LootContextParams.BLOCK_STATE, chestState)
            .withParameter(LootContextParams.TOOL, pickaxe).withParameter(LootContextParams.THIS_ENTITY, looter)
            .withParameter(LootContextParams.BLOCK_ENTITY, chestEntity).withParameter(LootContextParams.EXPLOSION_RADIUS, 4.0F);
         case "kill" -> {
            DamageSource source = v.level.damageSources().playerAttack(looter);
            b.withParameter(LootContextParams.ORIGIN, origin).withParameter(LootContextParams.THIS_ENTITY, victim)
               .withParameter(LootContextParams.DAMAGE_SOURCE, source).withParameter(LootContextParams.ATTACKING_ENTITY, looter)
               .withParameter(LootContextParams.DIRECT_ATTACKING_ENTITY, looter).withParameter(LootContextParams.LAST_DAMAGE_PLAYER, looter);
         }
         case "chest" -> b.withParameter(LootContextParams.ORIGIN, origin).withParameter(LootContextParams.THIS_ENTITY, looter).withLuck(2.0F);
         case "enchloc" -> b.withParameter(LootContextParams.ORIGIN, origin).withParameter(LootContextParams.THIS_ENTITY, looter)
            .withParameter(LootContextParams.ENCHANTMENT_LEVEL, 2).withParameter(LootContextParams.ENCHANTMENT_ACTIVE, true);
         case "enchitem" -> b.withParameter(LootContextParams.TOOL, pickaxe).withParameter(LootContextParams.ENCHANTMENT_LEVEL, 3);
         case "slots" -> b.withParameter(LootContextParams.ORIGIN, origin).withParameter(LootContextParams.THIS_ENTITY, looter)
            .withParameter(LootContextParams.CONTAINER, looter);
         default -> throw new IllegalArgumentException("profile " + profile);
      }
      return b.create(set);
   }

   static LootContext context(LootParams params, long seed) {
      return new LootContext.Builder(params).withOptionalRandomSeed(seed).create(Optional.empty());
   }

   static String line(String s) {
      return s.replace("\n", "\\n");
   }

   static <T> T parse(Codec<T> codec, String json) {
      DataResult<T> r = codec.parse(jsonOps, JsonParser.parseString(json));
      if (r.error().isPresent()) {
         out.println("E " + line(r.error().get().message()));
         return null;
      }
      return r.getOrThrow();
   }

   static ItemStack stack(String snbt) throws Exception {
      if (snbt.equals("-")) return ItemStack.EMPTY;
      return ItemStack.OPTIONAL_CODEC.parse(nbtOps, TagParser.parseCompoundFully(snbt)).getOrThrow();
   }

   static void print(ItemStack s) {
      out.println("I " + ItemStack.OPTIONAL_CODEC.encodeStart(nbtOps, s).getOrThrow());
   }

   static void validate(String setName, ContextKeySet set, ResourceKey<?> root, Validatable value) {
      ProblemReporter.Collector collector = new ProblemReporter.Collector();
      ValidationContext context = new ValidationContextSource(collector, v.server.reloadableRegistries().lookup()).context(set)
         .enterElement(new ProblemReporter.RootElementPathElement(root), root);
      value.validate(context);
      // a problem reached through two reporters (an entry validates its modifier twice) is one line: the report groups by path
      Set<String> lines = new LinkedHashSet<>();
      collector.forEach((path, problem) -> lines.add("V " + setName + " " + path + " | " + sortedParameters(line(problem.description()))
         + (problem.isFatal() ? " | fatal" : "")));
      lines.forEach(out::println);
   }

   /** The parameter list of "Parameters [..] are not provided" comes from {@code Set.of}, whose order changes per JVM run. */
   static String sortedParameters(String description) {
      java.util.regex.Matcher m = java.util.regex.Pattern.compile("^Parameters \\[(.*)\\] are not provided in this context$").matcher(description);
      if (!m.matches()) return description;
      String[] parts = m.group(1).split(", ");
      Arrays.sort(parts);
      return "Parameters [" + String.join(", ", parts) + "] are not provided in this context";
   }

   static <T> ResourceKey<T> root(ResourceKey<? extends net.minecraft.core.Registry<T>> registry, String id) {
      return ResourceKey.create(registry, Identifier.parse("synthetic:" + id));
   }

   static String bits(float f) {
      return Integer.toHexString(Float.floatToIntBits(f));
   }

   static void run(String kind, String id, String profile, String input, String json) throws Exception {
      ContextKeySet set = set(profile);
      switch (kind) {
         case "table" -> {
            LootTable table = parse(LootTable.DIRECT_CODEC, json);
            if (table == null) return;
            validate("table", table.getParamSet(), root(Registries.LOOT_TABLE, id), table);
            for (long seed : SEEDS) {
               out.println("S " + seed);
               for (ItemStack s : table.getRandomItems(params(profile, set), seed)) print(s);
            }
         }
         case "modifier" -> {
            LootItemFunction function = parse(LootItemFunctions.DIRECT_CODEC, json);
            if (function == null) return;
            validate(profile, set, root(Registries.ITEM_MODIFIER, id), function);
            validate("generic", LootContextParamSets.ALL_PARAMS, root(Registries.ITEM_MODIFIER, id), function);
            LootParams params = params(profile, set);
            for (long seed : SEEDS) {
               out.println("S " + seed);
               print(function.apply(stack(input), context(params, seed)));
            }
         }
         case "predicate" -> {
            LootItemCondition condition = parse(LootItemCondition.DIRECT_CODEC, json);
            if (condition == null) return;
            validate(profile, set, root(Registries.PREDICATE, id), condition);
            validate("generic", LootContextParamSets.ALL_PARAMS, root(Registries.PREDICATE, id), condition);
            LootParams params = params(profile, set);
            for (long seed : SEEDS) {
               out.println("S " + seed);
               out.println("B " + condition.test(context(params, seed)));
            }
         }
         case "int" -> {
            ContextIntProvider provider = parse(ContextIntProviders.DIRECT_CODEC, json);
            if (provider == null) return;
            validate(profile, set, root(Registries.CONTEXT_INT_PROVIDER, id), provider);
            validate("generic", LootContextParamSets.ALL_PARAMS, root(Registries.CONTEXT_INT_PROVIDER, id), provider);
            LootParams params = params(profile, set);
            for (long seed : SEEDS) {
               out.println("S " + seed);
               String unsafe;
               try {
                  unsafe = Integer.toString(provider.getIntUnsafe(context(params, seed)));
               } catch (ArithmeticException e) {
                  unsafe = "ERR";
               }
               out.println("N " + unsafe + " " + provider.getInt(context(params, seed)));
            }
         }
         case "float" -> {
            ContextFloatProvider provider = parse(ContextFloatProviders.DIRECT_CODEC, json);
            if (provider == null) return;
            validate(profile, set, root(Registries.CONTEXT_FLOAT_PROVIDER, id), provider);
            validate("generic", LootContextParamSets.ALL_PARAMS, root(Registries.CONTEXT_FLOAT_PROVIDER, id), provider);
            LootParams params = params(profile, set);
            for (long seed : SEEDS) {
               out.println("S " + seed);
               String unsafe;
               try {
                  unsafe = bits(provider.getFloatUnsafe(context(params, seed)));
               } catch (ArithmeticException e) {
                  unsafe = "ERR";
               }
               out.println("N " + unsafe + " " + bits(provider.getFloat(context(params, seed))));
            }
         }
         case "slots" -> {
            SlotSource source = parse(SlotSources.DIRECT_CODEC, json);
            if (source == null) return;
            validate(profile, set, root(Registries.SLOT_SOURCE, id), source);
            validate("generic", LootContextParamSets.ALL_PARAMS, root(Registries.SLOT_SOURCE, id), source);
            LootParams params = params(profile, set);
            for (long seed : SEEDS) {
               out.println("S " + seed);
               SlotCollection slots = source.provide(context(params, seed));
               slots.itemCopies().forEach(LootSyntheticOracle::print);
            }
         }
         default -> throw new IllegalArgumentException("kind " + kind);
      }
   }
}
