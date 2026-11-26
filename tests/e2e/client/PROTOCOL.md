# Real-client probe and control protocol

The real client (`scripts/real-client.sh`) runs the stock jar behind `scripts/e2e/E2eMain.java`. Everything the harness
reads from or sends to the client goes through the lines below. All JSON is one object per line, UTF-8, no raw
newlines (they are escaped).

## Output lines (in `run/clients/NAME/client.out`)

| Line | Meaning |
|---|---|
| `[E2E] {...}` | State heartbeat: on change and once per second while in a level. Fields: `screen` (class simple name or null), `screen_title`, `in_level` (level and player exist), `joined_ever`, `disconnect_reason` (text of the disconnect screen), `servers_pinged` (on the multiplayer screen: every listed server has answered its status ping or failed to; else null), `game_time` (client level game time, -1 outside a level), `chunks` (chunks received), `packets_in`, `packets_out` (play-phase and earlier packet totals of this JVM). |
| `[E2E-CTL] {...}` | Reply to one control command: `{"id": ID, "ok": true, "result": {...}}` or `{"id": ID, "ok": false, "error": "text"}`. |
| `[E2E-NET] {"blocked": HOST}` | A name the client tried to resolve and the resolver refused (the run is not isolated). |

The client's own log (`[time] [thread/LEVEL]: text`) is in the same file at INFO and above; `game/latest.log` also
holds DEBUG from `net.minecraft.network` and `net.minecraft.client.multiplayer` (decode failures are DEBUG only).
Rendered chat reaches both as `[CHAT] text` lines.

## Control channel

`real-client.sh start NAME ...` creates the FIFO `run/clients/NAME/ctl` and passes it as `-De2e.ctl=PATH`. One JSON
object per line is written to it (use `scripts/client-ctl.sh NAME JSON`, which writes the line and waits for the
`[E2E-CTL]` reply with the same `id`; exit 0 iff `ok`). Every command runs on the client's render thread, in arrival
order; a command is answered after it ran. Fields: `id` (string or number, echoed; `client-ctl.sh` adds one when
missing), `cmd`, and the arguments below. Unknown commands, missing arguments and exceptions answer `ok:false`.

| `cmd` | Arguments | Effect and `result` |
|---|---|---|
| `snapshot` | `path` (default `game/e2e/snapshot-ID.json`) | Writes the state snapshot (below); `result` = `{"path": ABSOLUTE_PATH}` |
| `chat` | `text` | Sends `text` as a chat message (a leading `/` is not a command here) |
| `command` | `text` (without the slash) | Sends the command as the client would from the chat box |
| `setScreen` | `screen`: `null`, `inventory`, `pause`, `chat`, `advancements`, `multiplayer`, `social`, `creative` | Opens or closes a screen (`multiplayer` = the server list over the title screen). Closing a container screen sends `ContainerClose` |
| `closeScreen` | | `player.closeContainer()` when in a level, else `setScreen(null)` |
| `click` | `slot`, `button` (0), `type`: `PICKUP`, `QUICK_MOVE`, `SWAP`, `CLONE`, `THROW`, `QUICK_CRAFT`, `PICKUP_ALL` | Container click on the open menu (`containerId` of the menu), as the screen would send it |
| `widgetClick` | `key` (translation key of the widget message, shown as `key` in the snapshot widgets: `gui.proceed`) or `class` (simple class name) with `ordinal` (default 0), never a position | Left-clicks the centre of that widget of the open screen; `ok:false` when the widget did not take the click |
| `key` | `key` (`key.keyboard.e`, `key.mouse.left`, ... or a GLFW name such as `e`, `escape`), `action`: `tap` (default), `press`, `release` | Delivers the key to the client's keyboard or mouse handler |
| `look` | `yaw`, `pitch` | Sets the local player's rotation |
| `use` | | What the right mouse button does: the client's own use-item step (block, entity, held item, both hands) |
| `attack` | | What the left mouse button does: the client's own attack step on the crosshair target |
| `hotbar` | `slot` (0-8) | Selects the hotbar slot |
| `respawn` | | Sends the respawn client command |
| `connect` | `address` (`HOST:PORT`) | Disconnects and joins that server (quick-play style) |
| `disconnect` | | Leaves the world and returns to the title screen |
| `screenshot` | `path` (default `game/screenshots/ID.png`) | Saves the current frame as PNG; `result` = `{"path": ...}` |
| `suggest` | `text` | Asks the command dispatcher for suggestions of `text` (as typed in chat, no slash); `result` = `{"suggestions": [...]}` |

## Snapshot

`format: 2`. Object, stable key order. A section that cannot be read is `{"error": "text"}` instead, never a missing
key. Every key is present in every snapshot (`null` when it does not apply).

```
format, screen, menu, player, inventory, level, entities, registries, tags, features, recipes, advancements,
commands, chat, scoreboard, tab_list, boss_bars, titles, packs, server, packets
```

* `screen`: null or `{class, title, widgets: [{class, message, key, x, y, width, height, active, visible}]}` (`key`: the translation key of the message, null for literal text); the multiplayer screen
  adds `servers: [{name, ip, state, status, motd, version, protocol, ping, players: {online, max}, player_list, icon_sha1}]`, its server
  list with what the status pings filled in (`state` is `INITIAL`, `PINGING`, `UNREACHABLE`, `INCOMPATIBLE` or `SUCCESSFUL`).
* `menu`: the player's current menu `{class, id, state_id, carried, slots: [{index, item}]}`; with no container open it is
  the player inventory menu (id 0). Null outside a level. Items are the full item-stack JSON of the client's codec (`{"id","count","components"}`), `{}` when empty.
* `player`: name, uuid, pos, rot, motion, health, food, saturation, xp, gamemode, abilities, effects, attributes, hotbar slot.
* `level`: dimension key, game time, day time, rain/thunder level, difficulty, spawn, world border, tick state,
  chunks loaded.
* `entities`: every entity of the client level in level iteration order (the order the client created them):
  `{id, uuid, type, pos, rot, motion, data: {accessor id: value}, equipment, passengers: [id], attributes}`.
* `registries`: synchronised registry key to entry count; `tags`: registry key to tag count; `features`: enabled flag ids.
* `packets`: `{in: {"PROTOCOL:flow/type": count}, out: {...}}` counted over the whole JVM life, every protocol phase.

Counts are the stable observable of a join: on a deterministic world (flat, peaceful, no mob spawning) two runs of the same
scenario give equal counts except for the types that depend on timing (`set_time`, chunk batch start/finish and their
acknowledgement, a recipe book or attribute update that lands before or after the sample); compare those with a tolerance.
