// Golden for ExperienceOrb.getExperienceValue/getIcon and ItemEntity.areMergable/merge (static logic).
//   java -cp "$EXTRACTOR:$CP" ItemEntityGolden.java > item_entity_golden.txt   (see scripts/golden-menu.sh for $CP / $EXTRACTOR)
import java.util.*;
import java.util.function.Supplier;
import net.minecraft.core.component.DataComponents;
import net.minecraft.network.chat.Component;
import net.minecraft.world.entity.ExperienceOrb;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.item.*;

public class ItemEntityGolden {
   public static void main(String[] args) throws Exception {
      extractor.Setup.bootstrap();
      StringBuilder sb = new StringBuilder();
      for (int n = 1; n <= 5000; n++) {
         StringBuilder parts = new StringBuilder();
         int amount = n;
         while (amount > 0) {
            int v = ExperienceOrb.getExperienceValue(amount);
            amount -= v;
            parts.append(parts.length() > 0 ? "," : "").append(v);
         }
         sb.append("orb_split ").append(n).append(' ').append(parts).append('\n');
      }
      for (int v = 0; v <= 3000; v++) {
         sb.append("orb_value ").append(v).append(' ').append(ExperienceOrb.getExperienceValue(v)).append('\n');
      }
      LinkedHashMap<String, Supplier<ItemStack>> kinds = new LinkedHashMap<>();
      kinds.put("diamond", () -> new ItemStack(Items.DIAMOND));
      kinds.put("stone", () -> new ItemStack(Items.STONE));
      kinds.put("named_diamond", () -> {
         ItemStack s = new ItemStack(Items.DIAMOND);
         s.set(DataComponents.CUSTOM_NAME, Component.literal("x"));
         return s;
      });
      kinds.put("ender_pearl", () -> new ItemStack(Items.ENDER_PEARL));
      kinds.put("iron_sword", () -> new ItemStack(Items.IRON_SWORD));
      kinds.put("damaged_sword", () -> {
         ItemStack s = new ItemStack(Items.IRON_SWORD);
         s.setDamageValue(5);
         return s;
      });
      int[] counts = {1, 5, 15, 16, 20, 40, 63, 64};
      for (var a : kinds.entrySet()) {
         for (var b : kinds.entrySet()) {
            for (int ca : counts) {
               for (int cb : counts) {
                  ItemStack to = a.getValue().get().copyWithCount(ca);
                  ItemStack from = b.getValue().get().copyWithCount(cb);
                  if (ca > to.getMaxStackSize() || cb > from.getMaxStackSize()) {
                     continue;
                  }
                  boolean mergable = ItemEntity.areMergable(to, from);
                  ItemStack toCopy = to.copy();
                  ItemStack fromCopy = from.copy();
                  ItemStack newTo = ItemEntity.merge(toCopy, fromCopy, 64);
                  sb.append("merge ").append(a.getKey()).append(' ').append(ca).append(' ').append(b.getKey()).append(' ').append(cb)
                     .append(' ').append(mergable).append(' ').append(newTo.getCount()).append(' ').append(fromCopy.getCount()).append('\n');
               }
            }
         }
      }
      System.out.print(sb);
   }
}
