import java.io.PrintStream;
import java.lang.reflect.Method;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.util.*;
import com.mojang.serialization.JsonOps;
import io.netty.buffer.ByteBuf;
import io.netty.buffer.Unpooled;
import net.minecraft.core.HolderSet;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.nbt.*;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.network.protocol.game.ClientboundMerchantOffersPacket;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.RegistryOps;
import net.minecraft.resources.ResourceKey;
import net.minecraft.util.Unit;
import net.minecraft.world.entity.EntityTypes;
import net.minecraft.world.entity.npc.villager.*;
import net.minecraft.world.entity.npc.wanderingtrader.WanderingTrader;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.trading.*;
import net.minecraft.world.level.storage.loot.LootContext;
import net.minecraft.world.level.storage.loot.LootParams;
import net.minecraft.world.level.storage.loot.parameters.LootContextParamSets;
import net.minecraft.world.level.storage.loot.parameters.LootContextParams;
import net.minecraft.world.phys.Vec3;

/**
 * Trade goldens (scripts/golden-trades.sh -> tests/fixtures/trades_golden.txt), run through scripts/entity-oracle.sh:
 * <pre>
 * S prof level type seed sha1(canon(MerchantOffers.CODEC nbt))   seeded context (LootContext.Builder.withOptionalRandomSeed)
 * W seed sha1                                                   wandering trader sets (buying, uncommon, common) seeded
 * R prof level type sha1 wirehex                                real Villager.getOffers() (level random sequences), in order
 * D key canonText                                               full canonical text of some cases, for diagnosis
 * M prof level type sha1 wirehex info                           the R offers after uses/demand/special price mutations
 * T wanderingtrader index sha1 wirehex                          real WanderingTrader.getOffers(), in order
 * K packethex                                                   a ClientboundMerchantOffersPacket (container 7, level 3, xp 150, true, false) of the first R offers
 * E registry id sha1(canon(codec nbt))                          every villager_trade / trade_set entry re-encoded with its codec
 * </pre>
 * canon: compound keys sorted, {@code {k:v,..}}, lists {@code [v,..]}, numbers with a type suffix, floats/doubles as raw bits.
 */
public final class TradesOracle {
   static final String[] PROFESSIONS = {"armorer", "butcher", "cartographer", "cleric", "farmer", "fisherman", "fletcher", "leatherworker", "librarian", "mason", "shepherd", "toolsmith", "weaponsmith"};
   static final String[] TYPES = {"desert", "jungle", "plains", "savanna", "snow", "swamp", "taiga"};
   static final long[] SEEDS = {1L, 2L, 7L, 12345L};

   static VanillaLevel v;
   static PrintStream out;
   static Method addDup, addNoDup;

   public static void main(String[] args) throws Exception {
      out = VanillaLevel.OUT;
      v = VanillaLevel.boot(new VanillaLevel.Options());
      addDup = AbstractVillager.class.getDeclaredMethod("addOffersFromItemListings", LootContext.class, MerchantOffers.class, HolderSet.class, int.class);
      addNoDup = AbstractVillager.class.getDeclaredMethod("addOffersFromItemListingsWithoutDuplicates", LootContext.class, MerchantOffers.class, HolderSet.class, int.class);
      addDup.setAccessible(true);
      addNoDup.setAccessible(true);

      // seeded
      for (String prof : PROFESSIONS) {
         for (int level = 1; level <= 5; level++) {
            for (String type : TYPES) {
               for (long seed : SEEDS) {
                  Villager villager = villager(prof, type, level);
                  MerchantOffers offers = new MerchantOffers();
                  seeded(villager, Identifier.withDefaultNamespace(prof + "/level_" + level), offers, seed);
                  String canon = canon(encode(offers));
                  out.println("S " + prof + " " + level + " " + type + " " + seed + " " + sha1(canon));
                  if (seed == 1L && (type.equals("plains") || type.equals("desert"))) out.println("D " + prof + "/" + level + "/" + type + " " + canon);
               }
            }
         }
      }
      for (long seed : SEEDS) {
         WanderingTrader trader = new WanderingTrader(EntityTypes.WANDERING_TRADER, v.level);
         trader.setPos(10.5, 70.0, -3.5);
         MerchantOffers offers = new MerchantOffers();
         for (String set : new String[] {"buying", "uncommon", "common"}) seeded(trader, Identifier.withDefaultNamespace("wandering_trader/" + set), offers, seed);
         String canon = canon(encode(offers));
         out.println("W " + seed + " " + sha1(canon));
         out.println("D wt/" + seed + " " + canon);
      }

      // real path
      MerchantOffers first = null;
      for (String prof : PROFESSIONS) {
         for (int level = 1; level <= 5; level++) {
            for (String type : TYPES) {
               Villager villager = villager(prof, type, level);
               MerchantOffers offers = villager.getOffers();
               if (first == null && !offers.isEmpty()) first = offers.copy();
               out.println("R " + prof + " " + level + " " + type + " " + sha1(canon(encode(offers))) + " " + wire(offers));
               if (level == 3 && type.equals("plains")) mutated(prof, level, type, offers);
            }
         }
      }
      for (int i = 0; i < 6; i++) {
         WanderingTrader trader = new WanderingTrader(EntityTypes.WANDERING_TRADER, v.level);
         trader.setPos(10.5, 70.0, -3.5);
         MerchantOffers offers = trader.getOffers();
         out.println("T wanderingtrader " + i + " " + sha1(canon(encode(offers))) + " " + wire(offers));
      }
      ByteBuf buf = Unpooled.buffer();
      ClientboundMerchantOffersPacket.STREAM_CODEC.encode(new RegistryFriendlyByteBuf(buf, v.server.registryAccess()), new ClientboundMerchantOffersPacket(7, first, 3, 150, true, false));
      out.println("K " + hex(buf));

      // registries
      var ops = RegistryOps.create(NbtOps.INSTANCE, v.server.registryAccess());
      var trades = v.server.registryAccess().lookupOrThrow(Registries.VILLAGER_TRADE);
      var sets = v.server.registryAccess().lookupOrThrow(Registries.TRADE_SET);
      List<String> lines = new ArrayList<>();
      trades.listElements().forEach(h -> lines.add("E villager_trade " + h.key().identifier() + " " + sha1(canon(VillagerTrade.CODEC.encodeStart(ops, h.value()).getOrThrow()))));
      sets.listElements().forEach(h -> lines.add("E trade_set " + h.key().identifier() + " " + sha1(canon(TradeSet.CODEC.encodeStart(ops, h.value()).getOrThrow()))));
      Collections.sort(lines);
      lines.forEach(out::println);
      out.flush();
      System.exit(0);
   }

   static Villager villager(String prof, String type, int level) {
      Villager villager = new Villager(EntityTypes.VILLAGER, v.level);
      villager.setPos(100.5, 64.0, -20.5);
      villager.setVillagerData(new VillagerData(
         BuiltInRegistries.VILLAGER_TYPE.get(Identifier.withDefaultNamespace(type)).orElseThrow(),
         BuiltInRegistries.VILLAGER_PROFESSION.get(Identifier.withDefaultNamespace(prof)).orElseThrow(), level));
      return villager;
   }

   static void seeded(AbstractVillager merchant, Identifier key, MerchantOffers offers, long seed) throws Exception {
      TradeSet set = v.server.registryAccess().lookupOrThrow(Registries.TRADE_SET).getOptional(ResourceKey.create(Registries.TRADE_SET, key)).orElseThrow();
      LootContext ctx = new LootContext.Builder(new LootParams.Builder(v.level)
         .withParameter(LootContextParams.ORIGIN, merchant.position())
         .withParameter(LootContextParams.THIS_ENTITY, merchant)
         .withParameter(LootContextParams.ADDITIONAL_COST_COMPONENT_ALLOWED, Unit.INSTANCE)
         .create(LootContextParamSets.VILLAGER_TRADE)).withOptionalRandomSeed(seed).create(Optional.empty());
      int n = set.calculateNumberOfTrades(ctx);
      (set.allowDuplicates() ? addDup : addNoDup).invoke(null, ctx, offers, set.trades(), n);
   }

   static void mutated(String prof, int level, String type, MerchantOffers original) {
      MerchantOffers offers = original.copy();
      StringBuilder info = new StringBuilder();
      for (int i = 0; i < offers.size(); i++) {
         MerchantOffer o = offers.get(i);
         for (int u = 0; u < i % 4; u++) o.increaseUses();
         if (i % 3 == 0) o.setToOutOfStock();
         if (i % 2 == 1) o.updateDemand();
         o.addToSpecialPriceDiff(i % 2 == 0 ? -3 : 5);
         ItemStack a = o.getCostA(), b = o.getCostB();
         boolean ok = o.satisfiedBy(a.copy(), b.copy());
         ItemStack shortA = a.copyWithCount(Math.max(0, a.getCount() - 1));
         boolean shortOk = o.satisfiedBy(shortA, b.copy());
         ItemStack ta = a.copyWithCount(a.getCount() + 2), tb = b.copyWithCount(b.isEmpty() ? 0 : b.getCount() + 1);
         boolean took = o.take(ta, tb);
         info.append(a.getCount()).append('/').append(b.getCount()).append('/').append(ok).append('/').append(shortOk).append('/').append(took).append('/').append(ta.getCount()).append('/').append(tb.getCount())
            .append('/').append(o.isOutOfStock()).append('/').append(o.getDemand()).append(';');
      }
      out.println("M " + prof + " " + level + " " + type + " " + sha1(canon(encode(offers))) + " " + wire(offers) + " " + info);
   }

   static Tag encode(MerchantOffers offers) {
      return MerchantOffers.CODEC.encodeStart(RegistryOps.create(NbtOps.INSTANCE, v.server.registryAccess()), offers).getOrThrow();
   }

   static String wire(MerchantOffers offers) {
      ByteBuf buf = Unpooled.buffer();
      MerchantOffers.STREAM_CODEC.encode(new RegistryFriendlyByteBuf(buf, v.server.registryAccess()), offers);
      return hex(buf);
   }

   static String hex(ByteBuf buf) {
      StringBuilder sb = new StringBuilder();
      for (int i = buf.readerIndex(); i < buf.writerIndex(); i++) sb.append(String.format("%02x", buf.getByte(i)));
      return sb.toString();
   }

   static String sha1(String s) {
      try {
         StringBuilder sb = new StringBuilder();
         for (byte b : MessageDigest.getInstance("SHA-1").digest(s.getBytes(StandardCharsets.UTF_8))) sb.append(String.format("%02x", b));
         return sb.toString();
      } catch (java.security.NoSuchAlgorithmException e) {
         throw new IllegalStateException(e);
      }
   }

   static String canon(Tag tag) {
      StringBuilder sb = new StringBuilder();
      canon(tag, sb);
      return sb.toString();
   }

   static void canon(Tag tag, StringBuilder sb) {
      switch (tag) {
         case CompoundTag c -> {
            sb.append('{');
            boolean first = true;
            for (String k : new TreeSet<>(c.keySet())) {
               if (!first) sb.append(',');
               first = false;
               sb.append(k).append(':');
               canon(c.get(k), sb);
            }
            sb.append('}');
         }
         case ListTag l -> {
            sb.append('[');
            for (int i = 0; i < l.size(); i++) {
               if (i > 0) sb.append(',');
               canon(l.get(i), sb);
            }
            sb.append(']');
         }
         case ByteTag t -> sb.append(t.byteValue()).append('b');
         case ShortTag t -> sb.append(t.shortValue()).append('s');
         case IntTag t -> sb.append(t.intValue()).append('i');
         case LongTag t -> sb.append(t.longValue()).append('l');
         case FloatTag t -> sb.append(Integer.toHexString(Float.floatToIntBits(t.floatValue()))).append('f');
         case DoubleTag t -> sb.append(Long.toHexString(Double.doubleToLongBits(t.doubleValue()))).append('d');
         case StringTag t -> sb.append('"').append(t.value()).append('"');
         case ByteArrayTag t -> sb.append("B").append(Arrays.toString(t.getAsByteArray()));
         case IntArrayTag t -> sb.append("I").append(Arrays.toString(t.getAsIntArray()));
         case LongArrayTag t -> sb.append("L").append(Arrays.toString(t.getAsLongArray()));
         default -> throw new IllegalStateException(tag.toString());
      }
   }
}
