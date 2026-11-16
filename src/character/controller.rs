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
    /// Skidding to a stop or into a reversal: 0..1, how hard (from the speed it's shedding).
    /// Animation leans against the slide.
    pub brake: f32,
    /// Pivoting round (a reversal at speed): +1 turning left, -1 right, 0 not pivoting.
    pub pivot: f32,
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
            brake: 0.0,
            pivot: 0.0,
        }
    }
}

/// Moves `current` toward `target` by at most `max_delta`.
fn approach(current: Vec3, target: Vec3, max_delta: f32) -> Vec3 {
    let diff = target - current;
    if diff.length() <= max_delta { target } else { current + diff.normalize() * max_delta }
}

/// How a grounded, free character's horizontal velocity changes this tick, Souls-style: speeding
/// up is quick (`GROUND_ACCEL`), shedding speed — letting go of the stick, or reversing — is a
/// little gentler (`GROUND_DECEL`): a short skid instead of a dead stop. Returns the rate (m/s²)
/// and whether this is a brake (stop or reversal at speed). Rolls and attacks don't come here:
/// their actions drive movement, so they cancel a skid at once.
pub fn ground_rate(horizontal: Vec3, target: Vec3) -> (f32, bool) {
    let speed = horizontal.length();
    let stopping = target.length_squared() < 0.01 && speed > 0.05;
    // Against the current travel by more than ~110°.
    let reversing = speed > 0.5 && target.length_squared() > 0.01 && horizontal.normalize().dot(target.normalize()) < -0.35;
    if stopping || reversing { (GROUND_DECEL, speed > 0.5) } else { (GROUND_ACCEL, false) }
}

/// Shortest signed angle difference `to - from`, in (-π, π].
fn angle_diff(from: f32, to: f32) -> f32 {
    let d = (to - from).rem_euclid(std::f32::consts::TAU);
    if d > std::f32::consts::PI { d - std::f32::consts::TAU } else { d }
}

/// Moves through other characters (not walls) while present: a leaping creature carries whoever
/// it catches instead of stopping at them (`crate::grab`).
#[derive(Component, Debug, Default)]
pub struct PassThroughCharacters;

#[allow(clippy::type_complexity)]
pub(super) fn move_character(
    time: Res<Time>,
    move_and_slide: MoveAndSlide,
    characters: Query<Entity, With<CharacterController>>,
    mut q: Query<(
        Entity,
        &Collider,
        &mut CharacterController,
        &mut MoveIntent,
        &CharacterAction,
        &Stamina,
        Option<&Guard>,
        &mut Transform,
        Has<PassThroughCharacters>,
        Option<&super::BodySize>,
    ), Without<super::Dormant>>,
) {
    let dt = time.delta_secs();
    for (entity, collider, mut ctrl, mut intent, action, stamina, guard, mut tf, pass_through, size) in &mut q {
        let size = size.copied().unwrap_or_default();
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
                let (accel, braking) = if ctrl.grounded { ground_rate(horizontal, target) } else { (AIR_ACCEL, false) };
                ctrl.wish_speed = intent.dir.length() * speed;
                // How hard it skids: by the speed it still carries (a jog stop leans less than a sprint's).
                ctrl.brake = if braking { (horizontal.length() / SPRINT_SPEED).min(1.0) } else { 0.0 };
                let reversing = braking && moving;
                // A pivot starts at speed and lasts until the reversal is through.
                let was_pivoting = ctrl.pivot != 0.0;
                ctrl.pivot = 0.0;
                if reversing && (horizontal.length() > PIVOT_MIN_SPEED || was_pivoting) && (intent.face.is_none() || ctrl.sprinting) {
                    let d = angle_diff(ctrl.facing, f32::atan2(-intent.dir.x, -intent.dir.z));
                    ctrl.pivot = d.signum();
                }
                (intent.dir, approach(horizontal, target, accel * dt))
            }
            ActionMovement::Locked => {
                ctrl.sprinting = false;
                (ctrl.brake, ctrl.pivot) = (0.0, 0.0);
                ctrl.wish_speed = 0.0;
                (Vec3::ZERO, approach(horizontal, Vec3::ZERO, GROUND_ACCEL * dt))
            }
            ActionMovement::Scripted(curve) => {
                ctrl.sprinting = false;
                ctrl.wish_speed = 0.0;
                (ctrl.brake, ctrl.pivot) = (0.0, 0.0);
                let a = action.current.expect("scripted movement needs an action");
                (Vec3::ZERO, a.dir * curve.speed(a.elapsed))
            }
            ActionMovement::Plunge | ActionMovement::Held => {
                // Plunge: keep the jump's horizontal momentum (a moving jump attack dives forward).
                // Held: whoever holds us set the velocity (`crate::grab`).
                ctrl.sprinting = false;
                (ctrl.brake, ctrl.pivot) = (0.0, 0.0);
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

        // Move and slide against the level geometry. A body lying dead ignores characters too,
        // else the living walking into it would shove the corpse around.
        let dead = matches!(action.current, Some(a) if a.kind == super::action::ActionKind::Dead);
        let filter = if pass_through || dead {
            SpatialQueryFilter::from_excluded_entities(characters.iter())
        } else {
            SpatialQueryFilter::from_excluded_entities([entity])
        };
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
            let radius = size.radius * GROUND_PROBE_RADIUS_FACTOR;
            let foot = tf.translation - Vec3::Y * (size.length * 0.5);
            // Probe bottom sits this far above the capsule bottom before the cast.
            let gap = GROUND_PROBE_LIFT + (size.radius - radius);
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
    fn stops_skid_a_little_and_reversals_brake_first() {
        // Integrate a stop at 60 Hz: how far does it slide?
        let slide = |v0: f32| {
            let (mut v, mut x, dt) = (Vec3::new(0.0, 0.0, -v0), 0.0, 1.0 / 60.0);
            let mut ticks = 0;
            while v.length() > 0.0 {
                let (rate, braking) = ground_rate(v, Vec3::ZERO);
                assert!(braking || v.length() <= 0.5);
                v = approach(v, Vec3::ZERO, rate * dt);
                x += v.length() * dt;
                ticks += 1;
            }
            (x, ticks as f32 / 60.0)
        };
        let (jog, jog_t) = slide(WALK_SPEED);
        let (sprint, sprint_t) = slide(SPRINT_SPEED);
        assert!((0.2..0.45).contains(&jog) && jog_t < 0.2, "jog stop: {jog} m in {jog_t} s");
        assert!((0.5..1.0).contains(&sprint) && sprint_t < 0.3, "sprint stop: {sprint} m in {sprint_t} s");
        // Reversing brakes at the gentler rate; speeding up (or steering) stays quick.
        let v = Vec3::new(0.0, 0.0, -SPRINT_SPEED);
        assert_eq!(ground_rate(v, Vec3::new(0.0, 0.0, 7.0)), (GROUND_DECEL, true));
        assert_eq!(ground_rate(v, Vec3::new(7.0, 0.0, -1.0)), (GROUND_ACCEL, false), "a sharp turn isn't a reversal");
        assert_eq!(ground_rate(Vec3::ZERO, Vec3::new(0.0, 0.0, -4.5)), (GROUND_ACCEL, false));
    }

    #[test]
    fn approach_clamps() {
        assert_eq!(approach(Vec3::ZERO, Vec3::X * 10.0, 1.0), Vec3::X);
        assert_eq!(approach(Vec3::ZERO, Vec3::X * 0.5, 1.0), Vec3::X * 0.5);
    }
}
