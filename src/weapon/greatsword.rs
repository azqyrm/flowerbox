//! Greatsword class: huge blades swung with the whole body — slow, wide, heavy sweeps that
//! punish standing close, with long recoveries to punish back. The Thorned Saint fights with one
//! (scaled up to its size); its smaller twin, *Penitence*, is the player's reward. Its ability is
//! the *Penitent Whirl*.

use bevy::prelude::*;

use super::{AttackSpec, GuardSpec, ModelSource, Moveset, Stance, Styled, SwingKey, WeaponClass, WeaponDef, light_sword};
use crate::humanoid::{Paint, Shape};
use crate::textures::Tex;

const fn sweep(startup: f32, recovery: f32, damage: f32, windup: SwingKey, strike: SwingKey) -> AttackSpec {
    AttackSpec { startup, active: 0.22, recovery, cancel: startup + 0.22 + recovery * 0.7, stamina_cost: 24.0, lunge: 0.9, damage_mult: damage, windup, strike, stance: Stance::Lunge }
}

/// Wide right → left, back-hand left → right (both sweeping down through the target: a greatsword
/// held out flat by a giant passes over a person's head), then a crushing overhead.
const COMBO: [AttackSpec; 3] = [
    sweep(0.62, 0.62, 1.0, SwingKey::new(-0.9, 0.2, -1.5, 1.25), SwingKey::new(0.75, 0.3, 1.0, 1.0)),
    sweep(0.5, 0.6, 1.05, SwingKey::new(0.8, 0.2, 1.1, 1.25), SwingKey::new(-0.8, 0.3, -1.2, 1.0)),
    sweep(0.75, 0.8, 1.35, SwingKey::new(0.1, -0.35, 0.0, 3.1), SwingKey::new(0.0, 0.5, 0.0, 0.6)),
];

pub const MOVESET: Moveset = Moveset {
    light: &COMBO,
    heavy: AttackSpec {
        startup: 0.2,
        active: 0.24,
        recovery: 0.8,
        cancel: 1.1,
        stamina_cost: 0.0,
        lunge: 1.1,
        damage_mult: 1.9,
        windup: SwingKey::new(0.2, -0.3, 0.1, 3.1),
        strike: SwingKey::new(0.0, 0.55, 0.0, 0.6),
        stance: Stance::Lunge,
    },
    heavy_charge_max: 0.9,
    heavy_charge_cost: 38.0,
    heavy_full_charge_mult: 2.5,
    running: AttackSpec {
        startup: 0.32,
        active: 0.2,
        recovery: 0.6,
        cancel: 0.95,
        stamina_cost: 30.0,
        lunge: 3.4,
        damage_mult: 1.5,
        windup: SwingKey::new(-0.6, 0.1, -0.9, 2.4),
        strike: SwingKey::new(0.5, -0.3, 0.6, 1.0),
        stance: Stance::Leap,
    },
    jump: AttackSpec { stamina_cost: 34.0, damage_mult: 2.1, ..light_sword::MOVESET.jump },
    guard: GuardSpec {
        arm_pitch: 0.45,
        elbow: 0.6,
        // Held high across the body, the point toward the floor in front.
        grip: Vec3::new(-2.3, 0.3, -0.4),
        off_arm_pitch: 0.5,
        off_elbow: 0.9,
        run_arm_pitch: 0.2,
        run_elbow: 0.5,
        run_grip: Vec3::new(1.4, 0.0, 0.5),
    },
    pounce: None,
    ability: Some(crate::ability::AbilityId::PenitentWhirl),
    ..light_sword::MOVESET
};

const IRON: Color = Color::srgb(0.32, 0.31, 0.33);
const BLOOD: LinearRgba = LinearRgba::rgb(5.0, 0.35, 0.15);

/// A long grip wound in leather, a thorned crossguard, a broad rune-etched blade with a glowing
/// fuller, and thorns along the spine.
const PENITENCE_PARTS: [Styled; 10] = [
    Styled::new(Shape::Cylinder(0.025, 0.34), 0.02, Paint::Textured(Color::srgb(0.18, 0.1, 0.06), Tex::Leather)),
    Styled::new(Shape::Ball(0.045), -0.16, Paint::Metal(IRON)),
    Styled::new(Shape::Box(Vec3::new(0.36, 0.05, 0.06)), 0.2, Paint::TexturedMetal(IRON, Tex::Rust)),
    Styled::at(Shape::Cone(0.03, 0.12), Vec3::new(0.2, 0.2, 0.0), Vec3::new(0.0, 0.0, -1.57), Paint::Metal(IRON)),
    Styled::at(Shape::Cone(0.03, 0.12), Vec3::new(-0.2, 0.2, 0.0), Vec3::new(0.0, 0.0, 1.57), Paint::Metal(IRON)),
    Styled::new(Shape::Box(Vec3::new(0.12, 1.22, 0.025)), 0.84, Paint::TexturedMetal(Color::srgb(0.6, 0.6, 0.62), Tex::Rust)),
    Styled::at(Shape::Box(Vec3::new(0.03, 1.05, 0.03)), Vec3::new(0.0, 0.8, 0.0), Vec3::ZERO, Paint::GlowTex(BLOOD, Tex::Runes)),
    Styled::at(Shape::Cone(0.06, 0.2), Vec3::new(0.0, 1.55, 0.0), Vec3::ZERO, Paint::Metal(Color::srgb(0.6, 0.6, 0.62))),
    Styled::at(Shape::Cone(0.02, 0.09), Vec3::new(0.07, 0.6, 0.0), Vec3::new(0.0, 0.0, -1.1), Paint::Metal(IRON)),
    Styled::at(Shape::Cone(0.02, 0.09), Vec3::new(0.07, 1.0, 0.0), Vec3::new(0.0, 0.0, -1.1), Paint::Metal(IRON)),
];

/// The player's greatsword (the Thorned Saint's reward).
pub static PENITENCE: WeaponDef = WeaponDef {
    name: "Penitence",
    class: WeaponClass::Greatsword,
    base_damage: 34.0,
    blade: (0.24, 1.62),
    model: ModelSource::Styled(&PENITENCE_PARTS),
    guard_block: 0.55,
    stance_damage: 22.0,
};

/// The Saint's own blade: the same shape, its body is 1.6× as big (the blade reach matches).
pub static SAINT_BLADE: WeaponDef = WeaponDef { name: "Saint's Penitence", base_damage: 30.0, blade: (0.35, 2.55), stance_damage: 30.0, ..PENITENCE };
