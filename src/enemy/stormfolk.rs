//! The Storm Isle's master (level 5): **the Stormcaller**, a gaunt figure in a sea-soaked cloak
//! with a crown of iron spikes that spit sparks and a lightning rod for a staff — Thunderclap (a
//! slam with a wide shockwave), Chain Lightning (fast sparks thrown in a tight fan), a storm dash and
//! a whirl of the rod.

use bevy::prelude::*;
use std::f32::consts::FRAC_PI_2;

use super::fighter::{Enrage, FighterProfile, Special};
use super::kinds::{Body, Brain, EnemyDef};
use crate::ability::AbilityId;
use crate::character::BodySize;
use crate::humanoid::skirt::Skirt;
use crate::humanoid::{Bone, HumanoidLook, HumanoidSpec, Paint, Part, Shape};
use crate::textures::Tex;
use crate::weapon::{ModelSource, Styled, WeaponClass, WeaponDef};

const SPARK: LinearRgba = LinearRgba::rgb(4.0, 6.0, 12.0);

const ROD_PARTS: [Styled; 5] = [
    Styled::new(Shape::Cylinder(0.035, 1.8), 0.4, Paint::TexturedMetal(Color::srgb(0.2, 0.2, 0.22), Tex::Rust)),
    Styled::new(Shape::Ring(0.02, 0.12), 1.2, Paint::Metal(Color::srgb(0.55, 0.42, 0.25))),
    Styled::at(Shape::Cone(0.05, 0.4), Vec3::new(0.0, 1.5, 0.0), Vec3::ZERO, Paint::Metal(Color::srgb(0.6, 0.6, 0.65))),
    Styled::at(Shape::Cone(0.03, 0.3), Vec3::new(0.08, 1.35, 0.0), Vec3::new(0.0, 0.0, -0.5), Paint::Metal(Color::srgb(0.6, 0.6, 0.65))),
    Styled::at(Shape::Ball(0.07), Vec3::new(0.0, 1.72, 0.0), Vec3::ZERO, Paint::Glow(SPARK)),
];

/// The Stormcaller's lightning rod (a greatsword's moves, long reach).
pub static LIGHTNING_ROD: WeaponDef = WeaponDef {
    name: "Lightning Rod",
    class: WeaponClass::Greatsword,
    base_damage: 26.0,
    blade: (0.6, 1.9),
    model: ModelSource::Styled(&ROD_PARTS),
    guard_block: 0.4,
    stance_damage: 26.0,
};

pub const STORMCALLER_PROFILE: FighterProfile = FighterProfile {
    name: "Stormcaller",
    aggro_range: 26.0,
    leash: 60.0,
    move_factor: 0.85,
    sprint_beyond: 8.0,
    attack_range: 3.4,
    heavy_chance: 0.25,
    extra_swings: (1, 2),
    recover_time: (0.8, 1.4),
    patrol_factor: 0.0,
    specials: &[
        Special::new(AbilityId::ChainLightning, (5.0, 20.0), 0.5, 5.0),
        Special::new(AbilityId::Thunderclap, (0.0, 6.0), 0.35, 8.0),
        Special::new(AbilityId::DashStrike, (6.0, 16.0), 0.45, 6.0),
        Special::new(AbilityId::Whirlwind, (0.0, 4.5), 0.3, 9.0),
        Special::new(AbilityId::ChainLightning, (0.0, 20.0), 0.45, 3.5).enraged(),
        Special::new(AbilityId::Thunderclap, (0.0, 8.0), 0.4, 6.0).enraged(),
    ],
    stalker: false,
    enrage: Some(Enrage { below: 0.5, roar: Some(AbilityId::Thunderclap), speed: 1.3, recover: 0.55 }),
};

fn stormcaller() -> HumanoidSpec {
    let cloak = Color::srgb(0.12, 0.14, 0.18);
    let iron = Color::srgb(0.25, 0.26, 0.28);
    let mut extras = vec![
        // A drowned face: grey-blue, eyes glowing white-blue; a beard of weed.
        Part::new(Bone::Head, Shape::Box(Vec3::new(0.12, 0.14, 0.04)), Vec3::new(0.0, -0.06, -0.09), Vec3::new(0.2, 0.0, 0.0), Paint::Textured(Color::srgb(0.18, 0.28, 0.2), Tex::Cloth)),
        // The crown of spikes, each tipped with a spark.
        Part::new(Bone::Head, Shape::Ring(0.02, 0.13), Vec3::new(0.0, 0.17, 0.0), Vec3::ZERO, Paint::TexturedMetal(iron, Tex::Rust)),
    ];
    for k in 0..7 {
        let a = k as f32 / 7.0 * std::f32::consts::TAU;
        let (s, c) = a.sin_cos();
        extras.push(Part::new(Bone::Head, Shape::Cone(0.025, 0.2), Vec3::new(0.13 * c, 0.26, 0.13 * s), Vec3::new(0.25 * s, 0.0, -0.25 * c), Paint::TexturedMetal(iron, Tex::Rust)));
        extras.push(Part::new(Bone::Head, Shape::Ball(0.02), Vec3::new(0.16 * c, 0.37, 0.16 * s), Vec3::ZERO, Paint::Glow(SPARK)));
    }
    // A cowl of oiled sailcloth.
    extras.push(Part::new(Bone::Chest, Shape::Ring(0.06, 0.2), Vec3::new(0.0, 0.42, 0.0), Vec3::new(FRAC_PI_2 * 0.15, 0.0, 0.0), Paint::Textured(cloak, Tex::Cloth)));
    HumanoidSpec {
        look: HumanoidLook {
            skin: Color::srgb(0.42, 0.48, 0.52),
            top: cloak,
            bottom: cloak,
            hair: Color::srgb(0.15, 0.2, 0.15),
            eye_glow: Some(SPARK * 2.0),
            bald: true,
            extras,
            skirts: vec![
                Skirt::robe(0.1, Vec2::new(0.21, 0.14), Vec2::new(0.34, 0.31), 1.0, Paint::Textured(cloak, Tex::Cloth)),
                Skirt { bone: Bone::Chest, top: 0.44, waist: Vec2::new(0.25, 0.16), hem: Vec2::new(0.42, 0.36), length: 1.3, span: 1.9, paint: Paint::Textured(Color::srgb(0.08, 0.1, 0.13), Tex::Cloth) },
            ],
            ..HumanoidSpec::hollow().look
        },
        ..HumanoidSpec::hollow()
    }
    .scaled(1.4)
}

pub const STORMCALLER: EnemyDef = EnemyDef {
    name: "The Stormcaller",
    hp: 1500.0,
    stamina: 700.0,
    stance: 140.0,
    light: 4500,
    backstab: false,
    size: BodySize { radius: 0.55, length: 1.5 },
    target_height: 0.9,
    body: Body::Humanoid(stormcaller),
    weapon: &LIGHTNING_ROD,
    brain: Brain::Fighter(&STORMCALLER_PROFILE),
    boss: true,
};
