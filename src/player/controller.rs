//! Kinematic character controller: camera-relative movement, turning, gravity,
//! ground detection, jump (coyote time + buffer) and sprint. Uses avian's move-and-slide.

use avian3d::prelude::*;
use bevy::prelude::*;

use super::action::{ActionMovement, PlayerAction};
use super::stats::Stamina;
use crate::input::PlayerInput;
use crate::tuning::*;

pub struct ControllerPlugin;

impl Plugin for ControllerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, move_character.in_set(super::PlayerSystems::Movement));
    }
}

#[derive(Component, Debug)]
pub struct CharacterController {
    pub velocity: Vec3,
    pub grounded: bool,
    /// Time left in which a jump is still allowed after leaving the ground.
    pub coyote: f32,
    /// Facing angle around Y; the model looks along -Z at 0.
    pub facing: f32,
    /// Actually sprinting this tick (drains stamina).
    pub sprinting: bool,
}

impl Default for CharacterController {
    fn default() -> Self {
        Self { velocity: Vec3::ZERO, grounded: false, coyote: 0.0, facing: 0.0, sprinting: false }
    }
}

/// Moves `current` toward `target` by at most `max_delta`.
fn approach(current: Vec3, target: Vec3, max_delta: f32) -> Vec3 {
    let diff = target - current;
    if diff.length() <= max_delta { target } else { current + diff.normalize() * max_delta }
}

/// Shortest signed angle difference `to - from`, in (-π, π].
fn angle_diff(from: f32, to: f32) -> f32 {
    let d = (to - from).rem_euclid(std::f32::consts::TAU);
    if d > std::f32::consts::PI { d - std::f32::consts::TAU } else { d }
}

fn move_character(
    time: Res<Time>,
    move_and_slide: MoveAndSlide,
    mut q: Query<(Entity, &Collider, &mut CharacterController, &mut PlayerInput, &PlayerAction, &Stamina, &mut Transform)>,
) {
    let dt = time.delta_secs();
    for (entity, collider, mut ctrl, mut input, action, stamina, mut tf) in &mut q {
        let free = action.current.is_none();
        let accel = if ctrl.grounded { GROUND_ACCEL } else { AIR_ACCEL };
        let horizontal = Vec3::new(ctrl.velocity.x, 0.0, ctrl.velocity.z);

        // Horizontal velocity: from input, slowed, locked, or driven by the action.
        let (wish, horizontal) = match action.movement() {
            ActionMovement::Free | ActionMovement::Slowed(_) => {
                let factor = if let ActionMovement::Slowed(f) = action.movement() { f } else { 1.0 };
                ctrl.sprinting = free && input.sprint && ctrl.grounded && stamina.can_spend()
                    && input.move_dir.length_squared() > 0.01;
                let speed = if ctrl.sprinting { SPRINT_SPEED } else { WALK_SPEED } * factor;
                (input.move_dir, approach(horizontal, input.move_dir * speed, accel * dt))
            }
            ActionMovement::Locked => {
                ctrl.sprinting = false;
                (Vec3::ZERO, approach(horizontal, Vec3::ZERO, GROUND_ACCEL * dt))
            }
            ActionMovement::Scripted(speed) => {
                ctrl.sprinting = false;
                let a = action.current.expect("scripted movement needs an action");
                (Vec3::ZERO, a.dir * speed(a.elapsed))
            }
        };

        // Turn toward the move direction (actions keep their facing).
        if wish.length_squared() > 0.01 {
            let target = f32::atan2(-wish.x, -wish.z);
            let d = angle_diff(ctrl.facing, target);
            let step = TURN_SPEED * dt;
            ctrl.facing += d.clamp(-step, step);
        }

        // Gravity, coyote time and jump.
        let mut vy = ctrl.velocity.y - GRAVITY * dt;
        if ctrl.grounded {
            ctrl.coyote = COYOTE_TIME;
            vy = vy.max(-2.0); // keep pressing into the floor so contact is detected each tick
        } else {
            ctrl.coyote = (ctrl.coyote - dt).max(0.0);
        }
        if free && input.jump_buffer > 0.0 && ctrl.coyote > 0.0 {
            vy = (2.0 * GRAVITY * JUMP_HEIGHT).sqrt();
            ctrl.coyote = 0.0;
            input.jump_buffer = 0.0;
        }

        // Move and slide against the level geometry.
        let filter = SpatialQueryFilter::from_excluded_entities([entity]);
        let out = move_and_slide.move_and_slide(
            collider,
            tf.translation,
            Quat::IDENTITY,
            horizontal + Vec3::Y * vy,
            time.delta(),
            &MoveAndSlideConfig::default(),
            &filter,
            |_| MoveAndSlideHitResponse::Accept,
        );
        tf.translation = out.position;
        ctrl.velocity = out.projected_velocity;

        // Ground probe: a short downward cast after moving. The sweep above is mostly
        // horizontal while walking and often misses the floor, so it can't decide grounding.
        // Skipped while rising so a jump isn't immediately re-grounded.
        ctrl.grounded = false;
        if ctrl.velocity.y <= 0.0 {
            let probe = move_and_slide.spatial_query.cast_shape(
                collider,
                tf.translation,
                Quat::IDENTITY,
                Dir3::NEG_Y,
                &ShapeCastConfig::from_max_distance(GROUND_PROBE_DISTANCE),
                &filter,
            );
            if let Some(hit) = probe.filter(|h| h.normal1.y > GROUND_NORMAL_MIN_Y) {
                ctrl.grounded = true;
                // Snap down onto the floor, keeping the controller's skin gap.
                tf.translation.y -= (hit.distance - GROUND_SKIN).max(0.0);
                ctrl.velocity.y = 0.0;
            }
        }
        tf.rotation = Quat::from_rotation_y(ctrl.facing);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    #[test]
    fn angle_diff_takes_short_way() {
        assert!((angle_diff(0.1, -0.1) + 0.2).abs() < 1e-5);
        assert!((angle_diff(PI - 0.1, -PI + 0.1) - 0.2).abs() < 1e-5);
    }

    #[test]
    fn approach_clamps() {
        assert_eq!(approach(Vec3::ZERO, Vec3::X * 10.0, 1.0), Vec3::X);
        assert_eq!(approach(Vec3::ZERO, Vec3::X * 0.5, 1.0), Vec3::X * 0.5);
    }
}
