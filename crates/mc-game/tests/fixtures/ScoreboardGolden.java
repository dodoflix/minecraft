import com.mojang.serialization.*;
import net.minecraft.ChatFormatting;
import net.minecraft.nbt.*;
import net.minecraft.network.chat.*;
import net.minecraft.network.chat.numbers.*;
import net.minecraft.world.scores.*;
import net.minecraft.world.scores.criteria.*;
import java.nio.file.*;
import java.util.*;

/**
 * Usage: scripts/vanilla-java.sh crates/mc-game/tests/fixtures/ScoreboardGolden.java crates/mc-game/tests/fixtures/scoreboard_in.snbt crates/mc-game/tests/fixtures/scoreboard.golden crates/mc-game/tests/fixtures/scoreboard_roundtrip.golden
 * Output: crates/mc-game/tests/fixtures/scoreboard.golden and scoreboard_roundtrip.golden (two files, so the program opens them itself: the `-o` wrapper atomicity covers one file)
 * 1. builds a rich Scoreboard with the real classes and writes `ScoreboardSaveData.Packed` as SNBT,
 * 2. decodes `scoreboard_in.snbt` (produced by our encoder) with the real codec, loads it into a Scoreboard,
 *    stores it again and writes the result.
 */
public class ScoreboardGolden {
  static class Board extends Scoreboard {
    ScoreboardSaveData.Packed store() { return new ScoreboardSaveData.Packed(packObjectives(), packPlayerScores(), packDisplaySlots(), packPlayerTeams()); }
    void load(ScoreboardSaveData.Packed d) {
      d.objectives().forEach(this::loadObjective);
      d.scores().forEach(this::loadPlayerScore);
      d.displaySlots().forEach((slot, name) -> setDisplayObjective(slot, getObjective(name)));
      d.teams().forEach(this::loadPlayerTeam);
    }
  }
  static String enc(Board b) {
    return ScoreboardSaveData.Packed.CODEC.encodeStart(NbtOps.INSTANCE, b.store()).getOrThrow().toString();
  }
  public static void main(String[] a) throws Exception {
    net.minecraft.SharedConstants.tryDetectVersion();
    net.minecraft.server.Bootstrap.bootStrap();
    Board b = new Board();
    Objective dummy = b.addObjective("kills", ObjectiveCriteria.DUMMY, Component.literal("Kills"), ObjectiveCriteria.RenderType.INTEGER, false, null);
    Objective hp = b.addObjective("hp", ObjectiveCriteria.HEALTH, Component.literal("HP").withStyle(ChatFormatting.RED), ObjectiveCriteria.RenderType.HEARTS, true, BlankFormat.INSTANCE);
    Objective trig = b.addObjective("trig", ObjectiveCriteria.TRIGGER, Component.translatable("some.key", "arg"), ObjectiveCriteria.RenderType.INTEGER, true, new FixedFormat(Component.literal("fixed")));
    Objective tk = b.addObjective("tk", ObjectiveCriteria.TEAM_KILL.get(TeamColor.RED), Component.literal("TK"), ObjectiveCriteria.RenderType.INTEGER, false, new StyledFormat(Style.EMPTY.withColor(ChatFormatting.GOLD).withBold(true)));
    ScoreAccess s = b.getOrCreatePlayerScore(ScoreHolder.forNameOnly("Alice"), dummy);
    s.set(5);
    s = b.getOrCreatePlayerScore(ScoreHolder.forNameOnly("Bob"), dummy);
    s.set(-3); s.unlock(); s.display(Component.literal("Bobby")); s.numberFormatOverride(new FixedFormat(Component.literal("x")));
    s = b.getOrCreatePlayerScore(ScoreHolder.forNameOnly("Alice"), trig);
    s.set(0); s.unlock();
    s = b.getOrCreatePlayerScore(ScoreHolder.forNameOnly("#hidden"), tk, true);
    s.set(2147483647);
    b.setDisplayObjective(DisplaySlot.SIDEBAR, dummy);
    b.setDisplayObjective(DisplaySlot.LIST, hp);
    b.setDisplayObjective(DisplaySlot.TEAM_RED, tk);
    PlayerTeam t = b.addPlayerTeam("red_team");
    t.setDisplayName(Component.literal("Reds"));
    t.setColor(Optional.of(TeamColor.RED));
    t.setAllowFriendlyFire(false);
    t.setSeeFriendlyInvisibles(false);
    t.setPlayerPrefix(Component.literal("[R] ").withStyle(ChatFormatting.BOLD));
    t.setPlayerSuffix(Component.literal("!"));
    t.setNameTagVisibility(Team.Visibility.HIDE_FOR_OTHER_TEAMS);
    t.setDeathMessageVisibility(Team.Visibility.NEVER);
    t.setCollisionRule(Team.CollisionRule.PUSH_OWN_TEAM);
    b.addPlayerToTeam("Alice", t);
    b.addPlayerToTeam("Bob", t);
    PlayerTeam plain = b.addPlayerTeam("plain");
    b.addPlayerToTeam("Carol", plain);
    Files.writeString(Path.of(a[1]), enc(b) + "\n");
    // reverse direction
    Board c = new Board();
    Tag in = TagParser.parseCompoundFully(Files.readString(Path.of(a[0])).trim());
    ScoreboardSaveData.Packed p = ScoreboardSaveData.Packed.CODEC.parse(NbtOps.INSTANCE, in).getOrThrow();
    c.load(p);
    Files.writeString(Path.of(a[2]), enc(c) + "\n");
  }
}
