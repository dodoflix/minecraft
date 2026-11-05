import java.lang.reflect.Field;
import java.util.*;
import net.minecraft.core.NonNullList;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.*;
import net.minecraft.network.Connection;
import net.minecraft.network.protocol.Packet;
import net.minecraft.network.protocol.game.ClientboundSetPlayerInventoryPacket;
import net.minecraft.resources.Identifier;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.server.network.ServerGamePacketListenerImpl;
import net.minecraft.util.Prediction;
import net.minecraft.world.entity.*;
import net.minecraft.world.ItemStackWithSlot;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.entity.player.*;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.inventory.*;
import net.minecraft.world.Container;
import net.minecraft.world.item.*;
import net.minecraft.world.level.Level;
import com.mojang.serialization.Codec;
import net.minecraft.SharedConstants;
import net.minecraft.resources.RegistryDataLoader;
import net.minecraft.core.*;
import net.minecraft.core.registries.Registries;
import net.minecraft.server.*;
import net.minecraft.server.packs.PackType;
import net.minecraft.server.packs.repository.ServerPacksSource;
import net.minecraft.server.packs.resources.MultiPackResourceManager;
import net.minecraft.tags.TagLoader;
import net.minecraft.world.item.crafting.*;
import java.util.concurrent.Executor;

// Golden generator for crates/mc-game/tests/recipe_book_golden.rs: the REAL vanilla ServerRecipeBook (add/remove/seen/
// settings/initial packets, pack NBT) over the real RecipeManager.
public class RecipeBookGolden {
   static final StringBuilder OUT = new StringBuilder();
   static final List<String> events = new ArrayList<>();
   static net.minecraft.core.RegistryAccess.Frozen access;
   static void op(String s) { OUT.append("OP ").append(s).append('\n'); }
   static void out(String s) { OUT.append("OUT ").append(s).append('\n'); }

   static <T> String hex(net.minecraft.network.codec.StreamCodec<net.minecraft.network.RegistryFriendlyByteBuf, T> c, T p) {
      net.minecraft.network.RegistryFriendlyByteBuf buf = new net.minecraft.network.RegistryFriendlyByteBuf(io.netty.buffer.Unpooled.buffer(), access);
      c.encode(buf, p);
      StringBuilder b = new StringBuilder();
      while (buf.isReadable()) b.append(String.format("%02x", buf.readByte() & 0xff));
      return b.toString();
   }

   static <T> String hexF(net.minecraft.network.codec.StreamCodec<net.minecraft.network.FriendlyByteBuf, T> c, T p) {
      net.minecraft.network.FriendlyByteBuf buf = new net.minecraft.network.FriendlyByteBuf(io.netty.buffer.Unpooled.buffer());
      c.encode(buf, p);
      StringBuilder b = new StringBuilder();
      while (buf.isReadable()) b.append(String.format("%02x", buf.readByte() & 0xff));
      return b.toString();
   }

   static <T> String hexB(net.minecraft.network.codec.StreamCodec<io.netty.buffer.ByteBuf, T> c, T p) {
      io.netty.buffer.ByteBuf buf = io.netty.buffer.Unpooled.buffer();
      c.encode(buf, p);
      StringBuilder b = new StringBuilder();
      while (buf.isReadable()) b.append(String.format("%02x", buf.readByte() & 0xff));
      return b.toString();
   }

   public static class RecListener extends ServerGamePacketListenerImpl {
      public RecListener() { super(null, null, null, null); }
      @Override public void send(Packet<?> p) {
         if (p instanceof net.minecraft.network.protocol.game.ClientboundRecipeBookAddPacket x) events.add("PKT add " + hex(net.minecraft.network.protocol.game.ClientboundRecipeBookAddPacket.STREAM_CODEC, x));
         else if (p instanceof net.minecraft.network.protocol.game.ClientboundRecipeBookRemovePacket x) events.add("PKT remove " + hexB(net.minecraft.network.protocol.game.ClientboundRecipeBookRemovePacket.STREAM_CODEC, x));
         else if (p instanceof net.minecraft.network.protocol.game.ClientboundRecipeBookSettingsPacket x) events.add("PKT settings " + hexF(net.minecraft.network.protocol.game.ClientboundRecipeBookSettingsPacket.STREAM_CODEC, x));
         else events.add("PKT other");
      }
   }
   public static class FakeAdvancements extends net.minecraft.server.PlayerAdvancements {
      public FakeAdvancements() { super(null, null, null, null, null); }
      @Override public <T extends net.minecraft.advancements.CriterionTriggerInstance> Map<TriggerInstanceKey, T> getTriggerMapForType(net.minecraft.advancements.triggers.CriterionTrigger<T> t) { events.add("TRIGGER"); return null; }
   }
   public static class FakePlayer extends ServerPlayer {
      public FakePlayer() { super(null, null, null, null); }
      public FakeAdvancements adv;
      @Override public net.minecraft.server.PlayerAdvancements getAdvancements() { return adv; }
   }
   static sun.misc.Unsafe U;

   public static void main(String[] args) throws Exception {
      try { run(); } catch (Throwable t) { t.printStackTrace(); System.exit(1); }
      System.out.print(OUT);
   }

   static String idList(Collection<?> keys) {
      List<String> l = new ArrayList<>();
      for (Object k : keys) l.add(((net.minecraft.resources.ResourceKey<?>) k).identifier().toString());
      Collections.sort(l);
      return String.join(",", l);
   }

   static void run() throws Exception {
      SharedConstants.tryDetectVersion();
      Bootstrap.bootStrap();
      Executor direct = Runnable::run;
      MultiPackResourceManager resources = new MultiPackResourceManager(PackType.SERVER_DATA, List.of(ServerPacksSource.createVanillaPackSource().fullResources()));
      LayeredRegistryAccess<RegistryLayer> layers = RegistryLayer.createRegistryAccess();
      List<Registry.PendingTags<?>> staticTags = TagLoader.loadTagsForExistingRegistries(resources, layers.getLayer(RegistryLayer.STATIC));
      staticTags.forEach(Registry.PendingTags::apply);
      var worldContext = TagLoader.buildUpdatedLookups(layers.getAccessForLoading(RegistryLayer.WORLD), staticTags);
      var world = RegistryDataLoader.load(resources, worldContext, RegistryDataLoader.WORLD_REGISTRIES, direct).join();
      List<HolderLookup.RegistryLookup<?>> dimContext = new ArrayList<>(worldContext);
      world.listRegistries().forEach(dimContext::add);
      var dims = RegistryDataLoader.load(resources, dimContext, RegistryDataLoader.DIMENSION_REGISTRIES, direct).join();
      layers = layers.replaceFrom(RegistryLayer.WORLD, world, dims);
      var loaded = ReloadableServerRegistries.reload(layers, staticTags, resources, direct).join();
      var comps = BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(loaded.lookupWithUpdatedTags());
      comps.forEach(net.minecraft.core.component.DataComponentInitializers.PendingComponents::apply);
      RecipeManager rm = new RecipeManager(loaded.lookupWithUpdatedTags());
      rm.finalizeRecipeLoading(FeatureFlags.VANILLA_SET);
      Field uf = sun.misc.Unsafe.class.getDeclaredField("theUnsafe"); uf.setAccessible(true); U = (sun.misc.Unsafe) uf.get(null);

      access = loaded.layers().compositeAccess();
      List<RecipeHolder<?>> learnable = new ArrayList<>(rm.getLearnableRecipes());
      List<RecipeHolder<?>> all = new ArrayList<>(rm.getRecipes());
      for (int scenario = 0; scenario < 5; scenario++) {
         Random r = new Random(5 + scenario);
         FakePlayer p = (FakePlayer) U.allocateInstance(FakePlayer.class);
         p.adv = (FakeAdvancements) U.allocateInstance(FakeAdvancements.class);
         p.connection = (RecListener) U.allocateInstance(RecListener.class);
         net.minecraft.stats.ServerRecipeBook book = new net.minecraft.stats.ServerRecipeBook((id, o) -> rm.listDisplaysForRecipe(id, o));
         op("scenario " + scenario);
         for (int step = 0; step < 60; step++) {
            int kind = r.nextInt(10);
            if (kind < 3) {
               List<RecipeHolder<?>> pick = new ArrayList<>();
               int n = 1 + r.nextInt(4);
               for (int i = 0; i < n; i++) pick.add(r.nextInt(8) == 0 ? all.get(r.nextInt(all.size())) : learnable.get(r.nextInt(learnable.size())));
               op("add " + pick.stream().map(h -> h.id().identifier().toString()).collect(java.util.stream.Collectors.joining(",")));
               int c = book.addRecipes(pick, p);
               events.add("RET " + c);
            } else if (kind < 5) {
               List<RecipeHolder<?>> pick = new ArrayList<>();
               int n = 1 + r.nextInt(3);
               List<net.minecraft.resources.ResourceKey<Recipe<?>>> known = new ArrayList<>(book.pack().known());
               for (int i = 0; i < n; i++) pick.add(!known.isEmpty() && r.nextInt(3) > 0 ? rm.byKey(known.get(r.nextInt(known.size()))).get() : learnable.get(r.nextInt(learnable.size())));
               op("remove " + pick.stream().map(h -> h.id().identifier().toString()).collect(java.util.stream.Collectors.joining(",")));
               int c = book.removeRecipes(pick, p);
               events.add("RET " + c);
            } else if (kind < 7) {
               int idx = r.nextInt(1800);
               op("seen " + idx);
               var info = rm.getRecipeFromDisplay(new net.minecraft.world.item.crafting.display.RecipeDisplayId(idx));
               if (info != null) book.removeHighlight(info.parent().id());
            } else if (kind < 9) {
               var t = net.minecraft.world.inventory.RecipeBookType.values()[r.nextInt(4)];
               boolean open = r.nextBoolean(), filt = r.nextBoolean();
               op("settings " + t.ordinal() + " " + open + " " + filt);
               book.setBookSetting(t, open, filt);
            } else {
               op("initial");
               book.sendInitialRecipeBook(p);
            }
            out("known " + idList(book.pack().known()));
            out("highlight " + idList(book.pack().highlight()));
            var packed = book.pack();
            out("settings " + hexF(net.minecraft.network.protocol.game.ClientboundRecipeBookSettingsPacket.STREAM_CODEC, new net.minecraft.network.protocol.game.ClientboundRecipeBookSettingsPacket(packed.settings())));
            for (String e : events) out(e);
            events.clear();
         }
      }
   }
}
