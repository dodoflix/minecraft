import net.minecraft.core.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.decoration.ItemFrame;
import net.minecraft.world.entity.monster.Shulker;
import net.minecraft.world.item.*;
import net.minecraft.world.level.chunk.status.ChunkStatus;
import net.minecraft.world.level.block.Rotation;

public class EntityProbe {
    public static void main(String[] a) throws Exception {
        FeatureOracleBase.setup();
        var region = FeatureOracleBase.newRegion(1L, 0, 0, ChunkStatus.FEATURES, 1);
        for (var type : new EntityType<?>[]{EntityTypes.EVOKER, EntityTypes.VINDICATOR, EntityTypes.ALLAY}) {
            Mob mob = (Mob) type.create(region.getLevel(), EntitySpawnReason.STRUCTURE);
            mob.setPersistenceRequired();
            mob.snapTo(new BlockPos(5, 70, 6), 0.0F, 0.0F);
            mob.finalizeSpawn(region, region.getCurrentDifficultyAt(mob.blockPosition()), EntitySpawnReason.STRUCTURE, null);
            region.addFreshEntityWithPassengers(mob);
        }
        Shulker s = EntityTypes.SHULKER.create(region.getLevel(), EntitySpawnReason.STRUCTURE);
        s.setPos(5.5, 70, 6.5);
        region.addFreshEntity(s);
        ItemFrame f = new ItemFrame(region.getLevel(), new BlockPos(5, 70, 6), Rotation.NONE.rotate(Direction.SOUTH));
        f.setItem(new ItemStack(Items.ELYTRA), false);
        region.addFreshEntity(f);
        for (var e : ((net.minecraft.world.level.chunk.ProtoChunk) region.getChunk(0, 0)).getEntities()) System.out.println(e);
    }
}
