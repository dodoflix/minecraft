//! `DataLayer` vs. the real vanilla class replaying random op sequences (`tests/fixtures/data_layer/DataLayerGolden.java`).

use mc_world::data_layer::{DataLayer, InvalidSize, SIZE};

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap())
        .collect()
}

fn state(d: &DataLayer) -> String {
    [
        d.is_definitely_homogenous(),
        d.is_empty(),
        d.is_definitely_filled_with(0),
        d.is_definitely_filled_with(15),
        d.is_definitely_filled_with(7),
    ]
    .iter()
    .map(|&b| if b { '1' } else { '0' })
    .collect()
}

#[test]
fn matches_vanilla_data_layer() {
    let golden = include_str!("../fixtures/data_layer/golden.txt");
    let mut layer = DataLayer::new();
    let mut last_filled = DataLayer::new();
    let mut in_fill = false;
    let (mut ops, mut scenarios) = (0, 0);
    for line in golden.lines() {
        let (kind, rest) = line.split_once(' ').unwrap();
        let f: Vec<&str> = rest.split(' ').collect();
        let n = |i: usize| f[i].parse::<i32>().unwrap();
        match kind {
            "SIZE" => {
                let len: usize = f[0].parse().unwrap();
                match DataLayer::from_bytes(vec![0; len]) {
                    Ok(_) => assert_eq!(rest, format!("{len} ok")),
                    Err(e) => assert_eq!(rest, format!("{len} {e}")),
                }
            }
            "FILL" => {
                in_fill = true;
                last_filled = DataLayer::filled(n(0));
                assert_eq!(format!("{} get={}", n(0), last_filled.get(3, 4, 5)), rest);
            }
            "FILLDATA" => {
                let data = *last_filled.get_data();
                let distinct: std::collections::HashSet<u8> = data.iter().copied().collect();
                assert_eq!(
                    format!("{} {} {} {}", n(0), data[0], data[SIZE - 1], distinct.len()),
                    rest
                );
            }
            "SCN" => {
                in_fill = false;
                scenarios += 1;
                layer = match f[0] {
                    "empty" => DataLayer::new(),
                    "filled15" => DataLayer::filled(15),
                    "filled3" => DataLayer::filled(3),
                    _ => DataLayer::new(), // replaced by INIT
                };
            }
            "INIT" => layer = DataLayer::from_bytes(unhex(f[0])).unwrap(),
            "S" => {
                ops += 1;
                layer.set(n(0), n(1), n(2), n(3));
            }
            "G" => {
                ops += 1;
                assert_eq!(layer.get(n(0), n(1), n(2)), n(3), "op {ops}");
            }
            "F" => {
                ops += 1;
                layer.fill(n(0));
            }
            "C" => {
                ops += 1;
                let c = layer.copy();
                layer.set(n(0), n(1), n(2), 7);
                assert_eq!(
                    (c.get(n(0), n(1), n(2)), layer.get(n(0), n(1), n(2))),
                    (n(3), n(4)),
                    "copy independence"
                );
                layer = c;
            }
            "COPYDATA" => assert_eq!(
                format!(
                    "{}{}",
                    u8::from(layer.is_definitely_homogenous()),
                    u8::from(layer.is_empty())
                ),
                f.join("")
            ),
            "STATE" => assert_eq!(state(if in_fill { &last_filled } else { &layer }), f[0]),
            "LAYER" => assert_eq!(layer.layer_to_string(3).replace('\n', "|"), rest),
            "STR" => assert_eq!(layer.to_string().replace('\n', "|"), rest),
            "DATA" => assert_eq!(hex(layer.get_data()), f[0]),
            other => panic!("bad line {other}"),
        }
    }
    assert_eq!(scenarios, 4);
    assert!(ops > 2000);
}

#[test]
fn borrowed_data_view_matches_materialization() {
    let filled = DataLayer::filled(15);
    assert_eq!(&filled.data()[..2], &[0xff, 0xff]);
    assert!(filled.is_definitely_filled_with(15)); // `data()` does not materialize
    let mut m = DataLayer::filled(9);
    let expected = *m.get_data();
    assert_eq!(*DataLayer::filled(9).data(), expected);
    assert_eq!(
        DataLayer::from_bytes(vec![1; 10]).unwrap_err(),
        InvalidSize(10)
    );
}
