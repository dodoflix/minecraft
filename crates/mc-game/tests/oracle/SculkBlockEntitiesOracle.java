import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.util.*;
import net.minecraft.core.BlockPos;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.NbtIo;
import net.minecraft.nbt.TagParser;
import net.minecraft.resources.Identifier;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.entity.BlockEntityType;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.Property;

/** Golden for the worldgen block entities (sculk catalyst/sensor/calibrated sensor/shrieker, creaking heart, potent sulfur,
 *  end portal/gateway): a fresh entity's `saveWithFullMetadata`, and `loadStatic` (id/x/y/z + the case's NBT) followed by
 *  `saveWithFullMetadata` and `getUpdateTag`. Line: case TAB block TAB props TAB input-snbt TAB saved-hex TAB saved-snbt TAB update-hex TAB has-update-packet TAB packet-tag-hex (saveCustomOnly).
 *  Usage: scripts/vanilla-java.sh -o crates/mc-game/tests/fixtures/sculk_block_entities.txt crates/mc-game/tests/oracle/SculkBlockEntitiesOracle.java crates/mc-game/tests/fixtures/sculk_block_entities.txt
 *  Output: crates/mc-game/tests/fixtures/sculk_block_entities.txt */
public class SculkBlockEntitiesOracle {
    static final BlockPos POS = new BlockPos(10, 64, -20);
    static RegistryAccess REG; // the static layer: every built-in registry (game_event for the vibration codecs)
    static final StringBuilder OUT = new StringBuilder();
    static int n;

    static BlockState state(String block, String props) {
        Block b = BuiltInRegistries.BLOCK.getValue(Identifier.withDefaultNamespace(block));
        BlockState s = b.defaultBlockState();
        if (!props.isEmpty()) {
            for (String kv : props.split(",")) {
                String[] p = kv.split("=");
                Property<?> prop = b.getStateDefinition().getProperty(p[0]);
                s = set(s, prop, p[1]);
            }
        }
        return s;
    }

    static <T extends Comparable<T>> BlockState set(BlockState s, Property<T> prop, String v) {
        return s.setValue(prop, prop.getValue(v).orElseThrow());
    }

    static BlockEntityType<?> typeOf(BlockState s) {
        for (BlockEntityType<?> t : BuiltInRegistries.BLOCK_ENTITY_TYPE) if (t.isValid(s)) return t;
        throw new IllegalStateException(s.toString());
    }

    static String hex(CompoundTag tag) throws Exception {
        ByteArrayOutputStream bytes = new ByteArrayOutputStream();
        NbtIo.write(tag, new DataOutputStream(bytes));
        StringBuilder sb = new StringBuilder();
        for (byte b : bytes.toByteArray()) sb.append(String.format("%02x", b & 0xFF));
        return sb.toString();
    }

    static void line(String name, String block, String props, String input) throws Exception {
        BlockState s = state(block, props);
        BlockEntityType<?> type = typeOf(s);
        BlockEntity be;
        if (input.equals("-")) {
            be = type.create(POS, s);
        } else {
            CompoundTag tag = TagParser.parseCompoundFully(input);
            tag.putString("id", BuiltInRegistries.BLOCK_ENTITY_TYPE.getKey(type).toString());
            tag.putInt("x", POS.getX());
            tag.putInt("y", POS.getY());
            tag.putInt("z", POS.getZ());
            be = BlockEntity.loadStatic(POS, s, tag, REG);
            if (be == null) throw new IllegalStateException(name);
        }
        CompoundTag saved = be.saveWithFullMetadata(REG);
        CompoundTag update = be.getUpdateTag(REG);
        OUT.append(name).append('\t').append(block).append('\t').append(props).append('\t').append(input).append('\t')
            .append(hex(saved)).append('\t').append(saved).append('\t').append(hex(update)).append('\t')
            .append(be.getClass().getMethod("getUpdatePacket").getDeclaringClass() != BlockEntity.class).append('\t')
            .append(hex(be.saveCustomOnly(REG))).append('\n');
        n++;
    }

    static final String EVT = "{game_event:\"minecraft:step\",distance:3.5f,pos:[1.0d,2.5d,-3.0d]}";
    static final String EVT_FULL = "{game_event:\"minecraft:block_destroy\",distance:0.0f,pos:[0.5d,70.0d,0.5d],source:[I;1,2,3,4],projectile_owner:[I;-1,-2,-3,-4]}";

    public static void main(String[] args) throws Exception {
        try { run(args[0]); } catch (Throwable t) { t.printStackTrace(); System.exit(1); }
    }

    static void run(String out) throws Exception {
        net.minecraft.SharedConstants.tryDetectVersion();
        net.minecraft.server.Bootstrap.bootStrap();
        REG = RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);

        // ---- fresh entities in several states
        for (String bloom : new String[]{"false", "true"}) line("fresh_catalyst_" + bloom, "sculk_catalyst", "bloom=" + bloom, "-");
        for (String phase : new String[]{"inactive", "active", "cooldown"})
            for (String wl : new String[]{"false", "true"})
                line("fresh_sensor_" + phase + "_" + wl, "sculk_sensor", "sculk_sensor_phase=" + phase + ",waterlogged=" + wl + ",power=" + (phase.equals("active") ? 15 : 0), "-");
        for (String f : new String[]{"north", "east", "south", "west"})
            line("fresh_calibrated_" + f, "calibrated_sculk_sensor", "facing=" + f + ",power=7,sculk_sensor_phase=active,waterlogged=true", "-");
        for (String shrieking : new String[]{"false", "true"})
            for (String summon : new String[]{"false", "true"})
                line("fresh_shrieker_" + shrieking + "_" + summon, "sculk_shrieker", "shrieking=" + shrieking + ",can_summon=" + summon + ",waterlogged=" + summon, "-");
        for (String axis : new String[]{"x", "y", "z"}) line("fresh_creaking_heart_" + axis, "creaking_heart", "axis=" + axis, "-");
        line("fresh_creaking_heart_natural", "creaking_heart", "axis=y,natural=true", "-");
        for (String st : new String[]{"dry", "dormant", "erupting"}) line("fresh_potent_sulfur_" + st, "potent_sulfur", "potent_sulfur_state=" + st, "-");
        line("fresh_end_portal", "end_portal", "", "-");
        line("fresh_end_gateway", "end_gateway", "", "-");

        // ---- sculk catalyst: SculkSpreader cursors
        line("cat_empty", "sculk_catalyst", "", "{}");
        line("cat_one", "sculk_catalyst", "", "{cursors:[{pos:[I;1,2,3],charge:5}]}");
        line("cat_defaults", "sculk_catalyst", "", "{cursors:[{pos:[I;1,2,3],charge:0,decay_delay:1,update_delay:0}]}");
        line("cat_all", "sculk_catalyst", "", "{cursors:[{pos:[I;-1,2,3],charge:1000,decay_delay:0,update_delay:12,facings:[\"up\",\"north\"]}]}");
        line("cat_facings_unsorted_dups", "sculk_catalyst", "", "{cursors:[{pos:[I;1,2,3],charge:3,facings:[\"east\",\"up\",\"down\",\"up\",\"west\"]}]}");
        line("cat_facings_empty", "sculk_catalyst", "", "{cursors:[{pos:[I;1,2,3],charge:3,facings:[]}]}");
        line("cat_facings_invalid", "sculk_catalyst", "", "{cursors:[{pos:[I;1,2,3],charge:3,facings:[\"up\",\"sideways\"]}]}");
        line("cat_facings_not_list", "sculk_catalyst", "", "{cursors:[{pos:[I;1,2,3],charge:3,facings:\"up\"}]}");
        line("cat_charge_too_big", "sculk_catalyst", "", "{cursors:[{pos:[I;1,2,3],charge:1001}]}");
        line("cat_charge_negative", "sculk_catalyst", "", "{cursors:[{pos:[I;1,2,3],charge:-1}]}");
        line("cat_one_bad_of_two", "sculk_catalyst", "", "{cursors:[{pos:[I;1,2,3],charge:7},{pos:[I;1,2],charge:7}]}");
        line("cat_decay_delay_2", "sculk_catalyst", "", "{cursors:[{pos:[I;1,2,3],decay_delay:2}]}");
        line("cat_update_delay_negative", "sculk_catalyst", "", "{cursors:[{pos:[I;1,2,3],update_delay:-4}]}");
        line("cat_update_delay_big", "sculk_catalyst", "", "{cursors:[{pos:[I;1,2,3],update_delay:2147483647}]}");
        line("cat_no_pos", "sculk_catalyst", "", "{cursors:[{charge:7}]}");
        line("cat_pos_list", "sculk_catalyst", "", "{cursors:[{pos:[1,2,3],charge:7}]}");
        line("cat_pos_bytes", "sculk_catalyst", "", "{cursors:[{pos:[B;1,2,3],charge:7}]}");
        line("cat_numeric_types", "sculk_catalyst", "", "{cursors:[{pos:[I;1,2,3],charge:5.0d,decay_delay:1b,update_delay:3s}]}");
        line("cat_charge_string", "sculk_catalyst", "", "{cursors:[{pos:[I;1,2,3],charge:\"5\"}]}");
        line("cat_not_list", "sculk_catalyst", "", "{cursors:{pos:[I;1,2,3]}}");
        line("cat_extra_fields", "sculk_catalyst", "", "{foo:1b,cursors:[{pos:[I;1,2,3],charge:2,extra:1b}],stats:{total:1}}");
        StringBuilder many = new StringBuilder("{cursors:[");
        for (int i = 0; i < 32; i++) many.append(i > 0 ? "," : "").append("{pos:[I;").append(i).append(",0,").append(-i).append("],charge:").append(i + 1).append("}");
        line("cat_32", "sculk_catalyst", "", many + "]}");
        many.append(",{pos:[I;99,0,0],charge:1}");
        line("cat_33", "sculk_catalyst", "", many + "]}");
        line("cat_components", "sculk_catalyst", "", "{components:{\"minecraft:custom_data\":{a:1b}},cursors:[{pos:[I;1,2,3],charge:2}]}");
        line("cat_components_invalid", "sculk_catalyst", "", "{components:{\"minecraft:nonexistent\":1b},cursors:[{pos:[I;1,2,3],charge:2}]}");

        // ---- sculk sensor / calibrated sensor / shrieker: last_vibration_frequency, warning_level, listener
        String[][] sensors = {{"sculk_sensor", "sculk_sensor_phase=active,power=9,waterlogged=true"}, {"calibrated_sculk_sensor", "facing=west,power=3,sculk_sensor_phase=cooldown"}};
        for (String[] s : sensors) {
            String b = s[0], p = s[1];
            line(b + "_empty", b, p, "{}");
            line(b + "_freq", b, p, "{last_vibration_frequency:6}");
            line(b + "_freq_types", b, p, "{last_vibration_frequency:6.7d}");
            line(b + "_freq_string", b, p, "{last_vibration_frequency:\"6\"}");
            line(b + "_freq_long", b, p, "{last_vibration_frequency:5000000000L}");
            line(b + "_listener_empty", b, p, "{listener:{}}");
            line(b + "_listener_min", b, p, "{listener:{selector:{tick:-1L}}}");
            line(b + "_listener_default_fields", b, p, "{listener:{selector:{tick:-1L},event_delay:0}}");
            line(b + "_listener_delay", b, p, "{last_vibration_frequency:3,listener:{selector:{tick:-1L},event_delay:17}}");
            line(b + "_listener_delay_negative", b, p, "{listener:{selector:{tick:-1L},event_delay:-1}}");
            line(b + "_listener_event", b, p, "{last_vibration_frequency:1,listener:{event:" + EVT + ",selector:{tick:-1L},event_delay:4}}");
            line(b + "_listener_event_full", b, p, "{listener:{event:" + EVT_FULL + ",selector:{tick:-1L}}}");
            line(b + "_listener_event_bare_name", b, p, "{listener:{event:{game_event:\"step\",distance:1.0f,pos:[0.0d,0.0d,0.0d]},selector:{tick:-1L}}}");
            line(b + "_listener_event_unknown", b, p, "{listener:{event:{game_event:\"nothing\",distance:1.0f,pos:[0.0d,0.0d,0.0d]},selector:{tick:5L},event_delay:2}}");
            line(b + "_listener_event_bad_distance", b, p, "{listener:{event:{game_event:\"step\",distance:-1.0f,pos:[0.0d,0.0d,0.0d]},selector:{tick:-1L}}}");
            line(b + "_listener_event_bad_pos", b, p, "{listener:{event:{game_event:\"step\",distance:1.0f,pos:[0.0d,0.0d]},selector:{tick:-1L}}}");
            line(b + "_listener_event_int_pos", b, p, "{listener:{event:{game_event:\"step\",distance:1,pos:[1,2,3]},selector:{tick:-1L}}}");
            line(b + "_listener_event_bad_source", b, p, "{listener:{event:{game_event:\"step\",distance:1.0f,pos:[0.0d,0.0d,0.0d],source:[I;1,2,3],projectile_owner:\"x\"},selector:{tick:-1L}}}");
            line(b + "_listener_selector_event", b, p, "{listener:{selector:{event:" + EVT_FULL + ",tick:123456789012L}}}");
            line(b + "_listener_selector_event_int_tick", b, p, "{listener:{selector:{event:" + EVT + ",tick:12}}}");
            line(b + "_listener_selector_tick_no_event", b, p, "{listener:{selector:{tick:42L}}}");
            line(b + "_listener_selector_bad_event", b, p, "{listener:{selector:{event:{game_event:\"zzz\"},tick:42L}}}");
            line(b + "_listener_selector_no_tick", b, p, "{listener:{selector:{event:" + EVT + "}}}");
            line(b + "_listener_both", b, p, "{last_vibration_frequency:15,listener:{event:" + EVT + ",selector:{event:" + EVT_FULL + ",tick:99L},event_delay:20}}");
            line(b + "_listener_not_compound", b, p, "{last_vibration_frequency:2,listener:\"x\"}");
            line(b + "_components", b, p, "{components:{\"minecraft:custom_name\":\"Sensor\"},last_vibration_frequency:2}");
        }
        for (String[] s : new String[][]{{"sculk_shrieker", "shrieking=true,can_summon=true,waterlogged=true"}}) {
            String b = s[0], p = s[1];
            line(b + "_empty", b, p, "{}");
            line(b + "_warning", b, p, "{warning_level:3}");
            line(b + "_warning_string", b, p, "{warning_level:\"3\"}");
            line(b + "_warning_double", b, p, "{warning_level:2.9d}");
            line(b + "_listener_min", b, p, "{warning_level:1,listener:{selector:{tick:-1L}}}");
            line(b + "_listener_event", b, p, "{warning_level:1,listener:{event:" + EVT_FULL + ",selector:{event:" + EVT + ",tick:7L},event_delay:9}}");
            line(b + "_listener_invalid", b, p, "{warning_level:1,listener:{selector:{tick:-1L},event_delay:-3}}");
            line(b + "_components", b, p, "{components:{\"minecraft:custom_data\":{x:1}},warning_level:4}");
        }

        // ---- creaking heart
        String ch = "creaking_heart", chp = "axis=x,natural=true";
        line("creaking_empty", ch, chp, "{}");
        line("creaking_uuid", ch, chp, "{creaking:[I;1,2,3,4]}");
        line("creaking_uuid_neg", ch, chp, "{creaking:[I;-1,-2,-2147483648,2147483647]}");
        line("creaking_uuid_short", ch, chp, "{creaking:[I;1,2,3]}");
        line("creaking_uuid_long", ch, chp, "{creaking:[I;1,2,3,4,5]}");
        line("creaking_uuid_string", ch, chp, "{creaking:\"00000001-0000-0002-0000-000300000004\"}");
        line("creaking_uuid_list", ch, chp, "{creaking:[1,2,3,4]}");
        line("creaking_extra", ch, chp, "{creaking:[I;9,8,7,6],foo:\"bar\",components:{\"minecraft:custom_data\":{a:1}}}");

        // ---- potent sulfur
        String ps = "potent_sulfur", psp = "potent_sulfur_state=dormant";
        line("sulfur_empty", ps, psp, "{}");
        line("sulfur_countdown", ps, psp, "{countdown:25}");
        line("sulfur_countdown_zero", ps, psp, "{countdown:0}");
        line("sulfur_countdown_double", ps, psp, "{countdown:5.7d}");
        line("sulfur_countdown_string", ps, psp, "{countdown:\"5\"}");
        line("sulfur_countdown_long", ps, psp, "{countdown:4294967297L}");

        // ---- end portal / gateway
        line("portal_empty", "end_portal", "", "{}");
        line("portal_extra", "end_portal", "", "{Age:5L,foo:1b,components:{\"minecraft:custom_data\":{a:1}}}");
        String g = "end_gateway";
        line("gateway_empty", g, "", "{}");
        line("gateway_age", g, "", "{Age:123L}");
        line("gateway_age_int", g, "", "{Age:123}");
        line("gateway_exit", g, "", "{Age:5L,exit_portal:[I;100,60,-100]}");
        line("gateway_exit_exact", g, "", "{exit_portal:[I;100,60,-100],ExactTeleport:1b}");
        line("gateway_exact_false", g, "", "{ExactTeleport:0b}");
        line("gateway_exact_int", g, "", "{ExactTeleport:2}");
        line("gateway_exact_string", g, "", "{ExactTeleport:\"true\"}");
        line("gateway_exit_out_of_bounds_y", g, "", "{exit_portal:[I;0,20000000,0]}");
        line("gateway_exit_in_bounds_y", g, "", "{exit_portal:[I;0,19999999,0],ExactTeleport:1b}");
        line("gateway_exit_out_of_bounds_x", g, "", "{exit_portal:[I;30000000,0,0]}");
        line("gateway_exit_negative_bounds", g, "", "{exit_portal:[I;-30000000,-20000000,-30000000]}");
        line("gateway_exit_negative_out", g, "", "{exit_portal:[I;-30000001,0,0]}");
        line("gateway_exit_short", g, "", "{exit_portal:[I;1,2]}");
        line("gateway_exit_list", g, "", "{exit_portal:[1,2,3]}");
        line("gateway_components", g, "", "{Age:1L,components:{\"minecraft:custom_data\":{a:1}}}");

        java.nio.file.Files.writeString(java.nio.file.Path.of(out), OUT.toString());
    }
}
