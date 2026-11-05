import java.io.PrintStream;
import java.util.*;
import net.minecraft.core.Holder;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.item.enchantment.Enchantments;
import net.minecraft.world.item.enchantment.ItemEnchantments;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.storage.loot.LootParams;
import net.minecraft.world.level.storage.loot.LootTable;
import net.minecraft.world.level.storage.loot.parameters.LootContextParamSets;
import net.minecraft.world.level.storage.loot.parameters.LootContextParams;
import net.minecraft.world.phys.Vec3;

/**
 * Loot golden generator: every block loot table x tool x seed (block param set) and every chest table x seed (chest param set),
 * rolled by the real vanilla {@code LootTable.getRandomItems(params, seed)}. Output (stdout): {@code T table paramset seed tool}
 * then one {@code I item count +component,-component} line per stack. Run: scripts/entity-oracle.sh LootOracle.java
 */
public final class LootOracle {
   public static void main(String[] args) throws Exception {
      PrintStream out = VanillaLevel.OUT;
      VanillaLevel v = VanillaLevel.boot(new VanillaLevel.Options());
      var access = v.server.registryAccess();
      var lookup = v.server.reloadableRegistries().lookup();
      var enchants = access.lookupOrThrow(Registries.ENCHANTMENT);
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
      long[] seeds = {1L, 2L, 3L, 12345L};
      List<ResourceKey<LootTable>> keys = new ArrayList<>();
      lookup.lookupOrThrow(Registries.LOOT_TABLE).listElementIds().forEach(keys::add);
      keys.sort(Comparator.comparing(k -> k.identifier().toString()));
      for (ResourceKey<LootTable> key : keys) {
         String path = key.identifier().getPath();
         LootTable table = v.server.reloadableRegistries().getLootTable(key);
         if (path.startsWith("blocks/")) {
            Block block = BuiltInRegistries.BLOCK.getOptional(Identifier.withDefaultNamespace(path.substring(7))).orElse(null);
            if (block == null) continue;
            BlockState state = block.defaultBlockState();
            for (var tool : tools.entrySet()) {
               for (long seed : seeds) {
                  LootParams params = new LootParams.Builder(v.level).withParameter(LootContextParams.ORIGIN, Vec3.atCenterOf(new net.minecraft.core.BlockPos(0, 64, 0)))
                     .withParameter(LootContextParams.TOOL, tool.getValue()).withParameter(LootContextParams.BLOCK_STATE, state).create(LootContextParamSets.BLOCK);
                  print(out, key, "block", seed, tool.getKey(), table.getRandomItems(params, seed));
               }
            }
         } else if (path.startsWith("chests/") && table.getParamSet() == LootContextParamSets.CHEST) {
            for (long seed : seeds) {
               LootParams params = new LootParams.Builder(v.level).withParameter(LootContextParams.ORIGIN, Vec3.atCenterOf(new net.minecraft.core.BlockPos(0, 64, 0))).create(LootContextParamSets.CHEST);
               print(out, key, "chest", seed, "-", table.getRandomItems(params, seed));
            }
         }
      }
      out.flush();
      System.exit(0);
   }

   static void print(PrintStream out, ResourceKey<LootTable> key, String set, long seed, String tool, List<ItemStack> items) {
      out.println("T " + key.identifier() + " " + set + " " + seed + " " + tool);
      for (ItemStack s : items) {
         List<String> comps = new ArrayList<>();
         var split = s.getComponentsPatch().split();
         split.added().forEach(c -> comps.add("+" + BuiltInRegistries.DATA_COMPONENT_TYPE.getKey(c.type())));
         split.removed().forEach(t -> comps.add("-" + BuiltInRegistries.DATA_COMPONENT_TYPE.getKey(t)));
         Collections.sort(comps);
         out.println("I " + BuiltInRegistries.ITEM.getKey(s.getItem()) + " " + s.getCount() + " " + String.join(",", comps));
      }
   }
}
