//! `ProblemReporter.ScopedCollector` of a block entity load (`BlockDataAccessor.setData`): the problems of the field
//! decodes that went through [`super::read_codec`] while a scope is open are logged as `Serialization errors:` when it
//! closes.
//!
//! shortcut: only `ValueInput.read(key, codec)` failures are reported, at the root of the block entity (no child path
//! elements for nested reads); the type-mismatch problems of the scalar getters are not, extend when a corpus needs them.

use std::cell::RefCell;

use mc_core::pos::BlockPos;
use mc_data::BlockEntityType;

thread_local! {
    static PROBLEMS: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
}

/// `reporter.report(problem)` of the open scope; nothing without one.
pub(super) fn report(description: impl FnOnce() -> String) {
    PROBLEMS.with(|problems| {
        if let Some(problems) = problems.borrow_mut().as_mut() {
            problems.push(description());
        }
    });
}

/// Runs `f` with a collector open and logs its problems (`ScopedCollector.close`) under `logger`, rooted at the
/// block entity of type `ty` at `pos` (`BlockEntity.problemPath`).
pub fn with_scoped_collector<R>(
    logger: &str,
    ty: BlockEntityType,
    pos: BlockPos,
    f: impl FnOnce() -> R,
) -> R {
    let outer = PROBLEMS.with(|problems| problems.borrow_mut().replace(Vec::new()));
    let result = f();
    let problems = PROBLEMS
        .with(|problems| std::mem::replace(&mut *problems.borrow_mut(), outer))
        .unwrap_or_default();
    if !problems.is_empty() {
        let root = format!(
            "{} // {}@BlockPos{{x={}, y={}, z={}}}",
            ty.name(),
            canonical_class_name(ty),
            pos.x(),
            pos.y(),
            pos.z()
        );
        let report = match problems.as_slice() {
            [only] => format!("{root}: {only}"),
            _ => {
                let lines: String = problems.iter().map(|p| format!("\n  {p}")).collect();
                format!("{root}:{lines}")
            }
        };
        log::warn!("[{logger}] Serialization errors:\n{report}");
    }
    result
}

/// `getClass().getCanonicalName()` of the vanilla class behind a block entity type.
fn canonical_class_name(ty: BlockEntityType) -> String {
    const PKG: &str = "net.minecraft.world.level.block.entity";
    let name = ty.name().strip_prefix("minecraft:").unwrap_or(ty.name());
    let class = match name {
        "piston" => {
            return "net.minecraft.world.level.block.piston.PistonMovingBlockEntity".to_owned();
        }
        "vault" => return format!("{PKG}.vault.VaultBlockEntity"),
        "mob_spawner" => "SpawnerBlockEntity",
        "end_portal" => "TheEndPortalBlockEntity",
        "end_gateway" => "TheEndGatewayBlockEntity",
        "chiseled_bookshelf" => "ChiseledBookShelfBlockEntity",
        "brushable_block" => "BrushableBlockEntity",
        _ => "",
    };
    if !class.is_empty() {
        return format!("{PKG}.{class}");
    }
    // `structure_block` is `StructureBlockEntity`, not `StructureBlockBlockEntity`
    let camel: String = name
        .strip_suffix("_block")
        .unwrap_or(name)
        .split('_')
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_ascii_uppercase().to_string() + chars.as_str()
            })
        })
        .collect();
    format!("{PKG}.{camel}BlockEntity")
}
