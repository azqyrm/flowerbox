//! The Drowned Library's dwellers (level 3): the **Scribe** (a small, quick, ink-stained reader
//! with a quill-knife; backstab it while it reads), the **Page Hound** (a lean hound of pasted
//! pages and ink) and the boss, **the Archivist** (a towering keeper with a quill as long as a
//! lance and a tome the size of a door: Tome Slam, Ink Volley, Page Storm, Hush).

use bevy::prelude::*;
use std::f32::consts::FRAC_PI_2;

use super::fighter::{Enrage, FighterProfile, Special};
use super::kinds::{Body, Brain, EnemyDef};
use super::warg;
use crate::ability::AbilityId;
use crate::character::BodySize;
use crate::humanoid::skirt::Skirt;
use crate::humanoid::{Bone, HumanoidLook, HumanoidSpec, Paint, Part, Shape};
use crate::quadruped::{QBone, QuadrupedLook, QuadrupedSpec};
use crate::textures::Tex;
use crate::tuning::*;
use crate::weapon::{ModelSource, Styled, WeaponClass, WeaponDef};

const INK: Color = Color::srgb(0.05, 0.045, 0.07);
const PARCHMENT: Color = Color::srgb(0.78, 0.72, 0.58);
const INK_GLOW: LinearRgba = LinearRgba::rgb(1.2, 0.35, 3.0);

// ── Weapons ──────────────────────────────────────────────────────────────

const QUILL_KNIFE_PARTS: [Styled; 4] = [
    Styled::new(Shape::Cylinder(0.02, 0.18), 0.0, Paint::Textured(Color::srgb(0.2, 0.12, 0.08), Tex::Leather)),
    Styled::new(Shape::Box(Vec3::new(0.09, 0.025, 0.03)), 0.1, Paint::Metal(Color::srgb(0.35, 0.3, 0.25))),
    // A narrow, ink-black blade cut like a nib.
    Styled::new(Shape::Box(Vec3::new(0.035, 0.5, 0.01)), 0.36, Paint::Metal(Color::srgb(0.12, 0.11, 0.14))),
    Styled::at(Shape::Cone(0.018, 0.12), Vec3::new(0.0, 0.66, 0.0), Vec3::ZERO, Paint::Metal(Color::srgb(0.12, 0.11, 0.14))),
];

/// The Scribe's quill-knife: short, quick.
pub static QUILL_KNIFE: WeaponDef = WeaponDef {
    name: "Quill-Knife",
    class: WeaponClass::LightSword,
    base_damage: 14.0,
    blade: (0.12, 0.7),
    model: ModelSource::Styled(&QUILL_KNIFE_PARTS),
    guard_block: 0.3,
    stance_damage: 9.0,
};

const QUILL_PARTS: [Styled; 6] = [
    // A shaft of black wood, a silver ferrule, a long steel nib, and the feather along it.
    Styled::new(Shape::Cylinder(0.035, 0.9), 0.0, Paint::Textured(Color::srgb(0.08, 0.06, 0.06), Tex::Leather)),
    Styled::new(Shape::Cylinder(0.05, 0.12), 0.48, Paint::Metal(Color::srgb(0.7, 0.7, 0.75))),
    Styled::new(Shape::Box(Vec3::new(0.09, 1.05, 0.02)), 1.05, Paint::GlowTex(INK_GLOW, Tex::Runes)),
    Styled::at(Shape::Cone(0.06, 0.3), Vec3::new(0.0, 1.72, 0.0), Vec3::ZERO, Paint::Metal(Color::srgb(0.6, 0.6, 0.66))),
    Styled::at(Shape::Box(Vec3::new(0.2, 0.9, 0.012)), Vec3::new(0.1, -0.35, 0.0), Vec3::new(0.0, 0.0, 0.18), Paint::Color(Color::srgb(0.85, 0.82, 0.75))),
    Styled::at(Shape::Box(Vec3::new(0.14, 0.6, 0.012)), Vec3::new(-0.07, -0.3, 0.01), Vec3::new(0.0, 0.0, -0.15), Paint::Color(Color::srgb(0.8, 0.77, 0.7))),
];

/// The Archivist's quill: a lance-long nib on a black shaft (a greatsword's moves). Its reward.
pub static ARCHIVIST_QUILL: WeaponDef = WeaponDef {
    name: "Archivist's Quill",
    class: WeaponClass::Greatsword,
    base_damage: 31.0,
    blade: (0.5, 1.85),
    model: ModelSource::Styled(&QUILL_PARTS),
    guard_block: 0.45,
    stance_damage: 24.0,
};

/// The boss's own: the same quill, bigger and heavier.
pub static ARCHIVIST_QUILL_BOSS: WeaponDef = WeaponDef { name: "The Archivist's Quill", base_damage: 28.0, blade: (0.7, 2.7), stance_damage: 30.0, ..ARCHIVIST_QUILL };

// ── The Scribe ───────────────────────────────────────────────────────────

pub const SCRIBE_PROFILE: FighterProfile = FighterProfile {
    name: "Scribe",
    aggro_range: 9.0,
    leash: 18.0,
    move_factor: 0.95,
    sprint_beyond: 5.0,
    attack_range: 1.8,
    heavy_chance: 0.1,
    extra_swings: (1, 3),
    recover_time: (0.4, 0.9),
    patrol_factor: 0.25,
    specials: &[Special::new(AbilityId::DashStrike, (4.0, 9.0), 0.35, 7.0)],
    stalker: false,
    enrage: None,
};

fn scribe() -> HumanoidSpec {
    let robe = Color::srgb(0.12, 0.11, 0.13);
    let extras = vec![
        // A deep hood, ink-stained fingers, spectacles glinting, a satchel of scrolls.
        Part::new(Bone::Head, Shape::Ball(0.15), Vec3::new(0.0, 0.118, 0.05), Vec3::ZERO, Paint::Textured(robe, Tex::Cloth)),
        Part::new(Bone::Head, Shape::Ring(0.008, 0.035), Vec3::new(-0.045, 0.09, -0.115), Vec3::new(FRAC_PI_2, 0.0, 0.0), Paint::Glow(LinearRgba::rgb(2.5, 2.2, 1.5))),
        Part::new(Bone::Head, Shape::Ring(0.008, 0.035), Vec3::new(0.045, 0.09, -0.115), Vec3::new(FRAC_PI_2, 0.0, 0.0), Paint::Glow(LinearRgba::rgb(2.5, 2.2, 1.5))),
        Part::new(Bone::HandL, Shape::Ball(0.05), Vec3::ZERO, Vec3::ZERO, Paint::Color(INK)),
        Part::new(Bone::Pelvis, Shape::Box(Vec3::new(0.16, 0.2, 0.08)), Vec3::new(0.2, 0.0, 0.05), Vec3::new(0.0, 0.0, 0.2), Paint::Textured(Color::srgb(0.3, 0.2, 0.12), Tex::Leather)),
        Part::new(Bone::Pelvis, Shape::Cylinder(0.03, 0.26), Vec3::new(0.2, 0.15, 0.05), Vec3::new(0.0, 0.0, 0.5), Paint::Color(PARCHMENT)),
        // A stoop: a hunched collar.
        Part::new(Bone::Chest, Shape::Ball(0.13), Vec3::new(0.0, 0.32, 0.07), Vec3::ZERO, Paint::Textured(robe, Tex::Cloth)),
    ];
    HumanoidSpec {
        look: HumanoidLook {
            skin: Color::srgb(0.5, 0.46, 0.4),
            top: robe,
            bottom: robe,
            hair: Color::srgb(0.6, 0.6, 0.6),
            eye_glow: Some(LinearRgba::rgb(3.0, 2.4, 1.2)),
            bald: true,
            extras,
            skirts: vec![Skirt::robe(0.1, Vec2::new(0.19, 0.12), Vec2::new(0.28, 0.25), 0.92, Paint::Textured(robe, Tex::Cloth))],
            ..HumanoidSpec::hollow().look
        },
        ..HumanoidSpec::hollow()
    }
    .scaled(0.94)
}

pub const SCRIBE: EnemyDef = EnemyDef {
    name: "Scribe",
    hp: 85.0,
    stamina: 120.0,
    stance: 22.0,
    light: 55,
    backstab: true,
    // A person's capsule (bodies stand on it); the body itself is a little smaller.
    size: BodySize { radius: PLAYER_RADIUS, length: PLAYER_CAPSULE_LENGTH },
    target_height: 0.5,
    body: Body::Humanoid(scribe),
    weapon: &QUILL_KNIFE,
    brain: Brain::Fighter(&SCRIBE_PROFILE),
    boss: false,
};

// ── The Page Hound ───────────────────────────────────────────────────────

fn page_hound() -> QuadrupedSpec {
    let mut spec = QuadrupedSpec::warg();
    let mut extras = spec.look.extras.clone();
    // Pages bristling along its back like hackles, ink dripping from the jaws.
    for i in 0..7 {
        let z = -0.35 + 0.12 * i as f32;
        extras.push(Part::new(QBone::Chest, Shape::Box(Vec3::new(0.16, 0.2, 0.01)), Vec3::new(0.0, 0.2, z), Vec3::new(-0.6, (i as f32 * 1.7).sin() * 0.4, 0.0), Paint::Color(PARCHMENT)));
    }
    extras.push(Part::new(QBone::Head, Shape::Cone(0.02, 0.12), Vec3::new(0.0, -0.1, -0.2), Vec3::new(std::f32::consts::PI, 0.0, 0.0), Paint::Color(INK)));
    spec.look = QuadrupedLook {
        fur: PARCHMENT,
        dark: INK,
        belly: Color::srgb(0.6, 0.55, 0.45),
        claws: INK,
        eye_glow: INK_GLOW * 3.0,
        shag: 18.0,
        shag_length: 0.14,
        fur_tex: Some(Tex::Bone),
        mane: false,
        extras,
    };
    spec.scaled(0.82)
}

pub const PAGE_HOUND: EnemyDef = EnemyDef {
    name: "Page Hound",
    hp: 110.0,
    stamina: 160.0,
    stance: 32.0,
    light: 75,
    backstab: false,
    size: BodySize { radius: PLAYER_RADIUS, length: PLAYER_CAPSULE_LENGTH },
    target_height: warg::WARG_TARGET_HEIGHT * 0.8,
    body: Body::Quadruped(page_hound),
    weapon: &crate::weapon::warg_fangs::WARG_FANGS,
    brain: Brain::Beast(&warg::PROFILE),
    boss: false,
};

// ── The Archivist ────────────────────────────────────────────────────────

pub const ARCHIVIST_PROFILE: FighterProfile = FighterProfile {
    name: "Archivist",
    aggro_range: 22.0,
    leash: 60.0,
    move_factor: 0.75,
    sprint_beyond: 9.0,
    attack_range: 3.8,
    heavy_chance: 0.25,
    extra_swings: (1, 2),
    recover_time: (0.8, 1.5),
    patrol_factor: 0.0,
    specials: &[
        Special::new(AbilityId::InkVolley, (5.0, 18.0), 0.5, 5.5),
        Special::new(AbilityId::TomeSlam, (0.0, 5.5), 0.35, 8.0),
        Special::new(AbilityId::PageStorm, (0.0, 5.0), 0.3, 9.0),
        Special::new(AbilityId::DashStrike, (7.0, 16.0), 0.4, 7.0),
        Special::new(AbilityId::Hush, (0.0, 8.0), 0.35, 10.0).enraged(),
        Special::new(AbilityId::InkVolley, (0.0, 18.0), 0.45, 4.0).enraged(),
    ],
    stalker: false,
    enrage: Some(Enrage { below: 0.5, roar: Some(AbilityId::Hush), speed: 1.2, recover: 0.6 }),
};

fn archivist() -> HumanoidSpec {
    let robe = Color::srgb(0.1, 0.08, 0.14);
    let gold = Color::srgb(0.75, 0.6, 0.3);
    let mut extras = vec![
        // A tall mitre-like hood, a mask of a closed book for a face, glowing ink eyes behind it.
        Part::new(Bone::Head, Shape::Ball(0.16), Vec3::new(0.0, 0.12, 0.04), Vec3::ZERO, Paint::Textured(robe, Tex::Cloth)),
        Part::new(Bone::Head, Shape::Cone(0.12, 0.42), Vec3::new(0.0, 0.36, 0.06), Vec3::new(0.15, 0.0, 0.0), Paint::Textured(robe, Tex::Cloth)),
        Part::new(Bone::Head, Shape::Box(Vec3::new(0.2, 0.24, 0.05)), Vec3::new(0.0, 0.1, -0.12), Vec3::ZERO, Paint::Textured(Color::srgb(0.35, 0.08, 0.06), Tex::Leather)),
        Part::new(Bone::Head, Shape::Box(Vec3::new(0.21, 0.02, 0.055)), Vec3::new(0.0, 0.17, -0.12), Vec3::ZERO, Paint::Metal(gold)),
        Part::new(Bone::Head, Shape::Box(Vec3::new(0.21, 0.02, 0.055)), Vec3::new(0.0, 0.03, -0.12), Vec3::ZERO, Paint::Metal(gold)),
        // Chains of keys and quills round the chest; the great tome hung at the hip.
        Part::new(Bone::Chest, Shape::Ring(0.015, 0.28), Vec3::new(0.0, 0.22, 0.0), Vec3::new(0.25, 0.0, 0.0), Paint::Metal(gold)),
        Part::new(Bone::Pelvis, Shape::Box(Vec3::new(0.12, 0.5, 0.4)), Vec3::new(-0.32, -0.1, 0.0), Vec3::ZERO, Paint::Textured(Color::srgb(0.3, 0.07, 0.05), Tex::Leather)),
        Part::new(Bone::Pelvis, Shape::Box(Vec3::new(0.1, 0.46, 0.36)), Vec3::new(-0.27, -0.1, 0.0), Vec3::ZERO, Paint::Color(PARCHMENT)),
        // A halo of slowly floating pages behind the head.
        Part::new(Bone::Chest, Shape::Ring(0.012, 0.45), Vec3::new(0.0, 0.55, 0.25), Vec3::new(FRAC_PI_2, 0.0, 0.0), Paint::Glow(INK_GLOW)),
    ];
    for i in 0..6 {
        let a = i as f32 / 6.0 * std::f32::consts::TAU;
        extras.push(Part::new(
            Bone::Chest,
            Shape::Box(Vec3::new(0.12, 0.16, 0.005)),
            Vec3::new(0.45 * a.cos(), 0.55 + 0.45 * a.sin(), 0.26),
            Vec3::new(0.0, 0.0, a),
            Paint::Color(PARCHMENT),
        ));
    }
    HumanoidSpec {
        look: HumanoidLook {
            skin: Color::srgb(0.3, 0.28, 0.3),
            top: robe,
            bottom: robe,
            hair: Color::BLACK,
            eye_glow: Some(INK_GLOW * 3.0),
            bald: true,
            extras,
            skirts: vec![
                Skirt::robe(0.1, Vec2::new(0.22, 0.14), Vec2::new(0.36, 0.33), 1.0, Paint::Textured(robe, Tex::Cloth)),
                Skirt { bone: Bone::Chest, top: 0.44, waist: Vec2::new(0.26, 0.16), hem: Vec2::new(0.4, 0.34), length: 1.25, span: 1.8, paint: Paint::Textured(Color::srgb(0.25, 0.05, 0.08), Tex::Cloth) },
            ],
            ..HumanoidSpec::hollow().look
        },
        ..HumanoidSpec::hollow()
    }
    .scaled(1.55)
}

pub const ARCHIVIST: EnemyDef = EnemyDef {
    name: "The Archivist",
    hp: 1600.0,
    stamina: 700.0,
    stance: 150.0,
    light: 4000,
    backstab: false,
    size: BodySize { radius: 0.6, length: 1.6 },
    target_height: 0.95,
    body: Body::Humanoid(archivist),
    weapon: &ARCHIVIST_QUILL_BOSS,
    brain: Brain::Fighter(&ARCHIVIST_PROFILE),
    boss: true,
};
