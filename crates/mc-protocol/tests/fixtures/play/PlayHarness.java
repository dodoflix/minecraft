// Shared harness for the play-packet golden generators (tests/fixtures/play/Gen*.java).
// Boots vanilla like a dedicated server (registries + data pack, see tools/extractor Setup) and encodes packets through
// the real `GameProtocols` codecs bound to a RegistryFriendlyByteBuf, so the bytes are exactly what vanilla puts on the wire.
// Output lines: `label:hex` where hex = VarInt packet id + body. Use `gen.sh Gen<Group>.java` to run a generator.
import io.netty.buffer.ByteBuf;
import io.netty.buffer.ByteBufUtil;
import io.netty.buffer.Unpooled;
import java.io.PrintWriter;
import net.minecraft.core.RegistryAccess;
import net.minecraft.network.ProtocolInfo;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.network.codec.StreamCodec;
import net.minecraft.network.protocol.Packet;
import net.minecraft.network.protocol.game.GameProtocols;

public final class PlayHarness {
   public static RegistryAccess.Frozen access;
   public static ProtocolInfo<?> clientbound;
   public static ProtocolInfo<?> serverbound;
   private static PrintWriter out;

   /** Serverbound codec context: creative mode (set_creative_mode_slot) and command-block permission (command_suggestion). */
   public static boolean infiniteMaterials = true;
   public static boolean commandBlocks = true;

   public static void init(String outFile) throws Exception {
      access = extractor.Setup.bootstrap();
      clientbound = GameProtocols.CLIENTBOUND_TEMPLATE.bind(RegistryFriendlyByteBuf.decorator(access));
      rebindServerbound();
      out = new PrintWriter(outFile);
   }

   public static void rebindServerbound() {
      serverbound = GameProtocols.SERVERBOUND_TEMPLATE.bind(RegistryFriendlyByteBuf.decorator(access), new GameProtocols.Context() {
         public boolean hasInfiniteMaterials() { return infiniteMaterials; }
         public boolean canUseCommandBlocks() { return commandBlocks; }
      });
   }

   @SuppressWarnings({"rawtypes", "unchecked"})
   private static void emit(String label, ProtocolInfo<?> info, Packet<?> p) {
      ByteBuf buf = Unpooled.buffer();
      ((StreamCodec) info.codec()).encode(buf, p);
      out.println(label + ":" + ByteBufUtil.hexDump(buf));
   }

   /** Clientbound play packet. */
   public static void cb(String label, Packet<?> p) { emit(label, clientbound, p); }

   /** Serverbound play packet (uses the current `infiniteMaterials`/`commandBlocks`). */
   public static void sb(String label, Packet<?> p) { emit(label, serverbound, p); }

   /** Raw line: encode with an arbitrary encoder into a RegistryFriendlyByteBuf, hex without packet id (for nested types). */
   public static void raw(String label, java.util.function.Consumer<RegistryFriendlyByteBuf> writer) {
      RegistryFriendlyByteBuf buf = new RegistryFriendlyByteBuf(Unpooled.buffer(), access);
      writer.accept(buf);
      out.println(label + ":" + ByteBufUtil.hexDump(buf));
   }

   public static void close() { out.close(); }
}
