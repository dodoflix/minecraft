import java.nio.file.*;
import java.util.*;
import net.minecraft.CrashReport;
import net.minecraft.ReportType;
import net.minecraft.SharedConstants;

/** `CrashReport.getFriendlyReport` for a fixed exception, categories and system rows, per report type, with what differs
 * between runs and between a JVM and a native server taken out: the witty comment, the time, the `-- Head --` block and the
 * `Stacktrace:` blocks (Java frames), and every `System Details` row but the version, operating system and CPU count.
 * Usage: scripts/vanilla-java.sh -o crates/mc-game-core/tests/fixtures/crash_report_golden.txt crates/mc-game-core/tests/fixtures/CrashReportGolden.java crates/mc-game-core/tests/fixtures/crash_report_golden.txt
 * Output: crates/mc-game-core/tests/fixtures/crash_report_golden.txt */
public class CrashReportGolden {
    static final Set<String> KEPT_ROWS = Set.of("Minecraft Version", "Minecraft Version ID", "Operating System", "CPUs", "Server Running", "Players");
    static final Set<String> MASKED_ROWS = Set.of("Operating System", "CPUs");

    static String normalise(String report) {
        List<String> out = new ArrayList<>();
        boolean head = false, stack = false, system = false;
        String[] lines = report.split("\n", -1);
        for (int i = 0; i < lines.length; i++) {
            String line = lines[i];
            if (line.equals("-- Head --")) { head = true; continue; }
            if (head) { head = !line.isEmpty(); continue; }
            if (line.equals("Stacktrace:")) { stack = true; continue; }
            if (stack) {
                if (line.startsWith("\tat ")) continue;
                stack = false;
            }
            if (i == 1) line = "// <comment>";
            else if (line.startsWith("Time: ")) line = "Time: <time>";
            else if (line.equals("-- System Details --")) system = true;
            else if (system && line.startsWith("\t")) {
                String key = line.substring(1, line.indexOf(':'));
                if (!KEPT_ROWS.contains(key)) continue;
                if (MASKED_ROWS.contains(key)) line = "\t" + key + ": <value>";
            }
            out.add(line);
        }
        return String.join("\n", out);
    }

    static Throwable exception() {
        Throwable t = new IllegalStateException("boom");
        t.setStackTrace(new StackTraceElement[] {
            new StackTraceElement("com.example.Server", "tick", "Server.java", 42),
            new StackTraceElement("java.lang.Thread", "run", "Thread.java", 1583)
        });
        return t;
    }

    static CrashReport report(String title, boolean categories) {
        CrashReport report = new CrashReport(title, exception());
        if (categories) {
            report.addCategory("Performance stats").setDetail("Random tick rate", "3").setDetail("Level stats", "minecraft:overworld: 12");
            report.addCategory("Empty category");
            report.addCategory("Detail kinds").setDetail("Number", 5).setDetail("Text", "two\nlines");
        }
        report.getSystemReport().setDetail("Server Running", "true");
        report.getSystemReport().setDetail("Server Running", "false");
        report.getSystemReport().setDetail("Players", "0 / 20; []");
        return report;
    }

    static final StringBuilder OUT = new StringBuilder();

    static void print(String name, String text) {
        OUT.append("## ").append(name).append('\n').append(text).append('\n');
    }

    public static void main(String[] a) throws Exception {
        SharedConstants.tryDetectVersion();
        CrashReport crash = report("Exception in server tick loop", true);
        print("crash with categories", normalise(crash.getFriendlyReport(ReportType.CRASH)));
        print("crash details", normalise(crash.getDetails()));
        print("crash without categories", normalise(report("Watching Server", false).getFriendlyReport(ReportType.CRASH)));
        print("profile with extra comments", normalise(report("Profiling", false).getFriendlyReport(ReportType.PROFILE, List.of("first extra", "second extra"))));
        print("test", normalise(report("Test failed", true).getFriendlyReport(ReportType.TEST)));
        print("network protocol error", normalise(report("Packet", true).getFriendlyReport(ReportType.NETWORK_PROTOCOL_ERROR, List.of("one extra"))));
        print("chunk io error", normalise(report("Chunk save failure", true).getFriendlyReport(ReportType.CHUNK_IO_ERROR)));
        Files.writeString(Path.of(a[0]), OUT.toString());
    }
}
