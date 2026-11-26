import com.google.gson.JsonElement;
import com.google.gson.JsonNull;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import com.mojang.blaze3d.platform.InputConstants;
import com.mojang.brigadier.CommandDispatcher;
import com.mojang.brigadier.ParseResults;
import com.mojang.brigadier.StringReader;
import java.io.BufferedReader;
import java.io.FileInputStream;
import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import net.minecraft.client.Minecraft;
import net.minecraft.client.Screenshot;
import net.minecraft.client.gui.components.AbstractWidget;
import net.minecraft.client.gui.screens.ChatScreen;
import net.minecraft.client.gui.screens.ConnectScreen;
import net.minecraft.client.gui.screens.PauseScreen;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.TitleScreen;
import net.minecraft.client.gui.screens.inventory.AbstractContainerScreen;
import net.minecraft.client.gui.screens.advancements.AdvancementsScreen;
import net.minecraft.client.gui.screens.inventory.CreativeModeInventoryScreen;
import net.minecraft.client.gui.screens.inventory.InventoryScreen;
import net.minecraft.client.gui.screens.multiplayer.JoinMultiplayerScreen;
import net.minecraft.client.gui.screens.social.SocialInteractionsScreen;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.input.MouseButtonEvent;
import net.minecraft.client.input.MouseButtonInfo;
import net.minecraft.client.multiplayer.ClientPacketListener;
import net.minecraft.client.multiplayer.ServerData;
import net.minecraft.client.multiplayer.resolver.ServerAddress;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.world.inventory.ContainerInput;
import org.lwjgl.sdl.SDLKeyboard;

/**
 * The command channel of the e2e client: JSON lines read from the FIFO named by -De2e.ctl, each executed on the
 * render thread, answered with one "[E2E-CTL] {id, ok, result|error}" line on stdout. Commands are listed in
 * tests/e2e/client/PROTOCOL.md. The FIFO is reopened after every writer closes it.
 */
final class E2eControl {
    private E2eControl() {}

    static void start() {
        String path = System.getProperty("e2e.ctl");
        if (path == null || path.isEmpty()) return;
        Thread t = new Thread(() -> read(path), "e2e-ctl");
        t.setDaemon(true);
        t.start();
    }

    private static void read(String path) {
        while (true) {
            // opening a FIFO for reading blocks until a writer appears
            try (BufferedReader r = new BufferedReader(new InputStreamReader(new FileInputStream(path), StandardCharsets.UTF_8))) {
                String line;
                while ((line = r.readLine()) != null) {
                    if (!line.isBlank()) submit(line);
                }
            } catch (Exception e) {
                reply(JsonNull.INSTANCE, null, "ctl channel: " + e);
                try {
                    Thread.sleep(500);
                } catch (InterruptedException ie) {
                    return;
                }
            }
        }
    }

    private static void submit(String line) {
        JsonObject cmd;
        JsonElement id = JsonNull.INSTANCE;
        try {
            cmd = JsonParser.parseString(line).getAsJsonObject();
            if (cmd.has("id")) id = cmd.get("id");
        } catch (Exception e) {
            reply(id, null, "bad json: " + e.getMessage());
            return;
        }
        Minecraft mc = Minecraft.getInstance();
        if (mc == null) {
            reply(id, null, "client not started yet");
            return;
        }
        JsonElement fid = id;
        mc.execute(() -> {
            try {
                run(mc, fid, cmd);
            } catch (Throwable e) {
                reply(fid, null, e.toString());
            }
        });
    }

    static void reply(JsonElement id, JsonObject result, String error) {
        JsonObject o = new JsonObject();
        o.add("id", id);
        o.addProperty("ok", error == null);
        if (error != null) o.addProperty("error", error);
        else if (result != null) o.add("result", result);
        System.out.println("[E2E-CTL] " + o);
        System.out.flush();
    }

    private static String str(JsonObject c, String key) {
        if (!c.has(key)) throw new IllegalArgumentException("missing argument: " + key);
        return c.get(key).getAsString();
    }

    private static int num(JsonObject c, String key, Integer dflt) {
        if (c.has(key)) return c.get(key).getAsInt();
        if (dflt == null) throw new IllegalArgumentException("missing argument: " + key);
        return dflt;
    }

    private static ClientPacketListener conn(Minecraft mc) {
        if (mc.player == null || mc.level == null) throw new IllegalStateException("not in a level");
        return mc.player.connection;
    }

    private static JsonObject pathResult(Path p) {
        JsonObject r = new JsonObject();
        r.addProperty("path", p.toAbsolutePath().toString());
        return r;
    }

    private static Path outPath(Minecraft mc, JsonObject c, JsonElement id, String prefix, String ext) {
        String name = c.has("path") ? c.get("path").getAsString() : prefix + (id.isJsonNull() ? Long.toString(System.currentTimeMillis()) : id.getAsString().replaceAll("[^A-Za-z0-9._-]", "_")) + ext;
        Path p = Path.of(name);
        return p.isAbsolute() ? p : mc.gameDirectory.toPath().resolve(p);
    }

    private static void run(Minecraft mc, JsonElement id, JsonObject c) throws Exception {
        String cmd = str(c, "cmd");
        JsonObject result = null;
        switch (cmd) {
            case "snapshot" -> {
                Path p = outPath(mc, c, id, "e2e/snapshot-", ".json");
                Files.createDirectories(p.getParent());
                Files.writeString(p, E2eSnapshot.build(mc).toString() + "\n");
                result = pathResult(p);
            }
            case "chat" -> conn(mc).sendChat(str(c, "text"));
            case "command" -> conn(mc).sendCommand(str(c, "text"));
            case "setScreen" -> setScreen(mc, str(c, "screen"));
            case "closeScreen" -> {
                if (mc.player != null) mc.player.closeContainer();
                else mc.gui.setScreen(null);
            }
            case "click" -> {
                LocalPlayer p = mc.player;
                if (p == null || mc.gameMode == null) throw new IllegalStateException("not in a level");
                ContainerInput type = ContainerInput.valueOf(c.has("type") ? str(c, "type") : "PICKUP");
                mc.gameMode.handleContainerInput(p.containerMenu.containerId, num(c, "slot", null), num(c, "button", 0), type, p);
            }
            case "widgetClick" -> {
                Screen s = mc.gui.screen();
                if (s == null) throw new IllegalStateException("no screen open");
                // by the translation key of the widget's message (`gui.proceed`), or by class and ordinal (`class`, `ordinal` 0 = the
                // first of that class): never by position, which changes with the language, window size and GUI scale
                String key = c.has("key") ? str(c, "key") : null;
                String cls = c.has("class") ? str(c, "class") : null;
                if ((key == null) == (cls == null)) throw new IllegalArgumentException("widgetClick needs exactly one of key and class");
                java.util.List<AbstractWidget> found = new java.util.ArrayList<>();
                for (AbstractWidget candidate : E2eSnapshot.widgets(s)) {
                    if (key != null ? key.equals(E2eSnapshot.messageKey(candidate)) : cls.equals(candidate.getClass().getSimpleName())) found.add(candidate);
                }
                int ordinal = num(c, "ordinal", 0);
                if (ordinal < 0 || ordinal >= found.size()) throw new IllegalStateException("no widget " + (key != null ? key : cls) + " #" + ordinal + " on " + s.getClass().getSimpleName());
                AbstractWidget w = found.get(ordinal);
                MouseButtonEvent e = new MouseButtonEvent(w.getX() + w.getWidth() / 2.0, w.getY() + w.getHeight() / 2.0,
                        new MouseButtonInfo(1, 0)); // 1 = left button in this client's input codes
                if (!w.mouseClicked(e, false)) throw new IllegalStateException("the widget did not take the click: " + w.getMessage().getString());
                w.mouseReleased(e);
            }
            case "key" -> key(mc, str(c, "key"), c.has("action") ? str(c, "action") : "tap");
            case "look" -> {
                if (mc.player == null) throw new IllegalStateException("not in a level");
                mc.player.setYRot(c.get("yaw").getAsFloat());
                mc.player.setXRot(c.get("pitch").getAsFloat());
            }
            // the private methods the real mouse buttons call: they honour the crosshair target, cooldowns and spectator mode
            case "use" -> invoke(mc, "startUseItem");
            case "attack" -> invoke(mc, "startAttack");
            case "hotbar" -> {
                if (mc.player == null) throw new IllegalStateException("not in a level");
                mc.player.getInventory().setSelectedSlot(num(c, "slot", null));
            }
            case "respawn" -> {
                if (mc.player == null) throw new IllegalStateException("not in a level");
                mc.player.respawn();
            }
            case "connect" -> {
                String address = str(c, "address");
                ServerAddress sa = ServerAddress.parseString(address);
                ConnectScreen.startConnecting(new TitleScreen(), mc, sa, new ServerData("e2e", address, ServerData.Type.OTHER), true, null);
            }
            case "disconnect" -> mc.disconnect(new TitleScreen(), false);
            case "screenshot" -> {
                Path p = outPath(mc, c, id, "screenshots/", ".png");
                Files.createDirectories(p.getParent());
                JsonElement fid = id;
                Screenshot.takeScreenshot(mc.gameRenderer.mainRenderTarget(), image -> {
                    try (image) {
                        image.writeToFile(p);
                        reply(fid, pathResult(p), null);
                    } catch (Throwable e) {
                        reply(fid, null, e.toString());
                    }
                });
                return; // answered by the callback once the frame is read back
            }
            case "suggest" -> {
                suggest(conn(mc), id, str(c, "text"));
                return;
            }
            default -> throw new IllegalArgumentException("unknown command: " + cmd);
        }
        reply(id, result, null);
    }

    private static void invoke(Minecraft mc, String method) throws ReflectiveOperationException {
        if (mc.player == null || mc.gameMode == null) throw new IllegalStateException("not in a level");
        java.lang.reflect.Method m = Minecraft.class.getDeclaredMethod(method);
        m.setAccessible(true);
        m.invoke(mc);
    }

    private static void setScreen(Minecraft mc, String name) {
        LocalPlayer p = mc.player;
        Screen s = switch (name) {
            case "null" -> null;
            case "pause" -> new PauseScreen(true);
            case "inventory" -> new InventoryScreen(needPlayer(p));
            case "creative" -> new CreativeModeInventoryScreen(needPlayer(p), needPlayer(p).connection.enabledFeatures(), true);
            case "chat" -> new ChatScreen("", false);
            case "advancements" -> new AdvancementsScreen(needPlayer(p).connection.getAdvancements());
            case "multiplayer" -> new JoinMultiplayerScreen(new TitleScreen());
            case "social" -> new SocialInteractionsScreen();
            default -> throw new IllegalArgumentException("unknown screen: " + name);
        };
        // a container screen leaves through closeContainer(), which tells the server and resets the player's menu
        if (p != null && mc.gui.screen() instanceof AbstractContainerScreen<?>) p.closeContainer();
        mc.gui.setScreen(s);
    }

    private static LocalPlayer needPlayer(LocalPlayer p) {
        if (p == null) throw new IllegalStateException("not in a level");
        return p;
    }

    private static void key(Minecraft mc, String name, String action) {
        String n = name.startsWith("key.") ? name : "key.keyboard." + name.toLowerCase(java.util.Locale.ROOT);
        InputConstants.Key k = InputConstants.getKey(n);
        long handle = mc.getWindow().handle();
        int[] actions = switch (action) {
            case "press" -> new int[] {1};
            case "release" -> new int[] {0};
            case "tap" -> new int[] {1, 0};
            default -> throw new IllegalArgumentException("unknown action: " + action);
        };
        for (int a : actions) {
            if (k.getType() == InputConstants.Type.MOUSE) {
                mc.mouseHandler.onButton(handle, new MouseButtonInfo(k.getValue(), 0), a);
            } else {
                mc.keyboardHandler.keyPress(handle, a,
                        new KeyEvent(k.getValue(), SDLKeyboard.SDL_GetKeyFromScancode(k.getValue(), (short) 0, false), 0));
            }
        }
    }

    private static void suggest(ClientPacketListener conn, JsonElement id, String text) {
        @SuppressWarnings({"unchecked", "rawtypes"})
        CommandDispatcher<Object> d = (CommandDispatcher) conn.getCommands();
        ParseResults<Object> parse = d.parse(new StringReader(text), conn.getSuggestionsProvider());
        d.getCompletionSuggestions(parse).whenComplete((s, err) -> {
            if (err != null) {
                reply(id, null, err.toString());
                return;
            }
            JsonObject r = new JsonObject();
            com.google.gson.JsonArray a = new com.google.gson.JsonArray();
            s.getList().forEach(sug -> {
                JsonObject o = new JsonObject();
                o.addProperty("text", sug.getText());
                o.addProperty("start", sug.getRange().getStart());
                o.addProperty("end", sug.getRange().getEnd());
                if (sug.getTooltip() != null) o.addProperty("tooltip", sug.getTooltip().getString());
                a.add(o);
            });
            r.add("suggestions", a);
            reply(id, r, null);
        });
    }
}
