import java.util.*;
import net.minecraft.nbt.*;
import net.minecraft.util.RandomSource;
import net.minecraft.world.entity.ai.gossip.*;
import net.minecraft.world.level.levelgen.LegacyRandomSource;
import com.mojang.serialization.Codec;

/** Random op scripts on vanilla GossipContainer: prints each op and the observable result (reputations, counts, NBT). */
public class GossipOracle {
   static GossipType[] T = GossipType.values();
   static UUID uuid(Random r, int pool) { return new UUID(0x1234L * (r.nextInt(pool) + 1) * 0x9E3779B97F4A7C15L, 77L * (r.nextInt(pool) + 3) * 0x517CC1B727220A95L); }

   static String dump(GossipContainer c) {
      // targets in HashMap order; types by ordinal (their order inside one target is identity-hash dependent in vanilla)
      StringBuilder sb = new StringBuilder();
      var map = c.getGossipEntries();
      for (var e : map.entrySet()) {
         sb.append(e.getKey()).append('[');
         for (GossipType t : T) { int v = e.getValue().getInt(t); if (v != 0) sb.append(t.id).append('=').append(v).append(','); }
         sb.append("] ");
      }
      return sb.toString().trim();
   }

   static String targetOrder(GossipContainer c) {
      Tag tag = GossipContainer.CODEC.encodeStart(NbtOps.INSTANCE, c).getOrThrow();
      StringBuilder sb = new StringBuilder();
      Set<String> seen = new LinkedHashSet<>();
      for (Tag e : (ListTag) tag) seen.add(((CompoundTag) e).getIntArray("Target").map(Arrays::toString).orElse("?"));
      return String.join(" ", seen);
   }

   static java.io.PrintStream OUT = System.out;
   public static void main(String[] args) throws Exception {
      net.minecraft.SharedConstants.tryDetectVersion();
      net.minecraft.server.Bootstrap.bootStrap();
      for (int scenario = 0; scenario < 6; scenario++) {
         Random r = new Random(1000 + scenario);
         GossipContainer c = new GossipContainer();
         int pool = new int[] {3, 8, 14, 30, 60, 6}[scenario];
         OUT.println("scenario " + scenario);
         for (int step = 0; step < 120; step++) {
            int op = r.nextInt(10);
            if (op <= 3) {
               UUID u = uuid(r, pool); GossipType t = T[r.nextInt(5)]; int a = r.nextInt(40) + 1;
               c.add(u, t, a);
               OUT.println("add " + u + " " + t.id + " " + a);
            } else if (op == 4) {
               UUID u = uuid(r, pool); GossipType t = T[r.nextInt(5)]; int a = r.nextInt(30) + 1;
               c.remove(u, t, a);
               OUT.println("remove_amount " + u + " " + t.id + " " + a);
            } else if (op == 5) {
               UUID u = uuid(r, pool); GossipType t = T[r.nextInt(5)];
               c.remove(u, t);
               OUT.println("remove " + u + " " + t.id);
            } else if (op == 6) {
               c.decay();
               OUT.println("decay");
            } else if (op == 7) {
               // transfer from a source with one gossip type per target
               GossipContainer s = new GossipContainer();
               int n = 1 + r.nextInt(8);
               List<String> lines = new ArrayList<>();
               for (int i = 0; i < n; i++) { UUID u = uuid(r, pool + 40); GossipType t = T[r.nextInt(5)]; int a = r.nextInt(40) + 1;
                  if (s.getReputation(u, x -> true) == 0) { s.add(u, t, a); lines.add(u + " " + t.id + " " + a); } }
               long seed = r.nextLong(); int max = 1 + r.nextInt(6);
               Set<UUID> before = new HashSet<>(c.getGossipEntries().keySet());
               int moved = c.transferFrom(s, new LegacyRandomSource(seed), max);
               Set<UUID> after = new HashSet<>(c.getGossipEntries().keySet()); after.removeAll(before);
               // several new targets are inserted in identity-hash (JVM-random) order: their bucket-collision order is unspecified from here on
               if (after.size() >= 2) OUT.println("unordered");
               OUT.println("transfer " + seed + " " + max + " " + lines.size() + " " + String.join(" ", lines) + " -> " + moved);
            } else if (op == 8) {
               GossipType t = T[r.nextInt(5)]; c.remove(t);
               OUT.println("remove_type " + t.id);
            } else {
               UUID u = uuid(r, pool); int mask = r.nextInt(32);
               int rep = c.getReputation(u, t -> (mask >> t.ordinal() & 1) != 0);
               GossipType t = T[r.nextInt(5)];
               long cnt = c.getCountForType(t, v -> v > 3.0);
               OUT.println("query " + u + " " + mask + " " + t.id + " -> " + rep + " " + cnt);
            }
            if (step % 10 == 9) { OUT.println("dump " + dump(c)); OUT.println("order " + targetOrder(c)); OUT.println("copy " + dump(c.copy())); }
         }
         // NBT round trip (sorted types)
         Tag tag = GossipContainer.CODEC.encodeStart(NbtOps.INSTANCE, c).getOrThrow();
         GossipContainer back = GossipContainer.CODEC.parse(NbtOps.INSTANCE, tag).getOrThrow();
         OUT.println("nbt_roundtrip " + dump(back));
      }
   }
}
