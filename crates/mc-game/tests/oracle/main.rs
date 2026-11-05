//! Oracle tests (T2): start the vanilla server and regenerate recorded fixtures; every test is `#[ignore]`d
//! (`cargo test -p mc-game --test oracle -- --ignored`).

#[path = "../golden/common/test_packs.rs"]
mod test_packs;

mod vanilla_oracle;
