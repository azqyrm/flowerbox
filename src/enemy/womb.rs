//! The Womb's brood (level 4, an original horror): the **Unborn** (small, pale, half-formed
//! things that rush in packs on raking fingers, an umbilical cord trailing; backstab them) and the
//! boss, **the Matron** (an impossibly tall, eyeless mother of the deep: long arms, a crown of
//! writhing tendrils, a wail that can't be blocked, bile sprayed in fans, a crushing embrace).

use bevy::prelude::*;
use std::f32::consts::{FRAC_PI_2, PI};

use super::fighter::{Enrage, FighterProfile, Special};
use super::kinds::{Body, Brain, EnemyDef};
use crate::ability::AbilityId;
use crate::character::BodySize;
use crate::humanoid::skirt::Skirt;
use crate::humanoid::{Bone, HumanoidLook, HumanoidSpec, Paint, Part, Shape};
use crate::textures::Tex;
use crate::tuning::*;
use crate::weapon::{ModelSource, WeaponClass, WeaponDef};

const PALE: Color = Color::srgb(0.92, 0.8, 0.78);
const CORD: Color = Color::srgb(0.45, 0.12, 0.14);

/// The Unborn's raking fingers (part of the body).
pub static UNBORN_FINGERS: WeaponDef = WeaponDef {
    name: "Unborn Fingers",
    class: WeaponClass::Claws,
    base_damage: 11.0,
    blade: (0.0, 0.42),
    model: ModelSource::Parts(&[]),
    guard_block: 0.0,
    stance_damage: 7.0,
};

/// The Matron's hands: long as swords.
pub static MATRON_TALONS: WeaponDef = WeaponDef {
    name: "The Matron's Talons",
    class: WeaponClass::Claws,
    base_damage: 30.0,
    blade: (0.0, 1.3),
    model: ModelSource::Parts(&[]),
    guard_block: 0.0,
    stance_damage: 26.0,
};

// ── The Unborn ───────────────────────────────────────────────────────────

pub const UNBORN_PROFILE: FighterProfile = FighterProfile {
    name: "Unborn",
    aggro_range: 12.0,
    leash: 24.0,
    move_factor: 1.0,
    sprint_beyond: 3.0,
    attack_range: 1.7,
    heavy_chance: 0.0,
    extra_swings: (2, 4),
    recover_time: (0.5, 1.1),
    patrol_factor: 0.15,
    specials: &[Special::new(AbilityId::RendingTurn, (0.0, 2.2), 0.3, 6.0)],
    stalker: false,
    enrage: None,
};

fn unborn() -> HumanoidSpec {
    let mut extras = vec![
        // An oversized, bald, veined head with sealed eyes and a gaping mouth.
        Part::new(Bone::Head, Shape::Ball(0.15), Vec3::new(0.0, 0.1, 0.02), Vec3::ZERO, Paint::Textured(PALE, Tex::Flesh)),
        Part::new(Bone::Head, Shape::Box(Vec3::new(0.06, 0.05, 0.02)), Vec3::new(0.0, 0.03, -0.13), Vec3::ZERO, Paint::Glow(LinearRgba::rgb(4.0, 0.3, 0.2))),
        // The cord trailing from the belly to the floor.
        Part::new(Bone::Pelvis, Shape::Capsule(0.025, 0.5), Vec3::new(0.0, -0.05, -0.15), Vec3::new(0.6, 0.0, 0.0), Paint::Textured(CORD, Tex::Flesh)),
        Part::new(Bone::Pelvis, Shape::Capsule(0.025, 0.5), Vec3::new(0.0, -0.32, -0.38), Vec3::new(1.3, 0.0, 0.0), Paint::Textured(CORD, Tex::Flesh)),
    ];
    // Long, bony fingers.
    for side in [-1.0f32, 1.0] {
        let hand = if side < 0.0 { Bone::HandL } else { Bone::HandR };
        for k in 0..3 {
            extras.push(Part::new(hand, Shape::Cone(0.012, 0.2), Vec3::new(0.02 * (k as f32 - 1.0), -0.12, 0.0), Vec3::new(PI, 0.0, 0.1 * (k as f32 - 1.0)), Paint::Textured(PALE, Tex::Bone)));
        }
    }
    HumanoidSpec {
        look: HumanoidLook {
            skin: PALE,
            top: PALE,
            bottom: PALE,
            hair: PALE,
            skin_tex: Some(Tex::Flesh),
            top_tex: Some(Tex::Flesh),
            bottom_tex: Some(Tex::Flesh),
            eye_glow: None,
            bald: true,
            extras,
            ..HumanoidSpec::hollow().look
        },
        ..HumanoidSpec::hollow()
    }
    .scaled(0.8)
}

pub const UNBORN: EnemyDef = EnemyDef {
    name: "Unborn",
    hp: 60.0,
    stamina: 140.0,
    stance: 18.0,
    light: 45,
    backstab: true,
    size: BodySize { radius: PLAYER_RADIUS, length: PLAYER_CAPSULE_LENGTH },
    target_height: 0.35,
    body: Body::Humanoid(unborn),
    weapon: &UNBORN_FINGERS,
    brain: Brain::Fighter(&UNBORN_PROFILE),
    boss: false,
};

// ── The Matron ───────────────────────────────────────────────────────────

pub const MATRON_PROFILE: FighterProfile = FighterProfile {
    name: "Matron",
    aggro_range: 24.0,
    leash: 60.0,
    move_factor: 0.7,
    sprint_beyond: 10.0,
    attack_range: 3.6,
    heavy_chance: 0.3,
    extra_swings: (1, 3),
    recover_time: (0.9, 1.6),
    patrol_factor: 0.0,
    specials: &[
        Special::new(AbilityId::BileSpray, (4.0, 16.0), 0.45, 6.0),
        Special::new(AbilityId::CrushingEmbrace, (0.0, 4.5), 0.35, 8.0),
        Special::new(AbilityId::RendingTurn, (0.0, 3.5), 0.35, 6.0),
        Special::new(AbilityId::Wail, (0.0, 9.0), 0.25, 12.0),
        Special::new(AbilityId::DashStrike, (7.0, 15.0), 0.35, 8.0).enraged(),
        Special::new(AbilityId::BileSpray, (0.0, 16.0), 0.4, 4.0).enraged(),
    ],
    stalker: false,
    enrage: Some(Enrage { below: 0.45, roar: Some(AbilityId::Wail), speed: 1.3, recover: 0.5 }),
};

fn matron() -> HumanoidSpec {
    let skin = Color::srgb(0.55, 0.35, 0.36);
    let veil = Color::srgb(0.25, 0.04, 0.06);
    let mut extras = vec![
        // An eyeless face: a smooth dome, a vertical mouth glowing from inside.
        Part::new(Bone::Head, Shape::Ball(0.14), Vec3::new(0.0, 0.12, -0.01), Vec3::ZERO, Paint::Textured(skin, Tex::Flesh)),
        Part::new(Bone::Head, Shape::Box(Vec3::new(0.02, 0.14, 0.02)), Vec3::new(0.0, 0.06, -0.135), Vec3::ZERO, Paint::Glow(LinearRgba::rgb(8.0, 1.5, 0.6))),
        // A swollen belly under the veil, glowing faintly through the skin.
        Part::new(Bone::Pelvis, Shape::Ball(0.26), Vec3::new(0.0, 0.18, -0.12), Vec3::ZERO, Paint::Textured(Color::srgb(0.7, 0.35, 0.3), Tex::Flesh)),
        Part::new(Bone::Pelvis, Shape::Ball(0.12), Vec3::new(0.0, 0.2, -0.3), Vec3::ZERO, Paint::Glow(LinearRgba::rgb(3.0, 0.4, 0.3))),
    ];
    // A crown of tendrils: long, drooping, splayed round the head.
    for k in 0..9 {
        let a = k as f32 / 9.0 * std::f32::consts::TAU;
        let (s, c) = a.sin_cos();
        extras.push(Part::new(
            Bone::Head,
            Shape::Capsule(0.022, 0.6 + 0.25 * ((k * 5 % 3) as f32)),
            Vec3::new(0.12 * c, 0.05, 0.12 * s + 0.05),
            Vec3::new(0.9 + 0.4 * s, 0.0, -0.9 * c),
            Paint::Textured(CORD, Tex::Flesh),
        ));
    }
    // Spurs of bone down the spine.
    for (i, y) in [0.1, 0.25, 0.4].into_iter().enumerate() {
        extras.push(Part::new(Bone::Chest, Shape::Cone(0.04, 0.16 - 0.03 * i as f32), Vec3::new(0.0, y, 0.15), Vec3::new(-FRAC_PI_2 * 0.6, 0.0, 0.0), Paint::Textured(Color::srgb(0.9, 0.86, 0.8), Tex::Bone)));
    }
    HumanoidSpec {
        look: HumanoidLook {
            skin,
            top: veil,
            bottom: veil,
            hair: CORD,
            skin_tex: Some(Tex::Flesh),
            eye_glow: None,
            bald: true,
            extras,
            skirts: vec![
                Skirt::robe(0.12, Vec2::new(0.22, 0.16), Vec2::new(0.42, 0.4), 1.05, Paint::Textured(veil, Tex::Flesh)),
                Skirt { bone: Bone::Chest, top: 0.46, waist: Vec2::new(0.24, 0.16), hem: Vec2::new(0.45, 0.4), length: 1.4, span: 1.6, paint: Paint::Textured(veil, Tex::Cloth) },
            ],
            ..HumanoidSpec::hollow().look
        },
        // Long-limbed: arms nearly to the knees.
        upper_arm: HumanoidSpec::hollow().upper_arm * 1.35,
        forearm: HumanoidSpec::hollow().forearm * 1.4,
        ..HumanoidSpec::hollow()
    }
    .scaled(1.7)
}

pub const MATRON: EnemyDef = EnemyDef {
    name: "The Matron",
    hp: 1800.0,
    stamina: 800.0,
    stance: 160.0,
    light: 5000,
    backstab: false,
    size: BodySize { radius: 0.65, length: 1.8 },
    target_height: 1.05,
    body: Body::Humanoid(matron),
    weapon: &MATRON_TALONS,
    brain: Brain::Fighter(&MATRON_PROFILE),
    boss: true,
};
