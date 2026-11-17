//! Every enemy kind as data: an [`EnemyDef`] per [`EnemyKind`] — its body (humanoid or
//! quadruped spec), capsule, health, stance, Light, weapon and brain (a fighter or beast profile).
//! [`spawn_enemy`] builds any of them; [`attach_body`] gives a body to an entity that already
//! exists (also used for online puppets, so a guest's copy looks and swings the same).
//!
//! Adding an enemy: a variant in `EnemyKind`, its `EnemyDef` in [`EnemyKind::def`] (a spec, a
//! profile, a weapon), and name it in a level file.

use bevy::prelude::*;
use std::f32::consts::{FRAC_PI_2, PI};

use super::beast::{BeastAi, BeastProfile};
use super::fighter::{Enrage, FighterAi, FighterProfile, Special};
use super::{Enemy, EnemyKind, warg};
use crate::ability::AbilityId;
use crate::character::stats::Stance;
use crate::character::{BodySize, character_bundle_sized};
use crate::character::controller::CharacterController;
use crate::combat::{Hurtbox, Team};
use crate::humanoid::anim::HumanoidAnimator;
use crate::humanoid::skirt::Skirt;
use crate::humanoid::{Bone, HumanoidLook, HumanoidSpec, Paint, Part, Shape, spawn_humanoid, symmetric};
use crate::lockon::Targetable;
use crate::quadruped::anim::QuadrupedAnimator;
use crate::quadruped::{QBone, QuadrupedSpec, spawn_quadruped};
use crate::textures::Tex;
use crate::tuning::*;
use crate::weapon::{EquipWeapon, ModelSource, WeaponClass, WeaponDef, Wielded, axe, claws, greatsword, hollow_blade, warg_fangs};

/// What an enemy's body is.
#[derive(Clone, Copy, Debug)]
pub enum Body {
    Humanoid(fn() -> HumanoidSpec),
    Quadruped(fn() -> QuadrupedSpec),
}

/// Which brain drives it.
#[derive(Clone, Copy, Debug)]
pub enum Brain {
    Fighter(&'static FighterProfile),
    Beast(&'static BeastProfile),
}

/// Everything about one kind of enemy.
#[derive(Clone, Copy, Debug)]
pub struct EnemyDef {
    pub name: &'static str,
    pub hp: f32,
    pub stamina: f32,
    /// Stance: how much stance damage breaks it (`character::stats::Stance`; a break stuns it).
    pub stance: f32,
    /// Light every player gets when it dies (`crate::light`): the harder, the more.
    pub light: u32,
    /// A small humanoid: can be backstabbed (`crate::critical`).
    pub backstab: bool,
    pub size: BodySize,
    /// Lock-on aim / health bar height above the capsule centre.
    pub target_height: f32,
    pub body: Body,
    pub weapon: &'static WeaponDef,
    pub brain: Brain,
    /// A boss: its name on the big health bar, a fog gate, "vanquished" once killed.
    pub boss: bool,
}

/// A boss (see `super::boss`).
#[derive(Component, Debug, Clone, Copy)]
pub struct Boss {
    pub name: &'static str,
}

impl EnemyKind {
    /// Every kind (tests walk the table).
    #[cfg_attr(not(test), allow(dead_code))]
    pub const ALL: [EnemyKind; 6] = [EnemyKind::Hollow, EnemyKind::Warg, EnemyKind::Acolyte, EnemyKind::Mourner, EnemyKind::Contorted, EnemyKind::Saint];

    pub fn def(self) -> &'static EnemyDef {
        match self {
            EnemyKind::Hollow => &HOLLOW,
            EnemyKind::Warg => &WARG,
            EnemyKind::Acolyte => &ACOLYTE,
            EnemyKind::Mourner => &MOURNER,
            EnemyKind::Contorted => &CONTORTED,
            EnemyKind::Saint => &SAINT,
        }
    }
}

const STANDARD: BodySize = BodySize { radius: PLAYER_RADIUS, length: PLAYER_CAPSULE_LENGTH };

// ── The Hollow ───────────────────────────────────────────────────────────

pub const HOLLOW_PROFILE: FighterProfile = FighterProfile {
    name: "Hollow",
    aggro_range: HOLLOW_AGGRO_RANGE,
    leash: HOLLOW_LEASH,
    move_factor: HOLLOW_MOVE_FACTOR,
    sprint_beyond: f32::INFINITY,
    attack_range: HOLLOW_ATTACK_RANGE,
    heavy_chance: HOLLOW_HEAVY_CHANCE,
    extra_swings: (0, 1),
    recover_time: HOLLOW_RECOVER_TIME,
    patrol_factor: HOLLOW_PATROL_FACTOR,
    specials: &[],
    stalker: false,
    enrage: None,
};

const HOLLOW: EnemyDef = EnemyDef {
    name: "Hollow",
    hp: HOLLOW_HP,
    stamina: HOLLOW_STAMINA,
    stance: 25.0,
    light: 40,
    backstab: true,
    size: STANDARD,
    target_height: 0.55,
    body: Body::Humanoid(HumanoidSpec::hollow),
    weapon: &hollow_blade::HOLLOW_SWORD,
    brain: Brain::Fighter(&HOLLOW_PROFILE),
    boss: false,
};

const WARG: EnemyDef = EnemyDef {
    name: "Warg",
    hp: WARG_HP,
    stamina: WARG_STAMINA,
    // A single light hit doesn't break its pounce wind-up any more (ROADMAP 8.9's open question).
    stance: 40.0,
    light: 90,
    backstab: false,
    size: STANDARD,
    target_height: warg::WARG_TARGET_HEIGHT,
    body: Body::Quadruped(QuadrupedSpec::warg),
    weapon: &warg_fangs::WARG_FANGS,
    brain: Brain::Beast(&warg::PROFILE),
    boss: false,
};

// ── The Ashen Acolyte: a hooded cathedral Hollow with a hatchet ─────────

const ACOLYTE_PROFILE: FighterProfile = FighterProfile {
    name: "Ashen Acolyte",
    aggro_range: 11.0,
    leash: 20.0,
    move_factor: 0.8,
    sprint_beyond: 7.0,
    attack_range: 2.0,
    heavy_chance: 0.2,
    extra_swings: (1, 2),
    recover_time: (0.6, 1.3),
    patrol_factor: 0.3,
    specials: &[],
    stalker: false,
    enrage: None,
};

fn acolyte() -> HumanoidSpec {
    let ash = Color::srgb(0.24, 0.22, 0.2);
    let mut extras = vec![
        // A pointed hood: a cowl round the head (the face looking out of it) and its peak.
        Part::new(Bone::Head, Shape::Ball(0.162), Vec3::new(0.0, 0.126, 0.054), Vec3::ZERO, Paint::Textured(ash, Tex::Cloth)),
        Part::new(Bone::Head, Shape::Cone(0.1, 0.26), Vec3::new(0.0, 0.3, 0.07), Vec3::new(0.3, 0.0, 0.0), Paint::Textured(ash, Tex::Cloth)),
        // A censer chain belt and a bone pendant.
        Part::new(Bone::Pelvis, Shape::Ring(0.015, 0.2), Vec3::new(0.0, 0.1, 0.0), Vec3::ZERO, Paint::Metal(Color::srgb(0.3, 0.28, 0.24))),
        Part::new(Bone::Chest, Shape::Box(Vec3::new(0.06, 0.1, 0.02)), Vec3::new(0.0, 0.3, -0.13), Vec3::ZERO, Paint::Textured(Color::srgb(0.85, 0.8, 0.7), Tex::Bone)),
    ];
    extras.extend(symmetric(&[Part::new(
        Bone::UpperArmR,
        Shape::Box(Vec3::new(0.13, 0.06, 0.13)),
        Vec3::new(0.0, 0.02, 0.0),
        Vec3::new(0.0, 0.0, -0.3),
        Paint::Textured(Color::srgb(0.2, 0.18, 0.16), Tex::Leather),
    )]));
    HumanoidSpec {
        look: HumanoidLook {
            skin: Color::srgb(0.38, 0.36, 0.33),
            top: Color::srgb(0.2, 0.19, 0.17),
            bottom: Color::srgb(0.16, 0.15, 0.13),
            hair: Color::srgb(0.1, 0.1, 0.1),
            eye_glow: Some(LinearRgba::rgb(2.0, 4.5, 6.0)),
            bald: true,
            extras,
            // The long robe, down to the ankles.
            skirts: vec![Skirt::robe(0.1, Vec2::new(0.2, 0.13), Vec2::new(0.3, 0.27), 0.98, Paint::Textured(ash, Tex::Cloth))],
            ..HumanoidSpec::hollow().look
        },
        ..HumanoidSpec::hollow()
    }
}

const ACOLYTE: EnemyDef = EnemyDef {
    name: "Ashen Acolyte",
    hp: 110.0,
    stamina: 110.0,
    stance: 30.0,
    light: 70,
    backstab: true,
    size: STANDARD,
    target_height: 0.55,
    body: Body::Humanoid(acolyte),
    weapon: &axe::WOODCUTTER_HATCHET,
    brain: Brain::Fighter(&ACOLYTE_PROFILE),
    boss: false,
};

// ── The Mourner: a veiled stalker that moves only when unwatched ────────

const MOURNER_PROFILE: FighterProfile = FighterProfile {
    name: "Mourner",
    aggro_range: 16.0,
    leash: 32.0,
    move_factor: 0.95,
    sprint_beyond: 4.0,
    attack_range: 2.4,
    heavy_chance: 0.25,
    extra_swings: (2, 3),
    recover_time: (0.4, 0.9),
    patrol_factor: 0.2,
    specials: &[
        Special::new(AbilityId::Shriek, (0.0, 5.0), 0.5, 9.0),
        Special::new(AbilityId::RendingTurn, (0.0, 2.6), 0.45, 6.0),
        Special::new(AbilityId::DashStrike, (5.0, 10.0), 0.6, 7.0),
    ],
    stalker: true,
    enrage: None,
};

fn mourner() -> HumanoidSpec {
    let bone = Color::srgb(0.86, 0.84, 0.78);
    let black = Color::srgb(0.03, 0.025, 0.03);
    let mut extras = vec![
        // A tall hood round the head (its veil hangs past the shoulders, below), a blank bone
        // mask in its opening, a glowing slit for a mouth.
        Part::new(Bone::Head, Shape::Ball(0.15), Vec3::new(0.0, 0.11, 0.05), Vec3::ZERO, Paint::Textured(black, Tex::Cloth)),
        Part::new(Bone::Head, Shape::Cone(0.09, 0.3), Vec3::new(0.0, 0.3, 0.08), Vec3::new(0.35, 0.0, 0.0), Paint::Textured(black, Tex::Cloth)),
        Part::new(Bone::Head, Shape::Box(Vec3::new(0.15, 0.21, 0.03)), Vec3::new(0.0, 0.1, -0.11), Vec3::new(0.1, 0.0, 0.0), Paint::Textured(bone, Tex::Bone)),
        Part::new(Bone::Head, Shape::Box(Vec3::new(0.014, 0.12, 0.01)), Vec3::new(0.0, 0.05, -0.13), Vec3::new(0.1, 0.0, 0.0), Paint::Glow(LinearRgba::rgb(9.0, 0.6, 0.3))),
    ];
    // Vertebrae pushing through the back of the robe.
    for (i, y) in [0.08, 0.2, 0.32, 0.44].into_iter().enumerate() {
        extras.push(Part::new(Bone::Chest, Shape::Cone(0.03, 0.12 + i as f32 * 0.02), Vec3::new(0.0, y, 0.1), Vec3::new(1.2, 0.0, 0.0), Paint::Textured(bone, Tex::Bone)));
    }
    // Long hooked fingers, strips of shroud hanging from the arms.
    let mut side = vec![];
    for (x, len) in [(-0.025, 0.26), (0.0, 0.32), (0.025, 0.28)] {
        side.push(Part::new(Bone::HandR, Shape::Cone(0.012, len), Vec3::new(x, -0.05 - len * 0.5, -0.01), Vec3::new(PI - 0.15, 0.0, 0.0), Paint::Textured(bone, Tex::Bone)));
    }
    side.push(Part::new(Bone::UpperArmR, Shape::Box(Vec3::new(0.02, 0.45, 0.1)), Vec3::new(0.04, -0.3, 0.03), Vec3::new(0.0, 0.0, 0.1), Paint::Textured(black, Tex::Cloth)));
    side.push(Part::new(Bone::ForearmR, Shape::Box(Vec3::new(0.02, 0.35, 0.08)), Vec3::new(0.03, -0.3, 0.03), Vec3::new(0.0, 0.0, 0.08), Paint::Textured(black, Tex::Cloth)));
    extras.extend(symmetric(&side));
    HumanoidSpec {
        pelvis_height: 1.08,
        spine: 0.12,
        shoulder_height: 0.4,
        shoulder_x: 0.22,
        neck_height: 0.5,
        upper_arm: 0.38,
        forearm: 0.42,
        hip_x: 0.1,
        thigh: 0.5,
        shin: 0.5,
        torso: Vec3::new(0.3, 0.5, 0.17),
        head_radius: 0.1,
        arm_radius: 0.035,
        leg_radius: 0.05,
        scale: 1.0,
        look: HumanoidLook {
            skin: bone,
            top: black,
            bottom: black,
            boots: Color::srgb(0.6, 0.58, 0.52),
            hair: black,
            eye_glow: None,
            skin_tex: Some(Tex::Bone),
            top_tex: Some(Tex::Cloth),
            bottom_tex: Some(Tex::Cloth),
            bald: true,
            extras,
            skirts: vec![
                // The robe it glides in, trailing on the floor.
                Skirt::robe(0.1, Vec2::new(0.16, 0.1), Vec2::new(0.36, 0.34), 1.16, Paint::Textured(black, Tex::Cloth)),
                // The hood's veil, hanging down the back past the shoulders.
                Skirt { bone: Bone::Chest, top: 0.54, waist: Vec2::new(0.1, 0.09), hem: Vec2::new(0.24, 0.2), length: 0.5, span: 2.0, paint: Paint::Textured(black, Tex::Cloth) },
            ],
            ..HumanoidSpec::human().look
        },
    }
    .scaled(1.12)
}

const MOURNER: EnemyDef = EnemyDef {
    name: "Mourner",
    hp: 280.0,
    stamina: 300.0,
    stance: 50.0,
    light: 200,
    backstab: false,
    size: BodySize { radius: 0.38, length: 1.65 },
    target_height: 0.9,
    body: Body::Humanoid(mourner),
    weapon: &claws::MOURNER_CLAWS,
    brain: Brain::Fighter(&MOURNER_PROFILE),
    boss: false,
};

// ── The Contorted: a body bent backwards, scuttling on all fours ────────

const CONTORTED_PROFILE: BeastProfile = BeastProfile {
    name: "Contorted",
    aggro_range: 14.0,
    leash: 26.0,
    chase_factor: 1.1,
    sprint_beyond: 3.0,
    bite_range: 2.0,
    combo: (2, 4),
    pounce_range: (2.5, 5.5),
    pounce_chance: 0.6,
    pounce_cooldown: 4.0,
    windup_turn: 3.6,
    recover_time: (0.2, 0.6),
    circle_distance: 2.8,
    patrol_factor: 0.4,
};

/// Its jaw: the beast moveset (bites and the grab), a weaker bite than the warg's.
pub static CONTORTED_MAW: WeaponDef = WeaponDef {
    name: "Contorted Maw",
    class: WeaponClass::WargFangs,
    base_damage: 15.0,
    blade: (0.0, 0.0),
    model: ModelSource::Parts(&[]),
    guard_block: 0.0,
    stance_damage: 20.0,
};

fn contorted() -> QuadrupedSpec {
    // Pale, veined, corpse-like skin (the flesh texture under a pale tint).
    let skin = Color::srgb(1.0, 0.93, 0.9);
    let hair = Color::srgb(0.02, 0.02, 0.02);
    let mut extras = vec![
        // An upside-down face under the skull: a wide glowing grin, the chin on top.
        Part::new(QBone::Head, Shape::Box(Vec3::new(0.15, 0.025, 0.02)), Vec3::new(0.0, 0.07, -0.12), Vec3::ZERO, Paint::Glow(LinearRgba::rgb(7.0, 0.8, 0.4))),
        Part::new(QBone::Head, Shape::Ball(0.08), Vec3::new(0.0, 0.11, -0.04), Vec3::ZERO, Paint::Skin),
        // The spine bowed up through the skin, ribs showing.
        Part::new(QBone::Hips, Shape::Cone(0.04, 0.16), Vec3::new(0.0, 0.2, -0.1), Vec3::new(-0.2, 0.0, 0.0), Paint::Textured(Color::srgb(0.9, 0.87, 0.8), Tex::Bone)),
        Part::new(QBone::Hips, Shape::Cone(0.04, 0.2), Vec3::new(0.0, 0.22, -0.32), Vec3::new(-0.1, 0.0, 0.0), Paint::Textured(Color::srgb(0.9, 0.87, 0.8), Tex::Bone)),
        Part::new(QBone::Chest, Shape::Cone(0.04, 0.17), Vec3::new(0.0, 0.2, 0.05), Vec3::new(0.15, 0.0, 0.0), Paint::Textured(Color::srgb(0.9, 0.87, 0.8), Tex::Bone)),
    ];
    // Long black hair hanging from the head to the floor.
    for i in 0..9 {
        let x = -0.1 + 0.025 * i as f32;
        let len = 0.45 + 0.12 * ((i * 7 % 5) as f32 / 5.0);
        extras.push(Part::new(QBone::Head, Shape::Box(Vec3::new(0.018, len, 0.012)), Vec3::new(x, -len * 0.5, 0.02 + 0.01 * (i % 3) as f32), Vec3::new(0.12, 0.0, x * 0.8), Paint::Color(hair)));
    }
    for side in [-1.0, 1.0] {
        for k in 0..3 {
            extras.push(Part::new(
                QBone::Chest,
                Shape::Box(Vec3::new(0.012, 0.012, 0.22)),
                Vec3::new(side * 0.17, -0.02 - 0.06 * k as f32, 0.02),
                Vec3::new(0.0, side * 0.35, FRAC_PI_2 * 0.15 * side),
                Paint::Textured(Color::srgb(0.9, 0.86, 0.78), Tex::Bone),
            ));
        }
    }
    QuadrupedSpec {
        body_length: 0.7,
        hips_back: 0.4,
        front_upper: 0.5,
        front_lower: 0.52,
        hind_upper: 0.5,
        hind_lower: 0.52,
        leg_x: 0.19,
        chest_radius: 0.21,
        hips_radius: 0.18,
        neck: Vec2::new(0.08, 0.22),
        head_radius: 0.13,
        snout: 0.06,
        tail: 0.01,
        leg_radius: 0.04,
        look: crate::quadruped::QuadrupedLook {
            fur: skin,
            dark: hair,
            belly: Color::srgb(0.7, 0.64, 0.6),
            claws: Color::srgb(0.25, 0.2, 0.18),
            eye_glow: LinearRgba::rgb(6.0, 7.0, 9.0),
            shag: 0.0,
            shag_length: 0.0,
            fur_tex: Some(Tex::Flesh),
            mane: false,
            extras,
        },
    }
}

const CONTORTED: EnemyDef = EnemyDef {
    name: "Contorted",
    hp: 220.0,
    stamina: 200.0,
    stance: 55.0,
    light: 160,
    backstab: false,
    size: STANDARD,
    target_height: 0.25,
    body: Body::Quadruped(contorted),
    weapon: &CONTORTED_MAW,
    brain: Brain::Beast(&CONTORTED_PROFILE),
    boss: false,
};

// ── The Thorned Saint (boss) ─────────────────────────────────────────────

const SAINT_PROFILE: FighterProfile = FighterProfile {
    name: "Thorned Saint",
    aggro_range: 20.0,
    leash: 60.0,
    move_factor: 0.8,
    sprint_beyond: 8.0,
    attack_range: 3.6,
    heavy_chance: 0.2,
    extra_swings: (1, 2),
    recover_time: (0.9, 1.7),
    patrol_factor: 0.0,
    specials: &[
        Special::new(AbilityId::Whirlwind, (0.0, 4.5), 0.35, 8.0),
        Special::new(AbilityId::DashStrike, (6.0, 15.0), 0.7, 6.0),
        Special::new(AbilityId::GroundSlam, (0.0, 4.8), 0.35, 9.0),
        Special::new(AbilityId::Shriek, (0.0, 6.0), 0.3, 11.0).enraged(),
        Special::new(AbilityId::Whirlwind, (0.0, 6.0), 0.4, 7.0).enraged(),
    ],
    stalker: false,
    enrage: Some(Enrage { below: 0.5, roar: Some(AbilityId::Shriek), speed: 1.25, recover: 0.55 }),
};

fn saint() -> HumanoidSpec {
    let iron = Color::srgb(0.3, 0.29, 0.3);
    let crimson = Color::srgb(0.35, 0.04, 0.05);
    let ember = LinearRgba::rgb(8.0, 1.6, 0.3);
    let mut extras = vec![
        // A crown of thorns and, behind the head, an iron halo set with glowing thorns.
        Part::new(Bone::Head, Shape::Ring(0.018, 0.13), Vec3::new(0.0, 0.2, 0.0), Vec3::ZERO, Paint::Textured(Color::srgb(0.2, 0.14, 0.08), Tex::Leather)),
        Part::new(Bone::Head, Shape::Ring(0.02, 0.32), Vec3::new(0.0, 0.18, 0.2), Vec3::new(FRAC_PI_2, 0.0, 0.0), Paint::TexturedMetal(iron, Tex::Rust)),
        // A tabard over the mail (its skirt and the cape are cloth, below).
        Part::new(Bone::Chest, Shape::Box(Vec3::new(0.34, 0.5, 0.03)), Vec3::new(0.0, 0.2, -0.17), Vec3::ZERO, Paint::Textured(crimson, Tex::Cloth)),
        Part::new(Bone::Chest, Shape::Box(Vec3::new(0.46, 0.12, 0.3)), Vec3::new(0.0, 0.44, 0.0), Vec3::ZERO, Paint::TexturedMetal(iron, Tex::Rust)),
        Part::new(Bone::Pelvis, Shape::Ring(0.03, 0.22), Vec3::new(0.0, 0.08, 0.0), Vec3::ZERO, Paint::Metal(Color::srgb(0.5, 0.4, 0.2))),
    ];
    for i in 0..10 {
        let a = i as f32 / 10.0 * std::f32::consts::TAU;
        let (s, c) = a.sin_cos();
        extras.push(Part::new(Bone::Head, Shape::Cone(0.012, 0.09), Vec3::new(s * 0.13, 0.22 + 0.02 * c, c * 0.13), Vec3::new(c * 0.6, 0.0, -s * 0.6), Paint::Color(Color::srgb(0.25, 0.17, 0.1))));
        extras.push(Part::new(Bone::Head, Shape::Cone(0.02, 0.12), Vec3::new(s * 0.36, 0.18 + c * 0.36, 0.2), Vec3::new(0.0, 0.0, -a), Paint::Glow(ember)));
    }
    // Spiked pauldrons and gauntlets.
    extras.extend(symmetric(&[
        Part::new(Bone::UpperArmR, Shape::Ball(0.13), Vec3::new(0.02, 0.0, 0.0), Vec3::ZERO, Paint::TexturedMetal(iron, Tex::Rust)),
        Part::new(Bone::UpperArmR, Shape::Cone(0.04, 0.18), Vec3::new(0.08, 0.1, 0.0), Vec3::new(0.0, 0.0, -0.6), Paint::Metal(iron)),
        Part::new(Bone::UpperArmR, Shape::Cone(0.035, 0.14), Vec3::new(0.1, 0.0, 0.06), Vec3::new(0.5, 0.0, -1.0), Paint::Metal(iron)),
        Part::new(Bone::ForearmR, Shape::Cylinder(0.065, 0.2), Vec3::new(0.0, -0.18, 0.0), Vec3::ZERO, Paint::TexturedMetal(iron, Tex::Rust)),
        Part::new(Bone::ShinR, Shape::Cylinder(0.08, 0.3), Vec3::new(0.0, -0.2, 0.0), Vec3::ZERO, Paint::TexturedMetal(iron, Tex::Rust)),
    ]));
    HumanoidSpec {
        torso: Vec3::new(0.52, 0.5, 0.3),
        head_radius: 0.12,
        arm_radius: 0.07,
        leg_radius: 0.095,
        look: HumanoidLook {
            skin: Color::srgb(0.3, 0.27, 0.26),
            top: Color::srgb(0.55, 0.55, 0.57),
            bottom: Color::srgb(0.45, 0.45, 0.47),
            boots: iron,
            hair: iron,
            eye_glow: Some(LinearRgba::rgb(12.0, 5.0, 1.5)),
            skin_tex: Some(Tex::Leather),
            top_tex: Some(Tex::Chainmail),
            bottom_tex: Some(Tex::Chainmail),
            bald: true,
            extras,
            skirts: vec![
                // The tabard's skirt, to the knees, and a tattered cape from the shoulders.
                Skirt::robe(0.1, Vec2::new(0.28, 0.17), Vec2::new(0.36, 0.28), 0.62, Paint::Textured(crimson, Tex::Cloth)),
                Skirt {
                    bone: Bone::Chest,
                    top: 0.42,
                    waist: Vec2::new(0.28, 0.16),
                    hem: Vec2::new(0.34, 0.3),
                    length: 1.2,
                    span: 1.7,
                    paint: Paint::Textured(Color::srgb(0.08, 0.06, 0.06), Tex::Cloth),
                },
            ],
            ..HumanoidSpec::human().look
        },
        ..HumanoidSpec::human()
    }
    .scaled(1.6)
}

const SAINT: EnemyDef = EnemyDef {
    name: "The Thorned Saint",
    hp: 1300.0,
    stamina: 600.0,
    stance: 140.0,
    light: 2500,
    backstab: false,
    size: BodySize { radius: 0.62, length: 1.66 },
    target_height: 1.0,
    body: Body::Humanoid(saint),
    weapon: &greatsword::SAINT_BLADE,
    brain: Brain::Fighter(&SAINT_PROFILE),
    boss: true,
};

// ── Spawning ─────────────────────────────────────────────────────────────

/// Builds an enemy of `kind` standing on `home` (floor point), facing `facing`.
#[allow(clippy::too_many_arguments)]
pub fn spawn_enemy(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    equip: &mut MessageWriter<EquipWeapon>,
    kind: EnemyKind,
    home: Vec3,
    patrol: Option<Vec3>,
    facing: f32,
    seed: u32,
) -> Entity {
    let def = kind.def();
    let tf = Transform::from_translation(home + Vec3::Y * (def.size.half_height() + 0.05)).with_rotation(Quat::from_rotation_y(facing));
    let e = commands
        .spawn((
            Name::new(def.name),
            Enemy { home, patrol, kind },
            Team::Hostile,
            Hurtbox,
            Targetable { height: def.target_height },
            character_bundle_sized(tf, def.hp, def.stamina, def.size),
        ))
        .id();
    match def.brain {
        Brain::Fighter(p) => commands.entity(e).insert(FighterAi::new(p, seed)),
        Brain::Beast(p) => commands.entity(e).insert(BeastAi::new(p, seed)),
    };
    if def.backstab {
        commands.entity(e).insert(crate::critical::Backstabbable);
    }
    if def.stance > 0.0 {
        commands.entity(e).insert(Stance::new(def.stance, true));
    }
    attach_body(commands, meshes, materials, equip, e, kind);
    commands.entity(e).insert(CharacterController { facing, ..default() });
    e
}

/// The body, animator, weapon and boss marker of `kind` on an existing entity (puppets too).
pub fn attach_body(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    equip: &mut MessageWriter<EquipWeapon>,
    entity: Entity,
    kind: EnemyKind,
) {
    let def = kind.def();
    let root = def.size.half_height();
    match def.body {
        Body::Humanoid(spec) => {
            let spec = spec();
            let rig = spawn_humanoid(commands, meshes, materials, entity, &spec, root);
            commands.entity(entity).insert((spec, rig, HumanoidAnimator::default()));
        }
        Body::Quadruped(spec) => {
            let spec = spec();
            let rig = spawn_quadruped(commands, meshes, materials, entity, &spec, root);
            commands.entity(entity).insert((spec, rig, QuadrupedAnimator::default()));
        }
    }
    if def.weapon.class.is_bodily() {
        commands.entity(entity).insert(Wielded::natural(def.weapon));
    } else {
        equip.write(EquipWeapon::right(entity, def.weapon));
    }
    if def.boss {
        commands.entity(entity).insert(Boss { name: def.name });
    }
    commands.entity(entity).insert(def.size);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_has_a_sane_definition() {
        for kind in EnemyKind::ALL {
            let d = kind.def();
            assert!(d.hp > 0.0 && d.stamina > 0.0, "{kind:?}");
            assert!(d.size.half_height() >= PLAYER_HALF_HEIGHT - 1e-4, "{kind:?} smaller than a person");
            if let Body::Humanoid(spec) = d.body {
                // The body stands on its capsule: head no higher than ~0.4 m above the capsule top.
                let s = spec();
                let height = s.pelvis_height + s.spine + s.neck_height + 2.0 * s.head_radius;
                assert!((height - 2.0 * d.size.half_height()).abs() < 0.45, "{kind:?}: body {height:.2} m vs capsule {:.2} m", 2.0 * d.size.half_height());
            }
            if let Brain::Fighter(p) = d.brain {
                assert!(p.specials.len() <= 6, "{kind:?}: too many specials");
            }
        }
    }

    #[test]
    fn the_boss_is_the_only_boss() {
        let bosses: Vec<_> = EnemyKind::ALL.iter().filter(|k| k.def().boss).collect();
        assert_eq!(bosses, vec![&EnemyKind::Saint]);
    }
}

#[cfg(test)]
mod reach_tests {
    use super::*;
    use crate::character::action::{ActionKind, ActiveAction};
    use crate::humanoid::HumanoidRig;

    /// Where a fighter's blade tip goes during its first light swing, relative to its feet
    /// (forward = -Z).
    fn sweep(kind: EnemyKind) -> Vec<Vec3> {
        let d = kind.def();
        let Body::Humanoid(spec) = d.body else { panic!() };
        let spec = spec();
        let rig = HumanoidRig { model: Entity::PLACEHOLDER, bones: [Entity::PLACEHOLDER; crate::humanoid::Bone::COUNT], root_height: d.size.half_height() };
        let tf = Transform::from_xyz(0.0, d.size.half_height(), 0.0);
        let mut a = ActiveAction::new(ActionKind::LightAttack(0), Vec3::NEG_Z, d.weapon.class);
        let (start, end) = (a.def.startup, a.def.startup + a.def.active);
        (0..=8)
            .filter_map(|i| {
                a.elapsed = start + (end - start) * i as f32 / 8.0;
                crate::combat::blade_world(&tf, &spec, &rig, d.weapon, crate::humanoid::Bone::HandR, &a).map(|(_, tip)| tip)
            })
            .collect()
    }

    #[test]
    fn big_blades_reach_a_player_in_front() {
        for kind in [EnemyKind::Hollow, EnemyKind::Acolyte, EnemyKind::Saint] {
            let tips = sweep(kind);
            assert!(!tips.is_empty(), "{kind:?}: no blade pose");
            let reach = tips.iter().map(|t| -t.z).fold(f32::MIN, f32::max);
            println!("{kind:?}: tips {tips:?}");
            assert!(reach > 1.0, "{kind:?}: the swing reaches only {reach:.2} m forward");
            // Somewhere in front the tip passes through where a person stands (below 1.7 m).
            let through = tips.iter().any(|t| (0.8..reach + 0.01).contains(&-t.z) && (0.2..1.7).contains(&t.y));
            assert!(through, "{kind:?}: the swing never passes through a person in front (tips {tips:?})");
        }
    }
}
