// Evidence generator for the UpdateTags packet: inner tag order of TagNetworkSerialization.serializeToNetwork over several runs.
// Usage: MC_EXTRA_CP=tools/extractor/build scripts/vanilla-java.sh crates/mc-protocol/tests/fixtures/components/HashOrderTagsProbe.java
// Output: stdout only (evidence probe, no fixture is written)
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.tags.*;
import net.minecraft.resources.*;
import java.util.*;
import java.lang.reflect.*;

public class HashOrderTagsProbe {
   public static void main(String[] a) throws Exception {
      var access = extractor.Setup.bootstrap();
      Method m = TagNetworkSerialization.class.getDeclaredMethod("serializeToNetwork", Registry.class);
      m.setAccessible(true);
      for (Registry<?> r : new Registry<?>[]{BuiltInRegistries.BLOCK, BuiltInRegistries.ITEM, BuiltInRegistries.ENTITY_TYPE, access.lookupOrThrow(Registries.ENCHANTMENT)}) {
         var p = (TagNetworkSerialization.NetworkPayload) m.invoke(null, r);
         StringBuilder sb = new StringBuilder();
         p.tags().keySet().forEach(k -> sb.append(k).append(','));
         System.out.println("TAGS " + r.key().identifier() + " " + p.tags().size() + " " + sb);
      }
   }
}
