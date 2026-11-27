import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import net.minecraft.world.level.block.Block;

/** `BlockState.toString()` of every block state, `stateId<TAB>text`, in state id order.
 * Usage: scripts/golden-blocks.sh
 * Output: crates/mc-data/tests/fixtures/block_state_strings.txt.gz */
public class BlockStateToString {
    public static void main(String[] args) throws Exception {
        net.minecraft.SharedConstants.tryDetectVersion();
        net.minecraft.server.Bootstrap.bootStrap();
        int total = Block.BLOCK_STATE_REGISTRY.size();
        StringBuilder out = new StringBuilder();
        for (int id = 0; id < total; id++) {
            out.append(id).append('\t').append(Block.stateById(id)).append('\n');
        }
        Files.writeString(Path.of(args[0]), out, StandardCharsets.UTF_8);
    }
}
