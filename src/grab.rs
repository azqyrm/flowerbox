//! Grabs: a creature's **pounce** (`ActionKind::Pounce`, e.g. the warg's) catches players.
//!
//! - **Wind-up** (`PounceSpec::windup`, the beast leans back): nothing happens yet. The AI keeps
//!   turning toward its target, so stepping aside doesn't help; you have to roll.
//! - **Leap** (`active`): the creature moves through characters ([`PassThroughCharacters`]). Any
//!   player who comes within `GRAB_REACH` of its jaws — standing in the way or at the landing
//!   spot — and isn't invulnerable (a roll's i-frames) is caught: the player's action becomes
//!   `ActionKind::Grabbed` and they are carried along in front of the jaws (`GRAB_HOLD_DISTANCE`).
//!   Invulnerable (rolling) at the moment the jaws first reach you = dodged: that pounce can't
//!   catch you any more, even if you're still in front of it when the i-frames end.
//! - **Bite**: `GRAB_BITE_AT` after being caught the jaws close for `PounceSpec::damage`. It can't be
//!   blocked or parried: a held character has no guard (the guard drops during any action). The
//!   hit staggers (Hurt) or kills, which also lets go.
//! - If the creature stops pouncing first (killed, staggered by someone else's hit), the victim is
//!   let go with a stagger and no bite.
//!
//! Online, a grab is decided on the victim's machine, like every enemy hit: each machine only
//! checks its own player against pouncing creatures (real ones or the host's puppets), and the
//! victim's machine applies the bite. Everyone else sees the replicated `Grabbed` action.

use bevy::prelude::*;

use crate::character::action::{ActionKind, ActionPhase, CharacterAction};
use crate::character::controller::{CharacterController, PassThroughCharacters};
use crate::character::stats::{DamageMessage, Invulnerable};
use crate::character::CharacterSystems;
use crate::combat::Team;
use crate::player::Player;
use crate::tuning::*;

pub struct GrabPlugin;

impl Plugin for GrabPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            (
                (pounce_pass_through, pounce_grabs).chain().in_set(CharacterSystems::Combat),
                hold_grabbed.in_set(CharacterSystems::Stats),
            ),
        );
    }
}

/// On a character held in `by`'s jaws.
#[derive(Component, Debug)]
pub struct Grabbed {
    pub by: Entity,
    bitten: bool,
}

/// Seconds after the leap ends during which a landing still catches (the jaws snapping shut).
const LANDING_GRACE: f32 = 0.06;

fn forward(facing: f32) -> Vec3 {
    Vec3::new(-facing.sin(), 0.0, -facing.cos())
}

/// True while `action` is in the catching part of a pounce (the leap, and the moment of landing).
fn catching(action: &CharacterAction) -> bool {
    action.current.is_some_and(|a| {
        a.kind == ActionKind::Pounce && a.elapsed >= a.def.startup && a.elapsed < a.def.startup + a.def.active + LANDING_GRACE
    })
}

/// Leaping creatures move through characters (and carry whoever they catch).
fn pounce_pass_through(mut commands: Commands, q: Query<(Entity, &CharacterAction, Has<PassThroughCharacters>)>) {
    for (entity, action, passing) in &q {
        let leaping = action.current.is_some_and(|a| a.kind == ActionKind::Pounce && a.phase() == ActionPhase::Active);
        if leaping && !passing {
            commands.entity(entity).insert(PassThroughCharacters);
        } else if !leaping && passing {
            commands.entity(entity).remove::<PassThroughCharacters>();
        }
    }
}

/// Our own player against every leaping creature (local or a remote puppet).
#[allow(clippy::type_complexity)]
fn pounce_grabs(
    mut commands: Commands,
    mut dodged: Local<Vec<(Entity, u32)>>,
    creatures: Query<(Entity, &Transform, &CharacterController, &CharacterAction, Option<&Team>), Without<Player>>,
    mut players: Query<(Entity, &Transform, &mut CharacterAction, Option<&Team>, Has<Invulnerable>, Has<Grabbed>), With<Player>>,
) {
    let Ok((victim, vtf, mut vaction, vteam, invulnerable, held)) = players.single_mut() else { return };
    let dead = vaction.current.is_some_and(|a| a.kind == ActionKind::Dead);
    if held || dead {
        return;
    }
    // Forget dodges of pounces that are over.
    dodged.retain(|(c, serial)| creatures.get(*c).is_ok_and(|(_, _, _, a, _)| a.serial == *serial && catching(a)));
    for (creature, tf, ctrl, action, team) in &creatures {
        if !catching(action) || (team.is_some() && team == vteam) || dodged.contains(&(creature, action.serial)) {
            continue;
        }
        let fwd = forward(ctrl.facing);
        let jaws = tf.translation + fwd * GRAB_HOLD_DISTANCE * 0.7;
        let close = (vtf.translation - jaws).with_y(0.0).length() < GRAB_REACH + PLAYER_RADIUS;
        if !close || (vtf.translation.y - tf.translation.y).abs() > 1.2 {
            continue;
        }
        if invulnerable {
            info!("rolled through a pounce");
            dodged.push((creature, action.serial));
            continue;
        }
        debug!("caught while {:?}", vaction.current.map(|a| (a.kind, a.elapsed)));
        vaction.force(ActionKind::Grabbed, -fwd);
        commands.entity(victim).try_insert(Grabbed { by: creature, bitten: false });
        let into_leap = action.current.map_or(0.0, |a| a.elapsed - a.def.startup);
        info!("grabbed by {creature}! ({into_leap:.2} s into the leap)");
        return;
    }
}

/// Carries held characters in front of the jaws, bites them, lets go.
#[allow(clippy::type_complexity)]
fn hold_grabbed(
    time: Res<Time>,
    mut commands: Commands,
    mut damage: MessageWriter<DamageMessage>,
    grabbers: Query<(&Transform, &CharacterController, &CharacterAction), Without<Grabbed>>,
    mut victims: Query<(Entity, &mut Grabbed, &Transform, &mut CharacterController, &mut CharacterAction)>,
) {
    let dt = time.delta_secs().max(1e-4);
    for (victim, mut grab, tf, mut ctrl, mut action) in &mut victims {
        let Some(held) = action.current.filter(|a| a.kind == ActionKind::Grabbed) else {
            commands.entity(victim).try_remove::<Grabbed>();
            continue;
        };
        let holder = grabbers.get(grab.by).ok().filter(|(_, _, a)| a.current.is_some_and(|c| c.kind == ActionKind::Pounce));
        let Some((gtf, gctrl, gaction)) = holder else {
            // The creature stopped (killed, staggered): dropped, no bite.
            info!("let go");
            action.force(ActionKind::Hurt, Vec3::ZERO);
            commands.entity(victim).try_remove::<Grabbed>();
            continue;
        };
        // Kept in front of the jaws (walls still stop us: the controller slides this velocity).
        let fwd = forward(gctrl.facing);
        let anchor = gtf.translation + fwd * GRAB_HOLD_DISTANCE;
        let pull = ((anchor - tf.translation).with_y(0.0) / dt).clamp_length_max(25.0);
        ctrl.velocity.x = pull.x;
        ctrl.velocity.z = pull.z;
        ctrl.facing = f32::atan2(fwd.x, fwd.z); // facing the creature
        if !grab.bitten && held.elapsed >= GRAB_BITE_AT {
            grab.bitten = true;
            let pounce = gaction.current.and_then(|a| a.weapon.moveset().pounce);
            let amount = pounce.map_or(100.0, |p| p.damage);
            info!("the jaws close: {amount} damage");
            damage.write(DamageMessage { target: victim, amount, from: Some(gtf.translation), attacker: Some(grab.by), stance: 0.0 });
        }
    }
}
