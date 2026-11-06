// Golden command-tree packets from the real vanilla classes.
// Usage: MC_EXTRA_CP=tools/extractor/build scripts/vanilla-java.sh -o crates/mc-game/tests/fixtures/commands/tree_golden_items.txt crates/mc-game/tests/fixtures/commands/CommandTreeGolden.java crates/mc-game/tests/fixtures/commands/tree_golden_items.txt clear,give,item,loot
// Output: crates/mc-game/tests/fixtures/commands/{tree_golden,tree_golden_items,tree_golden_state,data_tree_golden,execute_tree_golden,function_tree_golden}.txt, one per run;
//   the last argument is the comma-separated list of top-level commands kept in the `subset` lines of that fixture.
// Output lines: `<selection> <permission level> <subset|full> <hex of ClientboundCommandsPacket>`; `subset` keeps only the
// top-level commands listed in SUBSET (registration order preserved) - the commands the Rust port implements.
import com.mojang.brigadier.builder.ArgumentBuilder;
import com.mojang.brigadier.tree.CommandNode;
import com.mojang.brigadier.tree.RootCommandNode;
import io.netty.buffer.ByteBuf;
import io.netty.buffer.Unpooled;
import java.io.PrintWriter;
import java.util.*;
import net.minecraft.commands.*;
import net.minecraft.data.registries.VanillaRegistries;
import net.minecraft.network.FriendlyByteBuf;
import net.minecraft.network.protocol.game.ClientboundCommandsPacket;
import net.minecraft.resources.Identifier;
import net.minecraft.server.permissions.*;

public class CommandTreeGolden {
   static Set<String> SUBSET;

   static <S> void fill(CommandNode<S> source, CommandNode<S> target, S filter, Map<CommandNode<S>, CommandNode<S>> converted, Set<String> only) {
      for (CommandNode<S> child : source.getChildren()) {
         if (only != null && source instanceof RootCommandNode && !only.contains(child.getName())) continue;
         if (child.canUse(filter)) {
            ArgumentBuilder<S, ?> builder = child.createBuilder();
            if (builder.getRedirect() != null) {
               builder.redirect(converted.get(builder.getRedirect()));
            }
            CommandNode<S> node = builder.build();
            converted.put(child, node);
            target.addChild(node);
            if (!child.getChildren().isEmpty()) {
               fill(child, node, filter, converted, null);
            }
         }
      }
   }

   public static void main(String[] args) throws Exception {
      SUBSET = new HashSet<>(Arrays.asList(args[1].split(",")));
      extractor.Setup.bootstrap();
      CommandBuildContext ctx = Commands.createValidationContext(VanillaRegistries.createWorldLookup());
      CommandSourceStack noPerm = Commands.createCompilationContext(PermissionSet.NO_PERMISSIONS);
      ClientboundCommandsPacket.NodeInspector<CommandSourceStack> inspector = new ClientboundCommandsPacket.NodeInspector<CommandSourceStack>() {
         public Identifier suggestionId(com.mojang.brigadier.tree.ArgumentCommandNode<CommandSourceStack, ?> node) {
            var p = node.getCustomSuggestions();
            return p != null ? net.minecraft.commands.synchronization.SuggestionProviders.getName(p) : null;
         }
         public boolean isExecutable(CommandNode<CommandSourceStack> node) { return node.getCommand() != null; }
         public boolean isRestricted(CommandNode<CommandSourceStack> node) { return !node.getRequirement().test(noPerm); }
      };
      try (PrintWriter out = new PrintWriter(args[0])) {
         for (Commands.CommandSelection sel : new Commands.CommandSelection[]{Commands.CommandSelection.DEDICATED, Commands.CommandSelection.ALL}) {
            Commands cmds = new Commands(sel, ctx);
            for (int level = 0; level <= 4; level++) {
               CommandSourceStack source = Commands.createCompilationContext(LevelBasedPermissionSet.forLevel(PermissionLevel.byId(level)));
               for (boolean subset : new boolean[]{false, true}) {
                  if (subset && sel != Commands.CommandSelection.DEDICATED) continue;
                  RootCommandNode<CommandSourceStack> root = new RootCommandNode<>();
                  Map<CommandNode<CommandSourceStack>, CommandNode<CommandSourceStack>> converted = new HashMap<>();
                  converted.put(cmds.getDispatcher().getRoot(), root);
                  fill(cmds.getDispatcher().getRoot(), root, source, converted, subset ? SUBSET : null);
                  ClientboundCommandsPacket packet = new ClientboundCommandsPacket(root, inspector);
                  FriendlyByteBuf buf = new FriendlyByteBuf(Unpooled.buffer());
                  ClientboundCommandsPacket.STREAM_CODEC.encode(buf, packet);
                  byte[] bytes = new byte[buf.readableBytes()];
                  buf.readBytes(bytes);
                  StringBuilder hex = new StringBuilder();
                  for (byte b : bytes) hex.append(String.format("%02x", b));
                  out.println(sel + " " + level + " " + (subset ? "subset" : "full") + " " + hex);
               }
            }
         }
      }
   }
}
