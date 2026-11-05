//! `levelgen.feature.{TreeFeature, FallenTreeFeature}` and the tree sub-registries (`trunkplacers`, `foliageplacers`,
//! `rootplacers`, `treedecorators`, `featuresize`). One file per vanilla class; every placer/decorator/size type registers
//! itself in its table from its own file.

pub mod java_hash_set;

use mc_core::random::RandomSource;


/// `Util.shuffle(list, random)`.
pub fn shuffle<T>(list: &mut [T], random: &mut dyn RandomSource) {
    let mut i = list.len();
    while i > 1 {
        let swap_to = random.next_int_bounded(i as i32) as usize;
        list.swap(i - 1, swap_to);
        i -= 1;
    }
}
