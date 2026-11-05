// Prints `<input>\t<result>` for ItemPredicateArgument inputs, produced by the real vanilla classes:
// `OK@<cursor>|<bits of the predicate on the sample stacks>` or `ERR<cursor>:<message>`, then `|S<start>:<count>:<hashCode of the joined texts>:<first 6>`
// (the suggestions at the end of the input, sorted as brigadier sorts them).
// Usage: java -cp ./tools/extractor/build:$(scripts/vanilla-cp.sh) ItemPredicateGolden.java <out> <corpus>
import com.mojang.brigadier.StringReader;
import com.mojang.brigadier.exceptions.CommandSyntaxException;
import com.mojang.brigadier.suggestion.Suggestion;
import com.mojang.brigadier.suggestion.Suggestions;
import com.mojang.brigadier.suggestion.SuggestionsBuilder;
import java.io.PrintWriter;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import net.minecraft.commands.CommandBuildContext;
import net.minecraft.commands.Commands;
import net.minecraft.commands.arguments.item.ItemArgument;
import net.minecraft.commands.arguments.item.ItemPredicateArgument;
import net.minecraft.data.registries.VanillaRegistries;
import net.minecraft.world.item.ItemStack;
import net.minecraft.core.Registry;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.packs.PackType;
import net.minecraft.server.packs.resources.MultiPackResourceManager;
import net.minecraft.server.packs.repository.ServerPacksSource;
import net.minecraft.tags.TagLoader;
import net.minecraft.world.flag.FeatureFlags;

public class ItemPredicateGolden {
   static final String[] SAMPLES = {
      "stick", "diamond_sword[damage=10]", "diamond_sword", "enchanted_book", "apple", "stone", "potion", "chest",
      "stick[custom_name='\"x\"']", "diamond_sword[enchantments={'minecraft:sharpness':3}]", "stick[custom_data={a:1}]"
   };
   static final int[] COUNTS = {1, 3, 5};

   public static void main(String[] args) throws Exception {
      RegistryAccess.Frozen full = extractor.Setup.bootstrap();
      CommandBuildContext ctx = CommandBuildContext.simple(full, FeatureFlags.DEFAULT_FLAGS);
      ItemPredicateArgument arg = ItemPredicateArgument.itemPredicate(ctx);
      List<ItemStack> stacks = new ArrayList<>();
      for (String sample : SAMPLES) {
         for (int count : COUNTS) {
            ItemStack stack = ItemArgument.item(ctx).parse(new StringReader(sample)).createItemStack(1);
            stack.setCount(Math.min(count, stack.getMaxStackSize()));
            stacks.add(stack);
         }
      }
      try (PrintWriter out = new PrintWriter(args[0])) {
         for (String line : Files.readAllLines(Path.of(args[1]))) {
            if (line.isEmpty()) continue;
            StringBuilder result = new StringBuilder();
            StringReader reader = new StringReader(line);
            try {
               ItemPredicateArgument.Result parsed = arg.parse(reader);
               result.append("OK@").append(reader.getCursor()).append('|');
               for (ItemStack stack : stacks) {
                  result.append(parsed.test(stack) ? '1' : '0');
               }
            } catch (CommandSyntaxException e) {
               result.append("ERR").append(e.getCursor()).append(':').append(e.getMessage().replace("\n", "\\n"));
            }
            Suggestions suggestions = arg.listSuggestions(null, new SuggestionsBuilder(line, 0)).get();
            result.append("|S").append(suggestions.getRange().getStart()).append(':');
            List<String> texts = new ArrayList<>();
            for (Suggestion s : suggestions.getList()) texts.add(s.getText());
            String joined = String.join(",", texts);
            result.append(texts.size()).append(':').append(joined.hashCode()).append(':').append(String.join(",", texts.subList(0, Math.min(6, texts.size()))));
            out.println(line + "\t" + result);
         }
      }
   }
}
