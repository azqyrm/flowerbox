//! Lock-on (Q / middle mouse / right stick click). Locks onto the [`Targetable`] closest to the
//! centre of the view within `LOCK_RANGE`; the camera then keeps the target in view and the
//! player faces it, strafing around it (`MoveIntent::face`), and attacks aim at it. Pressing
//! again, the target dying (losing `Targetable`) or getting farther than `LOCK_BREAK_RANGE`
//! releases the lock. Pressing it with nothing to lock onto recentres the camera behind the player.

use avian3d::prelude::*;
use bevy::prelude::*;
use leafwing_input_manager::prelude::ActionState;

use crate::camera::ThirdPersonCamera;
use crate::character::MoveIntent;
use crate::character::controller::CharacterController;
use crate::input::{Action, GameplayBlocked, PlayerInput};
use crate::player::Player;
use crate::tuning::*;

pub struct LockOnPlugin;

impl Plugin for LockOnPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (toggle_lock_on, keep_lock_on).chain().after(crate::input::InputSystems::Gather));
    }
}

/// Something the player can lock onto; `height` = aim point above the entity's origin.
#[derive(Component, Debug, Clone, Copy)]
pub struct Targetable {
    pub height: f32,
}

/// The player's current lock-on target.
#[derive(Component, Debug, Default)]
pub struct LockOn {
    pub target: Option<Entity>,
}

/// Best target for a lock: within `LOCK_RANGE` and `LOCK_CONE` of the view direction, closest to
/// the view's centre (ties: nearer). Pure, for tests.
pub fn pick_target(from: Vec3, view: Vec3, candidates: impl IntoIterator<Item = (Entity, Vec3)>) -> Option<Entity> {
    let view = view.with_y(0.0).normalize_or(Vec3::NEG_Z);
    candidates
        .into_iter()
        .filter_map(|(e, pos)| {
            let to = (pos - from).with_y(0.0);
            let dist = to.length();
            if dist > LOCK_RANGE || dist < 1e-3 {
                return None;
            }
            let angle = view.angle_between(to / dist);
            (angle <= LOCK_CONE).then_some((e, angle + dist * 0.01))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(e, _)| e)
}

#[allow(clippy::type_complexity)]
fn toggle_lock_on(
    blocked: Res<GameplayBlocked>,
    mut player: Query<(&ActionState<Action>, &Transform, &CharacterController, &mut LockOn, &mut PlayerInput), With<Player>>,
    mut cams: Query<&mut ThirdPersonCamera>,
    targets: Query<(Entity, &Transform, &Targetable), Without<Player>>,
    spatial: SpatialQuery,
    characters: Query<(), With<CharacterController>>,
) {
    let Ok((actions, tf, ctrl, mut lock, mut input)) = player.single_mut() else { return };
    // Q, or the two-hand button tapped alone (`PlayerInput::lock_on_tap`).
    let tapped = std::mem::take(&mut input.lock_on_tap);
    if blocked.0 || !(actions.just_pressed(&Action::LockOn) || tapped) {
        return;
    }
    if lock.target.take().is_some() {
        info!("lock-on released");
        return;
    }
    let Ok(mut cam) = cams.single_mut() else { return };
    let view = Vec3::new(-cam.yaw.sin(), 0.0, -cam.yaw.cos());
    // Only what the player can see: no locking onto enemies behind walls or closed doors.
    let eye = tf.translation + Vec3::Y * 0.6;
    let visible = targets.iter().filter(|(target, t, aim)| {
        let skip = |e: Entity| e != *target && !characters.contains(e);
        crate::level::line_of_sight(&spatial, eye, t.translation + Vec3::Y * aim.height, &skip)
    });
    lock.target = pick_target(tf.translation, view, visible.map(|(e, t, _)| (e, t.translation)));
    match lock.target {
        Some(t) => info!("locked on {t}"),
        // Nothing to lock onto: swing the camera behind the player.
        None => cam.recenter = Some(ctrl.facing),
    }
}

/// Drops a lost target and makes the player face the target while locked.
fn keep_lock_on(
    mut player: Query<(&Transform, &mut LockOn, &mut MoveIntent), With<Player>>,
    targets: Query<&Transform, (With<Targetable>, Without<Player>)>,
) {
    let Ok((tf, mut lock, mut intent)) = player.single_mut() else { return };
    let target = lock.target.and_then(|t| targets.get(t).ok());
    match target {
        Some(t) if (t.translation - tf.translation).with_y(0.0).length() <= LOCK_BREAK_RANGE => {
            intent.face = Some((t.translation - tf.translation).with_y(0.0).normalize_or_zero());
        }
        _ => {
            if lock.target.take().is_some() {
                info!("lock-on lost");
            }
            intent.face = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_target_nearest_the_view_centre() {
        let (a, b, c) = (Entity::from_raw_u32(1).unwrap(), Entity::from_raw_u32(2).unwrap(), Entity::from_raw_u32(3).unwrap());
        let from = Vec3::ZERO;
        let view = Vec3::NEG_Z;
        // a: slightly off-centre, b: dead ahead but farther, c: behind.
        let picked = pick_target(from, view, [(a, Vec3::new(2.0, 0.0, -4.0)), (b, Vec3::new(0.0, 0.0, -9.0)), (c, Vec3::new(0.0, 0.0, 3.0))]);
        assert_eq!(picked, Some(b));
        assert_eq!(pick_target(from, view, [(c, Vec3::new(0.0, 0.0, 3.0))]), None, "behind the camera");
        assert_eq!(pick_target(from, view, [(b, Vec3::new(0.0, 0.0, -LOCK_RANGE - 1.0))]), None, "too far");
    }
}
