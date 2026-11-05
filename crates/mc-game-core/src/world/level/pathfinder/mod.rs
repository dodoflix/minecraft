//! `net.minecraft.world.level.pathfinder`: node evaluators, the A* [`PathFinder`], [`Path`]/[`Node`]/[`PathType`].
//!
//! # Conventions
//! * Vanilla's object graph is flattened: nodes live in the evaluator's pool and are referred to by
//!   [`NodeId`]; a [`Path`] is a shared handle (clone = alias, [`Path::copy`] = vanilla `copy()`), so brain memories
//!   and navigations see the same progress.
//! * Evaluators are driven through the [`NodeEvaluator`] trait; the mob and the level are not fields but travel in a
//!   [`PathEnv`]. The mob side is the narrow [`PathfindingMob`] view ([`EntityMob`] adapts a live entity).
//! * Evaluator "classes" (walk/swim/amphibious/fly) are tags on [`EvaluatorState`]; a mob-defined subclass (frog,
//!   creaking) embeds an `EvaluatorState`, implements `NodeEvaluator` and overrides what vanilla overrides; `super`
//!   is the matching `node_evaluator::default_*` function.

pub mod amphibious_node_evaluator;
pub mod binary_heap;
pub mod fly_node_evaluator;
pub mod mob_view;
pub mod node;
pub mod node_evaluator;
pub mod path;
pub mod path_finder;
pub mod path_type;
pub mod path_type_cache;
pub mod pathfinding_context;
pub mod swim_node_evaluator;
pub mod walk_node_evaluator;

pub use mob_view::{EntityMob, PathfindingMob};
pub use node::{Node, NodeId, Target};
pub use node_evaluator::{EvaluatorClass, EvaluatorState, NodeEvaluator, PathEnv};
pub use path::Path;
pub use path_finder::PathFinder;
pub use path_type::{PathType, PathfindingMalus};
pub use path_type_cache::PathTypeCache;
pub use pathfinding_context::{PathLevel, PathfindingContext};

/// `PathComputationType`.
pub use mc_data::block::PathComputationType;
