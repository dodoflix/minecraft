import com.google.gson.Gson;
import com.mojang.brigadier.StringReader;
import java.util.*;
import net.minecraft.core.component.DataComponents;
import net.minecraft.nbt.NbtOps;
import net.minecraft.nbt.Tag;
import net.minecraft.network.chat.Component;
import net.minecraft.network.protocol.game.ServerboundEditBookPacket;
import net.minecraft.network.protocol.game.ServerboundUseItemPacket;
import net.minecraft.server.network.Filterable;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.item.component.WritableBookContent;
import net.minecraft.world.item.component.WrittenBookContent;
import net.minecraft.world.level.GameType;

/**
 * Real-vanilla trace of the book handlers ({@code ServerGamePacketListenerImpl.handleEditBook}: update pages / sign,
 * {@code WritableBookItem} / {@code WrittenBookItem} use + {@code WrittenBookContent.resolveForItem}).
 * Usage: scripts/entity-oracle.sh crates/mc-game/tests/oracle/BooksOracle.java -o crates/mc-game/tests/fixtures/books_oracle.txt
 * Output: crates/mc-game/tests/fixtures/books_oracle.txt
 *
 * <pre>
 * SETUP slot SNBT          the stack put into the player's inventory slot (ItemStack.OPTIONAL_CODEC / NbtOps)
 * EDIT slot title|- pages  ServerboundEditBookPacket (title and pages as JSON), followed by the resulting `ITEM slot SNBT`
 * USE slot                 ServerboundUseItemPacket (main hand = selected hotbar slot), followed by `ITEM slot SNBT` and `PKT type` per
 *                          packet the player received
 * </pre>
 */
public final class BooksOracle {
   static VanillaLevel v;
   static VanillaLevel.FakePlayer fp;
   static final Gson GSON = new Gson();

   static String snbt(ItemStack s) {
      var ops = v.server.registryAccess().createSerializationContext(NbtOps.INSTANCE);
      Tag tag = ItemStack.OPTIONAL_CODEC.encodeStart(ops, s).getOrThrow();
      return tag.toString();
   }

   static void setup(int slot, ItemStack stack) {
      fp.player.getInventory().setItem(slot, stack);
      VanillaLevel.OUT.println("SETUP " + slot + " " + snbt(stack));
   }

   static void item(int slot) {
      VanillaLevel.OUT.println("ITEM " + slot + " " + snbt(fp.player.getInventory().getItem(slot)));
   }

   static void edit(int slot, String title, List<String> pages) {
      VanillaLevel.OUT.println("EDIT " + slot + " " + (title == null ? "-" : GSON.toJson(title)) + " " + GSON.toJson(pages));
      fp.player.connection.handleEditBook(new ServerboundEditBookPacket(slot, pages, Optional.ofNullable(title)));
      v.tick(1);
      item(slot);
   }

   static void use(int slot) {
      VanillaLevel.OUT.println("USE " + slot);
      fp.drain();
      fp.player.getInventory().setSelectedSlot(slot);
      fp.player.connection.handleUseItem(new ServerboundUseItemPacket(InteractionHand.MAIN_HAND, 0, 0f, 0f));
      item(slot);
      for (var p : fp.drain()) {
         VanillaLevel.OUT.println("PKT " + p.type());
      }
   }

   static ItemStack writable() {
      return new ItemStack(Items.WRITABLE_BOOK);
   }

   static ItemStack written(boolean resolved, List<Filterable<Component>> pages) {
      ItemStack s = new ItemStack(Items.WRITTEN_BOOK);
      s.set(DataComponents.WRITTEN_BOOK_CONTENT, new WrittenBookContent(Filterable.passThrough("T"), "Auth", 1, pages, resolved));
      return s;
   }

   public static void main(String[] args) throws Exception {
      VanillaLevel.Options o = new VanillaLevel.Options();
      o.levelType = "flat";
      v = VanillaLevel.boot(o);
      fp = v.connect("Booker", GameType.SURVIVAL, 0.5, -60, 2.5, 0, 0, false);
      v.tick(1);
      fp.drain();

      setup(0, writable());
      edit(0, null, List.of("Hello", "World\nline two", ""));
      edit(0, null, List.of());
      edit(0, null, List.of("§aformatted", "ünïcode ☃ \uD83D\uDE00"));
      edit(0, "My Book", List.of("page one", "page two"));
      // signed: no longer writable, edits are ignored
      edit(0, null, List.of("ignored"));
      edit(0, "Other", List.of("ignored"));

      ItemStack named = writable();
      named.set(DataComponents.CUSTOM_NAME, Component.literal("mine"));
      named.setCount(1);
      setup(40, named);
      edit(40, "Offhand", List.of("x"));
      // a non-book stack in the slot stays untouched
      setup(1, new ItemStack(Items.STONE, 5));
      edit(1, null, List.of("nope"));
      edit(1, "nope", List.of("nope"));

      // written book use: components of unresolved books are resolved (plain text stays), the book opens
      setup(2, written(false, List.of(Filterable.passThrough(Component.literal("p1")), new Filterable<>(Component.literal("raw"), Optional.of(Component.literal("filtered"))))));
      use(2);
      setup(4, written(true, List.of(Filterable.passThrough(Component.literal("done")))));
      use(4);
      // writable book use sends nothing (the client opens the editor itself)
      setup(5, writable());
      use(5);

      VanillaLevel.OUT.flush();
      System.exit(0);
   }
}
