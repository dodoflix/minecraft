// The `filefix` and `summary-after` verbs of DataFixerOracle: vanilla's FileFixerUpper on a copy of a world folder.
//
// Usage: scripts/datafix-oracle.sh filefix WORLD_DIR --out TREE.txt [--from V] [--to V] [--chunks] [--saves]
//        scripts/datafix-oracle.sh summary-after WORLD_DIR [--from V] [--to V] [--saves]
// WORLD_DIR is copied to `<saves>/world` in a temporary saves folder (never changed; with --saves it is a saves folder
// holding the level `world` and whatever lies next to it, copied as it is), opened as a level, and
// `FileFixerUpper.fix(access, unfixedLevelData, noop)` runs on it (`--to V`: the fix up to data version V; `--from V`:
// the copy's level.dat is stamped DataVersion V first). `summary-after` prints the SUMMARY line below to stdout.
// `filefix` writes TREE.txt, one record per line, tab separated:
//   LOG     LEVEL: message              the log events of net.minecraft.util.filefix (DEBUG and up) and every ERROR event, in order;
//                                       the temporary saves folder is written `<saves>`
//   THREW   class: message              what FileFixerUpper.fix threw, if it did (the saves folder written `<saves>`)
//   FIXED   sha256 snbt                 the returned level data (LastPlayed 0): sha256 of NbtIo.write and its text form
//   TREE    D path                      every folder of the saves folder afterwards (`world`, `world OUTDATED`, ...), sorted
//   TREE    F path size sha256          a file: gzip NBT (.dat, .dat_old) as sha256 of the decompressed bytes (level.dat: with
//                                       LastPlayed 0, written again by NbtIo.write), any other file as the sha256 of its bytes
//   TREE    R path chunks sha256        a region file: its chunk count and the sha256 of the lines `x,z,sha256(NbtIo.write(chunk))`
//   TREE    C path x z sha256           one chunk of a region file (with --chunks only, before its R line)
//   SUMMARY name=... version=...        the LevelSummary of the world afterwards (LastPlayed left out)
import com.mojang.serialization.Dynamic;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.security.MessageDigest;
import java.util.*;
import java.util.stream.Stream;
import java.util.zip.GZIPInputStream;
import net.minecraft.core.registries.Registries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.NbtAccounter;
import net.minecraft.nbt.NbtIo;
import net.minecraft.nbt.NbtOps;
import net.minecraft.nbt.NbtUtils;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.util.datafix.DataFixers;
import net.minecraft.util.worldupdate.UpgradeProgress;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.chunk.storage.RegionFile;
import net.minecraft.world.level.chunk.storage.RegionStorageInfo;
import net.minecraft.world.level.storage.LevelResource;
import net.minecraft.world.level.storage.LevelStorageSource;
import net.minecraft.world.level.storage.LevelSummary;
import org.apache.logging.log4j.Level;
import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.core.LogEvent;
import org.apache.logging.log4j.core.LoggerContext;
import org.apache.logging.log4j.core.appender.AbstractAppender;
import org.apache.logging.log4j.core.config.Configuration;
import org.apache.logging.log4j.core.config.LoggerConfig;
import org.apache.logging.log4j.core.config.Property;

public class FileFixVerbs {
  static final List<String> log = Collections.synchronizedList(new ArrayList<>());
  static String saves = "";

  static void run(String verb, String[] args, PrintStream proto) throws Exception {
    Path world = null;
    Path out = null;
    Integer from = null;
    Integer to = null;
    boolean chunks = false;
    boolean wholeSaves = false;
    for (int i = 1; i < args.length; i++) {
      switch (args[i]) {
        case "--out" -> out = Path.of(args[++i]);
        case "--from" -> from = Integer.parseInt(args[++i]);
        case "--to" -> to = Integer.parseInt(args[++i]);
        case "--chunks" -> chunks = true;
        case "--saves" -> wholeSaves = true;
        default -> {
          if (world != null) throw new IllegalArgumentException("unexpected argument " + args[i]);
          world = Path.of(args[i]);
        }
      }
    }
    if (world == null || (verb.equals("filefix") && out == null)) {
      throw new IllegalArgumentException(verb.equals("filefix")
          ? "filefix WORLD_DIR --out TREE.txt [--from V] [--to V] [--chunks] [--saves]"
          : "summary-after WORLD_DIR [--from V] [--to V] [--saves]");
    }
    Path savesDir = Files.createTempDirectory(Path.of(System.getProperty("java.io.tmpdir")), "filefix-oracle");
    saves = savesDir.toString();
    try {
      copy(world, wholeSaves ? savesDir : savesDir.resolve("world"));
      if (from != null) {
        Path levelDat = savesDir.resolve("world/level.dat");
        CompoundTag root = NbtIo.readCompressed(levelDat, NbtAccounter.unlimitedHeap());
        NbtUtils.addDataVersion(root.getCompoundOrEmpty("Data"), from);
        NbtIo.writeCompressed(root, levelDat);
      }
      List<String> lines = new ArrayList<>();
      installLogCapture();
      LevelStorageSource source = LevelStorageSource.createDefault(savesDir);
      try (LevelStorageSource.LevelStorageAccess access = source.validateAndCreateAccess("world")) {
        Dynamic<?> unfixed = access.getUnfixedDataTagWithFallback();
        try {
          UpgradeProgress progress = new UpgradeProgress.Noop();
          Dynamic<?> fixed = to == null
              ? DataFixers.getFileFixer().fix(access, unfixed, progress)
              : DataFixers.getFileFixer().fix(access, unfixed, progress, to);
          CompoundTag tag = (CompoundTag) fixed.convert(NbtOps.INSTANCE).getValue();
          tag.putLong("LastPlayed", 0L);
          lines.add("FIXED\t" + sha256(nbtBytes(tag)) + "\t" + DataFixerOracle.escape(tag.toString()));
        } catch (Throwable t) {
          lines.add("THREW\t" + t.getClass().getName() + ": "
              + DataFixerOracle.escape(String.valueOf(t.getMessage()).replace(saves, "<saves>")));
        }
      }
      synchronized (log) {
        List<String> logged = new ArrayList<>();
        for (String l : log) logged.add("LOG\t" + DataFixerOracle.escape(l.replace(saves, "<saves>")));
        lines.addAll(0, logged);
      }
      List<String> tree = new ArrayList<>();
      listTree(savesDir, savesDir, chunks, tree);
      for (String t : tree) lines.add("TREE\t" + DataFixerOracle.escape(t.replace(saves, "<saves>")));
      String summary = summary(source);
      lines.add("SUMMARY\t" + summary);
      if (verb.equals("filefix")) {
        Files.writeString(out, String.join("\n", lines) + "\n");
      } else {
        proto.println(summary);
      }
    } finally {
      deleteRecursively(savesDir);
    }
  }

  static String summary(LevelStorageSource source) {
    try (LevelStorageSource.LevelStorageAccess access = source.validateAndCreateAccess("world")) {
      Dynamic<?> data = access.getUnfixedDataTagWithFallback();
      LevelSummary s = access.fixAndGetSummaryFromTag(data);
      return "name=" + s.getLevelName() + " mode=" + s.getGameMode() + " hardcore=" + s.isHardcore()
          + " commands=" + s.hasCommands() + " experimental=" + s.isExperimental()
          + " manual=" + s.requiresManualConversion() + " fileFix=" + s.requiresFileFixing() + " compatible=" + s.isCompatible()
          + " backup=" + s.backupStatus().getTranslationKey() + " shouldBackup=" + s.shouldBackup() + " downgrade=" + s.isDowngrade()
          + " disabled=" + s.isDisabled() + " upload=" + s.canUpload() + " edit=" + s.canEdit() + " recreate=" + s.canRecreate()
          + " version=" + s.levelVersion().minecraftVersionName() + "," + s.levelVersion().minecraftVersion().version() + ","
          + s.levelVersion().minecraftVersion().series() + "," + s.levelVersion().snapshot();
    } catch (Throwable t) {
      return "ERROR " + t.getClass().getName() + ": " + DataFixerOracle.escape(String.valueOf(t.getMessage()));
    }
  }

  // ---- log capture ---------------------------------------------------------------------------------------------

  static void installLogCapture() {
    LoggerContext ctx = (LoggerContext) LogManager.getContext(false);
    Configuration cfg = ctx.getConfiguration();
    AbstractAppender appender = new AbstractAppender("filefix-oracle-log", null, null, true, Property.EMPTY_ARRAY) {
      @Override
      public void append(LogEvent event) {
        log.add(event.getLevel().name() + ": " + event.getMessage().getFormattedMessage());
      }
    };
    appender.start();
    cfg.addAppender(appender);
    LoggerConfig fileFix = new LoggerConfig("net.minecraft.util.filefix", Level.DEBUG, false);
    fileFix.addAppender(appender, null, null);
    cfg.addLogger("net.minecraft.util.filefix", fileFix);
    cfg.getRootLogger().addAppender(appender, Level.ERROR, null);
    ctx.updateLoggers();
  }

  // ---- the tree listing ----------------------------------------------------------------------------------------

  static byte[] nbtBytes(CompoundTag tag) throws IOException {
    ByteArrayOutputStream bytes = new ByteArrayOutputStream();
    NbtIo.write(tag, new DataOutputStream(bytes));
    return bytes.toByteArray();
  }

  static String sha256(byte[] b) {
    return DataFixerOracle.sha256(b);
  }

  static void listTree(Path root, Path dir, boolean chunks, List<String> out) throws Exception {
    List<Path> entries;
    try (Stream<Path> s = Files.list(dir)) {
      entries = s.sorted(Comparator.comparing(p -> p.getFileName().toString())).toList();
    }
    for (Path p : entries) {
      String rel = root.relativize(p).toString().replace(File.separatorChar, '/');
      if (Files.isDirectory(p, LinkOption.NOFOLLOW_LINKS)) {
        out.add("D " + rel);
        listTree(root, p, chunks, out);
      } else {
        fileLine(p, rel, chunks, out);
      }
    }
  }

  static void fileLine(Path p, String rel, boolean chunks, List<String> out) throws Exception {
    String name = p.getFileName().toString();
    byte[] bytes = Files.readAllBytes(p);
    if (name.endsWith(".mca")) {
      out.addAll(regionLines(p, rel, chunks));
    } else if (name.equals("level.dat")) {
      CompoundTag root = NbtIo.readCompressed(p, NbtAccounter.unlimitedHeap());
      root.getCompoundOrEmpty("Data").putLong("LastPlayed", 0L);
      byte[] raw = nbtBytes(root);
      out.add("F " + rel + " " + raw.length + " " + sha256(raw));
    } else if (name.endsWith(".dat") || name.endsWith(".dat_old")) {
      try {
        byte[] raw = new GZIPInputStream(new ByteArrayInputStream(bytes)).readAllBytes();
        out.add("F " + rel + " " + raw.length + " " + sha256(raw));
      } catch (IOException e) {
        out.add("F " + rel + " " + bytes.length + " " + sha256(bytes) + " raw");
      }
    } else {
      out.add("F " + rel + " " + bytes.length + " " + sha256(bytes));
    }
  }

  static List<String> regionLines(Path p, String rel, boolean verbose) throws Exception {
    String[] parts = p.getFileName().toString().split("\\.");
    int regionX = Integer.parseInt(parts[1]);
    int regionZ = Integer.parseInt(parts[2]);
    RegionStorageInfo info = new RegionStorageInfo("oracle", ResourceKey.create(Registries.DIMENSION, Identifier.parse("minecraft:overworld")), "chunk");
    List<String> lines = new ArrayList<>();
    StringBuilder all = new StringBuilder();
    int count = 0;
    try (RegionFile file = new RegionFile(info, p, p.getParent(), false)) {
      for (int z = 0; z < 32; z++) {
        for (int x = 0; x < 32; x++) {
          ChunkPos pos = new ChunkPos(regionX * 32 + x, regionZ * 32 + z);
          if (!file.hasChunk(pos)) continue;
          try (DataInputStream in = file.getChunkDataInputStream(pos)) {
            if (in == null) continue;
            String hash = sha256(nbtBytes(NbtIo.read(in, NbtAccounter.unlimitedHeap())));
            count++;
            all.append(pos.x()).append(',').append(pos.z()).append(',').append(hash).append('\n');
            if (verbose) lines.add("C " + rel + " " + pos.x() + " " + pos.z() + " " + hash);
          }
        }
      }
    }
    lines.add("R " + rel + " " + count + " " + sha256(all.toString().getBytes(StandardCharsets.UTF_8)));
    return lines;
  }

  // ---- files ---------------------------------------------------------------------------------------------------

  static void copy(Path from, Path to) throws IOException {
    try (Stream<Path> s = Files.walk(from)) {
      for (Path p : (Iterable<Path>) s::iterator) {
        // the repository tracks no zip files: a `*.zip.fixture` is the zip of the world
        String relative = from.relativize(p).toString();
        Path target = to.resolve(relative.endsWith(".zip.fixture") ? relative.substring(0, relative.length() - 8) : relative);
        if (Files.isDirectory(p)) Files.createDirectories(target);
        else Files.copy(p, target, StandardCopyOption.COPY_ATTRIBUTES);
      }
    }
  }

  static void deleteRecursively(Path dir) throws IOException {
    try (Stream<Path> s = Files.walk(dir)) {
      for (Path p : (Iterable<Path>) s.sorted(Comparator.reverseOrder())::iterator) Files.deleteIfExists(p);
    }
  }
}
