import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.IntArrayTag;
import net.minecraft.nbt.ListTag;
import net.minecraft.nbt.NbtAccounter;
import net.minecraft.nbt.NbtIo;
import net.minecraft.nbt.StringTag;
import net.minecraft.nbt.Tag;
import net.minecraft.resources.Identifier;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.players.NameAndId;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.chunk.storage.SimpleRegionStorage;

/**
 * The save, unload and reload verbs and the file dumps of {@link EntityOracleDriver} (they act on the level world verbs act on, `in the_nether unload 0 0`):
 *
 * <pre>
 * save [flush]                 MinecraftServer.saveEverything(true, flush, true): every level's saved data, chunks and entities, the players' files, the world data (what `/save-all [flush]` runs)
 * unload CX CZ                 setChunkForced(CX, CZ, false), then server ticks until the chunk and its entities are unloaded (at most 40); prints `UNLOAD CX CZ ticks=N loaded=B`.
 *                              Only the forced flag is removed: a chunk another ticket or a neighbouring forced chunk keeps stays loaded (`loaded=true`)
 * unloadall                    every forced chunk of the level is unforced, ticks until the level holds no chunk (at most 40); prints `UNLOADALL ticks=N loaded=N`
 * load CX CZ                   setChunkForced(CX, CZ, true), ticks until the chunk is ticking with its entities loaded (at most 40); prints `LOAD CX CZ ticks=N loaded=B`
 * reload CX CZ                 `unload` then `load` of that chunk (a chunk the unload could not drop prints `RELOAD CX CZ unloaded=false` and is left alone)
 * rejoin PLAYER [sim] [joinburst]   `spawn_player` with the saved data of PLAYER (`leave` first): its saved game mode, position and rotation, in the level world verbs act on
 * dump region:CX,CZ            `REGION CX,CZ SNBT`: the stored chunk (a write still queued counts), light and the UUIDs of known entities replaced (below); `none` when nothing is stored
 * dump entityfile:CX,CZ        `ENTITYFILE CX,CZ SNBT`: the stored entity chunk of the level
 * dump poifile:CX,CZ           `POIFILE CX,CZ SNBT`: the stored points of interest of the chunk
 * dump playerfile:PLAYER       `PLAYERFILE PLAYER SNBT`: the saved player data (`PlayerList.loadPlayerData`; `none` without a file)
 * dump statsfile:PLAYER        `STATSFILE PLAYER path=value;..`: the statistics file `players/stats/UUID.json` as sorted leaf entries (`none` without a file; only what a save wrote)
 * dump advancementsfile:PLAYER `ADVANCEMENTSFILE PLAYER path=value;..`: the progress file `players/advancements/UUID.json` the same way, the obtained times replaced by `T`
 * dump datafile:ID             `DATAFILE ID DataVersion=N SNBT`: the level's saved data file `data/ID.dat` (`none` without a file; only what `save` wrote); a `tickets` list (the chunk tickets: map order) is sorted
 * </pre>
 * Every UUID (an int array of four) that belongs to a named entity prints as that name (the UUIDs of two servers differ), any other as `?`.
 * References to entities (`refs`) follow their entity through an unload: a reference whose entity was removed names the entity of the same UUID afterwards.
 */
final class PersistVerbs {
   private static final int MAX_TICKS = 40;

   private PersistVerbs() {
   }

   static boolean run(EntityOracleDriver d, List<String> a) throws Exception {
      switch (a.get(0)) {
         case "save" -> {
            d.world();
            d.v.server.saveEverything(true, a.size() > 1 && a.get(1).equals("flush"), true);
         }
         case "unload" -> {
            d.world();
            d.rebinding = true;
            unload(d, Integer.parseInt(a.get(1)), Integer.parseInt(a.get(2)));
         }
         case "unloadall" -> {
            d.world();
            d.rebinding = true;
            ServerLevel level = d.v.level;
            for (long chunk : level.getForceLoadedChunks().toLongArray()) {
               level.setChunkForced(ChunkPos.getX(chunk), ChunkPos.getZ(chunk), false);
            }
            int ticks = 0;
            while (level.getChunkSource().getLoadedChunksCount() > 0 && ticks < MAX_TICKS) {
               d.tickOnce();
               ticks++;
            }
            d.out.println("UNLOADALL ticks=" + ticks + " loaded=" + level.getChunkSource().getLoadedChunksCount());
         }
         case "load" -> {
            d.world();
            d.rebinding = true;
            load(d, Integer.parseInt(a.get(1)), Integer.parseInt(a.get(2)));
         }
         case "reload" -> {
            d.world();
            d.rebinding = true;
            int cx = Integer.parseInt(a.get(1));
            int cz = Integer.parseInt(a.get(2));
            if (unload(d, cx, cz)) {
               load(d, cx, cz);
            } else {
               d.out.println("RELOAD " + cx + " " + cz + " unloaded=false");
            }
         }
         case "rejoin" -> {
            d.world();
            NameAndId who = new NameAndId(d.v.player(a.get(1)).player.getGameProfile());
            CompoundTag saved = d.v.server.getPlayerList().loadPlayerData(who).orElseThrow(() -> new IllegalStateException("no saved data of " + a.get(1)));
            ListTag pos = saved.getListOrEmpty("Pos");
            ListTag rotation = saved.getListOrEmpty("Rotation");
            List<String> join = new ArrayList<>(List.of("spawn_player", a.get(1), net.minecraft.world.level.GameType.byId(saved.getIntOr("playerGameType", 0)).getName(),
               Double.toString(pos.getDoubleOr(0, 0)), Double.toString(pos.getDoubleOr(1, 0)), Double.toString(pos.getDoubleOr(2, 0)),
               Float.toString(rotation.getFloatOr(0, 0)), Float.toString(rotation.getFloatOr(1, 0))));
            join.addAll(a.subList(2, a.size()));
            join.add("load");
            d.run(join);
         }
         default -> {
            return false;
         }
      }
      return true;
   }

   /** Unforces the chunk and ticks until the chunk and its entities are gone: whether it is. */
   private static boolean unload(EntityOracleDriver d, int cx, int cz) throws Exception {
      ServerLevel level = d.v.level;
      long key = ChunkPos.pack(cx, cz);
      level.setChunkForced(cx, cz, false);
      int ticks = 0;
      while ((level.getChunkSource().hasChunk(cx, cz) || level.areEntitiesLoaded(key)) && ticks < MAX_TICKS) {
         d.tickOnce();
         ticks++;
      }
      boolean loaded = level.getChunkSource().hasChunk(cx, cz) || level.areEntitiesLoaded(key);
      d.out.println("UNLOAD " + cx + " " + cz + " ticks=" + ticks + " loaded=" + loaded);
      return !loaded;
   }

   private static void load(EntityOracleDriver d, int cx, int cz) throws Exception {
      ServerLevel level = d.v.level;
      long key = ChunkPos.pack(cx, cz);
      level.setChunkForced(cx, cz, true);
      int ticks = 0;
      while (!level.isPositionTickingWithEntitiesLoaded(key) && ticks < MAX_TICKS) {
         d.tickOnce();
         ticks++;
      }
      d.out.println("LOAD " + cx + " " + cz + " ticks=" + ticks + " loaded=" + level.isPositionTickingWithEntitiesLoaded(key));
   }

   /** A reference to an entity that was removed names the entity with its UUID that is loaded now (an unloaded entity comes back as a new object). */
   static void rebind(EntityOracleDriver d) {
      Map<String, Entity> moved = new HashMap<>();
      for (var en : d.refs.entrySet()) {
         Entity old = en.getValue();
         if (!old.isRemoved() || old instanceof net.minecraft.world.entity.player.Player) {
            continue;
         }
         for (ServerLevel level : d.v.scope) {
            Entity now = level.getEntity(old.getUUID());
            if (now != null) {
               moved.put(en.getKey(), now);
               break;
            }
         }
      }
      d.refs.putAll(moved);
   }

   static boolean dump(EntityOracleDriver d, String kind, String arg) {
      switch (kind) {
         case "region" -> {
            ChunkPos pos = chunkPos(arg);
            d.out.println("REGION " + arg + " " + text(d, d.v.level.getChunkSource().chunkMap.read(pos).join().orElse(null), true));
         }
         case "entityfile" -> {
            Object entities = EntityOracleDriver.priv(d.v.level, ServerLevel.class, "entityManager");
            Object storage = EntityOracleDriver.priv(entities, net.minecraft.world.level.entity.PersistentEntitySectionManager.class, "permanentStorage");
            SimpleRegionStorage files = (SimpleRegionStorage)EntityOracleDriver.priv(storage, net.minecraft.world.level.chunk.storage.EntityStorage.class, "simpleRegionStorage");
            d.out.println("ENTITYFILE " + arg + " " + text(d, files.read(chunkPos(arg)).join().orElse(null), false));
         }
         case "poifile" -> {
            Object poi = d.v.level.getPoiManager();
            SimpleRegionStorage files = (SimpleRegionStorage)EntityOracleDriver.priv(poi, net.minecraft.world.level.chunk.storage.SectionStorage.class, "simpleRegionStorage");
            d.out.println("POIFILE " + arg + " " + text(d, files.read(chunkPos(arg)).join().orElse(null), false));
         }
         case "playerfile" -> {
            NameAndId who = new NameAndId(d.v.player(arg).player.getGameProfile());
            CompoundTag saved = d.v.server.getPlayerList().loadPlayerData(who).orElse(null);
            if (saved != null && saved.get("recipeBook") instanceof CompoundTag book) {
               // `ServerRecipeBook.known` is an identity hash set: its order differs from run to run
               EntityOracleDriver.sortedStrings(book, "recipes");
               EntityOracleDriver.sortedStrings(book, "toBeDisplayed");
            }
            d.out.println("PLAYERFILE " + arg + " " + text(d, saved, false));
         }
         case "statsfile", "advancementsfile" -> {
            String directory = kind.equals("statsfile") ? "players/stats" : "players/advancements";
            Path file = d.v.dir.resolve("world").resolve(directory).resolve(d.v.player(arg).player.getUUID() + ".json");
            List<String> leaves = new ArrayList<>();
            if (Files.exists(file)) {
               try (java.io.Reader reader = Files.newBufferedReader(file)) {
                  jsonLeaves("", com.google.gson.JsonParser.parseReader(reader), leaves);
               } catch (java.io.IOException ex) {
                  throw new java.io.UncheckedIOException(ex);
               }
               java.util.Collections.sort(leaves);
            }
            d.out.println(kind.toUpperCase() + " " + arg + " " + (leaves.isEmpty() ? "none" : String.join(";", leaves)));
         }
         case "datafile" -> {
            Identifier dimension = d.v.level.dimension().identifier();
            Path file = Identifier.parse(arg).withSuffix(".dat").resolveAgainst(d.v.dir.resolve("world/dimensions").resolve(dimension.getNamespace()).resolve(dimension.getPath()).resolve("data"));
            if (!Files.exists(file)) {
               d.out.println("DATAFILE " + arg + " none");
            } else {
               CompoundTag root;
               try {
                  root = NbtIo.readCompressed(file, NbtAccounter.unlimitedHeap());
               } catch (java.io.IOException ex) {
                  throw new java.io.UncheckedIOException(ex);
               }
               d.out.println("DATAFILE " + arg + " DataVersion=" + root.getIntOr("DataVersion", -1) + " " + text(d, EntityOracleDriver.sortedStrings(root.getCompoundOrEmpty("data"), "tickets"), false));
            }
         }
         default -> {
            return false;
         }
      }
      return true;
   }

   private static ChunkPos chunkPos(String arg) {
      String[] c = arg.split(",");
      return new ChunkPos(Integer.parseInt(c[0]), Integer.parseInt(c[1]));
   }

   private static String text(EntityOracleDriver d, CompoundTag tag, boolean stripLight) {
      if (tag == null) {
         return "none";
      }
      CompoundTag copy = tag.copy();
      if (stripLight && copy.get("sections") instanceof ListTag sections) {
         for (Tag section : sections) {
            ((CompoundTag)section).remove("BlockLight");
            ((CompoundTag)section).remove("SkyLight");
         }
      }
      return EntityOracleDriver.canon(names(d, copy));
   }

   /** The UUIDs inside `tag` as scenario names. */
   private static Tag names(EntityOracleDriver d, Tag tag) {
      switch (tag) {
         case IntArrayTag ints when ints.size() == 4 -> {
            UUID uuid = new UUID((long)ints.getAsIntArray()[0] << 32 | ints.getAsIntArray()[1] & 0xFFFFFFFFL, (long)ints.getAsIntArray()[2] << 32 | ints.getAsIntArray()[3] & 0xFFFFFFFFL);
            String name = d.uuidName(uuid);
            return StringTag.valueOf(name.equals(uuid.toString()) ? "?" : name);
         }
         case CompoundTag compound -> {
            for (String key : new ArrayList<>(compound.keySet())) {
               compound.put(key, names(d, compound.get(key)));
            }
            return compound;
         }
         case ListTag list -> {
            for (int i = 0; i < list.size(); i++) {
               list.set(i, names(d, list.get(i)));
            }
            return list;
         }
         default -> {
            return tag;
         }
      }
   }

   /** `path=value` for every leaf of `json`; an obtained time (`2026-11-16 10:41:34 +0300`) prints as `T`. */
   private static void jsonLeaves(String path, com.google.gson.JsonElement json, List<String> out) {
      if (json.isJsonObject()) {
         json.getAsJsonObject().entrySet().forEach(e -> jsonLeaves(path + "/" + e.getKey(), e.getValue(), out));
      } else if (json.isJsonArray()) {
         int i = 0;
         for (com.google.gson.JsonElement e : json.getAsJsonArray()) {
            jsonLeaves(path + "[" + i++ + "]", e, out);
         }
      } else {
         String value = json.getAsString();
         out.add(path + "=" + (value.matches("\\d{4}-\\d\\d-\\d\\d \\d\\d:\\d\\d:\\d\\d [+-]\\d{4}") ? "T" : value));
      }
   }
}
