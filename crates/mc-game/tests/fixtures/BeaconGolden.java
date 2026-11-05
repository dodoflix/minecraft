// Golden generator for beacon_golden.rs: BeaconBlockEntity.validateEffects over every (primary, secondary, levels) and the
// BeaconMenu effect encoding. Output lines: "primary secondary levels result" (effect = registry id + 1, 0 = none).
//   java -cp "$EXTRACTOR:$CP" BeaconGolden.java > beacon_golden.txt
import net.minecraft.core.Holder;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.effect.MobEffect;
import net.minecraft.world.inventory.BeaconMenu;
import net.minecraft.world.level.block.entity.BeaconBlockEntity;

public class BeaconGolden {
   public static void main(String[] args) throws Exception {
      extractor.Setup.bootstrap();
      int n = BuiltInRegistries.MOB_EFFECT.size();
      StringBuilder b = new StringBuilder();
      for (int p = 0; p <= n; p++) for (int s = 0; s <= n; s++) for (int levels = 0; levels <= 5; levels++) {
         Holder<MobEffect> primary = BeaconMenu.decodeEffect(p);
         Holder<MobEffect> secondary = BeaconMenu.decodeEffect(s);
         boolean r;
         try { r = BeaconBlockEntity.validateEffects(primary, secondary, levels); } catch (NullPointerException e) { r = false; }
         b.append(BeaconMenu.encodeEffect(primary)).append(' ').append(BeaconMenu.encodeEffect(secondary)).append(' ').append(levels).append(' ').append(r).append('\n');
      }
      System.out.print(b);
   }
}
