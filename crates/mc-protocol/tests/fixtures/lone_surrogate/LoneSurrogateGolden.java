import com.google.gson.JsonElement;
import com.mojang.brigadier.LiteralMessage;
import com.mojang.brigadier.StringReader;
import com.mojang.brigadier.exceptions.CommandSyntaxException;
import com.mojang.brigadier.exceptions.SimpleCommandExceptionType;
import com.mojang.serialization.DynamicOps;
import com.mojang.serialization.JsonOps;
import io.netty.buffer.ByteBuf;
import io.netty.buffer.ByteBufUtil;
import io.netty.buffer.Unpooled;
import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import net.minecraft.SharedConstants;
import net.minecraft.commands.CommandBuildContext;
import net.minecraft.commands.arguments.item.ItemArgument;
import net.minecraft.commands.arguments.item.ItemPredicateArgument;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.component.DataComponents;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.ListTag;
import net.minecraft.nbt.NbtAccounter;
import net.minecraft.nbt.NbtIo;
import net.minecraft.nbt.NbtOps;
import net.minecraft.nbt.SnbtPrinterTagVisitor;
import net.minecraft.nbt.StringTag;
import net.minecraft.nbt.Tag;
import net.minecraft.nbt.TagParser;
import net.minecraft.nbt.TextComponentTagVisitor;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.network.Utf8String;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.ComponentSerialization;
import net.minecraft.network.chat.ComponentUtils;
import net.minecraft.network.protocol.game.ClientboundSystemChatPacket;
import net.minecraft.network.protocol.login.ClientboundLoginDisconnectPacket;
import net.minecraft.network.protocol.status.ClientboundStatusResponsePacket;
import net.minecraft.network.protocol.status.ServerStatus;
import net.minecraft.server.dedicated.DedicatedServerProperties;
import net.minecraft.server.dialog.Dialog;
import net.minecraft.server.network.Filterable;
import net.minecraft.util.GsonHelper;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.component.WritableBookContent;
import net.minecraft.world.item.component.WrittenBookContent;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.item.DyeColor;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.block.entity.SignText;

/**
 * Golden generator for strings that hold lone UTF-16 surrogates (Java strings can, Rust strings cannot): what vanilla writes,
 * reads and prints for them. One file per consuming crate; every line is `op TAB escaped-input TAB cell...`, where `esc` writes
 * UTF-16 code units outside printable ASCII as \\uXXXX (so lone surrogates are visible), and binary results are lowercase hex.
 * A failing cell is `ERR:<exception class>:<message>`.
 *   nbt.tsv        NBT binary (modified UTF-8) and SNBT of strings, keys and tags
 *   text.tsv       text components from JSON / SNBT: string, JSON, NBT bytes, system chat packet bytes
 *   wire.tsv       network strings and the signs, books, item names and dialogs sent to a client
 *   brigadier.tsv  command syntax exception contexts (a context window can split a surrogate pair)
 *   game.tsv       items parsed by the command argument
 * Every op is replayed by the tests of the crate that consumes the file.
 * Usage: crates/mc-protocol/tests/fixtures/lone_surrogate/gen.sh
 * Output: the file of each crate named in gen.sh
 */
public class LoneSurrogateGolden {
   static final Map<String, StringBuilder> FILES = new LinkedHashMap<>();
   static RegistryAccess.Frozen access;
   static DynamicOps<Tag> nbtOps;
   static DynamicOps<JsonElement> jsonOps;

   interface Thr<T> {
      T get() throws Throwable;
   }

   static String esc(String s) {
      StringBuilder sb = new StringBuilder();
      for (int i = 0; i < s.length(); i++) {
         char c = s.charAt(i);
         if (c == '\\') sb.append("\\\\");
         else if (c == '\t') sb.append("\\t");
         else if (c == '\n') sb.append("\\n");
         else if (c == '\r') sb.append("\\r");
         else if (c < 0x20 || c > 0x7e) sb.append(String.format("\\u%04x", (int) c));
         else sb.append(c);
      }
      return sb.toString();
   }

   static String hex(byte[] b) {
      StringBuilder sb = new StringBuilder();
      for (byte x : b) sb.append(String.format(java.util.Locale.ROOT, "%02x", x));
      return sb.toString();
   }

   static byte[] unhex(String s) {
      byte[] b = new byte[s.length() / 2];
      for (int i = 0; i < b.length; i++) b[i] = (byte) Integer.parseInt(s.substring(2 * i, 2 * i + 2), 16);
      return b;
   }

   /** A cell: `esc` of the string result, or the escaped error. */
   static String cell(Thr<String> f) {
      try {
         return esc(f.get());
      } catch (Throwable t) {
         return err(t);
      }
   }

   /** `ERR:<class>:<message>`, then `<-<class>:<message>` for each cause. */
   static String err(Throwable t) {
      StringBuilder sb = new StringBuilder("ERR");
      for (String sep = ":"; t != null; t = t.getCause(), sep = "<-") sb.append(sep).append(t.getClass().getSimpleName()).append(':').append(t.getMessage());
      return esc(sb.toString());
   }

   static void row(String file, String op, String input, String... cells) {
      StringBuilder sb = FILES.computeIfAbsent(file, k -> new StringBuilder());
      sb.append(op).append('\t').append(esc(input));
      for (String c : cells) sb.append('\t').append(c);
      sb.append('\n');
   }

   static byte[] nbtAny(Tag t) throws Exception {
      ByteArrayOutputStream bos = new ByteArrayOutputStream();
      NbtIo.writeAnyTag(t, new DataOutputStream(bos));
      return bos.toByteArray();
   }

   static Tag readAny(byte[] b) throws Exception {
      return NbtIo.readAnyTag(new DataInputStream(new ByteArrayInputStream(b)), NbtAccounter.unlimitedHeap());
   }

   static byte[] nbtRoot(CompoundTag t) throws Exception {
      ByteArrayOutputStream bos = new ByteArrayOutputStream();
      NbtIo.write(t, new DataOutputStream(bos));
      return bos.toByteArray();
   }

   static String compoundKeys(Tag t) {
      StringBuilder sb = new StringBuilder();
      boolean first = true;
      for (String k : ((CompoundTag) t).keySet()) {
         if (!first) sb.append(',');
         first = false;
         sb.append(k);
      }
      return sb.toString();
   }

   static Tag parseSnbt(String s) throws Exception {
      return TagParser.create(NbtOps.INSTANCE).parseFully(s);
   }

   static RegistryFriendlyByteBuf regBuf() {
      return new RegistryFriendlyByteBuf(Unpooled.buffer(), access);
   }

   static String bytes(ByteBuf b) {
      return ByteBufUtil.hexDump(b);
   }

   static Component fromJson(String json) {
      JsonElement e = com.google.gson.JsonParser.parseString(json);
      return ComponentSerialization.CODEC.parse(jsonOps, e).getOrThrow();
   }

   static Component fromSnbt(String snbt) throws Exception {
      return ComponentSerialization.CODEC.parse(nbtOps, parseSnbt(snbt)).getOrThrow();
   }

   /** The cells of a component: string, JSON, NBT bytes, system chat packet bytes. */
   static String[] componentCells(Thr<Component> make) {
      Component[] c = new Component[1];
      String err = null;
      try {
         c[0] = make.get();
      } catch (Throwable t) {
         err = err(t);
      }
      if (err != null) return new String[] {err, err, err, err};
      return new String[] {
         cell(() -> c[0].getString()),
         cell(() -> GsonHelper.toStableString(ComponentSerialization.CODEC.encodeStart(jsonOps, c[0]).getOrThrow())),
         cell(() -> hex(nbtAny(ComponentSerialization.CODEC.encodeStart(nbtOps, c[0]).getOrThrow()))),
         cell(() -> {
            RegistryFriendlyByteBuf b = regBuf();
            ClientboundSystemChatPacket.STREAM_CODEC.encode(b, new ClientboundSystemChatPacket(c[0], false));
            return bytes(b);
         }),
      };
   }

   static String[] itemCells(Thr<ItemStack> make) {
      ItemStack[] s = new ItemStack[1];
      String err = null;
      try {
         s[0] = make.get();
      } catch (Throwable t) {
         err = err(t);
      }
      if (err != null) return new String[] {err, err, err};
      return new String[] {
         cell(() -> hex(nbtAny(ItemStack.CODEC.encodeStart(nbtOps, s[0]).getOrThrow()))),
         cell(() -> {
            RegistryFriendlyByteBuf b = regBuf();
            ItemStack.OPTIONAL_STREAM_CODEC.encode(b, s[0]);
            return bytes(b);
         }),
         cell(() -> s[0].getHoverName().getString()),
      };
   }

   /** The message of a thrown `CommandSyntaxException` and the component built from its raw message (what the client gets). */
   static String[] errorCells(Thr<Object> f) {
      CommandSyntaxException[] e = new CommandSyntaxException[1];
      try {
         f.get();
      } catch (CommandSyntaxException t) {
         e[0] = t;
      } catch (Throwable t) {
         return new String[] {err(t), err(t)};
      }
      if (e[0] == null) return new String[] {"parsed", "parsed"};
      return new String[] {
         cell(() -> e[0].getMessage()),
         cell(() -> hex(nbtAny(ComponentSerialization.CODEC.encodeStart(nbtOps, ComponentUtils.fromMessage(e[0].getRawMessage())).getOrThrow()))),
      };
   }

   static String[] concat(String[] a, String... b) {
      String[] r = new String[a.length + b.length];
      System.arraycopy(a, 0, r, 0, a.length);
      System.arraycopy(b, 0, r, a.length, b.length);
      return r;
   }

   static void nbtString(String s) {
      StringTag t = StringTag.valueOf(s);
      row("nbt.tsv", "nbt_string_write", s,
         cell(() -> hex(nbtAny(t))),
         cell(() -> ((StringTag) readAny(nbtAny(t))).value()),
         cell(() -> t.toString()),
         cell(() -> StringTag.quoteAndEscape(s)),
         cell(() -> Integer.toString(t.hashCode())),
         cell(() -> Boolean.toString(t.equals(StringTag.valueOf("\ufffd")))));
      CompoundTag c = new CompoundTag();
      c.putByte(s, (byte) 1);
      c.putByte("a", (byte) 2);
      row("nbt.tsv", "nbt_key_write", s,
         cell(() -> hex(nbtRoot(c))),
         cell(() -> compoundKeys(NbtIo.read(new DataInputStream(new ByteArrayInputStream(nbtRoot(c)))))),
         cell(() -> c.toString()));
      ListTag l = new ListTag();
      l.add(StringTag.valueOf(s));
      l.add(StringTag.valueOf(s + s));
      row("nbt.tsv", "nbt_list_write", s, cell(() -> hex(nbtAny(l))), cell(() -> l.toString()));
      row("wire.tsv", "net_write", s,
         cell(() -> {
            ByteBuf b = Unpooled.buffer();
            Utf8String.write(b, s, 32767);
            return bytes(b);
         }),
         cell(() -> hex(s.getBytes(StandardCharsets.UTF_8))),
         cell(() -> {
            ByteBuf b = Unpooled.buffer();
            Utf8String.write(b, s, 32767);
            return Utf8String.read(b, 32767);
         }));
      row("wire.tsv", "net_write_max2", s, cell(() -> {
         ByteBuf b = Unpooled.buffer();
         Utf8String.write(b, s, 2);
         return bytes(b);
      }));
   }

   static void nbtRead(String payloadHex) {
      byte[] payload = unhex(payloadHex);
      byte[] full = new byte[payload.length + 3];
      full[0] = 8;
      full[1] = (byte) (payload.length >> 8);
      full[2] = (byte) payload.length;
      System.arraycopy(payload, 0, full, 3, payload.length);
      row("nbt.tsv", "nbt_read", payloadHex,
         cell(() -> ((StringTag) readAny(full)).value()),
         cell(() -> readAny(full).toString()),
         cell(() -> hex(nbtAny(readAny(full)))));
      byte[] head = {10, 0, 0, 1, (byte) (payload.length >> 8), (byte) payload.length};
      // compound {<key>:1b} where the key is the bytes under test; the byte after the key is the value, then the end tag
      byte[] withValue = new byte[payload.length + 9];
      System.arraycopy(head, 0, withValue, 0, 6);
      System.arraycopy(payload, 0, withValue, 6, payload.length);
      withValue[payload.length + 6] = 5;
      withValue[payload.length + 7] = 0;
      withValue[payload.length + 8] = 0;
      row("nbt.tsv", "nbt_read_key", payloadHex,
         cell(() -> compoundKeys(NbtIo.read(new DataInputStream(new ByteArrayInputStream(withValue))))),
         cell(() -> hex(nbtRoot(NbtIo.read(new DataInputStream(new ByteArrayInputStream(withValue)))))));
      byte[] net = new byte[payload.length + 1];
      net[0] = (byte) payload.length;
      System.arraycopy(payload, 0, net, 1, payload.length);
      row("wire.tsv", "net_read", payloadHex, cell(() -> Utf8String.read(Unpooled.wrappedBuffer(net), 32767)));
   }

   /** Every byte string of length 1..4 over an alphabet of lead, continuation and invalid bytes, read as a network string. */
   static void netReadAll(byte[] prefix, int len, int[] alpha) {
      if (len > 0) {
         byte[] net = new byte[len + 1];
         net[0] = (byte) len;
         System.arraycopy(prefix, 0, net, 1, len);
         row("wire.tsv", "net_read_all", hex(java.util.Arrays.copyOf(prefix, len)), cell(() -> Utf8String.read(Unpooled.wrappedBuffer(net), 32767)));
      }
      if (len == 4) return;
      for (int a : alpha) {
         prefix[len] = (byte) a;
         netReadAll(prefix, len + 1, alpha);
      }
   }

   static void snbt(String text) {
      row("nbt.tsv", "snbt_parse", text,
         cell(() -> parseSnbt(text).toString()),
         cell(() -> hex(nbtAny(parseSnbt(text)))),
         cell(() -> new SnbtPrinterTagVisitor().visit(parseSnbt(text))));
      row("text.tsv", "tag_pretty", text,
         concat(componentCells(() -> new TextComponentTagVisitor("  ", TextComponentTagVisitor.PlainStyling.INSTANCE, true).visit(parseSnbt(text)))));
   }

   public static void main(String[] args) throws Exception {
      access = extractor.Setup.bootstrap();
      nbtOps = access.createSerializationContext(NbtOps.INSTANCE);
      jsonOps = access.createSerializationContext(JsonOps.INSTANCE);

      String[] strings = {"", "a", "\ud800", "\udc00", "\udbff", "\udfff", "a\ud800b", "\ud83d\ude00", "\ude00\ud83d", "a\ud83d", "\ude00b",
         "\ud800\ud800", "\ud83d\ud83d\ude00", "\ud83d\ude00\ud83d", "\ue000", "\ufffd", "\u0000", "\u00e9", "\u20ac", "x\ud83d\ude00y"};
      for (String s : strings) nbtString(s);

      // too long for writeUTF only because a lone surrogate takes 3 bytes: 21846 * 3 = 65538
      for (int n : new int[] {21845, 21846}) {
         String s = "\ud800".repeat(n);
         row("nbt.tsv", "nbt_string_write_rep:" + n, "\ud800", cell(() -> hex(nbtAny(StringTag.valueOf(s))).length() + ":" + hex(nbtAny(StringTag.valueOf(s))).substring(0, 20)));
      }

      // NbtIo reads modified UTF-8: a surrogate is 3 bytes (ed a0 80), a pair is two such sequences, 4-byte UTF-8 is an error
      for (String h : new String[] {"eda080", "edbfbf", "eda0bdb880", "edb880eda0bd", "eda0bdeda0bd", "efbfbd", "c080", "f09f9880", "e080", "eda0", "eda0bd", "61eda08062",
         "edc080", "eda0c0", "80", "ff", "e0a080", "c1bf", "e08080", "ed9fbf", "ee8080", "efbfbe", "c3", "e282", "f4908080", "f0808080", "418042", "f09f98", "eda0bdedb880", "f5808080", "f5", "f580", "f58080", "f7bfbfbf", "f5a0", "41f5"}) nbtRead(h);

      netReadAll(new byte[4], 0, new int[] {0x80, 0xa0, 0xbf, 0xc0, 0xc2, 0xe0, 0xed, 0xf0, 0xf4, 0x41, 0xff});

      // SNBT: `\ud800` escapes, in strings, keys and lists
      for (String s : new String[] {"\"\\ud800\"", "'\\ud83d\\ude00'", "\"\\ud83d\"", "\"a\\ud800b\"", "\"\\ude00\\ud83d\"", "{\"\\ud800\":1b}", "{\"\\ud800\":1,\"\\ufffd\":2,a:3}",
         "[\"\\ud800\",\"\\udc00\"]", "{a:\"\\ud83d\\ude00\"}", "{a:\"\\ud800\\ud800\"}", "\"\\ud83d\\\\ude00\"", "\"\\u00e9\\ud800\"", "\"\\x41\\ud800\"", "\"\\ud800\\u0041\"",
         "{text:\"\\ud800\"}", "{text:\"a\",extra:[\"\\ud83d\",\"\\ude00\"]}", "{id:\"a\",Count:1,tag:{x:\"\\ud800\"}}", "\"\\d800\"", "\"\\uzzzz\"", "\"\\ud8\""})
         snbt(s);

      // components from JSON and from SNBT (the command argument grammar)
      for (String s : new String[] {"\"\\ud800\"", "{\"text\":\"\\ud83d\"}", "{\"text\":\"a\\ud83d\",\"extra\":[\"\\ude00b\"]}", "{\"text\":\"x\",\"extra\":[\"\\ud83d\",\"\\ude00\"]}",
         "{\"translate\":\"%s\",\"fallback\":\"[%s]\",\"with\":[\"\\ud800\"]}", "{\"text\":\"b\",\"insertion\":\"\\ud800\"}",
         "{\"text\":\"b\",\"click_event\":{\"action\":\"copy_to_clipboard\",\"value\":\"\\ud800\"}}", "{\"text\":\"b\",\"hover_event\":{\"action\":\"show_text\",\"value\":\"\\ud800\"}}",
         "{\"keybind\":\"\\ud800\"}", "{\"translate\":\"\\ud800\"}", "{\"translate\":\"a\\ud83d\",\"fallback\":\"f\\ud800\"}", "{\"translate\":\"\\ud800\",\"with\":[\"x\"]}", "{\"translate\":\"b.\\ud800\",\"fallback\":\"x%s\",\"with\":[\"\\ud83d\"]}", "{\"text\":\"b\",\"font\":\"\\ud800\"}", "[\"\\ud83d\",\"\\ude00\"]", "\"\\ud83d\\ude00\"", "{\"text\":\"\\ud83d\\u0041\"}"})
         row("text.tsv", "comp_json", s, concat(componentCells(() -> fromJson(s))));
      // numbers of a JSON text beyond the range of a double or a float (`JsonOps.convertTo`: exact long, else float when exact, else double)
      for (String s : new String[] {"1e400", "-1e400", "1e-400", "3.4028236e38", "3.4028235e38", "0.1", "1.0E10", "12345678901234567890", "-9223372036854775808", "9223372036854775808", "1e2", "0.5e1", "100e-2"})
         row("text.tsv", "comp_num", s, concat(componentCells(() -> fromJson("{\"translate\":\"%s\",\"with\":[" + s + "]}"))));
      for (String s : new String[] {"\"\\ud800\"", "'\\ud83d'", "{text:\"\\ud83d\"}", "{text:\"a\\ud83d\",extra:[\"\\ude00b\"]}", "[\"\\ud83d\",\"\\ude00\"]",
         "{translate:\"%s\",fallback:\"[%s]\",with:[\"\\ud800\"]}", "{text:\"b\",insertion:\"\\ud800\"}",
         "{translate:\"\\ud800\"}", "{translate:\"a\\ud83d\",fallback:\"f\\ud800\"}",
         "{text:\"b\",click_event:{action:\"copy_to_clipboard\",value:\"\\ud800\"}}", "{text:\"b\",hover_event:{action:\"show_text\",value:\"\\ud800\"}}",
         "{text:\"a\",extra:[{text:\"\\ud83d\"},{text:\"\\ude00\"}]}"})
         row("text.tsv", "comp_snbt", s, concat(componentCells(() -> fromSnbt(s))));

      // signs, books and item names as a client receives them
      for (String s : new String[] {"\"\\ud800\"", "\"a\\ud83db\"", "\"\\ud83d\\ude00\"", "{\"text\":\"\\ude00\"}"}) {
         row("wire.tsv", "sign", s,
            cell(() -> {
               SignText t = SignText.EMPTY.asMutable().setLine(0, fromJson(s)).asImmutable();
               return hex(nbtAny(SignText.CODEC.encodeStart(nbtOps, t).getOrThrow()));
            }),
            cell(() -> {
               SignText t = SignText.EMPTY.asMutable().setLine(0, fromJson(s)).asImmutable();
               RegistryFriendlyByteBuf b = regBuf();
               SignText.STREAM_CODEC.encode(b, t);
               return bytes(b);
            }));
      }
      for (String s : new String[] {"\ud800", "a\ud83db", "\ud83d\ude00", "\ude00\ud83d"}) {
         row("wire.tsv", "book_page", s,
            cell(() -> hex(nbtAny(WritableBookContent.CODEC.encodeStart(nbtOps, new WritableBookContent(List.of(Filterable.passThrough(s)))).getOrThrow()))),
            cell(() -> {
               ByteBuf b = Unpooled.buffer();
               WritableBookContent.STREAM_CODEC.encode(b, new WritableBookContent(List.of(Filterable.passThrough(s))));
               return bytes(b);
            }),
            cell(() -> {
               ByteBuf b = Unpooled.buffer();
               WritableBookContent.STREAM_CODEC.encode(b, new WritableBookContent(List.of(Filterable.passThrough(s))));
               return ((Filterable<String>) WritableBookContent.STREAM_CODEC.decode(b).pages().get(0)).raw();
            }));
         row("wire.tsv", "book_title", s,
            cell(() -> {
               WrittenBookContent w = new WrittenBookContent(Filterable.passThrough(s), s, 0, List.of(Filterable.passThrough(Component.literal(s))), true);
               RegistryFriendlyByteBuf b = regBuf();
               WrittenBookContent.STREAM_CODEC.encode(b, w);
               return bytes(b);
            }),
            cell(() -> {
               WrittenBookContent w = new WrittenBookContent(Filterable.passThrough(s), s, 0, List.of(Filterable.passThrough(Component.literal(s))), true);
               return hex(nbtAny(WrittenBookContent.CODEC.encodeStart(nbtOps, w).getOrThrow()));
            }));
      }
      for (String s : new String[] {"\"\\ud800\"", "\"a\\ud83db\"", "\"\\ud83d\\ude00\"", "{text:\"\\ud800\",italic:false}"}) {
         row("wire.tsv", "item_name", s, concat(itemCells(() -> {
            ItemStack st = new ItemStack(Items.STICK);
            st.set(DataComponents.CUSTOM_NAME, fromSnbt(s));
            return st;
         })));
      }

      // items parsed by the item argument (as /give does)
      CommandBuildContext ctx = CommandBuildContext.simple(access, FeatureFlags.VANILLA_SET);
      for (String s : new String[] {"stick[custom_name=\"\\ud800\"]", "stick[custom_name='\"\\\\ud800\"']", "stick[item_name=\"\\ud83d\"]", "stick[custom_data={x:\"\\ud800\"}]",
         "stick[lore=[\"\\ud800\"]]", "writable_book[writable_book_content={pages:[\"\\ud800\"]}]", "stick[custom_name={text:\"a\\ud83d\",extra:[\"\\ude00\"]}]",
         "stick[custom_data={\"\\ud800\":1}]", "stick[custom_name=\"\\ud83d\\ude00\"]",
         "stick[block_state={\"\\ud800\":\"\\ud83d\"}]", "stick[block_state={a:\"\\ude00\"}]", "stick[custom_model_data={strings:[\"\\ud800\",\"a\\ud83d\"]}]",
         "potion[potion_contents={custom_name:\"\\ud800\"}]", "filled_map[map_decorations={\"\\ud800\":{type:\"player\",x:1d,z:1d,rotation:1f}}]"})
         row("game.tsv", "item_cmd", s, concat(itemCells(() -> new ItemArgument(ctx).parse(new StringReader(s)).createItemStack(1))));

      // item argument failures that quote a string with a lone surrogate: the message of the exception and the component the client gets
      String[] itemErrors = {"stick[rarity=\"\\ud800\"]", "stick[item_model=\"\\ud800\"]", "stick[item_model=\"a:\\ud83d\"]", "stick[tooltip_style=\"a\\ud83d\"]",
         "stick[jukebox_playable=\"\\ude00x\"]", "stick[enchantments={\"\\ud800\":1}]", "stick[custom_name={text:\"a\",color:\"\\ud800\"}]",
         "stick[custom_data=\"\\ud800\"]", "stick[lore=[{text:\"a\",font:\"\\ud800\"}]]",
         "stick[item_model=\"\\ud800:a\"]", "stick[item_model=\":\\ud800\"]", "stick[item_model=\"\\ud83d\\ude00a\\ud800:b\"]", "stick[item_model=\"a:\\ud83d\\ude00\\ud800\"]",
         "debug_stick[debug_stick_state={\"minecraft:stone\":\"\\ud800\"}]"};
      for (String s : itemErrors) {
         row("game.tsv", "item_err", s, errorCells(() -> new ItemArgument(ctx).parse(new StringReader(s)).createItemStack(1)));
         row("game.tsv", "item_pred_err", s, errorCells(() -> ItemPredicateArgument.itemPredicate(ctx).parse(new StringReader(s))));
      }

      // dialogs as sent to a client: the form controls (`initial`, `on_true`, `label_format`, option ids, keys) and command templates keep a lone surrogate
      String dialogHead = "{type:\"minecraft:notice\",title:\"t\",";
      String[] dialogs = {
         dialogHead + "inputs:[{type:\"minecraft:text\",key:\"k\",label:\"l\",initial:\"\\ud800\",max_length:5}]}",
         dialogHead + "inputs:[{type:\"minecraft:text\",key:\"k\",label:\"l\",initial:\"\\ud800\\ud800\",max_length:1}]}",
         dialogHead + "inputs:[{type:\"minecraft:text\",key:\"k\",label:\"l\",initial:\"\\ud83d\\ude00\",max_length:2}]}",
         dialogHead + "inputs:[{type:\"minecraft:boolean\",key:\"k\",label:\"l\",on_true:\"\\ud800\",on_false:\"a\\ud83d\"}]}",
         dialogHead + "inputs:[{type:\"minecraft:number_range\",key:\"k\",label:\"l\",label_format:\"\\ud800\",start:0f,end:1f}]}",
         dialogHead + "inputs:[{type:\"minecraft:single_option\",key:\"k\",label:\"l\",options:[{id:\"\\ud800\",initial:1b},\"\\ud83d\"]}]}",
         dialogHead + "inputs:[{type:\"minecraft:text\",key:\"\\ud800\",label:\"l\"}]}",
         dialogHead + "inputs:[{type:\"minecraft:text\",key:\"a\\ud83d\\ude00\",label:\"l\"}]}",
         dialogHead + "action:{label:\"x\",action:{type:\"minecraft:dynamic/run_command\",template:\"say \\ud800 $(a)\"}}}",
         dialogHead + "action:{label:\"x\",action:{type:\"minecraft:dynamic/run_command\",template:\"say $(a\\ud800)\"}}}",
         dialogHead + "action:{label:\"x\",action:{type:\"minecraft:dynamic/run_command\",template:\"\\ud83d$(a\"}}}",
         dialogHead + "action:{label:\"x\",action:{type:\"minecraft:dynamic/run_command\",template:\"\\ude00\"}}}",
      };
      for (String s : dialogs)
         row("wire.tsv", "dialog", s,
            cell(() -> hex(nbtAny(Dialog.DIRECT_CODEC.encodeStart(nbtOps, Dialog.DIRECT_CODEC.parse(nbtOps, parseSnbt(s)).getOrThrow()).getOrThrow()))),
            cell(() -> {
               ByteBuf b = Unpooled.buffer();
               Dialog.CONTEXT_FREE_STREAM_CODEC.encode(b, Dialog.DIRECT_CODEC.parse(nbtOps, parseSnbt(s)).getOrThrow());
               return bytes(b);
            }));

      // `resource-pack-prompt` of server.properties: a prompt that decodes only partly is kept (`resultOrPartial`), one that does not parse is dropped
      for (String s : new String[] {"{\"text\":\"hi\"}", "\"plain\"", "{\"text\":\"a\",\"color\":\"nope\"}", "{\"text\":\"a\",\"extra\":[\"b\",1,{\"text\":\"c\"}]}", "[{\"text\":\"a\"},{\"text\":5,\"bold\":\"x\"}]",
         "{\"translate\":\"k\",\"with\":[1,{\"bad\":[]}]}", "{\"text\":\"a\",\"extra\":[{\"text\":\"b\",\"extra\":[true]}]}", "not json", "{\"text\":\"a\",\"click_event\":{\"action\":\"run_command\"}}", "{\"text\":\"x\",\"bold\":\"maybe\"}", "{}",
         "{\"selector\":\"@e\",\"separator\":5}", "{\"text\":\"a\"} trailing", "[]", "[1]"})
         row("game.tsv", "pack_prompt", s, cell(() -> {
            java.util.Properties props = new java.util.Properties();
            props.setProperty("resource-pack", "http://localhost/pack.zip");
            props.setProperty("resource-pack-id", "00000000-0000-0000-0000-000000000001");
            props.setProperty("resource-pack-sha1", "0123456789abcdef0123456789abcdef01234567");
            props.setProperty("resource-pack-prompt", s);
            var info = new DedicatedServerProperties(props).serverResourcePackInfo;
            if (info.isEmpty()) return "no pack";
            Component prompt = info.get().prompt();
            return prompt == null ? "no prompt" : hex(nbtAny(ComponentSerialization.CODEC.encodeStart(nbtOps, prompt).getOrThrow()));
         }));

      // the status response and the login disconnect: `lenientJson` prints with Gson (no HTML escaping), `Utf8String.write` encodes
      // with Netty's `writeUtf8` (a lone surrogate is `?`, a high surrogate swallows the next unit)
      for (String s : new String[] {"\ud800", "\udc00", "a\ud83d", "\ud83d\ud83d", "\ud83dx", "\ud83d\ude00", "\ude00\ud83d", "<&>='", "a\u2028\ud800\u2029b", "\ud83d\ud83d\ud83d\ude00"}) {
         Component description = Component.literal(s);
         row("wire.tsv", "status_response", s,
            cell(() -> {
               ByteBuf b = Unpooled.buffer();
               ClientboundStatusResponsePacket.STREAM_CODEC.encode(b, new ClientboundStatusResponsePacket(new ServerStatus(description, Optional.empty(), Optional.empty(), Optional.empty(), false)));
               return bytes(b);
            }),
            cell(() -> {
               ByteBuf b = Unpooled.buffer();
               ClientboundLoginDisconnectPacket.STREAM_CODEC.encode(b, new ClientboundLoginDisconnectPacket(description));
               return bytes(b);
            }));
      }

      // brigadier: the context of a syntax error is a window of the input in UTF-16 units; it can split a pair
      String[][] contexts = {{"\ud83d\ude00abc", "1"}, {"\ud83d\ude00abc", "2"}, {"x\ud83d\ude00", "2"}, {"abcdefghi\ud83d\ude00z", "10"},
         {"abcdefghi\ud83d\ude00z", "11"}, {"\ud83d\ude00\ud83d\ude00\ud83d\ude00\ud83d\ude00\ud83d\ude00\ud83d\ude00", "11"}, {"\ud83d\ude00\ud83d\ude00\ud83d\ude00\ud83d\ude00\ud83d\ude00\ud83d\ude00", "12"},
         {"\ud83d\ude00\ud83d\ude00\ud83d\ude00\ud83d\ude00\ud83d\ude00\ud83d\ude00", "9"}, {"say \ud83d\ude00\ud83d\ude00\ud83d\ude00 hi", "7"}, {"\ud800abc", "1"}};
      for (String[] c : contexts) {
         int cursor = Integer.parseInt(c[1]);
         SimpleCommandExceptionType type = new SimpleCommandExceptionType(new LiteralMessage("Expected something"));
         row("brigadier.tsv", "ctx:" + cursor, c[0],
            cell(() -> {
               StringReader r = new StringReader(c[0]);
               r.setCursor(cursor);
               return type.createWithContext(r).getContext();
            }),
            cell(() -> {
               StringReader r = new StringReader(c[0]);
               r.setCursor(cursor);
               return type.createWithContext(r).getMessage();
            }),
            cell(() -> {
               StringReader r = new StringReader(c[0]);
               r.setCursor(cursor);
               return hex(nbtAny(ComponentSerialization.CODEC.encodeStart(nbtOps, Component.literal(type.createWithContext(r).getContext())).getOrThrow()));
            }));
      }

      Path outDir = Path.of(args[0]);
      for (Map.Entry<String, StringBuilder> e : FILES.entrySet()) Files.writeString(outDir.resolve(e.getKey()), e.getValue().toString());
   }
}
