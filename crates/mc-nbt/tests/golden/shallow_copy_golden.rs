//! `CompoundTag.shallowCopy()` key order after further puts (`fixtures/ShallowCopyOracle.java`): the copy's table size is
//! `ceil(n / 0.75)` rounded up to a power of two on JDK 19+.

use mc_nbt::{CompoundTag, nbt_io};

#[test]
fn shallow_copy_presizes_like_the_jdk() {
    let mut checked = 0;
    for line in include_str!("../fixtures/shallow_copy_golden.tsv").lines() {
        let (n, want) = line.split_once('\t').unwrap();
        let n: i32 = n.parse().unwrap();
        let c: CompoundTag = (0..n).map(|i| (format!("k{i}"), i)).collect();
        let mut d = c.shallow_copy();
        for i in 0..7 {
            d.put_int(format!("z{i}"), i);
        }
        let got: String = nbt_io::to_bytes(&d)
            .iter()
            .map(|x| format!("{x:02x}"))
            .collect();
        assert_eq!(got, want, "n = {n}");
        checked += 1;
    }
    assert_eq!(checked, 60);
}
