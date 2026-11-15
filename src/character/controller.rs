//! Kinematic character controller for any character: follows its [`MoveIntent`] (turning,
//! walking, sprinting, jumping with coyote time) unless the running action overrides movement.
//! Gravity, ground detection and collision via avian's move-and-slide.

use avian3d::prelude::*;
use bevy::prelude::*;

use super::action::{ActionMovement, ActionPhase, CharacterAction};
use super::MoveIntent;
use super::guard::Guard;
use super::stats::Stamina;
use crate::tuning::*;

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
    /// Velocity actually achieved last tick (displacement / dt). Unlike `velocity` it doesn't
    /// jitter while pressing against or sliding along walls, so animation reads this.
    pub moved: Vec3,
    /// Horizontal speed the character is trying to move at (0 while an action drives movement).
    pub wish_speed: f32,
}

impl Default for CharacterController {
    fn default() -> Self {
        Self {
            velocity: Vec3::ZERO,
            grounded: false,
            coyote: 0.0,
            facing: 0.0,
            sprinting: false,
            moved: Vec3::ZERO,
            wish_speed: 0.0,
        }
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

#[allow(clippy::type_complexity)]
pub(super) fn move_character(
    time: Res<Time>,
    move_and_slide: MoveAndSlide,
    mut q: Query<(Entity, &Collider, &mut CharacterController, &mut MoveIntent, &CharacterAction, &Stamina, Option<&Guard>, &mut Transform)>,
) {
    let dt = time.delta_secs();
    for (entity, collider, mut ctrl, mut intent, action, stamina, guard, mut tf) in &mut q {
        let guarding = guard.is_some_and(Guard::is_up);
        let free = action.current.is_none();
        let horizontal = Vec3::new(ctrl.velocity.x, 0.0, ctrl.velocity.z);

        // Horizontal velocity: from the intent, slowed, locked, or driven by the action.
        let (wish, horizontal) = match action.movement() {
            ActionMovement::Free | ActionMovement::Slowed(_) => {
                let mut factor = if let ActionMovement::Slowed(f) = action.movement() { f } else { 1.0 };
                if guarding {
                    factor *= GUARD_MOVE_FACTOR;
                }
                let moving = intent.dir.length_squared() > 0.01;
                ctrl.sprinting = free && !guarding && intent.sprint && ctrl.grounded && stamina.can_spend() && moving;
                let speed = if ctrl.sprinting { SPRINT_SPEED } else { WALK_SPEED } * factor;
                let target = if ctrl.grounded {
                    intent.dir * speed
                } else {
                    // In the air: keep the momentum you jumped with (a running jump stays fast).
                    intent.dir * speed.max(horizontal.length())
                };
                let accel = if ctrl.grounded { GROUND_ACCEL } else { AIR_ACCEL };
                ctrl.wish_speed = intent.dir.length() * speed;
                (intent.dir, approach(horizontal, target, accel * dt))
            }
            ActionMovement::Locked => {
                ctrl.sprinting = false;
                ctrl.wish_speed = 0.0;
                (Vec3::ZERO, approach(horizontal, Vec3::ZERO, GROUND_ACCEL * dt))
            }
            ActionMovement::Scripted(curve) => {
                ctrl.sprinting = false;
                ctrl.wish_speed = 0.0;
                let a = action.current.expect("scripted movement needs an action");
                (Vec3::ZERO, a.dir * curve.speed(a.elapsed))
            }
            ActionMovement::Plunge => {
                // Keep the jump's horizontal momentum: a moving jump attack dives forward.
                ctrl.sprinting = false;
                ctrl.wish_speed = 0.0;
                (Vec3::ZERO, horizontal)
            }
        };

        // Turn toward the move direction (actions keep their facing). Locked on (`face`), turn
        // toward the target instead and strafe; sprinting still runs where you steer.
        let look = match intent.face {
            Some(f) if action.current.is_none() && !ctrl.sprinting && f.length_squared() > 0.01 => f,
            _ => wish,
        };
        if look.length_squared() > 0.01 {
            let target = f32::atan2(-look.x, -look.z);
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
        // Jump attack: hover during the wind-up, then dive.
        if let (ActionMovement::Plunge, Some(a)) = (action.movement(), action.current) {
            match a.phase() {
                ActionPhase::Startup => vy = 0.0,
                ActionPhase::Active => vy = -PLUNGE_SPEED,
                _ => {}
            }
        }
        if free && intent.jump_buffer > 0.0 && ctrl.coyote > 0.0 {
            vy = (2.0 * GRAVITY * JUMP_HEIGHT).sqrt();
            ctrl.coyote = 0.0;
            intent.jump_buffer = 0.0;
        }

        // Move and slide against the level geometry.
        let filter = SpatialQueryFilter::from_excluded_entities([entity]);
        let start = tf.translation;
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
        // Skipped while rising (by intent: the slide's projected velocity can come back with a
        // tiny +y in the wall/floor crease) so a jump isn't immediately re-grounded.
        //
        // The probe is a sphere a bit thinner than the capsule, centred on its bottom
        // hemisphere and lifted slightly: a cast that starts touching something (the floor, or
        // a wall the character is pressing into) returns nothing or a garbage normal, which
        // made grounding flicker and the animation stutter against walls (N16).
        ctrl.grounded = false;
        if vy <= 0.0 {
            let radius = PLAYER_RADIUS * GROUND_PROBE_RADIUS_FACTOR;
            let foot = tf.translation - Vec3::Y * (PLAYER_CAPSULE_LENGTH * 0.5);
            // Probe bottom sits this far above the capsule bottom before the cast.
            let gap = GROUND_PROBE_LIFT + (PLAYER_RADIUS - radius);
            let probe = move_and_slide.spatial_query.cast_shape(
                &Collider::sphere(radius),
                foot + Vec3::Y * GROUND_PROBE_LIFT,
                Quat::IDENTITY,
                Dir3::NEG_Y,
                &ShapeCastConfig::from_max_distance(gap + GROUND_PROBE_DISTANCE),
                &filter,
            );
            if let Some(hit) = probe.filter(|h| h.normal1.y > GROUND_NORMAL_MIN_Y) {
                ctrl.grounded = true;
                // Snap down onto the floor, keeping the controller's skin gap.
                tf.translation.y -= (hit.distance - gap - GROUND_SKIN).max(0.0);
                ctrl.velocity.y = 0.0;
            }
        }
        tf.rotation = Quat::from_rotation_y(ctrl.facing);
        if dt > 0.0 {
            ctrl.moved = (tf.translation - start) / dt;
        }
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
