//! LZ4Block framing: lz4-java must decode the frames we write (`fixtures/region/Lz4Tool.java`, run through
//! `scripts/vanilla-java.sh`).

use crate::region_common::*;
use crate::region_oracle::run_java;
use mc_world::region::region_file_version::VERSION_LZ4;

const NAMES: [&str; 6] = ["empty", "one", "short", "repeat", "random", "mixed"];

#[test]
#[ignore = "oracle: runs scripts/vanilla-java.sh"]
fn lz4_java_decodes_our_frames() {
    let dir = fixtures().join("lz4");
    let tmp = tempfile::tempdir().unwrap();
    for n in NAMES {
        let bin = std::fs::read(dir.join(format!("{n}.bin"))).unwrap();
        std::fs::write(
            tmp.path().join(format!("{n}.rs.lz4")),
            VERSION_LZ4.compress(&bin).unwrap(),
        )
        .unwrap();
        std::fs::write(tmp.path().join(format!("{n}.bin")), &bin).unwrap();
    }
    let out = run_java("Lz4Tool.java", &["check", tmp.path().to_str().unwrap()]);
    assert_eq!(
        out.lines().filter(|l| l.starts_with("OK ")).count(),
        NAMES.len(),
        "{out}"
    );
}
