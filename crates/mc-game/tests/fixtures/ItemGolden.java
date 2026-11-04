// Golden for the data-driven Item defaults (Tool speed / correct tool, use duration / animation).
//   scripts/golden-menu.sh also regenerates item_golden.txt
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.item.*;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.BlockState;

public class ItemGolden {
   public static void main(String[] args) throws Exception {
      extractor.Setup.bootstrap();
      StringBuilder sb = new StringBuilder();
      for (Item item : BuiltInRegistries.ITEM) {
         ItemStack s = new ItemStack(item);
         String name = BuiltInRegistries.ITEM.getKey(item).toString();
         boolean tool = s.has(net.minecraft.core.component.DataComponents.TOOL);
         if (tool) {
            for (Block b : BuiltInRegistries.BLOCK) {
               BlockState st = b.defaultBlockState();
               float speed = s.getDestroySpeed(st);
               boolean correct = s.isCorrectToolForDrops(st);
               if (speed != 1.0F || correct) sb.append("tool ").append(name).append(' ').append(BuiltInRegistries.BLOCK.getKey(b)).append(' ').append(Float.floatToIntBits(speed)).append(' ').append(correct).append('\n');
            }
         }
         sb.append("use ").append(name).append(' ').append(item.getUseDuration(s, null)).append(' ').append(item.getUseAnimation(s)).append('\n');
      }
      System.out.print(sb);
   }
}
