import agenttest.Sample;
import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.HashMap;
import java.util.Map;
import java.util.zip.GZIPInputStream;

/**
 * Self-test of the coverage agent.
 *
 * Usage: scripts/vanilla-agent.sh selftest
 * Output: "agent self-test passed" on success; any failed expectation throws.
 */
public final class Selftest {
    private static Map<String, String[]> rows(Path file) throws Exception {
        Map<String, String[]> rows = new HashMap<>();
        try (BufferedReader r = new BufferedReader(new InputStreamReader(new GZIPInputStream(Files.newInputStream(file))))) {
            for (String line; (line = r.readLine()) != null; ) {
                String[] f = line.split("\t");
                if (!line.startsWith("#") && f[0].equals("agenttest/Sample")) {
                    rows.put(f[1], f);
                }
            }
        }
        return rows;
    }

    private static int bits(String[] row) {
        int n = 0;
        for (char c : row[5].toCharArray()) {
            n += Integer.bitCount(Character.digit(c, 16));
        }
        return n;
    }

    private static void check(boolean ok, String what) {
        if (!ok) {
            throw new AssertionError(what);
        }
    }

    public static void main(String[] args) throws Exception {
        Path out = Path.of(args[0]);
        Sample.pick(1);
        Sample.sw(1);
        Sample.parse("7");
        Sample.loop(3);
        mcagent.Agent.dump(out);
        Map<String, String[]> a = rows(out);
        check(a.get("unused")[5].equals("0"), "unused method must not be marked");
        check(a.get("pick")[4].equals("3") && bits(a.get("pick")) == 2, "pick(1): entry and one branch only: " + String.join(" ", a.get("pick")));
        check(bits(a.get("sw")) < Integer.parseInt(a.get("sw")[4]), "sw(1) leaves the other cases unmarked");
        check(bits(a.get("sw")) >= 2, "sw(1) marks entry and its case");
        check(bits(a.get("parse")) == Integer.parseInt(a.get("parse")[4]) - 1, "parse(\"7\") leaves the handler unmarked");
        check(bits(a.get("loop")) == Integer.parseInt(a.get("loop")[4]), "a loop that runs marks every block");
        Sample.pick(-1);
        Sample.sw(0);
        Sample.sw(5);
        Sample.parse("x");
        mcagent.Agent.dump(out);
        Map<String, String[]> b = rows(out);
        for (String m : new String[] {"pick", "sw", "parse", "loop"}) {
            check(bits(b.get(m)) == Integer.parseInt(b.get(m)[4]), m + " fully marked after both branches ran");
        }
        System.out.println("agent self-test passed");
    }
}
