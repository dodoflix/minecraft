//! Play `entity` packet group against vanilla's `GameProtocols` bytes (generator: fixtures/play/GenEntity.java).
mod common;
use common::*;
use mc_core::ident::Identifier;
use mc_core::phys::vec3::Vec3;
use mc_data::{Attribute, Item, MobEffect};
use mc_protocol::components::{ItemStack, DAMAGE, MAX_DAMAGE, MAX_STACK_SIZE, UNBREAKABLE};
use mc_protocol::{Decode, Packet};
use mc_protocol::codec::*;
use mc_protocol::packets::play::*;

const IDS: [i32; 15] = [
    1,
    2,
    127,
    128,
    255,
    16383,
    16384,
    2097151,
    2097152,
    268435455,
    268435456,
    i32::MAX,
    -1,
    -128,
    i32::MIN,
];
const DOUBLES: [f64; 11] = [
    0.0,
    -0.0,
    1.0,
    -1.5,
    0.1,
    1e-300,
    1e300,
    f64::MAX,
    f64::from_bits(1),
    f64::NEG_INFINITY,
    123456.789012345,
];
const FLOATS: [f32; 10] = [
    0.0,
    -0.0,
    1.0,
    -90.5,
    180.0,
    360.25,
    0.1,
    f32::MAX,
    f32::from_bits(1),
    f32::INFINITY,
];

fn v(i: usize) -> Vec3 {
    Vec3::new(
        DOUBLES[i % 11],
        DOUBLES[(i * 3 + 1) % 11],
        DOUBLES[(i * 7 + 2) % 11],
    )
}

/// What a `ROTATION_BYTE` field holds after a round trip.
fn angle(deg: f32) -> Angle {
    Angle(Angle::unpack(Angle::pack(deg)))
}

/// What an `LpVec3` field holds after a round trip (the encoding is lossy).
fn lp(x: f64, y: f64, z: f64) -> LpVec3 {
    from_bytes(&to_bytes(&LpVec3(Vec3::new(x, y, z)))).unwrap()
}

fn id(s: &str) -> Identifier {
    Identifier::parse(s).unwrap()
}

fn simple_id_packets(g: &Golden) {
    for (i, &e) in IDS.iter().enumerate() {
        let i32_ = i as i32;
        g.check(
            &format!("animate/{i}"),
            &ClientboundAnimatePacket {
                id: e,
                action: (i % 3) as u8,
            },
        );
        g.check(
            &format!("hurt_animation/{i}"),
            &ClientboundHurtAnimationPacket {
                id: e,
                yaw: FLOATS[i % 10],
            },
        );
        g.check(
            &format!("entity_event/{i}"),
            &ClientboundEntityEventPacket {
                entity_id: e,
                event_id: (i32_ * 17 - 128) as i8,
            },
        );
        g.check(
            &format!("rotate_head/{i}"),
            &ClientboundRotateHeadPacket {
                entity_id: e,
                y_head_rot: (i32_ * 19 - 128) as i8,
            },
        );
        g.check(
            &format!("set_camera/{i}"),
            &ClientboundSetCameraPacket { camera_id: e },
        );
        g.check(
            &format!("take_item_entity/{i}"),
            &ClientboundTakeItemEntityPacket {
                item_id: e,
                player_id: IDS[(i + 4) % 15],
                amount: IDS[(i + 9) % 15],
            },
        );
        g.check(
            &format!("mount_screen_open/{i}"),
            &ClientboundMountScreenOpenPacket {
                container_id: e,
                inventory_columns: IDS[(i + 3) % 15],
                entity_id: IDS[(i + 6) % 15],
            },
        );
        g.check(
            &format!("projectile_power/{i}"),
            &ClientboundProjectilePowerPacket {
                id: e,
                acceleration_power: DOUBLES[i % 11],
            },
        );
    }
    g.check(
        "animate/id0",
        &ClientboundAnimatePacket { id: 0, action: 255 },
    );
    g.check(
        "animate/action255",
        &ClientboundAnimatePacket { id: 5, action: 255 },
    );
    g.check(
        "entity_event/id0",
        &ClientboundEntityEventPacket {
            entity_id: 0,
            event_id: 127,
        },
    );
    g.check(
        "rotate_head/id0",
        &ClientboundRotateHeadPacket {
            entity_id: 0,
            y_head_rot: -1,
        },
    );
    g.check(
        "set_camera/id0",
        &ClientboundSetCameraPacket { camera_id: 0 },
    );
    for (i, &e) in IDS.iter().enumerate() {
        let dest = if i % 3 == 0 { 0 } else { IDS[(i + 5) % 15] };
        g.check(
            &format!("set_entity_link/{i}"),
            &ClientboundSetEntityLinkPacket {
                source_id: e,
                dest_id: dest,
            },
        );
    }
    g.check(
        "set_entity_link/zero_source",
        &ClientboundSetEntityLinkPacket {
            source_id: 0,
            dest_id: -1,
        },
    );
}

fn swing_and_damage(g: &Golden) {
    let mut sw = 0;
    for hand in [InteractionHand::MainHand, InteractionHand::OffHand] {
        for kind in [
            SwingAnimationType::None,
            SwingAnimationType::Whack,
            SwingAnimationType::Stab,
        ] {
            for duration in [0, 6, 300, -1] {
                g.check(
                    &format!("swing_animation/{sw}"),
                    &ClientboundSwingAnimationPacket {
                        entity_id: 1000 + sw + 1,
                        hand,
                        animation: SwingAnimation { kind, duration },
                    },
                );
                sw += 1;
            }
        }
    }
    g.check(
        "swing_animation/id0",
        &ClientboundSwingAnimationPacket {
            entity_id: 0,
            hand: InteractionHand::OffHand,
            animation: SwingAnimation {
                kind: SwingAnimationType::Stab,
                duration: i32::MAX,
            },
        },
    );
    for (i, kind) in [
        SwingAnimationType::None,
        SwingAnimationType::Whack,
        SwingAnimationType::Stab,
    ]
    .into_iter()
    .enumerate()
    {
        g.check_value(&format!("raw/swing_animation_type/{i}"), &kind);
    }
    // unknown ids are `none` (ByIdMap ZERO)
    assert_eq!(
        from_bytes::<SwingAnimationType>(&[3]).unwrap(),
        SwingAnimationType::None
    );
    assert_eq!(
        from_bytes::<SwingAnimationType>(&[0xff, 0xff, 0xff, 0xff, 0x0f]).unwrap(),
        SwingAnimationType::None
    );

    let mut n = 0;
    for type_id in [0, 1, 15, 31] {
        for ids in [
            [1, -1, -1],
            [300, 5, -1],
            [-5, -1, 7],
            [i32::MAX, 0, 0],
            [16384, 16383, 16385],
        ] {
            let p = |source_position| ClientboundDamageEventPacket {
                entity_id: ids[0],
                source_type: type_id,
                source_cause_id: ids[1],
                source_direct_id: ids[2],
                source_position,
            };
            g.check(&format!("damage_event/{n}"), &p(None));
            n += 1;
            g.check(&format!("damage_event/{n}"), &p(Some(v(n + 1))));
            n += 1;
        }
    }
    // -1 (no entity) travels as VarInt 0
    let mut b = to_bytes(&ClientboundDamageEventPacket {
        entity_id: 1,
        source_type: 0,
        source_cause_id: -1,
        source_direct_id: -1,
        source_position: None,
    });
    assert_eq!(b, [1, 0, 0, 0, 0]);
    b[3] = 0x80; // truncated VarInt
    assert!(from_bytes::<ClientboundDamageEventPacket>(&b).is_err());
}

fn move_entity(g: &Golden) {
    let deltas: [[i16; 3]; 5] = [
        [0, 0, 0],
        [1, -1, 2],
        [32767, -32768, 255],
        [-300, 4096, -4096],
        [128, 0, -129],
    ];
    let mut n = 0usize;
    for ground in [false, true] {
        for d in deltas {
            let delta = VecDelta::Linear {
                xa: d[0],
                ya: d[1],
                za: d[2],
            };
            let e = IDS[n % 15];
            g.check(
                &format!("move_entity_pos/{n}"),
                &ClientboundMoveEntityPosPacket {
                    entity_id: e,
                    delta: delta.clone(),
                    on_ground: ground,
                },
            );
            g.check(
                &format!("move_entity_pos_rot/{n}"),
                &ClientboundMoveEntityPosRotPacket {
                    entity_id: e,
                    delta,
                    y_rot: (n as i32 * 31) as i8,
                    x_rot: (-(n as i32) * 29) as i8,
                    on_ground: ground,
                },
            );
            n += 1;
        }
    }
    for steps in [1i32, 2, 3, 10, 63, 64, 200] {
        let list: Vec<VecDeltaStep> = (0..steps)
            .map(|i| VecDeltaStep {
                xa: (i * 7 - 100) as i16,
                ya: (-i * 11) as i16,
                za: (i * 300) as i16,
                ticks: i * i * 5,
            })
            .collect();
        let delta = VecDelta::Stepped(list);
        g.check(
            &format!("move_entity_pos/stepped{steps}"),
            &ClientboundMoveEntityPosPacket {
                entity_id: 42,
                delta: delta.clone(),
                on_ground: steps % 2 == 0,
            },
        );
        g.check(
            &format!("move_entity_pos_rot/stepped{steps}"),
            &ClientboundMoveEntityPosRotPacket {
                entity_id: -42,
                delta,
                y_rot: -128,
                x_rot: 127,
                on_ground: steps % 2 == 1,
            },
        );
    }
    // An empty Stepped writes no steps, so a reader sees a Linear: encode-only comparison.
    let empty = ClientboundMoveEntityPosPacket {
        entity_id: 9,
        delta: VecDelta::Stepped(vec![]),
        on_ground: true,
    };
    let mut ours = Vec::new();
    write_varint(&mut ours, ClientboundMoveEntityPosPacket::ID);
    mc_protocol::Encode::encode(&empty, &mut ours);
    assert_eq!(hex(&ours), hex(g.bytes("move_entity_pos/stepped_empty")));
    let mut n = 0;
    for ground in [false, true] {
        for (i, &e) in IDS.iter().enumerate() {
            let i = i as i32;
            g.check(
                &format!("move_entity_rot/{n}"),
                &ClientboundMoveEntityRotPacket {
                    entity_id: e,
                    on_ground: ground,
                    y_rot: (i * 23 - 128) as i8,
                    x_rot: (127 - i * 17) as i8,
                },
            );
            n += 1;
        }
    }

    // vanilla's size check: step count vs readable bytes / 7
    let mut bad = vec![1, 10];
    bad.extend([0u8; 10]);
    assert_eq!(
        ClientboundMoveEntityPosPacket::decode(&mut &bad[..]),
        Err(DecodeError::msg(
            "VecDelta with size 5 is bigger than allowed 1"
        ))
    );
    // properties = -1 as VarInt: step count 2^31-1
    let huge = [1, 0xff, 0xff, 0xff, 0xff, 0x0f, 0, 0, 0, 0, 0, 0, 0];
    assert_eq!(
        ClientboundMoveEntityPosRotPacket::decode(&mut &huge[..]),
        Err(DecodeError::msg(
            "VecDelta with size 2147483647 is bigger than allowed 1"
        ))
    );
}

fn minecart_and_position_sync(g: &Golden) {
    g.check(
        "move_minecart/empty",
        &ClientboundMoveMinecartPacket {
            entity_id: 7,
            lerp_steps: vec![],
        },
    );
    let zero = MinecartStep {
        position: Vec3::ZERO,
        movement: Vec3::ZERO,
        y_rot: Angle(0.0),
        x_rot: Angle(0.0),
        weight: 0.0,
    };
    g.check(
        "move_minecart/zero",
        &ClientboundMoveMinecartPacket {
            entity_id: i32::MAX,
            lerp_steps: vec![zero],
        },
    );
    for count in [1usize, 2, 5, 130] {
        let steps = (0..count)
            .map(|i| MinecartStep {
                position: v(i),
                movement: v(i + 5),
                y_rot: angle((i as i32 * 11 - 90) as f32),
                x_rot: angle((i as i32 * 3 % 180) as f32),
                weight: FLOATS[i % 10],
            })
            .collect();
        g.check(
            &format!("move_minecart/{count}"),
            &ClientboundMoveMinecartPacket {
                entity_id: count as i32 * 1000,
                lerp_steps: steps,
            },
        );
    }
    g.check_value(
        "raw/minecart_step",
        &MinecartStep {
            position: v(3),
            movement: v(4),
            y_rot: angle(45.0),
            x_rot: angle(-45.0),
            weight: 0.5,
        },
    );

    let mut n = 0;
    for ground in [false, true] {
        for i in 0..4 {
            g.check(
                &format!("entity_position_sync/linear{n}"),
                &ClientboundEntityPositionSyncPacket {
                    id: IDS[i * 3],
                    position: PositionPath::Linear(v(i)),
                    y_rot: FLOATS[i],
                    x_rot: FLOATS[i + 4],
                    on_ground: ground,
                },
            );
            n += 1;
        }
    }
    for count in [1usize, 2, 3, 40] {
        let steps: Vec<PositionStep> = (0..count)
            .map(|i| PositionStep {
                position: v(i + 2),
                tick_offset: (i * i * 100) as i32 - 3,
            })
            .collect();
        let path = PositionPath::Stepped(steps);
        g.check(
            &format!("entity_position_sync/stepped{count}"),
            &ClientboundEntityPositionSyncPacket {
                id: count as i32,
                position: path.clone(),
                y_rot: 12.5,
                x_rot: -3.5,
                on_ground: count % 2 == 0,
            },
        );
        g.check_value(&format!("raw/position_path/stepped{count}"), &path);
    }
    g.check_value("raw/position_path/linear", &PositionPath::Linear(v(1)));

    // unknown path type ids are Linear (ByIdMap ZERO); a Stepped path needs at least one step
    let mut linear = vec![9];
    linear.extend(to_bytes(&v(1)));
    assert_eq!(
        from_bytes::<PositionPath>(&linear).unwrap(),
        PositionPath::Linear(v(1))
    );
    assert!(from_bytes::<PositionPath>(&[1, 0]).is_err());
}

fn relative_sets() -> Vec<Relatives> {
    use Relative::*;
    let mut sets = vec![Relatives::NONE];
    sets.extend(
        [X, Y, Z, YRot, XRot, DeltaX, DeltaY, DeltaZ, RotateDelta].map(|r| Relatives::NONE.with(r)),
    );
    sets.extend([Relatives::ALL, Relatives::ROTATION, Relatives::DELTA]);
    sets.push([X, DeltaZ, YRot].into_iter().collect());
    sets
}

fn teleport_and_player_position(g: &Golden) {
    for (n, relatives) in relative_sets().into_iter().enumerate() {
        let change = PositionMoveRotation {
            position: v(n),
            delta_movement: v(n + 3),
            y_rot: FLOATS[n % 10],
            x_rot: FLOATS[(n + 2) % 10],
        };
        g.check(
            &format!("teleport_entity/{n}"),
            &ClientboundTeleportEntityPacket {
                id: IDS[n % 15],
                change,
                relatives,
                on_ground: n % 2 == 0,
            },
        );
        g.check(
            &format!("player_position/{n}"),
            &ClientboundPlayerPositionPacket {
                id: IDS[(n + 1) % 15],
                change,
                relatives,
            },
        );
    }
    for i in 0..4 {
        g.check(
            &format!("player_rotation/{i}"),
            &ClientboundPlayerRotationPacket {
                y_rot: FLOATS[i + 2],
                relative_y: i & 1 != 0,
                x_rot: FLOATS[i + 3],
                relative_x: i & 2 != 0,
            },
        );
    }
    for i in 0..4 {
        g.check(
            &format!("move_vehicle/{i}"),
            &ClientboundMoveVehiclePacket {
                moving_to: PositionAndRotation {
                    position: v(i),
                    y_rot: FLOATS[i],
                    x_rot: FLOATS[i + 3],
                },
            },
        );
    }
}

fn motion_passengers_remove(g: &Golden) {
    let lps = [
        [0.0, 0.0, 0.0],
        [1e-6, 0.0, 0.0],
        [0.5, -0.25, 0.125],
        [1.0, 1.0, 1.0],
        [-1.0, 2.0, -3.0],
        [3.0, 3.0, 3.0],
        [4.0, 0.0, 0.0],
        [100.5, -2000.25, 7.0],
        [1.7179869183E10, -1.7179869183E10, 0.0],
        [1e12, 0.0, -1e12],
        [f64::NAN, 5.0, 0.0],
        [0.0784, -0.0784, 0.0],
        [-0.0, 3.0517578125E-5, 0.0],
    ];
    for (i, l) in lps.iter().enumerate() {
        g.check(
            &format!("set_entity_motion/{i}"),
            &ClientboundSetEntityMotionPacket {
                id: IDS[i % 15],
                movement: lp(l[0], l[1], l[2]),
            },
        );
    }
    for count in [0usize, 1, 2, 5, 50] {
        let ids: Vec<i32> = (0..count).map(|i| IDS[i % 15]).collect();
        g.check(
            &format!("set_passengers/{count}"),
            &ClientboundSetPassengersPacket {
                vehicle: 100 + count as i32,
                passengers: ids.clone(),
            },
        );
        g.check(
            &format!("remove_entities/{count}"),
            &ClientboundRemoveEntitiesPacket { entity_ids: ids },
        );
    }
    g.check(
        "remove_entities/big",
        &ClientboundRemoveEntitiesPacket {
            entity_ids: (0..1000).map(|i| i * 997 - 500).collect(),
        },
    );

    // readVarIntArray() allows at most the readable bytes (counted before the count itself)
    assert_eq!(
        ClientboundSetPassengersPacket::decode(&mut &[1u8, 5, 1, 2][..]),
        Err(DecodeError::msg(
            "VarIntArray with size 5 is bigger than allowed 3"
        ))
    );
    assert_eq!(
        ClientboundSetPassengersPacket::decode(&mut &[1u8, 3, 1, 2, 3][..]).unwrap(),
        ClientboundSetPassengersPacket {
            vehicle: 1,
            passengers: vec![1, 2, 3]
        }
    );
    // the collection codec of remove_entities only fails on missing elements
    assert!(matches!(
        ClientboundRemoveEntitiesPacket::decode(&mut &[3u8, 1][..]),
        Err(DecodeError::Eof { .. })
    ));
}

fn mob_effects(g: &Golden) {
    let effect_ids = [0u32, 1, 5, 20, 38];
    for mask in 0..16usize {
        let effect = MobEffect::from_id(effect_ids[mask % 5]).unwrap();
        let entity_id = IDS[mask % 15];
        let amp = if mask == 15 { 255 } else { mask as i32 * 7 };
        let dur = match mask {
            14 => -1,
            13 => i32::MAX,
            _ => mask as i32 * 100,
        };
        let p = ClientboundUpdateMobEffectPacket {
            entity_id,
            effect,
            effect_amplifier: amp,
            effect_duration_ticks: dur,
            flags: mask as u8,
        };
        g.check(&format!("update_mob_effect/{mask}"), &p);
        assert_eq!(
            (
                p.is_ambient(),
                p.is_visible(),
                p.shows_icon(),
                p.should_blend()
            ),
            (mask & 1 != 0, mask & 2 != 0, mask & 4 != 0, mask & 8 != 0)
        );
        g.check(
            &format!("remove_mob_effect/{mask}"),
            &ClientboundRemoveMobEffectPacket { entity_id, effect },
        );
    }
    g.check(
        "update_mob_effect/neg_amp",
        &ClientboundUpdateMobEffectPacket {
            entity_id: 3,
            effect: MobEffect::SPEED,
            // vanilla's MobEffectInstance clamps the -5 we asked for to 0
            effect_amplifier: 0,
            effect_duration_ticks: 20,
            flags: 6,
        },
    );
    // the wire field itself is a plain VarInt
    let neg = ClientboundUpdateMobEffectPacket { entity_id: 3, effect: MobEffect::SPEED, effect_amplifier: -5, effect_duration_ticks: 20, flags: 6 };
    assert_eq!(from_bytes::<ClientboundUpdateMobEffectPacket>(&to_bytes(&neg)).unwrap(), neg);
    // unknown flag bits are carried through (vanilla stores the raw byte)
    let p = ClientboundUpdateMobEffectPacket {
        entity_id: 1,
        effect: MobEffect::SPEED,
        effect_amplifier: 0,
        effect_duration_ticks: 0,
        flags: 0xf3,
    };
    assert_eq!(
        from_bytes::<ClientboundUpdateMobEffectPacket>(&to_bytes(&p)).unwrap(),
        p
    );
    assert_eq!(
        ClientboundRemoveMobEffectPacket::decode(&mut &[1u8, 0xff, 0x7f][..]),
        Err(DecodeError::msg("No value with id 16383"))
    );
}

fn attributes(g: &Golden) {
    let attrs: Vec<Attribute> = [0u32, 3, 7, 12]
        .into_iter()
        .map(|i| Attribute::from_id(i).unwrap())
        .chain([Attribute::MAX_HEALTH, Attribute::MOVEMENT_SPEED])
        .collect();
    let ops = [
        AttributeModifierOperation::AddValue,
        AttributeModifierOperation::AddMultipliedBase,
        AttributeModifierOperation::AddMultipliedTotal,
    ];
    g.check(
        "update_attributes/empty",
        &ClientboundUpdateAttributesPacket {
            entity_id: 5,
            attributes: vec![],
        },
    );
    for count in [1usize, 2, 3, 6] {
        let attributes = (0..count)
            .map(|i| AttributeSnapshot {
                attribute: attrs[i % 6],
                base: DOUBLES[i % 11],
                modifiers: (0..i)
                    .map(|m| AttributeModifier {
                        id: id(&if m % 2 == 0 { "minecraft:effect.speed".to_string() } else { format!("mymod:mod/{m}") }),
                        amount: DOUBLES[(m + i) % 11],
                        operation: ops[m % 3],
                    })
                    .collect(),
            })
            .collect();
        g.check(
            &format!("update_attributes/{count}"),
            &ClientboundUpdateAttributesPacket {
                entity_id: count as i32 * 10,
                attributes,
            },
        );
    }
    for (i, op) in ops.into_iter().enumerate() {
        g.check_value(
            &format!("raw/attribute_modifier/{i}"),
            &AttributeModifier {
                id: id("a:b/c"),
                amount: -0.5,
                operation: op,
            },
        );
    }
    let max: Vec<AttributeSnapshot> = (0..128)
        .map(|i| AttributeSnapshot {
            attribute: attrs[i % 6],
            base: i as f64,
            modifiers: vec![],
        })
        .collect();
    g.check(
        "update_attributes/128",
        &ClientboundUpdateAttributesPacket {
            entity_id: 1,
            attributes: max.clone(),
        },
    );
    let mut over = max;
    over.push(AttributeSnapshot {
        attribute: attrs[0],
        base: 0.0,
        modifiers: vec![],
    });
    assert_eq!(
        from_bytes::<ClientboundUpdateAttributesPacket>(&to_bytes(
            &ClientboundUpdateAttributesPacket {
                entity_id: 1,
                attributes: over
            }
        )),
        Err(DecodeError::msg("129 elements exceeded max size of: 128"))
    );
    // operation ids out of range are ADD_VALUE
    assert_eq!(
        from_bytes::<AttributeModifierOperation>(&[3]).unwrap(),
        AttributeModifierOperation::AddValue
    );
    assert_eq!(
        from_bytes::<AttributeModifierOperation>(&[0xfd, 0xff, 0xff, 0xff, 0x0f]).unwrap(),
        AttributeModifierOperation::AddValue
    );
    // unknown attribute id
    assert_eq!(
        from_bytes::<ClientboundUpdateAttributesPacket>(&[1, 1, 0xff, 0x7f]),
        Err(DecodeError::msg("No value with id 16383"))
    );
}

fn look_at_and_abilities(g: &Golden) {
    let mut n = 0usize;
    for from in [Anchor::Feet, Anchor::Eyes] {
        g.check(
            &format!("player_look_at/pos/{n}"),
            &ClientboundPlayerLookAtPacket {
                from_anchor: from,
                x: 1.5,
                y: -2.5,
                z: 1e10,
                at_entity: None,
            },
        );
        n += 1;
        for to_anchor in [Anchor::Feet, Anchor::Eyes] {
            g.check(
                &format!("player_look_at/entity/{n}"),
                &ClientboundPlayerLookAtPacket {
                    from_anchor: from,
                    x: 0.5,
                    y: 64.0,
                    z: -0.5,
                    at_entity: Some(PlayerLookAtEntity {
                        entity: IDS[(n + 1) % 15],
                        to_anchor,
                    }),
                },
            );
            n += 1;
        }
    }
    g.check(
        "player_look_at/entity0",
        &ClientboundPlayerLookAtPacket {
            from_anchor: Anchor::Eyes,
            x: 0.0,
            y: 0.0,
            z: 0.0,
            at_entity: Some(PlayerLookAtEntity {
                entity: 0,
                to_anchor: Anchor::Feet,
            }),
        },
    );
    g.check_value("raw/anchor/feet", &Anchor::Feet);
    g.check_value("raw/anchor/eyes", &Anchor::Eyes);
    // `readEnum` indexes the constants array: ordinal 2 throws
    assert_eq!(
        from_bytes::<Anchor>(&[2]),
        Err(DecodeError::msg("Index 2 out of bounds for length 2"))
    );
    assert_eq!(
        from_bytes::<Anchor>(&[0xff, 0xff, 0xff, 0xff, 0x0f]),
        Err(DecodeError::msg("Index -1 out of bounds for length 2"))
    );

    for mask in 0..16usize {
        g.check(
            &format!("player_abilities/{mask}"),
            &ClientboundPlayerAbilitiesPacket {
                invulnerable: mask & 1 != 0,
                is_flying: mask & 2 != 0,
                can_fly: mask & 4 != 0,
                instabuild: mask & 8 != 0,
                flying_speed: FLOATS[mask % 10],
                walking_speed: FLOATS[(mask + 3) % 10],
            },
        );
    }
    // flag bits above the four known ones are ignored on read
    let p = ClientboundPlayerAbilitiesPacket::decode(
        &mut &[0xf5u8, 0x3f, 0x80, 0, 0, 0x3f, 0, 0, 0][..],
    )
    .unwrap();
    assert!(p.invulnerable && !p.is_flying && p.can_fly && !p.instabuild);
}

fn equipment(g: &Golden) {
    use EquipmentSlot::*;
    // Java ordinal order
    let slots = [MainHand, OffHand, Feet, Legs, Chest, Head, Body, Saddle];
    let mut stacks: Vec<ItemStack> = vec![
        ItemStack::with_count(Item::DIAMOND_SWORD, 1),
        ItemStack::with_count(Item::STONE, 64),
        ItemStack::empty(),
        ItemStack::with_count(Item::DIAMOND_PICKAXE, 1),
        ItemStack::with_count(Item::STONE, 16),
        ItemStack::with_count(Item::IRON_HELMET, 1),
        ItemStack::with_count(Item::APPLE, 2),
        ItemStack::with_count(Item::DIAMOND_SWORD, 1),
    ];
    stacks[3].set(DAMAGE, 100);
    stacks[4].set(MAX_STACK_SIZE, 16);
    stacks[5].set(UNBREAKABLE, ());
    stacks[5].set(DAMAGE, 3);
    stacks[7].set(MAX_DAMAGE, 2000);
    stacks[7].set(DAMAGE, 7);
    for i in 0..8 {
        g.check(&format!("set_equipment/single{i}"), &ClientboundSetEquipmentPacket { entity: IDS[i], slots: vec![(slots[i], stacks[i].clone())] });
        g.check(&format!("set_equipment/empty{i}"), &ClientboundSetEquipmentPacket { entity: IDS[i + 3], slots: vec![(slots[i], ItemStack::empty())] });
    }
    let all: Vec<_> = slots.into_iter().zip(stacks.iter().cloned()).collect();
    g.check("set_equipment/all", &ClientboundSetEquipmentPacket { entity: 77, slots: all.clone() });
    g.check("set_equipment/reversed", &ClientboundSetEquipmentPacket { entity: -77, slots: all.iter().rev().cloned().collect() });
    g.check("set_equipment/two", &ClientboundSetEquipmentPacket { entity: 0, slots: vec![(Head, stacks[5].clone()), (MainHand, stacks[0].clone())] });
    g.check(
        "set_equipment/dup",
        &ClientboundSetEquipmentPacket { entity: 5, slots: vec![(Saddle, stacks[1].clone()), (Saddle, stacks[2].clone()), (Saddle, stacks[3].clone())] },
    );
    // An empty list writes only the entity id; vanilla's do/while cannot read it back: encode-only comparison.
    let mut ours = Vec::new();
    write_varint(&mut ours, ClientboundSetEquipmentPacket::ID);
    mc_protocol::Encode::encode(&ClientboundSetEquipmentPacket { entity: 5, slots: vec![] }, &mut ours);
    assert_eq!(hex(&ours), hex(g.bytes("set_equipment/none")));
    assert!(matches!(ClientboundSetEquipmentPacket::decode(&mut &[5u8][..]), Err(DecodeError::Eof { .. })));
    // slot ordinals 8..=127 index past `EquipmentSlot.VALUES`; bit 7 only means "continue"
    assert_eq!(
        ClientboundSetEquipmentPacket::decode(&mut &[1u8, 8, 0][..]),
        Err(DecodeError::msg("Index 8 out of bounds for length 8"))
    );
    assert_eq!(
        ClientboundSetEquipmentPacket::decode(&mut &[1u8, 0xff, 0][..]),
        Err(DecodeError::msg("Index 127 out of bounds for length 8"))
    );
    let two = ClientboundSetEquipmentPacket::decode(&mut &[1u8, 0x80, 0, 0x01, 0][..]).unwrap();
    assert_eq!(two.slots, vec![(MainHand, ItemStack::empty()), (OffHand, ItemStack::empty())]);
}

#[test]
fn entity_packets_match_vanilla() {
    let g = Golden::load("entity");
    simple_id_packets(&g);
    swing_and_damage(&g);
    move_entity(&g);
    minecart_and_position_sync(&g);
    teleport_and_player_position(&g);
    motion_passengers_remove(&g);
    mob_effects(&g);
    attributes(&g);
    look_at_and_abilities(&g);
    equipment(&g);
    g.assert_all_used();
}
