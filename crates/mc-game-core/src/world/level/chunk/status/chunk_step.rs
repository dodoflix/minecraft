//! `ChunkStep`: one status of a pyramid with its direct and accumulated dependencies.
use super::ChunkDependencies;
use mc_world::status::ChunkStatus;

/// `ChunkStatusTasks::*` selector (the function a step runs; implemented in `server::level::chunk_status_tasks`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ChunkStatusTask {
    PassThrough,
    GenerateStructureStarts,
    LoadStructureStarts,
    GenerateStructureReferences,
    GenerateBiomes,
    BuildTerrain,
    GenerateFeatures,
    InitializeLight,
    Light,
    GenerateSpawn,
    Full,
}

#[derive(Clone, Debug)]
pub struct ChunkStep {
    pub target_status: ChunkStatus,
    pub direct_dependencies: ChunkDependencies,
    pub accumulated_dependencies: ChunkDependencies,
    pub block_state_write_radius: i32,
    pub task: ChunkStatusTask,
}

impl ChunkStep {
    /// `getAccumulatedRadiusOf(status)`.
    pub fn get_accumulated_radius_of(&self, status: ChunkStatus) -> i32 {
        if status == self.target_status {
            0
        } else {
            self.accumulated_dependencies.get_radius_of(status)
        }
    }
}

/// `ChunkStep.Builder`.
pub(super) struct ChunkStepBuilder {
    status: ChunkStatus,
    parent: Option<ChunkStep>,
    direct_dependencies_by_radius: Vec<ChunkStatus>,
    block_state_write_radius: i32,
    task: ChunkStatusTask,
}

impl ChunkStepBuilder {
    pub(super) fn new(status: ChunkStatus, parent: Option<&ChunkStep>) -> Self {
        match parent {
            None => {
                assert!(
                    status.parent() == status,
                    "Not starting with the first status: {}",
                    status.name()
                );
                Self {
                    status,
                    parent: None,
                    direct_dependencies_by_radius: Vec::new(),
                    block_state_write_radius: -1,
                    task: ChunkStatusTask::PassThrough,
                }
            }
            Some(parent) => {
                assert!(
                    parent.target_status.index() == status.index() - 1,
                    "Out of order status: {}",
                    status.name()
                );
                Self {
                    status,
                    parent: Some(parent.clone()),
                    direct_dependencies_by_radius: vec![parent.target_status],
                    block_state_write_radius: -1,
                    task: ChunkStatusTask::PassThrough,
                }
            }
        }
    }

    pub(super) fn add_requirement(mut self, status: ChunkStatus, radius: i32) -> Self {
        assert!(
            !status.is_or_after(self.status),
            "Status {} can not be required by {}",
            status.name(),
            self.status.name()
        );
        let previous = std::mem::take(&mut self.direct_dependencies_by_radius);
        let new_length = radius as usize + 1;
        if new_length > previous.len() {
            self.direct_dependencies_by_radius = vec![status; new_length];
        } else {
            self.direct_dependencies_by_radius = previous.clone();
        }
        for (slot, prev) in self
            .direct_dependencies_by_radius
            .iter_mut()
            .zip(&previous)
            .take(new_length)
        {
            *slot = ChunkStatus::max(*prev, status);
        }
        self
    }

    pub(super) fn block_state_write_radius(mut self, radius: i32) -> Self {
        self.block_state_write_radius = radius;
        self
    }

    pub(super) fn set_task(mut self, task: ChunkStatusTask) -> Self {
        self.task = task;
        self
    }

    pub(super) fn build(self) -> ChunkStep {
        let accumulated = self.build_accumulated_dependencies();
        ChunkStep {
            target_status: self.status,
            direct_dependencies: ChunkDependencies::new(self.direct_dependencies_by_radius),
            accumulated_dependencies: ChunkDependencies::new(accumulated),
            block_state_write_radius: self.block_state_write_radius,
            task: self.task,
        }
    }

    fn get_radius_of_parent(&self, status: ChunkStatus) -> usize {
        for i in (0..self.direct_dependencies_by_radius.len()).rev() {
            if self.direct_dependencies_by_radius[i].is_or_after(status) {
                return i;
            }
        }
        0
    }

    fn build_accumulated_dependencies(&self) -> Vec<ChunkStatus> {
        let Some(parent) = &self.parent else {
            return self.direct_dependencies_by_radius.clone();
        };
        let radius_of_parent = self.get_radius_of_parent(parent.target_status);
        let parent_dependencies = &parent.accumulated_dependencies;
        let direct = &self.direct_dependencies_by_radius;
        let len = (radius_of_parent + parent_dependencies.size()).max(direct.len());
        (0..len)
            .map(|distance| {
                // `distanceInParent` may be negative in Java: wrap like the `< 0` check.
                let distance_in_parent = distance as isize - radius_of_parent as isize;
                if distance_in_parent < 0
                    || distance_in_parent as usize >= parent_dependencies.size()
                {
                    direct[distance]
                } else if distance >= direct.len() {
                    parent_dependencies.get(distance_in_parent as i32)
                } else {
                    ChunkStatus::max(
                        direct[distance],
                        parent_dependencies.get(distance_in_parent as i32),
                    )
                }
            })
            .collect()
    }
}
