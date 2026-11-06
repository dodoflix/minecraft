//! `ChunkDependencies`: for each chebyshev distance from the center chunk, the status a chunk must have.
use mc_world::status::ChunkStatus;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChunkDependencies {
    dependency_by_radius: Vec<ChunkStatus>,
    radius_by_dependency: Vec<i32>,
}

impl ChunkDependencies {
    pub fn new(dependency_by_radius: Vec<ChunkStatus>) -> Self {
        let size = dependency_by_radius
            .first()
            .map_or(0, |s| s.index() as usize + 1);
        let mut radius_by_dependency = vec![0; size];
        for (radius, dependency) in dependency_by_radius.iter().enumerate() {
            let index = dependency.index() as usize;
            for slot in radius_by_dependency.iter_mut().take(index + 1) {
                *slot = radius as i32;
            }
        }
        Self {
            dependency_by_radius,
            radius_by_dependency,
        }
    }

    pub fn as_list(&self) -> &[ChunkStatus] {
        &self.dependency_by_radius
    }

    pub fn size(&self) -> usize {
        self.dependency_by_radius.len()
    }

    /// `getRadiusOf(status)`: panics (vanilla: `IllegalArgumentException`) outside the dependency range.
    pub fn get_radius_of(&self, status: ChunkStatus) -> i32 {
        let index = status.index() as usize;
        assert!(
            index < self.radius_by_dependency.len(),
            "Requesting a ChunkStatus({}) outside of dependency range({:?})",
            status.name(),
            self.dependency_by_radius
        );
        self.radius_by_dependency[index]
    }

    pub fn get_radius(&self) -> i32 {
        (self.dependency_by_radius.len() as i32 - 1).max(0)
    }

    pub fn get(&self, distance: i32) -> ChunkStatus {
        self.dependency_by_radius[distance as usize]
    }
}
