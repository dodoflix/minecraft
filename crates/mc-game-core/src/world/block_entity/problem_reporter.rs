//! `ProblemReporter.ScopedCollector` of a block entity load (`BlockDataAccessor.setData`) or of the entities of a chunk
//! (`EntityStorage`, `SerializableChunkData`): the problems of the field decodes that went through
//! [`super::read_codec`] while a scope is open are logged as `Serialization errors:` when it closes.
//!
//! shortcut: only `ValueInput.read(key, codec)` failures are reported, at the element that is being loaded (a block
//! entity, or an entity of a chunk's list; no child path elements for nested compounds); the type-mismatch problems of
//! the scalar getters are not, extend when a corpus needs them.

use std::cell::RefCell;

use mc_core::pos::BlockPos;
use mc_data::BlockEntityType;

/// The open `ProblemReporter.Collector`: the path of the reporter in use and the problems reported so far.
struct Collector {
    path: Vec<String>,
    problems: Vec<(Vec<String>, String)>,
}

thread_local! {
    static PROBLEMS: RefCell<Option<Collector>> = const { RefCell::new(None) };
}

/// `reporter.report(problem)` of the open scope; nothing without one.
pub(crate) fn report(description: impl FnOnce() -> String) {
    PROBLEMS.with(|collector| {
        if let Some(collector) = collector.borrow_mut().as_mut() {
            collector
                .problems
                .push((collector.path.clone(), description()));
        }
    });
}

/// `reporter.forChild(element)` for the duration of `f`: problems reported inside belong to the child path.
pub(crate) fn with_child<R>(element: impl FnOnce() -> String, f: impl FnOnce() -> R) -> R {
    let open = PROBLEMS.with(|collector| {
        collector
            .borrow_mut()
            .as_mut()
            .map(|collector| collector.path.push(element()))
            .is_some()
    });
    let result = f();
    if open {
        PROBLEMS.with(|collector| {
            if let Some(collector) = collector.borrow_mut().as_mut() {
                collector.path.pop();
            }
        });
    }
    result
}

/// One node of `ProblemReporter.Collector.ProblemTreeNode`: the problems of a path element and its child elements.
struct Node {
    element: String,
    problems: Vec<String>,
    children: Vec<Node>,
}

impl Node {
    fn new(element: &str) -> Self {
        Node {
            element: element.to_owned(),
            problems: Vec::new(),
            children: Vec::new(),
        }
    }

    fn child(&mut self, element: &str) -> &mut Node {
        let index = self
            .children
            .iter()
            .position(|c| c.element == element)
            .unwrap_or_else(|| {
                self.children.push(Node::new(element));
                self.children.len() - 1
            });
        &mut self.children[index]
    }

    /// `ProblemTreeNode.getLines`.
    fn lines(&self) -> Vec<String> {
        match (self.problems.len(), self.children.len()) {
            (0, 0) => Vec::new(),
            (0, 1) => {
                let mut lines = self.children[0].lines();
                lines[0] = format!("{}{}", self.element, lines[0]);
                lines
            }
            (1, 0) => vec![format!("{}: {}", self.element, self.problems[0])],
            _ => {
                let mut lines: Vec<String> = self
                    .children
                    .iter()
                    .flat_map(Node::lines)
                    .map(|l| format!("  {l}"))
                    .collect();
                lines.extend(self.problems.iter().map(|p| format!("  {p}")));
                lines.insert(0, format!("{}:", self.element));
                lines
            }
        }
    }
}

/// `ProblemReporter.Collector.getTreeReport`: the problems of `root`, grouped by their paths.
fn tree_report(root: &str, problems: Vec<(Vec<String>, String)>) -> String {
    let mut tree = Node::new(root);
    for (path, description) in problems {
        let mut node = &mut tree;
        for element in &path {
            node = node.child(element);
        }
        node.problems.push(description);
    }
    tree.lines().join("\n")
}

/// Runs `f` with a collector rooted at `root` open and logs its problems under `logger` when it ends
/// (`ProblemReporter.ScopedCollector.close`, `getTreeReport`).
pub fn with_scoped_collector<R>(logger: &str, root: &str, f: impl FnOnce() -> R) -> R {
    let outer = PROBLEMS.with(|collector| {
        collector.borrow_mut().replace(Collector {
            path: Vec::new(),
            problems: Vec::new(),
        })
    });
    let result = f();
    let collected =
        PROBLEMS.with(|collector| std::mem::replace(&mut *collector.borrow_mut(), outer));
    let problems = collected.map(|c| c.problems).unwrap_or_default();
    if !problems.is_empty() {
        log::warn!(
            "[{logger}] Serialization errors:\n{}",
            tree_report(root, problems)
        );
    }
    result
}

/// [`with_scoped_collector`] rooted at the block entity of type `ty` at `pos` (`BlockEntity.problemPath`).
pub fn with_block_entity_collector<R>(
    logger: &str,
    ty: BlockEntityType,
    pos: BlockPos,
    f: impl FnOnce() -> R,
) -> R {
    let root = format!(
        "{} // {}@BlockPos{{x={}, y={}, z={}}}",
        ty.name(),
        canonical_class_name(ty),
        pos.x(),
        pos.y(),
        pos.z()
    );
    with_scoped_collector(logger, &root, f)
}

/// `ChunkAccess.problemPath(pos)`.
pub fn chunk_root(pos: mc_core::pos::ChunkPos) -> String {
    format!("chunk@{pos}")
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

#[cfg(test)]
mod tests {
    use super::*;

    fn path(elements: &[&str]) -> Vec<String> {
        elements.iter().map(|e| (*e).to_owned()).collect()
    }

    #[test]
    fn one_problem_in_one_entity_is_one_line() {
        let report = tree_report(
            "chunk@[0, 1]",
            vec![(path(&[".Entities[3]"]), "bad".to_owned())],
        );
        assert_eq!(report, "chunk@[0, 1].Entities[3]: bad");
    }

    #[test]
    fn problems_of_several_entities_are_nested() {
        let report = tree_report(
            "chunk@[0, 1]",
            vec![
                (path(&[".Entities[3]"]), "a".to_owned()),
                (path(&[".Entities[3]", ".Passengers[0]"]), "b".to_owned()),
                (path(&[".Entities[5]"]), "c".to_owned()),
                (path(&[]), "root".to_owned()),
            ],
        );
        assert_eq!(
            report,
            "chunk@[0, 1]:\n  .Entities[3]:\n    .Passengers[0]: b\n    a\n  .Entities[5]: c\n  root"
        );
    }
}
