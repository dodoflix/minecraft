// Oracle for the data fixer port: vanilla's DataFixers run on NBT inputs, the chain listing and the schema type dump.
//
// Usage: scripts/datafix-oracle.sh VERB [ARGS]   (the script compiles this directory into target/datafix-oracle)
//   fixes-list                     the registered fixes: INDEX VERSION SUBVERSION KIND CLASS (kind data|file, registration order)
//   schema-dump KEY TYPE           the normalised description (SchemaWalk grammar) of type TYPE in schema KEY (version * 10 + sub)
//   schema-dump KEY all            TYPE<TAB>sha256 of every type of schema KEY
//   schema-dump all                every schema in key order, delta encoded: `KEY<TAB>*<TAB>-` opens a schema, then
//                                  KEY<TAB>TYPE<TAB>sha256 only where the hash differs from the last line of that type
//                                  (`-` as hash: the type is gone)
//   grammar-sample                 the descriptions of a small schema built with the real DSL (GrammarSample.java)
//   fix TYPE FROM TO FILE [OUT]    one record (below) for the NBT file FILE; OUT receives the result bytes
//                                  (TYPE `upgrade:DATAFIXTYPE[@DIMENSION[@GENERATOR]]`: SimpleRegionStorage.upgradeChunkTag with
//                                  default version FROM and target TO, the ChunkMap data fix context when a dimension is given;
//                                  TYPE `scan:DIMENSION[@GENERATOR]`: the partial chunk fix of StructureCheck, FROM and TO unused)
//   batch IN OUT                   records for every line of IN (`-` = stdin), written to OUT (`-` = stdout)
//   gen OPTIONS                    batch lines of schema-driven samples (SampleGen.java: --type --from --to --seed --n --invalid --mutate --matrix --shard --report)
//   filefix WORLD_DIR --out TREE.txt [--from V --to V] [--saves]   the file fixer on a copy of a world: log lines, level data, tree listing (FileFixVerbs.java)
//   summary-after WORLD_DIR [--from V --to V] [--saves]  the LevelSummary of that world afterwards (FileFixVerbs.java)
//   tables DIR                     the lookup tables of the fix classes as DIR/<class>.tsv (TableDump.java)
//   table-accessors                rows of the public table accessors over every key (TableDump.java)
// Output: the record formats below on stdout (log noise goes to stderr).
//
// TYPE is a DataFixTypes constant (CHUNK) or `ref:NAME`, a References type by its type name (ref:item_stack), which
// reaches the types no DataFixTypes names. Input of `batch`, tab separated: ID TYPE FROM TO BASE64(NbtIo bytes of a named root) or SNBT text starting with `{`.
// A TYPE `json:CONSTANT` takes strict JSON text as the input field and answers with the UTF-8 of `JsonElement.toString()` as the bytes.
// Record, tab separated: ID STATUS SHA256 BASE64 EXCEPTION EXCEPTION_HASH ERRORS ERRORS_HASH SNBT where STATUS is
//   ok       SHA256 and BASE64 describe NbtIo.write of the result (HashMap key order), EXCEPTION is -
//   threw    EXCEPTION is the class name of what update threw, EXCEPTION_HASH the sha256 of its message
//   bad      the input could not be read; EXCEPTION says why
// ERRORS counts the ERROR log events of the call, ERRORS_HASH is the sha256 of their messages joined with LF (- when none).
// SNBT is the result in text form with \\, \n, \t and \r escaped. A field without a value is `-`.
import com.mojang.datafixers.DSL;
import com.mojang.datafixers.DataFix;
import com.mojang.datafixers.DataFixer;
import com.mojang.datafixers.DataFixerUpper;
import com.mojang.datafixers.schemas.Schema;
import com.mojang.datafixers.types.Type;
import com.mojang.serialization.Dynamic;
import it.unimi.dsi.fastutil.ints.Int2ObjectSortedMap;
import java.io.*;
import java.lang.reflect.Field;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.MessageDigest;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.registries.Registries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.IntTag;
import net.minecraft.nbt.NbtAccounter;
import net.minecraft.nbt.NbtIo;
import net.minecraft.nbt.NbtOps;
import net.minecraft.nbt.NbtUtils;
import net.minecraft.nbt.visitors.CollectFields;
import net.minecraft.nbt.visitors.FieldSelector;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.level.ChunkMap;
import net.minecraft.util.datafix.DataFixTypes;
import net.minecraft.util.datafix.DataFixers;
import net.minecraft.util.datafix.fixes.References;
import net.minecraft.util.filefix.FileFix;
import net.minecraft.util.filefix.FileFixerUpper;
import net.minecraft.world.level.chunk.storage.RegionStorageInfo;
import net.minecraft.world.level.chunk.storage.SimpleRegionStorage;
import org.apache.logging.log4j.Level;
import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.core.LogEvent;
import org.apache.logging.log4j.core.LoggerContext;
import org.apache.logging.log4j.core.appender.AbstractAppender;
import org.apache.logging.log4j.core.config.Configuration;
import org.apache.logging.log4j.core.config.Property;

public class DataFixerOracle {
  /** The protocol channel: the real stdout. System.out is redirected to stderr so no log line can reach it. */
  static PrintStream proto;

  static final List<String> errors = Collections.synchronizedList(new ArrayList<>());

  public static void main(String[] args) throws Exception {
    proto = new PrintStream(new BufferedOutputStream(new FileOutputStream(FileDescriptor.out), 1 << 16), false, StandardCharsets.UTF_8);
    System.setOut(System.err);
    if (args.length == 0 || args[0].equals("-h") || args[0].equals("--help")) {
      System.err.println("Usage: scripts/datafix-oracle.sh fixes-list | schema-dump KEY TYPE|all | schema-dump all | fix TYPE FROM TO FILE [OUT] | batch IN OUT | gen OPTIONS | filefix WORLD_DIR --out TREE.txt | summary-after WORLD_DIR | tables DIR | table-accessors");
      System.exit(args.length == 0 ? 2 : 0);
    }
    Throwable[] failure = new Throwable[1];
    // a deep rewrite recurses deeply: run with a generous stack
    Thread t = new Thread(null, () -> {
      try {
        run(args);
      } catch (Throwable e) {
        failure[0] = e;
      }
    }, "datafix-oracle", 1L << 29);
    t.start();
    t.join();
    proto.flush();
    if (failure[0] != null) {
      // Bootstrap wraps System.err into the log, which this oracle's log configuration silences
      failure[0].printStackTrace(new PrintStream(new FileOutputStream(FileDescriptor.err), true));
      System.exit(1);
    }
    System.exit(0);
  }

  static void run(String[] args) throws Exception {
    SharedConstants.tryDetectVersion();
    Bootstrap.bootStrap();
    installErrorCapture();
    switch (args[0]) {
      case "fixes-list" -> fixesList();
      case "schema-dump" -> schemaDump(args);
      case "grammar-sample" -> proto.print(GrammarSample.dump());
      case "gen" -> SampleGen.run(args, proto);
      case "filefix", "summary-after" -> FileFixVerbs.run(args[0], args, proto);
      case "tables" -> {
        if (args.length != 2) throw new IllegalArgumentException("tables DIR");
        TableDump.dump(Path.of(args[1]));
      }
      case "table-accessors" -> TableDump.accessors(proto);
      case "fix" -> {
        if (args.length < 5) throw new IllegalArgumentException("fix TYPE FROM TO FILE [OUT]");
        byte[] input = Files.readAllBytes(Path.of(args[4]));
        if (input.length > 2 && (input[0] & 0xFF) == 0x1F && (input[1] & 0xFF) == 0x8B) {
          input = new java.util.zip.GZIPInputStream(new ByteArrayInputStream(input)).readAllBytes();
        }
        Outcome r = update(args[1], Integer.parseInt(args[2]), Integer.parseInt(args[3]), input);
        proto.println(r.line("-"));
        if (args.length > 5 && r.bytes != null) Files.write(Path.of(args[5]), r.bytes);
      }
      case "batch" -> {
        if (args.length != 3) throw new IllegalArgumentException("batch IN OUT");
        BufferedReader in = new BufferedReader(args[1].equals("-") ? new InputStreamReader(System.in, StandardCharsets.UTF_8) : Files.newBufferedReader(Path.of(args[1])));
        PrintStream out = args[2].equals("-") ? proto : new PrintStream(new BufferedOutputStream(new FileOutputStream(args[2])), false, StandardCharsets.UTF_8);
        String line;
        while ((line = in.readLine()) != null) {
          if (line.isEmpty()) continue;
          String[] f = line.split("\t", -1);
          if (f.length != 5) throw new IllegalArgumentException("batch line needs 5 tab separated fields: " + line);
          Outcome result = f[1].startsWith("json:")
              ? updateJson(f[1].substring(5), Integer.parseInt(f[2]), Integer.parseInt(f[3]), f[4])
              : update(f[1], Integer.parseInt(f[2]), Integer.parseInt(f[3]), input(f[4]));
          out.println(result.line(f[0]));
          out.flush();
        }
        out.flush();
      }
      default -> throw new IllegalArgumentException("unknown verb " + args[0]);
    }
  }

  // ---- error capture -------------------------------------------------------------------------------------------

  static void installErrorCapture() {
    LoggerContext ctx = (LoggerContext) LogManager.getContext(false);
    Configuration cfg = ctx.getConfiguration();
    AbstractAppender appender = new AbstractAppender("datafix-oracle-errors", null, null, true, Property.EMPTY_ARRAY) {
      @Override
      public void append(LogEvent event) {
        errors.add(event.getMessage().getFormattedMessage());
      }
    };
    appender.start();
    cfg.addAppender(appender);
    cfg.getRootLogger().addAppender(appender, Level.ERROR, null);
    ctx.updateLoggers();
  }

  // ---- update ---------------------------------------------------------------------------------------------------

  record Outcome(String status, byte[] bytes, String exception, String exceptionHash, List<String> errors, String snbt) {
    String line(String id) {
      String errorsHash = errors.isEmpty() ? "-" : sha256(String.join("\n", errors).getBytes(StandardCharsets.UTF_8));
      return String.join(
          "\t",
          id,
          status,
          bytes == null ? "-" : sha256(bytes),
          bytes == null ? "-" : Base64.getEncoder().encodeToString(bytes),
          exception,
          exceptionHash,
          Integer.toString(errors.size()),
          errorsHash,
          snbt);
    }
  }

  static String sha256(byte[] b) {
    try {
      return HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(b));
    } catch (java.security.NoSuchAlgorithmException e) {
      throw new IllegalStateException(e);
    }
  }

  static DSL.TypeReference reference(String typeName) {
    for (Field f : References.class.getFields()) {
      if (f.getType() == DSL.TypeReference.class) {
        try {
          DSL.TypeReference r = (DSL.TypeReference) f.get(null);
          if (r.typeName().equals(typeName)) return r;
        } catch (IllegalAccessException e) {
          throw new IllegalStateException(e);
        }
      }
    }
    throw new IllegalArgumentException("unknown References type " + typeName);
  }

  static String escape(String s) {
    return s.replace("\\", "\\\\").replace("\n", "\\n").replace("\t", "\\t").replace("\r", "\\r");
  }

  /** A batch input field: the base64 of NbtIo bytes, or SNBT text (starts with `{`) for hand written samples. */
  static byte[] input(String field) throws Exception {
    if (!field.startsWith("{")) return Base64.getDecoder().decode(field);
    ByteArrayOutputStream bytes = new ByteArrayOutputStream();
    NbtIo.write(net.minecraft.nbt.TagParser.parseCompoundFully(field), new DataOutputStream(bytes));
    return bytes.toByteArray();
  }

  static Outcome update(String type, int from, int to, byte[] nbt) {
    CompoundTag in;
    try {
      in = NbtIo.read(new DataInputStream(new ByteArrayInputStream(nbt)), NbtAccounter.unlimitedHeap());
    } catch (IOException | RuntimeException e) {
      return new Outcome("bad", null, e.getClass().getName(), "-", List.of(), "-");
    }
    errors.clear();
    try {
      DataFixer fixer = DataFixers.getDataFixer();
      CompoundTag out;
      if (type.startsWith("ref:")) {
        out = (CompoundTag) fixer.update(reference(type.substring(4)), new Dynamic<>(NbtOps.INSTANCE, in), from, to).getValue();
      } else if (type.startsWith("scan:")) {
        out = scanAndFixChunk(fixer, type.substring(5), in);
      } else if (type.startsWith("upgrade:")) {
        out = upgradeChunkTag(fixer, type.substring(8), from, to, in);
      } else {
        out = DataFixTypes.valueOf(type).update(fixer, in, from, to);
      }
      ByteArrayOutputStream bytes = new ByteArrayOutputStream();
      NbtIo.write(out, new DataOutputStream(bytes));
      return new Outcome("ok", bytes.toByteArray(), "-", "-", List.copyOf(errors), escape(out.toString()));
    } catch (Throwable t) {
      return new Outcome("threw", null, t.getClass().getName(), sha256(String.valueOf(t.getMessage()).getBytes(StandardCharsets.UTF_8)), List.copyOf(errors), "-");
    }
  }

  /** `json:TYPE` of `batch`: TYPE is a DataFixTypes constant over a strict JSON element (STATS, ADVANCEMENTS); the record's bytes are the UTF-8 of `JsonElement.toString()`. */
  static Outcome updateJson(String type, int from, int to, String text) {
    errors.clear();
    try {
      com.google.gson.JsonElement in = net.minecraft.util.StrictJsonParser.parse(text);
      DataFixer fixer = DataFixers.getDataFixer();
      com.google.gson.JsonElement out =
          DataFixTypes.valueOf(type).update(fixer, new Dynamic<>(com.mojang.serialization.JsonOps.INSTANCE, in), from, to).getValue();
      byte[] bytes = out.toString().getBytes(StandardCharsets.UTF_8);
      return new Outcome("ok", bytes, "-", "-", List.copyOf(errors), escape(out.toString()));
    } catch (Throwable t) {
      return new Outcome("threw", null, t.getClass().getName(), sha256(String.valueOf(t.getMessage()).getBytes(StandardCharsets.UTF_8)), List.copyOf(errors), "-");
    }
  }

  /**
   * `SimpleRegionStorage.upgradeChunkTag(tag, defaultVersion = FROM, context, targetVersion = TO)`; SPEC is
   * `DATAFIXTYPE[@DIMENSION[@GENERATOR]]`: with a dimension the data fix context tag of `ChunkMap` is injected (an empty
   * generator leaves it out).
   */
  static CompoundTag upgradeChunkTag(DataFixer fixer, String spec, int from, int to, CompoundTag in) throws IOException {
    String[] p = spec.split("@", -1);
    ResourceKey<net.minecraft.world.level.Level> dimension = ResourceKey.create(Registries.DIMENSION, Identifier.parse(p.length > 1 ? p[1] : "minecraft:overworld"));
    CompoundTag context = null;
    if (p.length > 1) {
      Optional<Identifier> generator = p.length > 2 && !p[2].isEmpty() ? Optional.of(Identifier.parse(p[2])) : Optional.empty();
      context = ChunkMap.getChunkDataFixContextTag(dimension, generator);
    }
    Path folder = Files.createTempDirectory(Path.of(System.getProperty("java.io.tmpdir")), "upgrade-oracle");
    SimpleRegionStorage storage = new SimpleRegionStorage(new RegionStorageInfo("oracle", dimension, "chunk"), folder, fixer, false, DataFixTypes.valueOf(p[0]));
    try {
      return storage.upgradeChunkTag(in, from, context, to);
    } finally {
      storage.close();
      Files.deleteIfExists(folder);
    }
  }

  /**
   * The partial fix of `StructureCheck.tryLoadFromStorage`: the fields `IOWorker.scanChunk` collects, the data fix context
   * of SPEC (`DIMENSION[@GENERATOR]`) injected, `DataFixTypes.CHUNK.updateToCurrentVersion` at the stored version.
   */
  static CompoundTag scanAndFixChunk(DataFixer fixer, String spec, CompoundTag chunk) {
    String[] p = spec.split("@", -1);
    CollectFields collect = new CollectFields(
        new FieldSelector(IntTag.TYPE, "DataVersion"),
        new FieldSelector("Level", "Structures", CompoundTag.TYPE, "Starts"),
        new FieldSelector("structures", CompoundTag.TYPE, "starts"));
    chunk.acceptAsRoot(collect);
    if (!(collect.getResult() instanceof CompoundTag chunkTag)) throw new IllegalStateException("no compound collected");
    int version = NbtUtils.getDataVersion(chunkTag);
    ResourceKey<net.minecraft.world.level.Level> dimension = ResourceKey.create(Registries.DIMENSION, Identifier.parse(p[0]));
    Optional<Identifier> generator = p.length > 1 && !p[1].isEmpty() ? Optional.of(Identifier.parse(p[1])) : Optional.empty();
    SimpleRegionStorage.injectDatafixingContext(chunkTag, ChunkMap.getChunkDataFixContextTag(dimension, generator));
    return DataFixTypes.CHUNK.updateToCurrentVersion(fixer, chunkTag, version);
  }

  // ---- chain and schemas ----------------------------------------------------------------------------------------

  static Object privateField(Object o, Class<?> owner, String name) throws ReflectiveOperationException {
    Field f = owner.getDeclaredField(name);
    f.setAccessible(true);
    return f.get(o);
  }

  static String simpleName(Object o) {
    String n = o.getClass().getName();
    return n.substring(n.lastIndexOf('.') + 1);
  }

  @SuppressWarnings("unchecked")
  static void fixesList() throws Exception {
    List<DataFix> data = (List<DataFix>) privateField(DataFixers.getDataFixer(), DataFixerUpper.class, "globalList");
    for (int i = 0; i < data.size(); i++) {
      int key = data.get(i).getVersionKey();
      proto.println(i + "\t" + key / 10 + "\t" + key % 10 + "\tdata\t" + simpleName(data.get(i)));
    }
    List<FileFix> file = (List<FileFix>) privateField(DataFixers.getFileFixer(), FileFixerUpper.class, "fileFixes");
    for (int i = 0; i < file.size(); i++) {
      int key = file.get(i).getSchema().getVersionKey();
      proto.println(i + "\t" + key / 10 + "\t" + key % 10 + "\tfile\t" + simpleName(file.get(i)));
    }
  }

  @SuppressWarnings("unchecked")
  static Int2ObjectSortedMap<Schema> schemas() throws Exception {
    return (Int2ObjectSortedMap<Schema>) privateField(DataFixers.getDataFixer(), DataFixerUpper.class, "schemas");
  }

  /** Schema.getType: a recursive type is described by its checked delegate, so its content is part of the hash. */
  static Type<?> typeOf(Schema schema, String name) {
    return schema.getType(() -> name);
  }

  static String hash(Schema schema, String name) {
    return sha256(SchemaWalk.describe(typeOf(schema, name)).getBytes(StandardCharsets.UTF_8));
  }

  static void schemaDump(String[] args) throws Exception {
    Int2ObjectSortedMap<Schema> schemas = schemas();
    if (args.length == 2 && args[1].equals("all")) {
      Map<String, String> last = new HashMap<>();
      for (var e : schemas.int2ObjectEntrySet()) {
        proto.println(e.getIntKey() + "\t*\t-");
        Set<String> names = new TreeSet<>(e.getValue().types());
        for (String name : names) {
          String h = hash(e.getValue(), name);
          if (!h.equals(last.put(name, h))) proto.println(e.getIntKey() + "\t" + name + "\t" + h);
        }
        for (String gone : new TreeSet<>(last.keySet())) {
          if (!names.contains(gone) && !last.get(gone).equals("-")) {
            last.put(gone, "-");
            proto.println(e.getIntKey() + "\t" + gone + "\t-");
          }
        }
      }
      return;
    }
    if (args.length != 3) throw new IllegalArgumentException("schema-dump KEY TYPE|all | schema-dump all");
    Schema schema = schemas.get(Integer.parseInt(args[1]));
    if (schema == null) throw new IllegalArgumentException("no schema with key " + args[1]);
    if (args[2].equals("all")) {
      for (String name : new TreeSet<>(schema.types())) proto.println(name + "\t" + hash(schema, name));
    } else {
      proto.print(SchemaWalk.describe(typeOf(schema, args[2])));
    }
  }
}
