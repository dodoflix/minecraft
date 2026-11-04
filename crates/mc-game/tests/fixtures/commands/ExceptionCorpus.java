// Prints `<command>\t<exception message>` for invalid commands, produced by the real vanilla command tree.
// Usage: java -cp tools/extractor/build:$(scripts/vanilla-cp.sh) ExceptionCorpus.java <out> <commands file>
// (one command per line; `~` lines are skipped). Messages are `CommandSyntaxException.getMessage()` (en_us text +
// brigadier's context suffix, newlines escaped as \n).
import com.mojang.brigadier.ParseResults;
import com.mojang.brigadier.exceptions.CommandSyntaxException;
import java.io.PrintWriter;
import java.nio.file.Files;
import java.nio.file.Path;
import net.minecraft.commands.CommandBuildContext;
import net.minecraft.commands.CommandSourceStack;
import net.minecraft.commands.Commands;
import net.minecraft.server.permissions.PermissionSet;
import net.minecraft.data.registries.VanillaRegistries;

public class ExceptionCorpus {
   public static void main(String[] args) throws Exception {
      extractor.Setup.bootstrap();
      CommandBuildContext ctx = Commands.createValidationContext(VanillaRegistries.createWorldLookup());
      Commands cmds = new Commands(Commands.CommandSelection.DEDICATED, ctx);
      CommandSourceStack source = Commands.createCompilationContext(PermissionSet.ALL_PERMISSIONS);
      try (PrintWriter out = new PrintWriter(args[0])) {
         for (String line : Files.readAllLines(Path.of(args[1]))) {
            if (line.isEmpty()) continue;
            ParseResults<CommandSourceStack> parse = cmds.getDispatcher().parse(line, source);
            String message;
            try {
               cmds.getDispatcher().execute(parse);
               message = "<no error>";
            } catch (CommandSyntaxException e) {
               message = e.getMessage();
            } catch (Throwable t) {
               message = "<threw " + t.getClass().getSimpleName() + ">";
            }
            out.println(line + "\t" + message.replace("\n", "\\n"));
         }
      }
   }
}
