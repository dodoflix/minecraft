//! Embedded data pack, tags and language file against the files on disk and vanilla's own tag resolution.

mod common;
use common::{read_json, repo_root};
use mc_data::block::Block;
use mc_data::item::Item;
use mc_data::{datapack, lang, registry, tags};
use std::path::Path;

fn walk(dir: &Path, rel: &str, out: &mut Vec<(String, std::path::PathBuf)>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let e = e.unwrap();
        let path = format!("{rel}/{}", e.file_name().to_str().unwrap());
        if e.file_type().unwrap().is_dir() {
            walk(&e.path(), &path, out);
        } else {
            out.push((path, e.path()));
        }
    }
}

#[test]
fn every_file_is_embedded_byte_exact() {
    let src = repo_root().join("ref/src");
    let mut files = Vec::new();
    walk(&src.join("data"), "data", &mut files);
    walk(&src.join("assets"), "assets", &mut files);
    assert!(files.len() > 9000);
    files.sort();
    // embedded = data + assets + version.json
    assert_eq!(datapack::paths().len(), files.len() + 1);
    for (path, disk) in &files {
        let want = std::fs::read(disk).unwrap();
        let got = datapack::read(path).unwrap_or_else(|| panic!("{path} missing"));
        assert!(got.as_ref() == want.as_slice(), "{path} differs");
    }
    assert!(datapack::exists("version.json"));
    assert!(datapack::read("data/minecraft/nope.json").is_none());
}

#[test]
fn listing() {
    let l: Vec<_> = datapack::list("data/minecraft/tags/block/mineable/").collect();
    assert!(l.contains(&"data/minecraft/tags/block/mineable/pickaxe.json"));
    assert!(l.iter().all(|p| p.starts_with("data/minecraft/tags/block/mineable/")));
    let loot = datapack::read_data("minecraft", "loot_table/blocks/stone.json").unwrap();
    assert!(String::from_utf8_lossy(&loot).contains("minecraft:stone"));
}

#[test]
fn lang_en_us() {
    let disk = read_json("ref/src/assets/minecraft/lang/en_us.json");
    assert_eq!(lang::en_us().len(), disk.as_object().unwrap().len());
    assert_eq!(lang::translate("block.minecraft.stone"), Some("Stone"));
    assert_eq!(lang::translate("no.such.key"), None);
    for (k, v) in disk.as_object().unwrap() {
        assert_eq!(lang::translate(k), v.as_str());
    }
}

/// Every tag of every built-in registry resolves to exactly what vanilla's TagLoader produced (ref/extracted/tags.json).
#[test]
fn tags_match_vanilla_tag_loader() {
    let vanilla = read_json("ref/extracted/tags.json");
    let mut n = 0;
    for (reg_name, tags_json) in vanilla.as_object().unwrap() {
        let reg = registry::by_name(reg_name).unwrap();
        for (tag, entries) in tags_json.as_object().unwrap() {
            let got = tags::names(reg.tag_dir(), tag).unwrap_or_else(|| panic!("{reg_name} {tag}"));
            let mut got: Vec<_> = got.into_iter().filter(|e| reg.id_of(e).is_some()).collect();
            got.sort();
            got.dedup();
            let want: Vec<String> = entries.as_array().unwrap().iter().map(|e| e.as_str().unwrap().to_string()).collect();
            assert_eq!(got, want, "{reg_name} {tag}");
            n += 1;
        }
    }
    assert!(n > 600, "{n}");
}

#[test]
fn typed_tags() {
    let pick = tags::get::<Block>("minecraft:mineable/pickaxe").unwrap();
    assert!(pick.contains(Block::STONE));
    assert!(!pick.contains(Block::OAK_PLANKS));
    assert!(Block::OAK_PLANKS.is_in_tag("planks"));
    assert!(tags::get::<Block>("no_such_tag").is_none());
    assert!(tags::get::<Item>("minecraft:logs").unwrap().contains(Item::OAK_LOG));
    // nested #references are resolved
    assert!(tags::get::<Block>("minecraft:logs").unwrap().contains(Block::OAK_LOG));
    assert!(tags::list("block").contains(&"minecraft:mineable/pickaxe".to_string()));
    // data-driven registries have tags too
    assert!(tags::names("worldgen/biome", "minecraft:is_ocean").unwrap().contains(&"minecraft:ocean".to_string()));
}
