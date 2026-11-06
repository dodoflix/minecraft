//! The scoreboard that `scoreboard_golden` and the oracle target's `scoreboard_fixture` both encode.

use mc_game::world::scores::score::for_name_only;
use mc_game::world::scores::{
    CollisionRule, DisplaySlot, NumberFormat, ObjectiveCriteria, RenderType, Scoreboard, TeamColor,
    Visibility,
};
use mc_text::{ChatFormatting, Component, Style, TranslatableArg};

pub fn build() -> Scoreboard {
    let mut b = Scoreboard::new();
    b.add_objective(
        "kills",
        ObjectiveCriteria::DUMMY,
        Component::literal("Kills"),
        RenderType::Integer,
        false,
        None,
    )
    .unwrap();
    b.add_objective(
        "hp",
        ObjectiveCriteria::HEALTH,
        Component::literal("HP").with_format(ChatFormatting::Red),
        RenderType::Hearts,
        true,
        Some(NumberFormat::Blank),
    )
    .unwrap();
    b.add_objective(
        "trig",
        ObjectiveCriteria::TRIGGER,
        Component::translatable("some.key", vec![TranslatableArg::String("arg".into())]),
        RenderType::Integer,
        true,
        Some(NumberFormat::Fixed(Component::literal("fixed"))),
    )
    .unwrap();
    b.add_objective(
        "tk",
        ObjectiveCriteria::team_kill(TeamColor::Red),
        Component::literal("TK"),
        RenderType::Integer,
        false,
        Some(NumberFormat::Styled(
            Style::EMPTY
                .with_color_formatting(Some(ChatFormatting::Gold))
                .with_bold(Some(true)),
        )),
    )
    .unwrap();
    let alice = for_name_only("Alice");
    let bob = for_name_only("Bob");
    b.get_or_create_player_score(&alice, "kills")
        .unwrap()
        .set(5)
        .unwrap();
    {
        let mut s = b.get_or_create_player_score(&bob, "kills").unwrap();
        s.set(-3).unwrap();
        s.unlock();
        s.set_display(Some(Component::literal("Bobby")));
        s.number_format_override(Some(NumberFormat::Fixed(Component::literal("x"))));
    }
    {
        let mut s = b.get_or_create_player_score(&alice, "trig").unwrap();
        s.set(0).unwrap();
        s.unlock();
    }
    b.get_or_create_player_score_forced(&for_name_only("#hidden"), "tk", true)
        .unwrap()
        .set(i32::MAX)
        .unwrap();
    b.set_display_objective(DisplaySlot::Sidebar, Some("kills"));
    b.set_display_objective(DisplaySlot::List, Some("hp"));
    b.set_display_objective(DisplaySlot::TeamRed, Some("tk"));
    b.add_player_team("red_team");
    {
        let mut t = b.team_mut("red_team").unwrap();
        t.set_display_name(Component::literal("Reds"));
        t.set_color(Some(TeamColor::Red));
        t.set_allow_friendly_fire(false);
        t.set_see_friendly_invisibles(false);
        t.set_player_prefix(Some(
            Component::literal("[R] ").with_format(ChatFormatting::Bold),
        ));
        t.set_player_suffix(Some(Component::literal("!")));
        t.set_name_tag_visibility(Visibility::HideForOtherTeams);
        t.set_death_message_visibility(Visibility::Never);
        t.set_collision_rule(CollisionRule::PushOwnTeam);
    }
    b.add_player_to_team("Alice", "red_team");
    b.add_player_to_team("Bob", "red_team");
    b.add_player_team("plain");
    b.add_player_to_team("Carol", "plain");
    b
}
