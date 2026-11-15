//! Light sword class: one-handed, quick swings. Template for new weapon classes.

use bevy::prelude::*;

use super::{AttackSpec, GuardSpec, ModelSource, Moveset, PartShape, Stance, SwingKey, WeaponClass, WeaponDef, WeaponPart};

const LIGHT_STARTUP: f32 = 0.15;
const LIGHT_ACTIVE: f32 = 0.12;
const LIGHT_RECOVERY: f32 = 0.25;

const fn light(windup: SwingKey, strike: SwingKey) -> AttackSpec {
    AttackSpec {
        startup: LIGHT_STARTUP,
        active: LIGHT_ACTIVE,
        recovery: LIGHT_RECOVERY,
        cancel: 0.35,
        stamina_cost: 15.0,
        lunge: 0.45,
        damage_mult: 1.0,
        windup,
        strike,
        stance: Stance::Lunge,
    }
}

/// Combo: right → left, left → right, overhead.
const LIGHT_COMBO: [AttackSpec; 3] = [
    light(SwingKey::new(-0.45, 0.0, -0.95, 1.45), SwingKey::new(0.55, 0.0, 0.75, 1.45)),
    light(SwingKey::new(0.5, 0.0, 0.8, 1.5), SwingKey::new(-0.45, 0.0, -0.95, 1.4)),
    light(SwingKey::new(0.0, 0.15, 0.0, 2.7), SwingKey::new(0.0, -0.3, 0.0, 0.95)),
];

pub const MOVESET: Moveset = Moveset {
    light: &LIGHT_COMBO,
    heavy: AttackSpec {
        startup: 0.12, // short: the charge is the wind-up
        active: 0.15,
        recovery: 0.4,
        cancel: 0.7,
        stamina_cost: 0.0, // paid when the charge starts
        lunge: 0.7,
        damage_mult: 1.6,
        // Raised overhead (the charge pose) → down in front.
        windup: SwingKey::new(0.15, 0.2, 0.1, 3.1),
        strike: SwingKey::new(0.0, -0.35, 0.0, 1.1),
        stance: Stance::Lunge,
    },
    heavy_charge_max: 0.65,
    heavy_charge_cost: 30.0,
    heavy_full_charge_mult: 2.0,
    running: AttackSpec {
        startup: 0.18,
        active: 0.16,
        recovery: 0.4,
        cancel: 0.58,
        stamina_cost: 20.0,
        lunge: 3.2,
        damage_mult: 1.3,
        // Diagonal: high right → low left.
        windup: SwingKey::new(-0.5, 0.1, -0.6, 2.6),
        strike: SwingKey::new(0.3, -0.35, 0.35, 1.1),
        stance: Stance::Leap,
    },
    running_hop: 3.2,
    jump: AttackSpec {
        startup: 0.2,
        active: 1.0, // max plunge time; landing cuts it short
        recovery: 0.35,
        cancel: 1.55, // can't cancel
        stamina_cost: 25.0,
        lunge: 0.0, // keeps the jump's momentum instead
        damage_mult: 1.8,
        windup: SwingKey::new(0.0, 0.15, 0.0, 3.05),
        strike: SwingKey::new(0.0, -0.4, 0.0, 0.75),
        stance: Stance::Tucked,
    },
    jump_strike_time: 0.12,
    guard: GuardSpec {
        arm_pitch: 0.25,
        elbow: 0.7,
        // Low guard: blade forward and down.
        grip: Vec3::new(-1.95, 0.0, -0.15),
        off_arm_pitch: 0.1,
        off_elbow: 0.3,
        // Sprint: arm low and back, blade trailing behind and down.
        run_arm_pitch: -0.3,
        run_elbow: 0.35,
        run_grip: Vec3::new(2.05, 0.0, 0.3),
    },
};

const STEEL: Color = Color::srgb(0.75, 0.76, 0.78);
const LEATHER: Color = Color::srgb(0.12, 0.08, 0.05);

const fn steel(shape: PartShape, y: f32) -> WeaponPart {
    WeaponPart { shape, color: STEEL, metallic: 1.0, roughness: 0.3, offset: Vec3::new(0.0, y, 0.0) }
}

const LONGSWORD_PARTS: [WeaponPart; 4] = [
    steel(PartShape::Sphere(0.035), -0.1),
    WeaponPart {
        shape: PartShape::Cylinder { radius: 0.022, height: 0.2 },
        color: LEATHER,
        metallic: 0.0,
        roughness: 0.8,
        offset: Vec3::ZERO,
    },
    steel(PartShape::Box(Vec3::new(0.22, 0.035, 0.04)), 0.11),
    steel(PartShape::Box(Vec3::new(0.06, 0.9, 0.015)), 0.58),
];

/// The starting weapon.
pub static LONGSWORD: WeaponDef = WeaponDef {
    name: "Longsword",
    class: WeaponClass::LightSword,
    base_damage: 20.0,
    blade: (0.13, 1.03),
    model: ModelSource::Parts(&LONGSWORD_PARTS),
};

const RUST: Color = Color::srgb(0.45, 0.3, 0.2);

pub(super) const SHORTSWORD_PARTS: [WeaponPart; 4] = [
    WeaponPart { shape: PartShape::Sphere(0.03), color: RUST, metallic: 0.6, roughness: 0.7, offset: Vec3::new(0.0, -0.09, 0.0) },
    WeaponPart {
        shape: PartShape::Cylinder { radius: 0.02, height: 0.17 },
        color: LEATHER,
        metallic: 0.0,
        roughness: 0.8,
        offset: Vec3::ZERO,
    },
    WeaponPart {
        shape: PartShape::Box(Vec3::new(0.16, 0.03, 0.035)),
        color: RUST,
        metallic: 0.6,
        roughness: 0.7,
        offset: Vec3::new(0.0, 0.1, 0.0),
    },
    WeaponPart {
        shape: PartShape::Box(Vec3::new(0.055, 0.62, 0.014)),
        color: RUST,
        metallic: 0.6,
        roughness: 0.6,
        offset: Vec3::new(0.0, 0.42, 0.0),
    },
];

/// A second light-sword model: same moveset, shorter (less reach) and weaker.
pub static SHORTSWORD: WeaponDef = WeaponDef {
    name: "Rusted Shortsword",
    class: WeaponClass::LightSword,
    base_damage: 16.0,
    blade: (0.12, 0.73),
    model: ModelSource::Parts(&SHORTSWORD_PARTS),
};

const AXE_PARTS: [WeaponPart; 4] = [
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

/// A hand axe: swung like a light sword for now (its own class later, N15). Hits harder, reaches
/// less far; only the head cuts.
pub static RUSTED_AXE: WeaponDef = WeaponDef {
    name: "Rusted Axe",
    class: WeaponClass::LightSword,
    base_damage: 24.0,
    blade: (0.5, 0.72),
    model: ModelSource::Parts(&AXE_PARTS),
};

/// Nothing in the right hand: the same swings, but with the fist.
pub static FISTS: WeaponDef = WeaponDef {
    name: "Fists",
    class: WeaponClass::LightSword,
    base_damage: 8.0,
    blade: (-0.02, 0.1),
    model: ModelSource::Parts(&[]),
};
