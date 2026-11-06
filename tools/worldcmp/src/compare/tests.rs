use mc_nbt::{CompoundTag, ListTag, Tag};

use super::*;

fn pack(values: &[u32], bits: usize) -> Vec<i64> {
    let per = 64 / bits;
    let mut out = vec![0i64; values.len().div_ceil(per)];
    for (i, v) in values.iter().enumerate() {
        out[i / per] |= (*v as i64) << ((i % per) * bits);
    }
    out
}

fn state(name: &str, props: &[(&str, &str)]) -> Tag {
    if props.is_empty() {
        return Tag::String(name.into());
    }
    let mut c = CompoundTag::new();
    c.put_string("id", name);
    let mut p = CompoundTag::new();
    for (k, v) in props {
        p.put_string(*k, *v);
    }
    c.put("properties", Tag::Compound(p));
    Tag::Compound(c)
}

/// `block_states` for 4096 entries from `(position, palette index)` overrides on index 0; `pad` extra unused palette
/// entries force a wider bit width without changing the content.
fn blocks(palette: Vec<Tag>, set: &[(usize, u32)], pad: usize) -> CompoundTag {
    let mut idx = vec![0u32; 4096];
    for (i, v) in set {
        idx[*i] = *v;
    }
    let mut palette = palette;
    for n in 0..pad {
        palette.push(state(&format!("minecraft:pad{n}"), &[]));
    }
    let bits = 4.max(ceil_log2(palette.len()));
    let mut c = CompoundTag::new();
    if palette.len() > 1 {
        c.put_long_array("data", pack(&idx, bits));
    }
    c.put("palette", Tag::List(palette.into_iter().collect()));
    c
}

fn biomes(palette: &[&str], idx: &[u32]) -> CompoundTag {
    let mut c = CompoundTag::new();
    if palette.len() > 1 {
        c.put_long_array("data", pack(idx, ceil_log2(palette.len()).max(1)));
    }
    c.put(
        "palette",
        Tag::List(palette.iter().map(|s| Tag::String((*s).into())).collect()),
    );
    c
}

fn section(y: i8, b: CompoundTag, bio: CompoundTag) -> Tag {
    let mut s = CompoundTag::new();
    s.put_byte("Y", y);
    s.put("block_states", Tag::Compound(b));
    s.put("biomes", Tag::Compound(bio));
    s.put_byte_array("SkyLight", vec![1; 2048]);
    Tag::Compound(s)
}

fn chunk(sections: Vec<Tag>) -> CompoundTag {
    let mut c = CompoundTag::new();
    c.put_string("Status", "minecraft:full");
    c.put_long("LastUpdate", 5);
    c.put_long("InhabitedTime", 7);
    c.put_int("DataVersion", 5023);
    c.put("sections", Tag::List(sections.into_iter().collect()));
    c
}

fn stone_chunk(set: &[(usize, u32)], pad: usize) -> CompoundTag {
    chunk(vec![section(
        0,
        blocks(
            vec![
                state("minecraft:air", &[]),
                state("minecraft:stone", &[]),
                state("minecraft:dirt", &[]),
            ],
            set,
            pad,
        ),
        biomes(&["minecraft:plains"], &[0; 64]),
    )])
}

fn cmp(a: &CompoundTag, b: &CompoundTag) -> ChunkDiff {
    compare_chunk(a, b, 10)
}

#[test]
fn identical_ignoring_light_and_times() {
    let a = stone_chunk(&[(5, 1)], 0);
    let mut b = stone_chunk(&[(5, 1)], 0);
    b.put_long("LastUpdate", 99);
    b.put_long("InhabitedTime", 1234);
    assert!(cmp(&a, &b).is_empty());
}

#[test]
fn blocks_are_palette_independent() {
    // same content, different palette order and a wider bit width (palette padded to 20 entries -> 5 bits)
    let a = stone_chunk(&[(5, 1), (4095, 2)], 0);
    let b = chunk(vec![section(
        0,
        blocks(
            vec![
                state("minecraft:dirt", &[]),
                state("minecraft:air", &[]),
                state("minecraft:stone", &[]),
            ],
            &[],
            17,
        )
        .tap(|c| {
            // rebuild: index 1 = air everywhere except pos 5 = stone (2), pos 4095 = dirt (0)
            let mut idx = vec![1u32; 4096];
            idx[5] = 2;
            idx[4095] = 0;
            c.put_long_array("data", pack(&idx, 5));
        }),
        biomes(&["minecraft:plains"], &[0; 64]),
    )]);
    let d = cmp(&a, &b);
    assert!(d.is_empty(), "{:?}", d.cats);
}

trait Tap: Sized {
    fn tap(mut self, f: impl FnOnce(&mut Self)) -> Self {
        f(&mut self);
        self
    }
}
impl Tap for CompoundTag {}

#[test]
fn block_difference_position_and_histogram() {
    let a = stone_chunk(&[(0x123, 1), (0x124, 1)], 0);
    let b = stone_chunk(&[(0x123, 2)], 0);
    let d = cmp(&a, &b);
    let blocks = &d.cats[&Category::Blocks];
    assert_eq!(blocks.count, 2);
    // 0x123 = y 1, z 2, x 3
    assert!(
        blocks.samples[0].contains("local (3,1,2)"),
        "{:?}",
        blocks.samples
    );
    assert!(blocks.samples[0].contains("minecraft:stone (A) != minecraft:dirt (B)"));
    assert_eq!(d.block_hist["minecraft:stone -> minecraft:dirt"], 1);
    assert_eq!(d.block_hist["minecraft:stone -> minecraft:air"], 1);
}

#[test]
fn block_state_properties_compare_and_missing_section_is_air() {
    let slab = |t: &str| {
        chunk(vec![section(
            2,
            blocks(
                vec![
                    state("minecraft:air", &[]),
                    state(
                        "minecraft:oak_slab",
                        &[("type", t), ("waterlogged", "false")],
                    ),
                ],
                &[(9, 1)],
                0,
            ),
            biomes(&["minecraft:plains"], &[0; 64]),
        )])
    };
    let d = cmp(&slab("top"), &slab("bottom"));
    assert_eq!(d.block_hist["minecraft:oak_slab [state differs]"], 1);
    // all-air section vs omitted section: equal
    let air = chunk(vec![section(
        3,
        blocks(vec![state("minecraft:air", &[])], &[], 0),
        biomes(&["minecraft:plains"], &[0; 64]),
    )]);
    assert!(cmp(&air, &chunk(vec![])).is_empty());
    // omitted section vs section with blocks: differs
    assert_eq!(
        cmp(&slab("top"), &chunk(vec![])).cats[&Category::Blocks].count,
        1
    );
}

#[test]
fn biome_difference() {
    let mk = |idx: &[u32]| {
        chunk(vec![section(
            0,
            blocks(vec![state("minecraft:air", &[])], &[], 0),
            biomes(&["minecraft:plains", "minecraft:desert"], idx),
        )])
    };
    let mut i = [0u32; 64];
    let a = mk(&i);
    i[21] = 1; // x=1, z=1, y=1 (index = y*16 + z*4 + x)
    let d = cmp(&a, &mk(&i));
    let b = &d.cats[&Category::Biomes];
    assert_eq!(b.count, 1);
    assert!(
        b.samples[0].contains("cell (1,1,1): minecraft:plains (A) != minecraft:desert (B)"),
        "{:?}",
        b.samples
    );
}

#[test]
fn heightmaps() {
    let hm = |v: &[u32]| {
        let mut h = CompoundTag::new();
        h.put_long_array("WORLD_SURFACE", pack(v, 9));
        h
    };
    let mut a = stone_chunk(&[], 0);
    let mut b = a.clone();
    let mut v = vec![70u32; 256];
    a.put("Heightmaps", Tag::Compound(hm(&v)));
    b.put("Heightmaps", Tag::Compound(hm(&v)));
    assert!(cmp(&a, &b).is_empty());
    v[17] = 71; // x=1, z=1
    b.put("Heightmaps", Tag::Compound(hm(&v)));
    let d = cmp(&a, &b);
    assert!(
        d.cats[&Category::Heightmaps].samples[0]
            .contains("WORLD_SURFACE column (1,1): 70 (A) != 71 (B)")
    );
    b.put("Heightmaps", Tag::Compound(CompoundTag::new()));
    assert!(
        cmp(&a, &b).cats[&Category::Heightmaps].samples[0].contains("WORLD_SURFACE: only in A")
    );
}

fn be(id: &str, x: i32, y: i32, z: i32, extra: &str) -> Tag {
    let mut c = CompoundTag::new();
    c.put_string("id", id);
    c.put_int("x", x);
    c.put_int("y", y);
    c.put_int("z", z);
    c.put_string("extra", extra);
    Tag::Compound(c)
}

#[test]
fn block_entities_order_insensitive_and_diffed() {
    let mut a = stone_chunk(&[], 0);
    let mut b = a.clone();
    a.put(
        "block_entities",
        Tag::List(
            vec![
                be("minecraft:chest", 1, 2, 3, "a"),
                be("minecraft:barrel", 4, 5, 6, "b"),
            ]
            .into(),
        ),
    );
    b.put(
        "block_entities",
        Tag::List(
            vec![
                be("minecraft:barrel", 4, 5, 6, "b"),
                be("minecraft:chest", 1, 2, 3, "a"),
            ]
            .into(),
        ),
    );
    assert!(cmp(&a, &b).is_empty());
    b.put(
        "block_entities",
        Tag::List(
            vec![
                be("minecraft:barrel", 4, 5, 6, "CHANGED"),
                be("minecraft:sign", 9, 9, 9, ""),
            ]
            .into(),
        ),
    );
    let d = cmp(&a, &b);
    let s = &d.cats[&Category::BlockEntities].samples;
    assert_eq!(d.cats[&Category::BlockEntities].count, 3, "{s:?}");
    assert!(
        s.iter()
            .any(|m| m.contains("minecraft:barrel @ (4,5,6).extra: \"b\" (A) != \"CHANGED\" (B)")),
        "{s:?}"
    );
    assert!(
        s.iter()
            .any(|m| m.contains("minecraft:chest @ (1,2,3): only in A"))
    );
    assert!(
        s.iter()
            .any(|m| m.contains("minecraft:sign @ (9,9,9): only in B"))
    );
}

fn start(id_children: &[(&str, i32)], gen_depth: i32) -> Tag {
    let mut s = CompoundTag::new();
    s.put_string("id", "minecraft:village");
    s.put_int("references", 1);
    s.put_int("ChunkX", 0);
    s.put_int("ChunkZ", 0);
    let mut kids = ListTag::new();
    for (id, depth) in id_children {
        let mut k = CompoundTag::new();
        k.put_string("id", *id);
        k.put_int("GD", gen_depth + depth);
        k.put_int("O", 2);
        k.put_int_array("BB", vec![0, 60, 0, 10, 70, 10]);
        kids.push(Tag::Compound(k));
    }
    s.put("Children", Tag::List(kids));
    Tag::Compound(s)
}

fn with_structures(starts: &[(&str, Tag)], refs: &[(&str, Vec<i64>)]) -> CompoundTag {
    let mut c = stone_chunk(&[], 0);
    let mut st = CompoundTag::new();
    let mut s = CompoundTag::new();
    for (k, v) in starts {
        s.put(*k, v.clone());
    }
    st.put("starts", Tag::Compound(s));
    let mut r = CompoundTag::new();
    for (k, v) in refs {
        r.put_long_array(*k, v.clone());
    }
    st.put("References", Tag::Compound(r));
    c.put("structures", Tag::Compound(st));
    c
}

#[test]
fn structure_starts_and_references() {
    let village = start(&[("minecraft:vl", 0), ("minecraft:vh", 1)], 0);
    let a = with_structures(
        &[("minecraft:village_plains", village.clone())],
        &[("minecraft:village_plains", vec![1, 2, 3])],
    );
    // references: same set, different order -> equal
    let b = with_structures(
        &[("minecraft:village_plains", village.clone())],
        &[("minecraft:village_plains", vec![3, 1, 2])],
    );
    assert!(cmp(&a, &b).is_empty());
    // start missing in B
    let b = with_structures(&[], &[("minecraft:village_plains", vec![1, 2, 3])]);
    let d = cmp(&a, &b);
    assert_eq!(
        (
            d.starts["minecraft:village_plains"].only_a,
            d.starts["minecraft:village_plains"].only_b
        ),
        (1, 0)
    );
    // piece depth differs deep inside
    let b = with_structures(
        &[(
            "minecraft:village_plains",
            start(&[("minecraft:vl", 0), ("minecraft:vh", 2)], 0),
        )],
        &[("minecraft:village_plains", vec![1, 2, 3])],
    );
    let d = cmp(&a, &b);
    assert_eq!(d.starts["minecraft:village_plains"].differs, 1);
    assert!(
        d.cats[&Category::StructureStarts].samples[0].contains("Children[1].GD: 1 (A) != 2 (B)"),
        "{:?}",
        d.cats[&Category::StructureStarts].samples
    );
    // references differ: chunk keys decoded
    let key = (7i64 & 0xFFFF_FFFF) | (-3i64 << 32);
    let b = with_structures(
        &[("minecraft:village_plains", village)],
        &[("minecraft:village_plains", vec![1, 2, 3, key])],
    );
    let d = cmp(&a, &b);
    assert!(
        d.cats[&Category::StructureReferences].samples[0].contains("only in B: [7,-3]"),
        "{:?}",
        d.cats
    );
}

#[test]
fn ticks_post_processing_status_light_other() {
    let mut a = stone_chunk(&[], 0);
    let mut b = a.clone();
    let tick = |x: i32, t: i32| {
        let mut c = CompoundTag::new();
        c.put_string("i", "minecraft:water");
        c.put_int("x", x);
        c.put_int("t", t);
        Tag::Compound(c)
    };
    a.put(
        "fluid_ticks",
        Tag::List(vec![tick(1, 5), tick(2, 5)].into()),
    );
    b.put(
        "fluid_ticks",
        Tag::List(vec![tick(2, 5), tick(1, 5)].into()),
    );
    assert!(cmp(&a, &b).is_empty());
    b.put(
        "fluid_ticks",
        Tag::List(vec![tick(2, 5), tick(1, 6)].into()),
    );
    assert_eq!(cmp(&a, &b).cats[&Category::Ticks].count, 2);

    let pp =
        |v: &[i16]| Tag::List(vec![Tag::List(v.iter().map(|s| Tag::Short(*s)).collect())].into());
    a.put("PostProcessing", pp(&[1, 2]));
    b.put(
        "fluid_ticks",
        Tag::List(vec![tick(1, 5), tick(2, 5)].into()),
    );
    b.put("PostProcessing", pp(&[1, 3]));
    let d = cmp(&a, &b);
    assert_eq!(
        d.cats[&Category::PostProcessing].samples,
        vec!["PostProcessing[0][1]: 2s (A) != 3s (B)"]
    );

    b.put("PostProcessing", pp(&[1, 2]));
    b.put_string("Status", "minecraft:features");
    b.put_boolean("isLightOn", true);
    b.put_int("DataVersion", 1);
    let d = cmp(&a, &b);
    assert_eq!(
        d.cats.keys().copied().collect::<Vec<_>>(),
        vec![Category::Status, Category::IsLightOn, Category::Other]
    );
}

#[test]
fn entities_ignore_uuid_and_match_by_position() {
    let ent = |id: &str, x: f64, uuid: i32, hp: f32| {
        let mut c = CompoundTag::new();
        c.put_string("id", id);
        c.put(
            "Pos",
            Tag::List(vec![Tag::Double(x), Tag::Double(64.0), Tag::Double(2.5)].into()),
        );
        c.put_int_array("UUID", vec![uuid, 0, 0, 0]);
        c.put_float("Health", hp);
        c.put_int("EggLayTime", uuid * 7);
        c
    };
    let wrap = |e: Vec<CompoundTag>| {
        let mut c = CompoundTag::new();
        c.put(
            "Entities",
            Tag::List(e.into_iter().map(Tag::Compound).collect()),
        );
        c
    };
    let mut d = ChunkDiff::new(10);
    compare_entity_chunks(
        Some(&wrap(vec![ent("minecraft:cow", 1.0, 1, 10.0)])),
        Some(&wrap(vec![ent("minecraft:cow", 1.0, 99, 10.0)])),
        &mut d,
    );
    assert!(d.is_empty());
    compare_entity_chunks(
        Some(&wrap(vec![ent("minecraft:cow", 1.0, 1, 10.0)])),
        Some(&wrap(vec![ent("minecraft:cow", 1.0, 1, 9.0)])),
        &mut d,
    );
    assert!(d.cats[&Category::Entities].samples[0].contains("Health: 10.0f (A) != 9.0f (B)"));
    let mut d = ChunkDiff::new(10);
    compare_entity_chunks(
        Some(&wrap(vec![ent("minecraft:cow", 1.0, 1, 10.0)])),
        None,
        &mut d,
    );
    assert!(d.cats[&Category::Entities].samples[0].contains("only in A"));
}

#[test]
fn sample_cap_does_not_cap_count() {
    let set: Vec<(usize, u32)> = (0..100).map(|i| (i, 1)).collect();
    let d = compare_chunk(&stone_chunk(&set, 0), &stone_chunk(&[], 0), 3);
    assert_eq!(
        (
            d.cats[&Category::Blocks].count,
            d.cats[&Category::Blocks].samples.len()
        ),
        (100, 3)
    );
}

#[test]
fn undecodable_section_is_reported() {
    let mut bad = stone_chunk(&[(1, 1)], 0);
    if let Some(Tag::List(l)) = bad.get_mut("sections")
        && let Some(Tag::Compound(s)) = l.iter_mut().next()
        && let Some(Tag::Compound(b)) = s.get_mut("block_states")
    {
        b.put_long_array("data", vec![0; 3]);
    }
    let d = cmp(&stone_chunk(&[(1, 1)], 0), &bad);
    assert!(d.cats[&Category::Blocks].samples[0].contains("(B): undecodable block_states"));
}
