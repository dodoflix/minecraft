import java.io.PrintStream;
import java.lang.reflect.Field;
import java.util.*;
import java.util.function.Consumer;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.nbt.NbtOps;
import net.minecraft.nbt.Tag;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.RegistryOps;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.RandomSequence;
import net.minecraft.world.damagesource.DamageSource;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.storage.loot.LootParams;
import net.minecraft.world.level.storage.loot.LootTable;
import net.minecraft.world.level.storage.loot.parameters.LootContextParamSets;
import net.minecraft.world.level.storage.loot.parameters.LootContextParams;
import net.minecraft.world.phys.Vec3;

/**
 * Loot golden of every loot table the block golden does not roll (every non-block table outside {@code chests/}, plus the
 * vault view of the trial chamber rewards), rolled through the call sites vanilla uses: the real entity death path
 * ({@code LivingEntity.dropFromLootTable} on a spawned victim, including its {@code DeathLootTableSeed}), gifts, shearing, equipment,
 * archaeology, fishing, block and entity interaction, bartering, container-style tables and the vault parameters. Stdout:
 * {@code S line} (a scenario line of {@code EntityOracleDriver}: the replay runs it before the next case), {@code T kind table
 * key=value..} (one roll; the keys describe the parameters), then {@code R sequence snbt} (seed 0: the table's random sequence after
 * the roll) and one {@code I snbt} per stack ({@code ItemStack.OPTIONAL_CODEC}).
 * Usage: scripts/golden-loot-all.sh
 * Output: crates/mc-game/tests/fixtures/loot_all_golden.txt.gz
 */
public final class LootAllOracle {
   static final PrintStream OUT = VanillaLevel.OUT;
   static final long[] SEEDS = {0L, 1L, 7L};
   static final String[] DYES = {"white", "orange", "magenta", "light_blue", "yellow", "lime", "pink", "gray", "light_gray", "cyan", "purple",
      "blue", "brown", "green", "red", "black"};
   /** Where victims stand (flat world surface). */
   static final String AT = "8.5 -60 8.5";
   static final Vec3 ORIGIN = new Vec3(8.5, -60, 8.5);

   static EntityOracleDriver d;
   static VanillaLevel v;
   static RegistryOps<Tag> ops;
   static Field sequenceField;
   static int counter;
   static String weapon = "";
   static final Set<String> rolled = new TreeSet<>();

   static void s(String line) throws Exception {
      OUT.println("S " + line);
      d.lineNo++;
      d.line = line;
      d.run(EntityOracleDriver.tokens(line));
   }

   static Entity ref(String name) {
      return d.refs.get(name);
   }

   static void spawn(String name, String type, String nbt) throws Exception {
      s("spawn " + name + " " + type + " " + AT + (nbt.isEmpty() ? "" : " " + nbt));
   }

   static void weapon(String item) throws Exception {
      if (!weapon.equals(item)) {
         s("give Looter mainhand " + item);
         weapon = item;
      }
   }

   public static void main(String[] args) throws Exception {
      d = new EntityOracleDriver();
      s("spawn_player Looter survival 0.5 -60 0.5");
      v = d.v;
      ops = v.server.registryAccess().createSerializationContext(NbtOps.INSTANCE);
      sequenceField = LootTable.class.getDeclaredField("randomSequence");
      sequenceField.setAccessible(true);
      // attackers shared by the death cases
      s("spawn arrow1 arrow 12.5 -60 8.5");
      s("spawn skel1 skeleton 12.5 -60 10.5");
      s("spawn ghast1 ghast 12.5 -60 12.5");
      s("spawn fireball1 fireball 12.5 -55 12.5");
      s("spawn creeper1 creeper 12.5 -60 14.5");
      s("spawn charged1 creeper 12.5 -60 16.5 {powered:1b}");
      for (String variant : new String[] {"temperate", "warm", "cold"}) {
         s("spawn frog_" + variant + " frog 12.5 -60 18.5 {variant:\"minecraft:" + variant + "\"}");
      }
      var lookup = v.server.reloadableRegistries().lookup();
      List<ResourceKey<LootTable>> keys = new ArrayList<>();
      lookup.lookupOrThrow(Registries.LOOT_TABLE).listElementIds().forEach(keys::add);
      keys.sort(Comparator.comparing(k -> k.identifier().toString()));
      for (ResourceKey<LootTable> key : keys) {
         String id = key.identifier().toString();
         String path = key.identifier().getPath();
         LootTable table = v.server.reloadableRegistries().getLootTable(key);
         String set = BuiltInRegistries.CONTEXT_KEY_SET.getKey(table.getParamSet()).getPath();
         switch (set) {
            case "block" -> {
               continue;
            }
            case "chest" -> {
               if (path.startsWith("chests/")) {
                  if (path.startsWith("chests/trial_chambers/reward")) {
                     vault(key, table);
                     rolled.add(id);
                  }
                  continue;
               }
               chest(key, table);
            }
            case "entity" -> death(key, table);
            case "equipment" -> equipment(key, table);
            case "gift" -> gift(key, table);
            case "shearing" -> shearing(key, table);
            case "archaeology" -> archaeology(key, table);
            case "fishing" -> fishing(key, table);
            case "block_interact" -> blockInteract(key, table);
            case "entity_interact" -> entityInteract(key, table);
            case "barter" -> barter(key, table);
            default -> throw new IllegalStateException("table " + id + " has the unhandled parameter set " + set);
         }
         rolled.add(id);
      }
      OUT.println("# tables " + rolled.size());
      OUT.flush();
      System.exit(0);
   }

   // ------------------------------------------------------------------------------------------------ rolls

   static Optional<Identifier> sequenceOf(LootTable table) throws Exception {
      @SuppressWarnings("unchecked")
      Optional<Identifier> sequence = (Optional<Identifier>)sequenceField.get(table);
      return sequence;
   }

   /** Seed 0 rolls draw from the table's sequence, or from `Level.random` for a table without one: pin that before the roll. */
   static void pinLevelRandom(long seed) throws Exception {
      if (seed == 0L) {
         s("level_seed " + (++counter));
      }
   }

   static void emit(String kind, ResourceKey<LootTable> key, LootTable table, String header, long seed, List<ItemStack> items) throws Exception {
      OUT.println("T " + kind + " " + key.identifier() + " " + header);
      if (seed == 0L && sequenceOf(table).isPresent()) {
         Identifier sequence = sequenceOf(table).get();
         v.server.getRandomSequences().forAllSequences((id, state) -> {
            if (id.equals(sequence)) {
               OUT.println("R " + id + " " + RandomSequence.CODEC.encodeStart(NbtOps.INSTANCE, state).getOrThrow());
            }
         });
      }
      for (ItemStack stack : items) {
         OUT.println("I " + ItemStack.OPTIONAL_CODEC.encodeStart(ops, stack).getOrThrow());
      }
   }

   static ItemStack tool(String id) {
      return id.equals("-") ? ItemStack.EMPTY : new ItemStack(BuiltInRegistries.ITEM.getValue(EntityOracleDriver.id(id)));
   }

   static LootParams.Builder builder() {
      return new LootParams.Builder(v.level);
   }

   /** One roll of `params` with `seed` (0: the table's sequence or `Level.random`), printed with `header`. */
   static void roll(String kind, ResourceKey<LootTable> key, LootTable table, String header, LootParams params, long seed) throws Exception {
      pinLevelRandom(seed);
      emit(kind, key, table, header + " seed=" + seed, seed, table.getRandomItems(params, seed));
   }

   // ------------------------------------------------------------------------------------------------ entity death

   record State(String name, String nbt, String rideType, boolean fire, boolean finalized) {
      State(String name, String nbt, String rideType, boolean fire) {
         this(name, nbt, rideType, fire, false);
      }
   }

   static final class Source {
      final String name;
      final String type;
      final String by;
      final String direct;
      final boolean playerKilled;
      final String weapon;

      Source(String name, String type, String by, String direct, boolean playerKilled, String weapon) {
         this.name = name;
         this.type = type;
         this.by = by;
         this.direct = direct;
         this.playerKilled = playerKilled;
         this.weapon = weapon;
      }
   }

   static final String SWORD = "diamond_sword";
   static final String LOOTING3 = "diamond_sword[enchantments={\"minecraft:looting\":3}]";
   static final String FIRE3 = "diamond_sword[enchantments={\"minecraft:looting\":3,\"minecraft:fire_aspect\":2}]";
   static final List<Source> SOURCES = List.of(
      new Source("none", "generic", "-", "-", false, SWORD),
      new Source("explosion", "explosion", "-", "-", false, SWORD),
      new Source("fall", "fall", "-", "-", false, SWORD),
      new Source("magic", "magic", "-", "-", false, SWORD),
      new Source("lightning", "lightning_bolt", "-", "-", false, SWORD),
      new Source("player_looting0", "player_attack", "Looter", "Looter", true, SWORD),
      new Source("player_looting3", "player_attack", "Looter", "Looter", true, LOOTING3),
      new Source("player_fire", "player_attack", "Looter", "Looter", true, FIRE3),
      new Source("arrow_looting3", "arrow", "Looter", "arrow1", true, LOOTING3),
      new Source("skeleton_arrow", "arrow", "skel1", "arrow1", false, SWORD),
      new Source("frog_temperate", "mob_attack", "frog_temperate", "frog_temperate", false, SWORD),
      new Source("frog_warm", "mob_attack", "frog_warm", "frog_warm", false, SWORD),
      new Source("frog_cold", "mob_attack", "frog_cold", "frog_cold", false, SWORD),
      new Source("fireball", "fireball", "ghast1", "fireball1", false, SWORD),
      new Source("creeper", "explosion", "creeper1", "creeper1", false, SWORD));
   static final List<Source> CHARGED = List.of(
      new Source("none", "generic", "-", "-", false, SWORD),
      new Source("charged_creeper", "explosion", "charged1", "charged1", false, SWORD));

   /** The victims and states that put `path` through its conditions (the entity of the table and the states its conditions read). */
   static void death(ResourceKey<LootTable> key, LootTable table) throws Exception {
      String path = key.identifier().getPath();
      if (path.startsWith("charged_creeper/")) {
         String kind = path.substring("charged_creeper/".length());
         for (String type : new String[] {"creeper", "piglin", "skeleton", "wither_skeleton", "zombie", "pig"}) {
            if (kind.equals("root") || kind.equals(type)) {
               victim(key, table, type, List.of(new State("normal", "", null, false)), CHARGED);
            }
         }
         return;
      }
      String name = path.substring("entities/".length());
      if (name.equals("sheep")) {
         for (int c = 0; c < 16; c++) {
            victim(key, table, "sheep", sheepStates(c), SOURCES);
         }
         return;
      }
      if (name.startsWith("sheep/")) {
         int c = Arrays.asList(DYES).indexOf(name.substring(6));
         victim(key, table, "sheep", sheepStates(c), SOURCES);
         return;
      }
      List<State> states = new ArrayList<>(List.of(new State("normal", "", null, false)));
      if (!name.equals("player")) {
         states.add(new State("baby", "{Age:-24000,IsBaby:1b}", null, false));
         states.add(new State("burning", "", null, true));
      }
      switch (name) {
         case "slime", "magma_cube" -> {
            states.add(new State("size0", "{Size:0}", null, false));
            states.add(new State("size1", "{Size:1}", null, false));
            states.add(new State("size3", "{Size:3}", null, false));
         }
         // the ominous banner of a captain comes from `finalizeSpawn`
         case "pillager" -> states.add(new State("captain", "{PatrolLeader:1b}", null, false, true));
         case "zombie" -> states.add(new State("chicken_jockey", "{IsBaby:1b}", "chicken", false));
         case "husk" -> states.add(new State("camel_rider", "", "camel_husk", false));
         default -> {
         }
      }
      victim(key, table, name, states, SOURCES);
   }

   /** A sheep of that colour, shorn or not (`sheared` is read by the sheep tables). */
   static List<State> sheepStates(int color) {
      return List.of(new State("color" + color, "{Color:" + color + "b}", null, false),
         new State("sheared" + color, "{Color:" + color + "b,Sheared:1b}", null, false));
   }

   static void victim(ResourceKey<LootTable> key, LootTable table, String type, List<State> states, List<Source> sources) throws Exception {
      for (State state : states) {
         String vname = "v" + (++counter);
         String ride = null;
         if (state.rideType != null) {
            ride = "r" + counter;
            spawn(ride, state.rideType, "");
         }
         if (type.equals("player")) {
            s("spawn_player " + vname + " survival " + AT);
         } else if (state.finalized) {
            s("level_seed " + (++counter));
            s("mobspawn " + vname + " " + type + " " + AT + " " + state.nbt);
         } else {
            spawn(vname, type, state.nbt);
         }
         if (ride != null) {
            s("ride " + vname + " " + ride);
         }
         if (state.fire) {
            s("set " + vname + " fire 100");
         }
         LivingEntity victim = (LivingEntity)ref(vname);
         if (state.name.equals("captain") && !((net.minecraft.world.entity.raid.Raider)victim).isCaptain()) {
            throw new IllegalStateException("the captain state did not produce a captain");
         }
         if (state.name.equals("baby") && !victim.isBaby()) {
            cleanup(vname, ride);
            continue;
         }
         boolean full = state.name.equals("normal") || state.name.startsWith("color") || state.name.startsWith("sheared") || state.name.startsWith("size") || state.name.equals("captain")
            || state.rideType != null;
         for (Source source : sources) {
            if (!full && !(source.name.equals("none") || source.name.startsWith("player"))) {
               continue;
            }
            for (long seed : SEEDS) {
               s("nbt " + vname + " {DeathLootTableSeed:" + seed + "L}");
               if (source.playerKilled) {
                  weapon(source.weapon);
                  victim.setLastHurtByPlayer((Player)ref("Looter"), 100);
               } else {
                  weapon(SWORD);
               }
               Map<String, String> o = new HashMap<>();
               if (!source.by.equals("-")) {
                  o.put("by", source.by);
               }
               if (!source.direct.equals("-")) {
                  o.put("direct", source.direct);
               }
               DamageSource damage = d.damageSource(source.type, o);
               pinLevelRandom(seed);
               List<ItemStack> items = new ArrayList<>();
               victim.dropFromLootTable(v.level, damage, source.playerKilled, key, items::add);
               String nat = victim.getLootTable().map(k -> k.identifier().toString()).orElse("-");
               emit("death", key, table, "victim=" + vname + " source=" + source.type + " by=" + source.by + " direct=" + source.direct + " pk="
                  + (source.playerKilled ? 1 : 0) + " nat=" + nat + " state=" + state.name + " via=" + source.name + " seed=" + seed, seed, items);
            }
         }
         cleanup(vname, ride);
      }
   }

   static void cleanup(String name, String ride) throws Exception {
      if (!(ref(name) instanceof Player)) {
         s("kill " + name);
      }
      if (ride != null) {
         s("kill " + ride);
      }
   }

   // ------------------------------------------------------------------------------------------------ other call sites

   /** `Mob.equip(EquipmentTable)`: THIS_ENTITY + ORIGIN of the mob. */
   static void equipment(ResourceKey<LootTable> key, LootTable table) throws Exception {
      for (String type : new String[] {"zombie", "skeleton", "husk"}) {
         String vname = "v" + (++counter);
         spawn(vname, type, "");
         for (long seed : SEEDS) {
            Entity e = ref(vname);
            roll("equipment", key, table, "this=" + vname, builder().withParameter(LootContextParams.ORIGIN, e.position())
               .withParameter(LootContextParams.THIS_ENTITY, e).create(LootContextParamSets.EQUIPMENT), seed);
         }
         s("kill " + vname);
      }
   }

   /** `LivingEntity.dropFromGiftLootTable`: ORIGIN + THIS_ENTITY of the giver. */
   static void gift(ResourceKey<LootTable> key, LootTable table) throws Exception {
      String path = key.identifier().getPath();
      String type;
      List<String> nbts = new ArrayList<>();
      if (path.startsWith("gameplay/hero_of_the_village/")) {
         String profession = path.substring("gameplay/hero_of_the_village/".length(), path.length() - "_gift".length());
         type = "villager";
         if (profession.equals("baby")) {
            nbts.add("{Age:-24000}");
         } else {
            nbts.add("{VillagerData:{profession:\"minecraft:" + (profession.equals("unemployed") ? "none" : profession)
               + "\",level:1,type:\"minecraft:plains\"}}");
         }
      } else {
         type = switch (path) {
            case "gameplay/cat_morning_gift" -> "cat";
            case "gameplay/chicken_lay" -> "chicken";
            case "gameplay/armadillo_shed" -> "armadillo";
            case "gameplay/panda_sneeze" -> "panda";
            case "gameplay/sniffer_digging" -> "sniffer";
            case "gameplay/turtle_grow" -> "turtle";
            default -> throw new IllegalStateException("gift table " + path);
         };
         if (type.equals("chicken")) {
            for (String variant : new String[] {"temperate", "warm", "cold"}) {
               nbts.add("{variant:\"minecraft:" + variant + "\"}");
            }
         } else {
            nbts.add("");
         }
      }
      for (String nbt : nbts) {
         String vname = "v" + (++counter);
         spawn(vname, type, nbt);
         for (long seed : SEEDS) {
            Entity e = ref(vname);
            roll("gift", key, table, "this=" + vname, builder().withParameter(LootContextParams.ORIGIN, e.position())
               .withParameter(LootContextParams.THIS_ENTITY, e).create(LootContextParamSets.GIFT), seed);
         }
         s("kill " + vname);
      }
   }

   /** `LivingEntity.dropFromShearingLootTable`: ORIGIN + THIS_ENTITY + the shears. */
   static void shearing(ResourceKey<LootTable> key, LootTable table) throws Exception {
      String path = key.identifier().getPath();
      String type;
      List<String> nbts = new ArrayList<>();
      if (path.equals("shearing/sheep")) {
         type = "sheep";
         for (int c = 0; c < 16; c++) {
            nbts.add("{Color:" + c + "b}");
            nbts.add("{Color:" + c + "b,Sheared:1b}");
         }
      } else if (path.startsWith("shearing/sheep/")) {
         type = "sheep";
         int color = Arrays.asList(DYES).indexOf(path.substring("shearing/sheep/".length()));
         nbts.add("{Color:" + color + "b}");
         nbts.add("{Color:" + color + "b,Sheared:1b}");
      } else if (path.startsWith("shearing/mooshroom")) {
         type = "mooshroom";
         String variant = path.substring("shearing/mooshroom".length());
         if (variant.isEmpty()) {
            nbts.add("{Type:\"red\"}");
            nbts.add("{Type:\"brown\"}");
         } else {
            nbts.add("{Type:\"" + variant.substring(1) + "\"}");
         }
      } else {
         type = path.substring("shearing/".length());
         nbts.add("");
      }
      for (String nbt : nbts) {
         String vname = "v" + (++counter);
         spawn(vname, type, nbt);
         for (long seed : SEEDS) {
            Entity e = ref(vname);
            roll("shearing", key, table, "this=" + vname + " tool=shears", builder().withParameter(LootContextParams.ORIGIN, e.position())
               .withParameter(LootContextParams.THIS_ENTITY, e).withParameter(LootContextParams.TOOL, tool("shears"))
               .create(LootContextParamSets.SHEARING), seed);
         }
         s("kill " + vname);
      }
   }

   /** `BrushableBlockEntity.unpackLootTable`: ORIGIN + user luck + THIS_ENTITY (the user) + the brush. */
   static void archaeology(ResourceKey<LootTable> key, LootTable table) throws Exception {
      for (int luck : new int[] {0, 3}) {
         for (long seed : SEEDS) {
            roll("archaeology", key, table, "this=Looter tool=brush luck=" + luck, builder().withParameter(LootContextParams.ORIGIN, ORIGIN)
               .withLuck(luck).withParameter(LootContextParams.THIS_ENTITY, ref("Looter")).withParameter(LootContextParams.TOOL, tool("brush"))
               .create(LootContextParamSets.ARCHAEOLOGY), seed);
         }
      }
   }

   /** `FishingHook.retrieve`: ORIGIN + the rod + THIS_ENTITY (the bobber) + luck; `openWater` is the bobber's field. */
   static void fishing(ResourceKey<LootTable> key, LootTable table) throws Exception {
      for (boolean open : new boolean[] {true, false}) {
         String vname = "v" + (++counter);
         spawn(vname, "fishing_bobber", "");
         VanillaLevel.setField(ref(vname), "openWater", open);
         for (int luck : new int[] {0, 1, 3}) {
            for (long seed : SEEDS) {
               Entity hook = ref(vname);
               roll("fishing", key, table, "this=" + vname + " tool=fishing_rod luck=" + luck + " open=" + (open ? 1 : 0), builder()
                  .withParameter(LootContextParams.ORIGIN, hook.position()).withParameter(LootContextParams.TOOL, tool("fishing_rod"))
                  .withParameter(LootContextParams.THIS_ENTITY, hook).withLuck(luck).create(LootContextParamSets.FISHING), seed);
            }
         }
         s("kill " + vname);
      }
   }

   /** `Block.dropFromBlockInteractLootTable`: the interacted state, its centre, optional interacting entity and tool. */
   static void blockInteract(ResourceKey<LootTable> key, LootTable table) throws Exception {
      String path = key.identifier().getPath();
      List<String> states = switch (path) {
         case "carve/pumpkin" -> List.of("minecraft:pumpkin");
         case "harvest/beehive" -> List.of("minecraft:beehive[facing=north,honey_level=5]");
         case "harvest/cave_vine" -> List.of("minecraft:cave_vines[age=5,berries=true]");
         case "harvest/sweet_berry_bush" -> List.of("minecraft:sweet_berry_bush[age=2]", "minecraft:sweet_berry_bush[age=3]");
         case "till/rooted_dirt" -> List.of("minecraft:rooted_dirt");
         default -> throw new IllegalStateException("block interact table " + path);
      };
      BlockPos pos = new BlockPos(8, -60, 8);
      for (String spec : states) {
         BlockState state = d.state(spec);
         for (String tool : new String[] {"-", "shears", "iron_hoe"}) {
            for (String by : new String[] {"-", "Looter"}) {
               for (long seed : SEEDS) {
                  LootParams.Builder b = builder().withParameter(LootContextParams.BLOCK_STATE, state)
                     .withParameter(LootContextParams.ORIGIN, Vec3.atCenterOf(pos));
                  if (!by.equals("-")) {
                     b.withParameter(LootContextParams.INTERACTING_ENTITY, ref(by));
                  }
                  if (!tool.equals("-")) {
                     b.withParameter(LootContextParams.TOOL, tool(tool));
                  }
                  roll("interact", key, table, "state=" + spec + " tool=" + tool + " by=" + by, b.create(LootContextParamSets.BLOCK_INTERACT), seed);
               }
            }
         }
      }
   }

   /** `LivingEntity.dropFromEntityInteractLootTable`: the target, the interacting player and the tool. */
   static void entityInteract(ResourceKey<LootTable> key, LootTable table) throws Exception {
      String vname = "v" + (++counter);
      spawn(vname, "armadillo", "");
      for (long seed : SEEDS) {
         roll("entity_interact", key, table, "target=" + vname + " by=Looter tool=brush", builder()
            .withParameter(LootContextParams.TARGET_ENTITY, ref(vname)).withOptionalParameter(LootContextParams.INTERACTING_ENTITY, ref("Looter"))
            .withParameter(LootContextParams.TOOL, tool("brush")).create(LootContextParamSets.ENTITY_INTERACT), seed);
      }
      s("kill " + vname);
   }

   /** `PiglinAi.getBarterResponseItems`: THIS_ENTITY (the piglin). */
   static void barter(ResourceKey<LootTable> key, LootTable table) throws Exception {
      for (String nbt : new String[] {"", "{IsBaby:1b}"}) {
         String vname = "v" + (++counter);
         spawn(vname, "piglin", nbt);
         for (long seed : SEEDS) {
            roll("barter", key, table, "this=" + vname, builder().withParameter(LootContextParams.THIS_ENTITY, ref(vname))
               .create(LootContextParamSets.PIGLIN_BARTER), seed);
         }
         s("kill " + vname);
      }
   }

   /** The chest parameter set of a table that is not under `chests/`: ORIGIN and optionally the player as THIS_ENTITY with luck. */
   static void chest(ResourceKey<LootTable> key, LootTable table) throws Exception {
      for (String variant : new String[] {"none", "luck0", "luck1", "luck3"}) {
         for (long seed : SEEDS) {
            LootParams.Builder b = builder().withParameter(LootContextParams.ORIGIN, ORIGIN);
            if (!variant.equals("none")) {
               b.withParameter(LootContextParams.THIS_ENTITY, ref("Looter")).withLuck(variant.charAt(4) - '0');
            }
            roll("chest", key, table, "variant=" + variant, b.create(LootContextParamSets.CHEST), seed);
         }
      }
   }

   /** `VaultBlockEntity.Server.resolveItemsToEject`: ORIGIN, the player's luck, THIS_ENTITY and the inserted key. */
   static void vault(ResourceKey<LootTable> key, LootTable table) throws Exception {
      for (String tool : new String[] {"trial_key", "ominous_trial_key"}) {
         for (int luck : new int[] {0, 1, 3}) {
            for (long seed : SEEDS) {
               roll("vault", key, table, "this=Looter tool=" + tool + " luck=" + luck, builder().withParameter(LootContextParams.ORIGIN, ORIGIN)
                  .withLuck(luck).withParameter(LootContextParams.THIS_ENTITY, ref("Looter")).withParameter(LootContextParams.TOOL, tool(tool))
                  .create(LootContextParamSets.VAULT), seed);
            }
         }
      }
   }
}
