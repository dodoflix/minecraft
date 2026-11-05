// Differential golden corpus for every command argument type, produced by the real vanilla classes.
//
//   scripts/args-golden.sh [type ...]      (all types when none is named; writes crates/mc-game/tests/fixtures/args/<type>_golden.txt)
//
// Per type: a deterministic corpus of >= 250 inputs (valid seeds, every prefix of every seed = what a client
// sends while typing, mutations with boundary numbers / unicode / surrogate pairs / separators) is run through a real
// brigadier dispatcher `t <arg>`; per input the generator records the reader cursor after the parse, the argument
// value (type specific rendering), every parse exception (message + cursor), the execute() error and the
// suggestions at the end of the input (range + texts + tooltips; lists longer than 40 entries are recorded as count + Java
// String.hashCode of the whole list + the first 8). The Rust replays live in tests/args_<type>_golden.rs.
//
// Record format (all text escaped by `esc`: backslash, control chars and lone surrogates become a backslash pair and a u-XXXX escape):
//   @ <variant> <input>
//   R <cursor after parse, relative to the argument start, UTF-16 units>
//   A <start>-<end> <value>            or `A -` when the argument node was not parsed
//   X <message>@<cursor>               one per parse exception (cursor relative to the argument start)
//   E <message>@<cursor> | E ok        what execute() of the whole command line reports
//   S <start>-<end> <count> <hash> <items joined by TAB, tooltip appended after a 0x01 char>
import com.mojang.brigadier.CommandDispatcher;
import com.mojang.brigadier.ParseResults;
import com.mojang.brigadier.arguments.ArgumentType;
import com.mojang.brigadier.builder.LiteralArgumentBuilder;
import com.mojang.brigadier.builder.RequiredArgumentBuilder;
import com.mojang.brigadier.context.CommandContext;
import com.mojang.brigadier.context.ParsedArgument;
import com.mojang.brigadier.exceptions.CommandSyntaxException;
import com.mojang.brigadier.suggestion.Suggestion;
import com.mojang.brigadier.suggestion.Suggestions;
import com.mojang.brigadier.tree.CommandNode;
import java.io.PrintWriter;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.*;
import java.util.concurrent.CompletableFuture;
import java.util.function.Function;
import java.util.stream.Stream;
import net.minecraft.commands.CommandBuildContext;
import net.minecraft.commands.SharedSuggestionProvider;
import net.minecraft.core.RegistryAccess;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.permissions.PermissionSet;
import net.minecraft.world.flag.FeatureFlagSet;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.level.Level;
import net.minecraft.world.phys.Vec2;
import net.minecraft.world.phys.Vec3;

public class ArgsGolden {
   static RegistryAccess.Frozen access;
   static CommandBuildContext ctx;
   static final String[] TEAMS = {"red", "blue", "Team Name!", "sp ace", "\u00e9quipe"};

   /** A server without anything running: only the scoreboard, the registries and the (empty) player list answer. */
   static final class FakePlayerList extends net.minecraft.server.dedicated.DedicatedPlayerList {
      FakePlayerList() { super(null, null, null); }
      @Override public void broadcastAll(net.minecraft.network.protocol.Packet<?> packet) {}
      @Override public void broadcastAll(net.minecraft.network.protocol.Packet<?> packet, ResourceKey<Level> dimension) {}
   }

   static final class FakeScoreboard extends net.minecraft.server.ServerScoreboard {
      FakeScoreboard() { super(null); }
      @Override protected void setDirty() {}
   }

   static final class FakeServer extends net.minecraft.server.dedicated.DedicatedServer {
      FakeServer() { super(null, null, null, null, Optional.empty(), null, null, null, null, null); }
      FakeScoreboard scoreboard;
      FakePlayerList players;
      @Override public net.minecraft.server.ServerScoreboard getScoreboard() { return scoreboard; }
      @Override public net.minecraft.server.dedicated.DedicatedPlayerList getPlayerList() { return players; }
      @Override public String[] getPlayerNames() { return new String[0]; }
      @Override public Set<ResourceKey<Level>> levelKeys() { return Set.of(); }
      @Override public RegistryAccess.Frozen registryAccess() { return access; }
   }

   static <T> T allocate(Class<T> c) throws Exception {
      var f = sun.misc.Unsafe.class.getDeclaredField("theUnsafe");
      f.setAccessible(true);
      return c.cast(((sun.misc.Unsafe) f.get(null)).allocateInstance(c));
   }

   static FakeServer fakeServer() throws Exception {
      FakeServer server = allocate(FakeServer.class);
      server.players = allocate(FakePlayerList.class);
      FakeScoreboard sb = new FakeScoreboard();
      var f = net.minecraft.server.ServerScoreboard.class.getDeclaredField("server");
      f.setAccessible(true);
      f.set(sb, server);
      server.scoreboard = sb;
      return server;
   }

   /** The fixed scoreboard content mirrored by the Rust test server. */
   static void populate(net.minecraft.server.ServerScoreboard sb) {
      for (String team : TEAMS) sb.addPlayerTeam(team);
      for (String objective : OBJECTIVES) {
         sb.addObjective(objective, net.minecraft.world.scores.criteria.ObjectiveCriteria.DUMMY, net.minecraft.network.chat.Component.literal(objective),
            net.minecraft.world.scores.criteria.ObjectiveCriteria.RenderType.INTEGER, false, null);
      }
   }

   static final String[] OBJECTIVES = {"kills", "deaths", "Obj 1", "health_bar", "\u00e9"};

   /** The source of every golden run: nothing online, fixed teams/objectives (the Rust test server mirrors them). */
   static final class Src extends net.minecraft.commands.CommandSourceStack {
      Src(PermissionSet perms, FakeServer server) {
         super(net.minecraft.commands.CommandSource.NULL, Vec3.ZERO, Vec2.ZERO, null, perms, net.minecraft.network.chat.Component.empty(), server);
      }

      @Override public Collection<String> getOnlinePlayerNames() { return List.of(); }
      @Override public Stream<net.minecraft.resources.Identifier> getAvailableSounds() { return Stream.empty(); }
      @Override public Stream<net.minecraft.resources.Identifier> getAvailablePostEffects() { return Stream.empty(); }
      @Override public FeatureFlagSet enabledFeatures() { return FeatureFlags.DEFAULT_FLAGS; }
      @Override public <E> CompletableFuture<Suggestions> suggestRegistryElements(ResourceKey<? extends net.minecraft.core.Registry<E>> key, ElementSuggestionType elements,
         com.mojang.brigadier.suggestion.SuggestionsBuilder builder, CommandContext<?> context, java.util.function.Predicate<E> filter) {
         return access.lookup(key).map(registry -> {
            this.suggestRegistryElements((net.minecraft.core.HolderLookup<E>) registry, elements, builder, filter);
            return builder.buildFuture();
         }).orElseGet(Suggestions::empty);
      }
   }

   /** One argument type under test. */
   static final class Spec {
      final String name;
      final java.util.function.Supplier<ArgumentType<?>> type;
      final String[] seeds;
      Function<Object, String> render = v -> String.valueOf(v);
      List<String> variants = List.of("all");
      int target = 320;
      Spec(String name, java.util.function.Supplier<ArgumentType<?>> type, String... seeds) {
         this.name = name;
         this.type = type;
         this.seeds = seeds;
      }
      Spec render(Function<Object, String> r) { render = r; return this; }
      Spec variants(String... v) { variants = List.of(v); return this; }
      Spec target(int t) { target = t; return this; }
   }

   static final Map<String, Spec> SPECS = new LinkedHashMap<>();

   static void add(Spec s) {
      SPECS.put(s.name, s);
   }

   // ------------------------------------------------------------------------------------------------ escaping

   static String esc(String s) {
      StringBuilder b = new StringBuilder();
      for (int i = 0; i < s.length(); i++) {
         char c = s.charAt(i);
         if (c == '\\') {
            b.append("\\\\");
         } else if (c < 0x20 || c == 0x7f) {
            b.append(String.format("\\u%04x", (int) c));
         } else if (Character.isHighSurrogate(c) && i + 1 < s.length() && Character.isLowSurrogate(s.charAt(i + 1))) {
            b.append(c).append(s.charAt(++i));
         } else if (Character.isSurrogate(c)) {
            b.append(String.format("\\u%04x", (int) c));
         } else {
            b.append(c);
         }
      }
      return b.toString();
   }

   // ------------------------------------------------------------------------------------------------ corpus

   static final String POOL = "\"'\\ ~^@#:/_-.,=[]{}()!*+<>$%&0123456789abcdefxyzABCXYZ\u00e9\u00df\u0130\u4e2d\u0000\t\n\uffff\ud83d\ude00\u200b";

   /** Seeds, every non-empty prefix of every seed, then seeded random mutations until `target` unique inputs exist. */
   static List<String> corpus(Spec spec) {
      LinkedHashSet<String> out = new LinkedHashSet<>();
      out.add("");
      for (String seed : spec.seeds) {
         out.add(seed);
      }
      for (String seed : spec.seeds) {
         for (int i = 1; i < seed.length(); i++) {
            if (Character.isHighSurrogate(seed.charAt(i - 1))) continue;
            out.add(seed.substring(0, i));
         }
      }
      Random rnd = new Random(spec.name.hashCode() * 31L + 7);
      int[] pool = POOL.codePoints().toArray();
      String[] seeds = spec.seeds.length == 0 ? new String[] {""} : spec.seeds;
      int guard = 0;
      while (out.size() < spec.target && guard++ < 200000) {
         String seed = seeds[rnd.nextInt(seeds.length)];
         int[] cps = seed.codePoints().toArray();
         List<Integer> l = new ArrayList<>();
         for (int cp : cps) l.add(cp);
         int ops = 1 + rnd.nextInt(2);
         for (int k = 0; k < ops; k++) {
            int pos = l.isEmpty() ? 0 : rnd.nextInt(l.size() + 1);
            switch (rnd.nextInt(5)) {
               case 0 -> l.add(Math.min(pos, l.size()), pool[rnd.nextInt(pool.length)]);
               case 1 -> { if (!l.isEmpty()) l.remove(Math.min(pos, l.size() - 1)); }
               case 2 -> { if (!l.isEmpty()) l.set(Math.min(pos, l.size() - 1), pool[rnd.nextInt(pool.length)]); }
               case 3 -> { if (!l.isEmpty()) l.add(l.get(Math.min(pos, l.size() - 1))); }
               default -> { if (!l.isEmpty()) l.subList(0, Math.min(pos, l.size())).clear(); }
            }
         }
         StringBuilder b = new StringBuilder();
         for (int cp : l) b.appendCodePoint(cp);
         out.add(b.toString());
      }
      return new ArrayList<>(out);
   }

   // ------------------------------------------------------------------------------------------------ recording

   static String suggestions(Suggestions s, int offset) {
      List<Suggestion> list = s.getList();
      StringBuilder all = new StringBuilder();
      List<String> items = new ArrayList<>();
      for (Suggestion x : list) {
         String item = esc(x.getText()) + (x.getTooltip() == null ? "" : "\u0001" + esc(x.getTooltip().getString()));
         items.add(item);
         all.append(item).append('\t');
      }
      int hash = all.toString().hashCode();
      List<String> shown = items.size() > 40 ? items.subList(0, 8) : items;
      return "S " + (s.getRange().getStart() - offset) + "-" + (s.getRange().getEnd() - offset) + " " + items.size() + " " + hash + " " + String.join("\t", shown);
   }

   static String msg(CommandSyntaxException e, int offset) {
      return esc(e.getMessage()) + "@" + (e.getCursor() - offset);
   }

   static void run(Spec spec, Path dir) throws Exception {
      CommandDispatcher<net.minecraft.commands.CommandSourceStack> d = new CommandDispatcher<>();
      d.register(LiteralArgumentBuilder.<net.minecraft.commands.CommandSourceStack>literal("t")
         .then(RequiredArgumentBuilder.<net.minecraft.commands.CommandSourceStack, Object>argument("a", (ArgumentType<Object>) spec.type.get()).executes(c -> 1)));
      List<String> inputs = corpus(spec);
      Path out = dir.resolve(spec.name + "_golden.txt");
      try (PrintWriter w = new PrintWriter(Files.newBufferedWriter(out, StandardCharsets.UTF_8))) {
         for (String variant : spec.variants) {
            FakeServer server = fakeServer();
            populate(server.getScoreboard());
            Src src = new Src(variant.equals("none") ? PermissionSet.NO_PERMISSIONS : PermissionSet.ALL_PERMISSIONS, server);
            for (String input : inputs) {
               String cmd = "t " + input;
               w.println("@ " + variant + " " + esc(input));
               ParseResults<net.minecraft.commands.CommandSourceStack> pr;
               try {
                  pr = d.parse(cmd, src);
               } catch (Throwable t) {
                  w.println("THREW");
                  continue;
               }
               w.println("R " + (pr.getReader().getCursor() - 2));
               ParsedArgument<net.minecraft.commands.CommandSourceStack, ?> arg = pr.getContext().getArguments().get("a");
               if (arg == null) {
                  w.println("A -");
               } else {
                  String v;
                  try {
                     v = spec.render.apply(arg.getResult());
                  } catch (Throwable t) {
                     v = "<render threw " + t + ">";
                  }
                  w.println("A " + (arg.getRange().getStart() - 2) + "-" + (arg.getRange().getEnd() - 2) + " " + esc(v));
               }
               for (CommandSyntaxException e : pr.getExceptions().values()) {
                  w.println("X " + msg(e, 2));
               }
               try {
                  d.execute(pr);
                  w.println("E ok");
               } catch (CommandSyntaxException e) {
                  w.println("E " + msg(e, 2));
               } catch (Throwable t) {
                  w.println("E THREW");
               }
               try {
                  w.println(suggestions(d.getCompletionSuggestions(pr).get(), 2));
               } catch (Throwable t) {
                  w.println("S THREW");
               }
            }
         }
      }
      System.err.println(spec.name + ": " + inputs.size() + " inputs -> " + out);
   }

   public static void main(String[] args) throws Exception {
      access = extractor.Setup.bootstrap();
      ctx = CommandBuildContext.simple(access, FeatureFlags.DEFAULT_FLAGS);
      ArgsSpecs.register();
      Path dir = Path.of(args[0]);
      Set<String> wanted = new HashSet<>(Arrays.asList(args).subList(1, args.length));
      for (Spec s : SPECS.values()) {
         if (wanted.isEmpty() || wanted.contains(s.name)) run(s, dir);
      }
      System.exit(0);
   }
}
