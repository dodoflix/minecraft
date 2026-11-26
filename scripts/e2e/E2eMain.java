/**
 * Launches the stock 26.3 client unchanged next to two helpers: E2eProbe (read-only state: the "[E2E]" heartbeat, the
 * packet tap and the snapshot of E2eSnapshot) and E2eControl (the JSON-lines command channel). The protocol is
 * documented in tests/e2e/client/PROTOCOL.md; scripts/mc-client-status.py reads the output.
 */
public class E2eMain {
    public static void main(String[] args) throws Exception {
        E2eProbe.start();
        E2eControl.start();
        net.minecraft.client.main.Main.main(args);
    }
}
