import java.io.PrintStream;
import java.lang.reflect.Field;
import java.util.*;
import net.minecraft.core.BlockPos;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.nbt.NbtOps;
import net.minecraft.nbt.Tag;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.RegistryOps;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.RandomSequence;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.item.enchantment.Enchantments;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.EntityBlock;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.storage.loot.LootParams;
import net.minecraft.world.level.storage.loot.LootTable;
import net.minecraft.world.level.storage.loot.parameters.LootContextParamSets;
import net.minecraft.world.level.storage.loot.parameters.LootContextParams;
import net.minecraft.world.phys.Vec3;

/**
 * Loot golden generator: every block loot table x tool x seed x param variant (block param set) and every chest table x seed x
 * luck (chest param set), rolled by the real vanilla {@code LootTable.getRandomItems(params, seed)}. Stdout format:
 * {@code T table paramset seed tool variant}, then one {@code I snbt} line per stack ({@code ItemStack.OPTIONAL_CODEC} through NbtOps).
 * Seed 0 draws from the server's {@code RandomSequences}: the roll is followed by {@code R sequence snbt}, the sequence state after
 * the table. Block variants: {@code plain} (origin, tool, state) and {@code full} (+ the player as this_entity, the block entity of
 * an entity block carrying a custom name, explosion radius 4). Chest variants: {@code none} (no entity) and {@code luckN} (the
 * player as this_entity, luck N).
 * Usage: scripts/golden-loot.sh
 * Output: crates/mc-game/tests/fixtures/loot_golden.txt.gz
 */
public final class LootOracle {
   public static void main(String[] args) throws Exception {
      PrintStream out = VanillaLevel.OUT;
      VanillaLevel v = VanillaLevel.boot(new VanillaLevel.Options());
      var access = v.server.registryAccess();
      var lookup = v.server.reloadableRegistries().lookup();
      var enchants = access.lookupOrThrow(Registries.ENCHANTMENT);
      ServerPlayer player = v.connect("Looter", GameType.SURVIVAL, 0.5, 64, 0.5, 0, 0, false).player;
      var ops = access.createSerializationContext(NbtOps.INSTANCE);
      Map<String, ItemStack> tools = new LinkedHashMap<>();
      tools.put("empty", ItemStack.EMPTY);
      tools.put("iron_pickaxe", new ItemStack(Items.IRON_PICKAXE));
      ItemStack silk = new ItemStack(Items.DIAMOND_PICKAXE);
      silk.enchant(enchants.getOrThrow(Enchantments.SILK_TOUCH), 1);
      tools.put("silk_touch_pickaxe", silk);
      ItemStack fortune = new ItemStack(Items.DIAMOND_PICKAXE);
      fortune.enchant(enchants.getOrThrow(Enchantments.FORTUNE), 3);
      tools.put("fortune3_pickaxe", fortune);
      tools.put("shears", new ItemStack(Items.SHEARS));
      tools.put("diamond_axe", new ItemStack(Items.DIAMOND_AXE));
      ItemStack named = new ItemStack(Items.STONE);
      named.set(DataComponents.CUSTOM_NAME, Component.literal("Named"));
      long[] seeds = {0L, 1L, 2L, 3L, 12345L};
      long[] chestSeeds = {1L, 2L, 3L, 12345L};
      Field sequenceField = LootTable.class.getDeclaredField("randomSequence");
      sequenceField.setAccessible(true);
      BlockPos pos = new BlockPos(0, 64, 0);
      List<ResourceKey<LootTable>> keys = new ArrayList<>();
      lookup.lookupOrThrow(Registries.LOOT_TABLE).listElementIds().forEach(keys::add);
      keys.sort(Comparator.comparing(k -> k.identifier().toString()));
      for (ResourceKey<LootTable> key : keys) {
         String path = key.identifier().getPath();
         LootTable table = v.server.reloadableRegistries().getLootTable(key);
         @SuppressWarnings("unchecked")
         Optional<Identifier> sequence = (Optional<Identifier>) sequenceField.get(table);
         if (path.startsWith("blocks/")) {
            Block block = BuiltInRegistries.BLOCK.getOptional(Identifier.withDefaultNamespace(path.substring(7))).orElse(null);
            if (block == null) continue;
            BlockState state = block.defaultBlockState();
            BlockEntity blockEntity = block instanceof EntityBlock eb ? eb.newBlockEntity(pos, state) : null;
            if (blockEntity != null) blockEntity.applyComponentsFromItemStack(named);
            for (String variant : new String[] {"plain", "full"}) {
               for (var tool : tools.entrySet()) {
                  for (long seed : seeds) {
                     if (seed == 0L && sequence.isEmpty()) continue;
                     LootParams.Builder b = new LootParams.Builder(v.level).withParameter(LootContextParams.ORIGIN, Vec3.atCenterOf(pos))
                        .withParameter(LootContextParams.TOOL, tool.getValue()).withParameter(LootContextParams.BLOCK_STATE, state);
                     if (variant.equals("full")) {
                        b.withParameter(LootContextParams.THIS_ENTITY, player).withParameter(LootContextParams.EXPLOSION_RADIUS, 4.0F);
                        if (blockEntity != null) b.withParameter(LootContextParams.BLOCK_ENTITY, blockEntity);
                     }
                     roll(out, v, ops, key, table, sequence, "block", seed, tool.getKey(), variant, b.create(LootContextParamSets.BLOCK));
                  }
               }
            }
         } else if (path.startsWith("chests/") && table.getParamSet() == LootContextParamSets.CHEST) {
            for (String variant : new String[] {"none", "luck0", "luck1", "luck3"}) {
               for (long seed : chestSeeds) {
                  LootParams.Builder b = new LootParams.Builder(v.level).withParameter(LootContextParams.ORIGIN, Vec3.atCenterOf(pos));
                  if (!variant.equals("none")) b.withParameter(LootContextParams.THIS_ENTITY, player).withLuck(variant.charAt(4) - '0');
                  roll(out, v, ops, key, table, sequence, "chest", seed, "-", variant, b.create(LootContextParamSets.CHEST));
               }
            }
         }
      }
      out.flush();
      System.exit(0);
   }

   static void roll(PrintStream out, VanillaLevel v, RegistryOps<Tag> ops, ResourceKey<LootTable> key, LootTable table,
         Optional<Identifier> sequence, String set, long seed, String tool, String variant, LootParams params) {
      List<ItemStack> items = table.getRandomItems(params, seed);
      out.println("T " + key.identifier() + " " + set + " " + seed + " " + tool + " " + variant);
      if (seed == 0L) {
         v.server.getRandomSequences().forAllSequences((id, state) -> {
            if (id.equals(sequence.get())) out.println("R " + id + " " + RandomSequence.CODEC.encodeStart(NbtOps.INSTANCE, state).getOrThrow());
         });
      }
      for (ItemStack s : items) {
         out.println("I " + ItemStack.OPTIONAL_CODEC.encodeStart(ops, s).getOrThrow());
      }
   }
}
