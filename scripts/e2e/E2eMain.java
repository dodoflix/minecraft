import java.lang.reflect.Field;
import java.lang.reflect.Method;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.screens.DisconnectedScreen;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.multiplayer.ClientLevel;

/**
 * Launches the stock 26.3 client unchanged and, from a daemon thread, prints its observable state to stdout as
 * "[E2E] {json}" lines (on change, plus once per second while in a level). The stock client logs nothing for a
 * login/config-phase disconnect or for the current tick count, so scripts/mc-client-status.py reads these lines.
 * Read-only: touches no client state.
 */
public class E2eMain {
    public static void main(String[] args) throws Exception {
        Thread t = new Thread(E2eMain::watch, "e2e-watch");
        t.setDaemon(true);
        t.start();
        net.minecraft.client.main.Main.main(args);
    }

    private static String esc(String s) {
        if (s == null) return "null";
        StringBuilder b = new StringBuilder("\"");
        for (char c : s.toCharArray()) {
            if (c == '"' || c == '\\') b.append('\\').append(c);
            else if (c < 0x20) b.append(String.format("\\u%04x", (int) c));
            else b.append(c);
        }
        return b.append('"').toString();
    }

    private static void watch() {
        boolean joinedEver = false;
        String last = "";
        long lastBeat = 0;
        while (true) {
            try {
                Thread.sleep(200);
                Minecraft mc = Minecraft.getInstance();
                if (mc == null || mc.gui == null) continue;
                Screen screen = mc.gui.screen();
                ClientLevel level = mc.level;
                boolean inLevel = level != null && mc.player != null;
                joinedEver |= inLevel;
                String screenName = screen == null ? null : screen.getClass().getSimpleName();
                String title = screen == null ? null : screen.getTitle().getString();
                String reason = null;
                if (screen instanceof DisconnectedScreen) {
                    Field f = DisconnectedScreen.class.getDeclaredField("details");
                    f.setAccessible(true);
                    Object details = f.get(screen);
                    Method m = details.getClass().getDeclaredMethod("reason");
                    m.setAccessible(true);
                    reason = ((net.minecraft.network.chat.Component) m.invoke(details)).getString();
                }
                long gameTime = inLevel ? level.getLevelData().getGameTime() : -1;
                int chunks = inLevel ? level.getChunkSource().getLoadedChunksCount() : 0;
                String state = "{\"screen\":" + esc(screenName) + ",\"screen_title\":" + esc(title)
                        + ",\"in_level\":" + inLevel + ",\"joined_ever\":" + joinedEver
                        + ",\"disconnect_reason\":" + esc(reason);
                long now = System.currentTimeMillis();
                boolean changed = !state.equals(last);
                if (changed || (inLevel && now - lastBeat >= 1000)) {
                    last = state;
                    lastBeat = now;
                    System.out.println("[E2E] " + state + ",\"game_time\":" + gameTime + ",\"chunks\":" + chunks + "}");
                    System.out.flush();
                }
            } catch (InterruptedException e) {
                return;
            } catch (Throwable ignored) {
                // client still starting up / state mid-transition; try again next tick
            }
        }
    }
}
