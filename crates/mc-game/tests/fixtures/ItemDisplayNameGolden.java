// Golden for ItemStack.getDisplayName(): one JSON line per item (plain stack), plus custom_name / rarity / written book variants.
// Usage: MC_EXTRA_CP=tools/extractor/build scripts/vanilla-java.sh -o crates/mc-game/tests/fixtures/item_display_name_golden.txt crates/mc-game/tests/fixtures/ItemDisplayNameGolden.java
// Output: crates/mc-game/tests/fixtures/item_display_name_golden.txt
import com.mojang.serialization.JsonOps;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.ComponentSerialization;
import net.minecraft.world.item.*;
import net.minecraft.world.item.component.WrittenBookContent;
import net.minecraft.server.network.Filterable;

public class ItemDisplayNameGolden {
   static void emit(StringBuilder sb, String label, ItemStack s) {
      sb.append(label).append('\t').append(ComponentSerialization.CODEC.encodeStart(JsonOps.INSTANCE, s.getDisplayName()).getOrThrow()).append('\n');
   }

   public static void main(String[] args) throws Exception {
      extractor.Setup.bootstrap();
      StringBuilder sb = new StringBuilder();
      for (Item item : BuiltInRegistries.ITEM) {
         if (item == Items.AIR) continue;
         emit(sb, BuiltInRegistries.ITEM.getKey(item).toString(), new ItemStack(item));
      }
      ItemStack named = new ItemStack(Items.DIAMOND_SWORD, 3);
      named.set(DataComponents.CUSTOM_NAME, Component.literal("Hello"));
      emit(sb, "custom_name", named);
      ItemStack epic = new ItemStack(Items.DIRT);
      epic.set(DataComponents.RARITY, Rarity.EPIC);
      emit(sb, "epic", epic);
      ItemStack book = new ItemStack(Items.WRITTEN_BOOK);
      book.set(DataComponents.WRITTEN_BOOK_CONTENT, new WrittenBookContent(Filterable.passThrough("Title"), "me", 0, java.util.List.of(), true));
      emit(sb, "book", book);
      ItemStack blank = new ItemStack(Items.WRITTEN_BOOK);
      blank.set(DataComponents.WRITTEN_BOOK_CONTENT, new WrittenBookContent(Filterable.passThrough("  "), "me", 0, java.util.List.of(), true));
      emit(sb, "book_blank", blank);
      System.out.print(sb);
   }
}
