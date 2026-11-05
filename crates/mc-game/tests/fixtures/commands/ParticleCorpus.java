// Prints `<input>\tOK\t<wire hex>` / `<input>\tERR\t<message>` for `ParticleArgument.readParticle` over the lines of a file,
// produced by the real vanilla classes (wire hex = `ParticleTypes.STREAM_CODEC`).
// Usage: scripts/vanilla-java.sh ParticleCorpus.java <commands file> > particle_corpus.txt
import com.mojang.brigadier.StringReader;
import com.mojang.brigadier.exceptions.CommandSyntaxException;
import io.netty.buffer.ByteBuf;
import io.netty.buffer.ByteBufUtil;
import io.netty.buffer.Unpooled;
import java.nio.file.Files;
import java.nio.file.Path;
import net.minecraft.SharedConstants;
import net.minecraft.commands.CommandBuildContext;
import net.minecraft.commands.Commands;
import net.minecraft.commands.arguments.ParticleArgument;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.particles.ParticleOptions;
import net.minecraft.core.particles.ParticleTypes;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.data.registries.VanillaRegistries;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.server.Bootstrap;

public class ParticleCorpus {
   public static void main(String[] args) throws Exception {
      SharedConstants.tryDetectVersion();
      Bootstrap.bootStrap();
      CommandBuildContext ctx = Commands.createValidationContext(VanillaRegistries.createWorldLookup());
      RegistryAccess access = RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
      for (String line : Files.readAllLines(Path.of(args[0]))) {
         if (line.isEmpty()) continue;
         String result;
         try {
            StringReader reader = new StringReader(line);
            ParticleOptions options = ParticleArgument.readParticle(reader, ctx);
            RegistryFriendlyByteBuf buf = new RegistryFriendlyByteBuf(Unpooled.buffer(), access);
            ParticleTypes.STREAM_CODEC.encode(buf, options);
            result = "OK\t" + ByteBufUtil.hexDump(buf) + "\t" + reader.getRemaining().length();
         } catch (CommandSyntaxException e) {
            result = "ERR\t" + e.getMessage().replace("\n", "\\n");
         } catch (Throwable t) {
            result = "ERR\t<threw " + t.getClass().getSimpleName() + ": " + t.getMessage() + ">";
         }
         System.out.println(line + "\t" + result);
      }
   }
}
