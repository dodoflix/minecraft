//! `net.minecraft.stats.Stat` / `StatType`: a stat is a stat type plus a value of that type's registry.
use mc_core::ident::Identifier;
use mc_data::{Block, CustomStat, EntityType, Item, StatType};
pub use mc_protocol::packets::play::Stat;

/// The `StatType` (registry `minecraft:stat_type`) of `stat`.
pub fn stat_type(stat: &Stat) -> StatType {
    StatType::by_name(stat_type_name(stat)).expect("vanilla stat type")
}

/// Registry key of `stat`'s `StatType` (`BuiltInRegistries.STAT_TYPE.getKey(type)`).
pub fn stat_type_name(stat: &Stat) -> &'static str {
    match stat {
        Stat::Mined(_) => "minecraft:mined",
        Stat::Crafted(_) => "minecraft:crafted",
        Stat::Used(_) => "minecraft:used",
        Stat::Broken(_) => "minecraft:broken",
        Stat::PickedUp(_) => "minecraft:picked_up",
        Stat::Dropped(_) => "minecraft:dropped",
        Stat::Killed(_) => "minecraft:killed",
        Stat::KilledBy(_) => "minecraft:killed_by",
        Stat::Custom(_) => "minecraft:custom",
    }
}

/// Registry key of `stat`'s value in the type's registry (`type.getRegistry().getKey(value)`).
pub fn value_name(stat: &Stat) -> &'static str {
    match stat {
        Stat::Mined(b) => b.name(),
        Stat::Crafted(i)
        | Stat::Used(i)
        | Stat::Broken(i)
        | Stat::PickedUp(i)
        | Stat::Dropped(i) => i.name(),
        Stat::Killed(e) | Stat::KilledBy(e) => e.name(),
        Stat::Custom(c) => c.name(),
    }
}

/// `Stat.buildName(type, value)`: `<type>:<value>` with `:` replaced by `.` in both ids (`minecraft.mined:minecraft.stone`).
pub fn build_name(stat: &Stat) -> String {
    format!(
        "{}:{}",
        stat_type_name(stat).replace(':', "."),
        value_name(stat).replace(':', ".")
    )
}

/// A stat from the ids of its type and its value; `Err` is the codec error text (`Unknown registry key in <registry>: <id>`).
pub fn by_names(type_id: &Identifier, value_id: &Identifier) -> Result<Stat, String> {
    let type_key = type_id.to_string();
    let value_key = value_id.to_string();
    let unknown = |registry: &str| format!("Unknown registry key in {registry}: {value_key}");
    let stat = match type_key.as_str() {
        "minecraft:mined" => Stat::Mined(
            Block::by_name(&value_key)
                .ok_or_else(|| unknown("ResourceKey[minecraft:root / minecraft:block]"))?,
        ),
        "minecraft:crafted" => Stat::Crafted(item(&value_key)?),
        "minecraft:used" => Stat::Used(item(&value_key)?),
        "minecraft:broken" => Stat::Broken(item(&value_key)?),
        "minecraft:picked_up" => Stat::PickedUp(item(&value_key)?),
        "minecraft:dropped" => Stat::Dropped(item(&value_key)?),
        "minecraft:killed" => Stat::Killed(entity_type(&value_key)?),
        "minecraft:killed_by" => Stat::KilledBy(entity_type(&value_key)?),
        "minecraft:custom" => Stat::Custom(
            CustomStat::by_name(&value_key)
                .ok_or_else(|| unknown("ResourceKey[minecraft:root / minecraft:custom_stat]"))?,
        ),
        _ => {
            return Err(format!(
                "Unknown registry key in ResourceKey[minecraft:root / minecraft:stat_type]: {type_key}"
            ));
        }
    };
    Ok(stat)
}

fn item(key: &str) -> Result<Item, String> {
    Item::by_name(key).ok_or_else(|| {
        format!("Unknown registry key in ResourceKey[minecraft:root / minecraft:item]: {key}")
    })
}

fn entity_type(key: &str) -> Result<EntityType, String> {
    EntityType::by_name(key).ok_or_else(|| {
        format!(
            "Unknown registry key in ResourceKey[minecraft:root / minecraft:entity_type]: {key}"
        )
    })
}

/// `ObjectiveCriteria.byName` resolver for `<stat_type>:<value>` criteria: both ids must exist.
pub fn criteria_exists(type_id: &Identifier, value_id: &Identifier) -> bool {
    by_names(type_id, value_id).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        let stat = by_names(
            &Identifier::parse("minecraft:mined").unwrap(),
            &Identifier::parse("minecraft:stone").unwrap(),
        )
        .unwrap();
        assert_eq!(build_name(&stat), "minecraft.mined:minecraft.stone");
        assert_eq!(value_name(&stat), "minecraft:stone");
        assert!(
            by_names(
                &Identifier::parse("minecraft:mined").unwrap(),
                &Identifier::parse("minecraft:nope").unwrap()
            )
            .is_err()
        );
    }
}
