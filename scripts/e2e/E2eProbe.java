import com.google.gson.JsonObject;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.util.Map;
import java.util.concurrent.ConcurrentSkipListMap;
import java.util.concurrent.atomic.LongAdder;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.screens.DisconnectedScreen;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.multiplayer.JoinMultiplayerScreen;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.multiplayer.ServerData;
import org.apache.logging.log4j.Level;
import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.core.LogEvent;
import org.apache.logging.log4j.core.LoggerContext;
import org.apache.logging.log4j.core.appender.AbstractAppender;
import org.apache.logging.log4j.core.config.Configuration;
import org.apache.logging.log4j.core.config.LoggerConfig;
import org.apache.logging.log4j.core.config.Property;

/**
 * Read-only observation of the client: a daemon thread prints "[E2E] {json}" lines (on change, plus once per second
 * while in a level) and a packet tap counts every packet by protocol phase and type. The stock client logs nothing for
 * a login/config-phase disconnect or for the current tick count, so these lines are the only source of both.
 */
final class E2eProbe {
    /** Packets by "PROTOCOL:flow/type", in = received by the client, out = sent by it. */
    static final Map<String, LongAdder> IN = new ConcurrentSkipListMap<>();
    static final Map<String, LongAdder> OUT = new ConcurrentSkipListMap<>();

    private E2eProbe() {}

    static void start() {
        tap();
        Thread t = new Thread(E2eProbe::watch, "e2e-watch");
        t.setDaemon(true);
        t.start();
    }

    /**
     * PacketDecoder and PacketEncoder log every packet at DEBUG with its protocol and type; an appender on exactly those
     * two loggers sees every phase from the first handshake packet on, with no race against connection setup. The
     * loggers stay additive so their ERROR lines still reach the log.
     */
    private static void tap() {
        LoggerContext ctx = (LoggerContext) LogManager.getContext(false);
        Configuration cfg = ctx.getConfiguration();
        AbstractAppender app = new AbstractAppender("e2e-tap", null, null, true, Property.EMPTY_ARRAY) {
            @Override
            public void append(LogEvent e) {
                Object[] p = e.getMessage().getParameters();
                if (e.getLevel() != Level.DEBUG || p == null || p.length < 2) return; // not the encoder's ERROR line
                (e.getLoggerName().endsWith("PacketDecoder") ? IN : OUT)
                        .computeIfAbsent(p[0] + ":" + p[1], k -> new LongAdder()).increment();
            }
        };
        app.start();
        for (String name : new String[] {"net.minecraft.network.PacketDecoder", "net.minecraft.network.PacketEncoder"}) {
            LoggerConfig lc = new LoggerConfig(name, Level.DEBUG, true);
            lc.addAppender(app, null, null);
            cfg.addLogger(name, lc);
        }
        ctx.updateLoggers();
    }

    static long total(Map<String, LongAdder> m) {
        return m.values().stream().mapToLong(LongAdder::sum).sum();
    }

    static JsonObject counts(Map<String, LongAdder> m) {
        JsonObject o = new JsonObject();
        m.forEach((k, v) -> o.addProperty(k, v.sum()));
        return o;
    }

    /** On the multiplayer screen: whether every listed server has answered its status ping (or failed to); else null. */
    static Boolean serversPinged(Screen screen) {
        if (!(screen instanceof JoinMultiplayerScreen jm)) return null;
        for (int i = 0; i < jm.getServers().size(); i++) {
            ServerData.State s = jm.getServers().get(i).state();
            if (s == ServerData.State.INITIAL || s == ServerData.State.PINGING) return false;
        }
        return true;
    }

    /** Text of the disconnect screen, which has no public accessor. */
    static String disconnectReason(Screen screen) throws ReflectiveOperationException {
        if (!(screen instanceof DisconnectedScreen)) return null;
        Field f = DisconnectedScreen.class.getDeclaredField("details");
        f.setAccessible(true);
        Object details = f.get(screen);
        Method m = details.getClass().getDeclaredMethod("reason");
        m.setAccessible(true);
        return ((net.minecraft.network.chat.Component) m.invoke(details)).getString();
    }

    private static void watch() {
        boolean joinedEver = false;
        String last = "";
        long lastBeat = 0;
        while (true) {
            try {
                // a screen that lives for one client tick (a prompt's parent after Disconnect) must not slip between two samples
                Thread.sleep(5);
                Minecraft mc = Minecraft.getInstance();
                if (mc == null || mc.gui == null) continue;
                Screen screen = mc.gui.screen();
                ClientLevel level = mc.level;
                boolean inLevel = level != null && mc.player != null;
                joinedEver |= inLevel;
                JsonObject o = new JsonObject();
                o.addProperty("screen", screen == null ? null : screen.getClass().getSimpleName());
                o.addProperty("screen_title", screen == null ? null : screen.getTitle().getString());
                o.addProperty("in_level", inLevel);
                o.addProperty("joined_ever", joinedEver);
                o.addProperty("disconnect_reason", disconnectReason(screen));
                o.addProperty("servers_pinged", serversPinged(screen));
                String state = o.toString();
                o.addProperty("t", System.currentTimeMillis());
                o.addProperty("game_time", inLevel ? level.getLevelData().getGameTime() : -1);
                o.addProperty("chunks", inLevel ? level.getChunkSource().getLoadedChunksCount() : 0);
                o.addProperty("packets_in", total(IN));
                o.addProperty("packets_out", total(OUT));
                long now = System.currentTimeMillis();
                boolean changed = !state.equals(last);
                if (changed || (inLevel && now - lastBeat >= 1000)) {
                    last = state;
                    lastBeat = now;
                    System.out.println("[E2E] " + o);
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
