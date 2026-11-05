// Evidence generator for STATUS.md "Hash-order findings": run it several times (scripts/vanilla-java.sh style, extractor.Setup on the classpath)
// and compare the ENCH_JSON / ENCH_WIRE (identity-hashed Holder map) and HASHED / COMPS (identity-hashed DataComponentType) lines.
import com.google.gson.JsonElement;
import com.mojang.serialization.*;
import io.netty.buffer.Unpooled;
import net.minecraft.core.component.*;
import net.minecraft.nbt.*;
import net.minecraft.network.*;
import net.minecraft.resources.*;
import net.minecraft.util.*;
import net.minecraft.world.item.*;
import net.minecraft.world.item.enchantment.*;
import java.util.*;

public class HashOrderProbe {
   public static void main(String[] a) throws Exception {
      var access = extractor.Setup.bootstrap();
      var jops = access.createSerializationContext(JsonOps.INSTANCE);
      var hops = access.createSerializationContext(HashOps.CRC32C_INSTANCE);
      String j = "{\"minecraft:sharpness\":5,\"minecraft:unbreaking\":3,\"minecraft:mending\":1,\"minecraft:fortune\":3,\"minecraft:looting\":2,\"minecraft:efficiency\":5,\"minecraft:protection\":4,\"minecraft:thorns\":3}";
      ItemEnchantments e = ItemEnchantments.CODEC.parse(jops, StrictJsonParser.parse(j)).getOrThrow();
      System.out.println("ENCH_JSON " + ItemEnchantments.CODEC.encodeStart(jops, e).getOrThrow());
      var buf = new RegistryFriendlyByteBuf(Unpooled.buffer(), access);
      ItemEnchantments.STREAM_CODEC.encode(buf, e);
      byte[] b = new byte[buf.readableBytes()]; buf.readBytes(b);
      StringBuilder sb = new StringBuilder();
      for (byte x : b) sb.append(String.format("%02x", x));
      System.out.println("ENCH_WIRE " + sb);
      StringBuilder ks = new StringBuilder();
      e.keySet().forEach(h -> ks.append(h.getRegisteredName()).append(' ').append(System.identityHashCode(h)).append(' '));
      System.out.println("ENCH_KEYS " + ks);
      // 10-component stack
      String s = "{\"id\":\"minecraft:diamond_sword\",\"count\":1,\"components\":{\"minecraft:custom_name\":\"x\",\"minecraft:damage\":3,\"minecraft:repair_cost\":2,\"minecraft:enchantments\":{\"minecraft:sharpness\":2},\"minecraft:unbreakable\":{},\"minecraft:max_damage\":10,\"minecraft:rarity\":\"epic\",\"minecraft:lore\":[\"a\"],\"minecraft:custom_model_data\":{\"floats\":[1.0]},\"minecraft:glider\":{},\"minecraft:enchantment_glint_override\":true,\"minecraft:item_name\":\"n\"}}";
      ItemStack st = ItemStack.CODEC.parse(jops, StrictJsonParser.parse(s)).getOrThrow();
      HashedPatchMap.HashGenerator gen = c -> c.encodeValue(hops).getOrThrow().asInt();
      buf = new RegistryFriendlyByteBuf(Unpooled.buffer(), access);
      HashedStack.STREAM_CODEC.encode(buf, HashedStack.create(st, gen));
      b = new byte[buf.readableBytes()]; buf.readBytes(b);
      sb = new StringBuilder();
      for (byte x : b) sb.append(String.format("%02x", x));
      System.out.println("HASHED " + sb);
      System.out.println("PATCH " + st.getComponentsPatch());
      StringBuilder cs = new StringBuilder();
      for (var t : st.getComponents()) cs.append(t.type()).append(' ');
      System.out.println("COMPS " + cs);
   }
}
