// Golden: ChunkLevel constants, both ChunkPyramids (dependencies, write radius), TicketStorage NBT codec,
// ChunkTrackingView.difference/contains. Run:
//   scripts/vanilla-java.sh crates/mc-game/tests/fixtures/chunkmap/ChunkLevelGolden.java crates/mc-game/tests/fixtures/chunkmap/chunk_level_golden.txt
import com.mojang.serialization.DataResult;
import java.io.PrintWriter;
import java.util.*;
import net.minecraft.nbt.*;
import net.minecraft.server.level.*;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.TicketStorage;
import net.minecraft.world.level.chunk.status.*;

public class ChunkLevelGolden {
    public static void main(String[] args) throws Exception {
        net.minecraft.SharedConstants.tryDetectVersion();
        net.minecraft.server.Bootstrap.bootStrap();
        try (PrintWriter out = new PrintWriter(args[0])) {
            out.println("MAX_LEVEL " + ChunkLevel.MAX_LEVEL + " RADIUS " + ChunkLevel.RADIUS_AROUND_FULL_CHUNK);
            for (ChunkStatus s : ChunkStatus.getStatusList()) {
                out.println("BYSTATUS " + s.getName() + " " + ChunkLevel.byStatus(s));
            }
            for (int l = 0; l <= ChunkLevel.MAX_LEVEL + 1; l++) {
                ChunkStatus gs = ChunkLevel.generationStatus(l);
                out.println("GENSTATUS " + l + " " + (gs == null ? "none" : gs.getName()) + " " + ChunkLevel.fullStatus(l));
            }
            for (ChunkPyramid p : new ChunkPyramid[]{ChunkPyramid.GENERATION_PYRAMID, ChunkPyramid.LOADING_PYRAMID}) {
                out.println(p == ChunkPyramid.GENERATION_PYRAMID ? "PYRAMID gen" : "PYRAMID load");
                for (ChunkStep step : p.steps()) {
                    out.println("STEP " + step.targetStatus().getName() + " direct=" + step.directDependencies() + " acc=" + step.accumulatedDependencies() + " write=" + step.blockStateWriteRadius());
                }
            }
            out.println("MAXCOORD " + ChunkPyramid.MAX_CHUNK_COORDINATE_VALUE);
            // TicketStorage codec
            TicketStorage ts = new TicketStorage();
            ts.addTicket(new Ticket(TicketType.FORCED, 31), new ChunkPos(3, -4));
            ts.addTicket(new Ticket(TicketType.PORTAL, 30), new ChunkPos(0, 0));
            ts.addTicket(new Ticket(TicketType.PLAYER_LOADING, 31), new ChunkPos(5, 5));
            ts.addTicket(new Ticket(TicketType.UNKNOWN, 20), new ChunkPos(-7, 9));
            for (int i = 0; i < 6; i++) ts.addTicket(new Ticket(TicketType.FORCED, 31), new ChunkPos(i * 11 - 20, 100 - i * 7));
            DataResult<Tag> enc = TicketStorage.CODEC.encodeStart(NbtOps.INSTANCE, ts);
            out.println("ENC " + enc.getOrThrow());
            String[] inputs = {
                "{tickets:[{chunk_pos:[I;1,2],type:\"minecraft:portal\",level:30,ticks_left:5L}]}",
                "{tickets:[{chunk_pos:[I;1,2],type:\"minecraft:portal\",level:30}]}",
                "{}",
                "{tickets:[]}",
                "{tickets:[{chunk_pos:[I;1,2,3],type:\"minecraft:portal\",level:30}]}",
                "{tickets:[{chunk_pos:[I;1,2],type:\"minecraft:nope\",level:30}]}",
                "{tickets:[{chunk_pos:[I;1,2],type:\"minecraft:portal\",level:-1}]}",
                "{tickets:[{chunk_pos:[I;1,2],type:\"minecraft:portal\",level:3.9d,ticks_left:7b}]}",
            };
            for (String in : inputs) {
                DataResult<TicketStorage> r = TicketStorage.CODEC.parse(NbtOps.INSTANCE, TagParser.parseCompoundFully(in));
                if (r.result().isPresent()) {
                    out.println("DEC ok " + TicketStorage.CODEC.encodeStart(NbtOps.INSTANCE, r.result().get()).getOrThrow());
                } else {
                    out.println("DEC err");
                }
            }
            // tracking view
            Random rnd = new Random(7);
            for (int i = 0; i < 60; i++) {
                ChunkTrackingView a = ChunkTrackingView.of(new ChunkPos(rnd.nextInt(12) - 6, rnd.nextInt(12) - 6), 2 + rnd.nextInt(8));
                ChunkTrackingView b = rnd.nextInt(6) == 0 ? ChunkTrackingView.EMPTY : ChunkTrackingView.of(new ChunkPos(rnd.nextInt(30) - 15, rnd.nextInt(30) - 15), 2 + rnd.nextInt(8));
                ChunkTrackingView from = rnd.nextInt(8) == 0 ? ChunkTrackingView.EMPTY : a;
                StringBuilder sb = new StringBuilder("DIFF " + describe(from) + " " + describe(b));
                List<String> enter = new ArrayList<>(), leave = new ArrayList<>();
                ChunkTrackingView.difference(from, b, p -> enter.add(p.x() + "," + p.z()), p -> leave.add(p.x() + "," + p.z()));
                out.println(sb + " | " + String.join(" ", enter) + " | " + String.join(" ", leave));
            }
        }
    }
    static String describe(ChunkTrackingView v) {
        return v instanceof ChunkTrackingView.Positioned p ? p.center().x() + "," + p.center().z() + "," + p.viewDistance() : "E";
    }
}
