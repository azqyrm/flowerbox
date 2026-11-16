//! Axe class: heavy, slower, **vertical** chops — the way an axe is really swung: hauled up over
//! the shoulder, brought down in front with the body behind it. Combo: overhead chop, a diagonal
//! chop from the upper right, then a heavier overhead finisher. Heavy: a huge overhead blow from
//! far behind the head. Running: a leaping chop. Jump: a plunging chop. Ability: *Warcry*.

use bevy::prelude::*;

use super::light_sword::RUST;
use super::{AttackSpec, ModelSource, Moveset, PartShape, Stance, SwingKey, WeaponClass, WeaponDef, WeaponPart, light_sword};

const fn chop(startup: f32, recovery: f32, stamina: f32, lunge: f32, damage: f32, windup: SwingKey, strike: SwingKey) -> AttackSpec {
    AttackSpec {
        startup,
        active: 0.14,
        recovery,
        cancel: startup + 0.14 + 0.22,
        stamina_cost: stamina,
        lunge,
        damage_mult: damage,
        windup,
        strike,
        stance: Stance::Lunge,
    }
}

/// Up over the head (arm pitch ≈ π, chest leaning back) → down in front (chest bent forward).
const COMBO: [AttackSpec; 3] = [
    chop(0.48, 0.46, 18.0, 0.55, 1.0, SwingKey::new(0.1, 0.28, 0.05, 2.95), SwingKey::new(0.0, -0.38, 0.0, 0.65)),
    // From over the right shoulder, diagonally down to the left knee.
    chop(0.44, 0.44, 18.0, 0.5, 1.05, SwingKey::new(-0.45, 0.2, -0.55, 2.7), SwingKey::new(0.35, -0.32, 0.4, 0.7)),
    chop(0.62, 0.6, 22.0, 0.8, 1.35, SwingKey::new(0.0, 0.4, 0.0, 3.1), SwingKey::new(0.0, -0.5, 0.0, 0.5)),
];

pub const MOVESET: Moveset = Moveset {
    light: &COMBO,
    heavy: AttackSpec {
        startup: 0.16,
        active: 0.16,
        recovery: 0.55,
        cancel: 0.85,
        stamina_cost: 0.0, // paid when the charge starts
        lunge: 0.8,
        damage_mult: 1.75,
        windup: SwingKey::new(0.15, 0.45, 0.1, 3.2),
        strike: SwingKey::new(0.0, -0.55, 0.0, 0.45),
        stance: Stance::Lunge,
    },
    heavy_charge_max: 0.8,
    heavy_charge_cost: 34.0,
    heavy_full_charge_mult: 2.2,
    running: AttackSpec {
        startup: 0.24,
        active: 0.18,
        recovery: 0.5,
        cancel: 0.72,
        stamina_cost: 24.0,
        lunge: 3.0,
        damage_mult: 1.4,
        windup: SwingKey::new(0.0, 0.35, 0.0, 3.0),
        strike: SwingKey::new(0.0, -0.45, 0.0, 0.55),
        stance: Stance::Leap,
    },
    running_hop: 3.4,
    jump: AttackSpec {
        startup: 0.24,
        active: 1.0, // max plunge time; landing cuts it short
        recovery: 0.45,
        cancel: 1.69, // can't cancel
        stamina_cost: 28.0,
        lunge: 0.0,
        damage_mult: 2.0,
        windup: SwingKey::new(0.0, 0.3, 0.0, 3.1),
        strike: SwingKey::new(0.0, -0.5, 0.0, 0.6),
        stance: Stance::Tucked,
    },
    jump_strike_time: 0.14,
    ability: Some(crate::ability::AbilityId::Warcry),
    ..light_sword::MOVESET
};

const RUSTED_AXE_PARTS: [WeaponPart; 4] = [
    WeaponPart {
        shape: PartShape::Cylinder { radius: 0.021, height: 0.82 },
        color: Color::srgb(0.3, 0.2, 0.11),
        metallic: 0.0,
        roughness: 0.85,
        offset: Vec3::new(0.0, 0.26, 0.0),
    },
    // Head: the bit sticks out along +X (the side the sword's edge faces), a short poll behind.
    WeaponPart {
        shape: PartShape::Box(Vec3::new(0.15, 0.17, 0.022)),
        color: RUST,
        metallic: 0.6,
        roughness: 0.65,
        offset: Vec3::new(0.08, 0.6, 0.0),
    },
    WeaponPart {
        shape: PartShape::Box(Vec3::new(0.05, 0.08, 0.04)),
        color: RUST,
        metallic: 0.6,
        roughness: 0.7,
        offset: Vec3::new(-0.03, 0.62, 0.0),
    },
    WeaponPart { shape: PartShape::Sphere(0.026), color: RUST, metallic: 0.6, roughness: 0.7, offset: Vec3::new(0.0, -0.15, 0.0) },
];

/// A hand axe. Hits harder than a sword, reaches less far; only the head cuts.
pub static RUSTED_AXE: WeaponDef = WeaponDef {
    name: "Rusted Axe",
    class: WeaponClass::Axe,
    base_damage: 24.0,
    blade: (0.5, 0.72),
    model: ModelSource::Parts(&RUSTED_AXE_PARTS),
    guard_block: 0.3,
};

const WOOD: Color = Color::srgb(0.42, 0.29, 0.16);
const BITE: Color = Color::srgb(0.55, 0.56, 0.58);

/// A woodcutter's hatchet: shorter haft, a small bright head.
const HATCHET_PARTS: [WeaponPart; 3] = [
    WeaponPart { shape: PartShape::Cylinder { radius: 0.019, height: 0.66 }, color: WOOD, metallic: 0.0, roughness: 0.8, offset: Vec3::new(0.0, 0.2, 0.0) },
    WeaponPart { shape: PartShape::Box(Vec3::new(0.12, 0.13, 0.02)), color: BITE, metallic: 0.8, roughness: 0.4, offset: Vec3::new(0.065, 0.48, 0.0) },
    WeaponPart { shape: PartShape::Box(Vec3::new(0.045, 0.06, 0.035)), color: BITE, metallic: 0.8, roughness: 0.45, offset: Vec3::new(-0.025, 0.5, 0.0) },
];

/// A light axe: a little weaker than the rusted one, a natural partner for it (two axes).
pub static WOODCUTTER_HATCHET: WeaponDef = WeaponDef {
    name: "Woodcutter's Hatchet",
    class: WeaponClass::Axe,
    base_damage: 20.0,
    blade: (0.4, 0.56),
    model: ModelSource::Parts(&HATCHET_PARTS),
    guard_block: 0.25,
};
