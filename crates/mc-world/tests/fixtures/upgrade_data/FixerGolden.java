// Golden generator for upgrade_data::BlockFixer: runs the real vanilla `UpgradeData.BlockFixers` (via reflection).
//   FIXMAP  <block> <fixer>                       every block's fixer (MAP.getOrDefault(block, DEFAULT))
//   CHEST / STEM <state> <direction> <neighbour> <result state | EXC:class> <events>
//           only the combinations where the result differs from `state`, events were recorded, or an exception was thrown
// Run: scripts/vanilla-java.sh crates/mc-world/tests/fixtures/upgrade_data/FixerGolden.java crates/mc-world/tests/fixtures/upgrade_data/fixer_golden.txt
import java.io.PrintWriter;
import java.lang.reflect.*;
import java.util.*;
import net.minecraft.commands.arguments.blocks.BlockStateParser;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.level.LevelAccessor;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.UpgradeData;

public class FixerGolden {
    static String s(BlockState st) { return BlockStateParser.serialize(st); }

    public static void main(String[] args) throws Exception {
        net.minecraft.SharedConstants.tryDetectVersion();
        net.minecraft.server.Bootstrap.bootStrap();
        Class<?> fixersClass = Class.forName("net.minecraft.world.level.chunk.UpgradeData$BlockFixers");
        Object[] constants = fixersClass.getEnumConstants();
        Field mapField = UpgradeData.class.getDeclaredField("MAP");
        mapField.setAccessible(true);
        @SuppressWarnings("unchecked")
        Map<Block, Object> map = (Map<Block, Object>) mapField.get(null);
        Map<String, Object> byName = new HashMap<>();
        for (Object c : constants) byName.put(c.toString(), c);
        Method update = UpgradeData.BlockFixer.class.getMethod("updateShape", BlockState.class, Direction.class, BlockState.class, LevelAccessor.class, BlockPos.class, BlockPos.class);

        try (PrintWriter out = new PrintWriter(args[0])) {
            for (Block b : BuiltInRegistries.BLOCK) {
                Object f = map.getOrDefault(b, byName.get("DEFAULT"));
                out.println("FIXMAP\t" + BuiltInRegistries.BLOCK.getKey(b) + "\t" + f);
            }
            List<String> events = new ArrayList<>();
            LevelAccessor level = (LevelAccessor) Proxy.newProxyInstance(FixerGolden.class.getClassLoader(), new Class<?>[] {LevelAccessor.class}, (proxy, m, a) -> {
                switch (m.getName()) {
                    case "setBlock" -> {
                        BlockPos p = (BlockPos) a[0];
                        events.add("setBlock(" + p.getX() + "," + p.getY() + "," + p.getZ() + "," + s((BlockState) a[1]) + "," + a[2] + (a.length > 3 ? "," + a[3] : "") + ")");
                        return true;
                    }
                    case "getBlockEntity" -> {
                        BlockPos p = (BlockPos) a[0];
                        events.add("getBE(" + p.getX() + "," + p.getY() + "," + p.getZ() + ")");
                        return null;
                    }
                    case "hashCode" -> { return 1; }
                    case "equals" -> { return proxy == a[0]; }
                    case "toString" -> { return "level"; }
                }
                Class<?> r = m.getReturnType();
                if (r == boolean.class) return false;
                if (r == int.class) return 0;
                if (r == long.class) return 0L;
                if (r == float.class) return 0f;
                if (r == double.class) return 0d;
                return null;
            });
            BlockPos pos = new BlockPos(5, 64, 5);
            List<BlockState> others = List.of(Blocks.STONE.defaultBlockState(), Blocks.PUMPKIN.defaultBlockState(), Blocks.MELON.defaultBlockState(),
                Blocks.CARVED_PUMPKIN.defaultBlockState(), Blocks.ATTACHED_PUMPKIN_STEM.defaultBlockState(), Blocks.AIR.defaultBlockState());
            for (String which : new String[] {"CHEST", "STEM_BLOCK"}) {
                Object fixer = byName.get(which);
                List<BlockState> states = new ArrayList<>();
                List<BlockState> neighbours = new ArrayList<>();
                if (which.equals("CHEST")) {
                    for (Block b : new Block[] {Blocks.CHEST, Blocks.TRAPPED_CHEST}) {
                        for (BlockState st : b.getStateDefinition().getPossibleStates()) {
                            states.add(st);
                            if (!s(st).contains("waterlogged=true")) neighbours.add(st);
                        }
                    }
                    neighbours.addAll(others);
                } else {
                    for (Block b : new Block[] {Blocks.PUMPKIN_STEM, Blocks.MELON_STEM}) states.addAll(b.getStateDefinition().getPossibleStates());
                    neighbours.addAll(others);
                    neighbours.addAll(states);
                }
                for (BlockState st : states) {
                    for (BlockState nb : neighbours) {
                        for (Direction d : Direction.values()) {
                            events.clear();
                            String res;
                            try {
                                BlockState r = (BlockState) update.invoke(fixer, st, d, nb, level, pos, pos.relative(d));
                                res = s(r);
                            } catch (InvocationTargetException e) {
                                res = "EXC:" + e.getCause().getClass().getName();
                            }
                            if (!res.equals(s(st)) || !events.isEmpty())
                                out.println((which.equals("CHEST") ? "CHEST" : "STEM") + "\t" + s(st) + "\t" + d.getName() + "\t" + s(nb) + "\t" + res + "\t" + String.join(";", events));
                        }
                    }
                }
            }
        }
    }
}
