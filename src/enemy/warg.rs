//! The warg: a wolf the size of a pony. A quadruped body (`QuadrupedSpec::warg`), its fangs as a
//! natural weapon (`weapon::warg_fangs`: quick bites and the pounce grab) and the beast AI with
//! [`PROFILE`].

use bevy::prelude::*;

use super::beast::{BeastAi, BeastProfile};
use super::{Enemy, EnemyKind};
use crate::character::character_bundle;
use crate::character::controller::CharacterController;
use crate::combat::{Hurtbox, Team};
use crate::lockon::Targetable;
use crate::quadruped::anim::QuadrupedAnimator;
use crate::quadruped::{QuadrupedSpec, spawn_quadruped};
use crate::tuning::*;
use crate::weapon::EquippedWeapon;
use crate::weapon::warg_fangs::WARG_FANGS;

pub const PROFILE: BeastProfile = BeastProfile {
    name: "warg",
    aggro_range: 13.0,
    leash: 24.0,
    chase_factor: 0.95,
    sprint_beyond: 6.0,
    bite_range: 2.1,
    combo: (1, 3),
    pounce_range: (2.2, 4.8),
    pounce_chance: 0.55,
    pounce_cooldown: 5.0,
    windup_turn: 2.6,
    recover_time: (0.35, 0.9),
    circle_distance: 3.2,
    patrol_factor: 0.3,
};

/// Lock-on aim and enemy bar: a little above the capsule centre (the back).
pub const WARG_TARGET_HEIGHT: f32 = 0.2;

/// Spawns a warg standing at `home` (floor point), facing `facing`.
#[allow(clippy::too_many_arguments)]
pub fn spawn_warg(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    home: Vec3,
    patrol: Option<Vec3>,
    facing: f32,
    seed: u32,
) -> Entity {
    let tf = Transform::from_translation(home + Vec3::Y * (PLAYER_HALF_HEIGHT + 0.05)).with_rotation(Quat::from_rotation_y(facing));
    let warg = commands
        .spawn((
            Name::new("Warg"),
            Enemy { home, patrol, kind: EnemyKind::Warg },
            BeastAi::new(&PROFILE, seed),
            Hurtbox,
            Team::Hostile,
            Targetable { height: WARG_TARGET_HEIGHT },
            character_bundle(tf, WARG_HP, WARG_STAMINA),
            EquippedWeapon::natural(&WARG_FANGS),
        ))
        .id();
    attach_warg_body(commands, meshes, materials, warg);
    commands.entity(warg).insert(CharacterController { facing, ..default() });
    warg
}

/// The warg's body and animator on `entity` (also used for online puppets).
pub fn attach_warg_body(commands: &mut Commands, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>, entity: Entity) {
    let spec = QuadrupedSpec::warg();
    let rig = spawn_quadruped(commands, meshes, materials, entity, &spec, PLAYER_HALF_HEIGHT);
    commands.entity(entity).insert((spec, rig, QuadrupedAnimator::default()));
}
