//! Shared building blocks of every enemy brain ([`super::fighter`] for humanoids,
//! [`super::beast`] for four-legged creatures). A brain is one system that reads [`Senses`] and
//! writes the character's `MoveIntent` / `ActionIntent` — the same interface player input uses.
//!
//! - [`Senses::gather`]: the nearest living player (local or an online puppet), distance and
//!   direction to it and to home.
//! - [`Senses::sees`]: a clear line of sight within a range (walls and closed doors hide you; not a
//!   whole floor apart).
//! - [`Rng`]: a deterministic xorshift (autopilot runs repeat).
//! - [`Steering`]: pacing a patrol, circling, and sidestepping when stuck on an obstacle.
//! - [`chain_combo`]: queue the next light attack just before the cancel window.

use avian3d::prelude::*;
use bevy::prelude::*;

use super::Enemy;
use crate::character::action::{ActionKind, ActionRequest, CharacterAction};
use crate::character::controller::CharacterController;
use crate::character::{ActionIntent, MoveIntent};
use crate::combat::Team;
use crate::tuning::WALK_SPEED;
use crate::weapon::AttackHand;

/// Deterministic random numbers (xorshift), seeded per spawn point.
#[derive(Debug, Clone, Copy)]
pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Self {
        Self(seed.max(1))
    }

    /// Uniform in [0, 1).
    pub fn roll(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x & 0xFFFF) as f32 / 65536.0
    }

    /// Uniform in [a, b).
    pub fn range(&mut self, (a, b): (f32, f32)) -> f32 {
        a + (b - a) * self.roll()
    }

    /// -1 or +1.
    pub fn sign(&mut self) -> f32 {
        if self.roll() < 0.5 { -1.0 } else { 1.0 }
    }
}

/// What an enemy knows about its target this tick.
#[derive(Debug, Clone, Copy)]
pub struct Senses {
    /// The nearest living player, if any.
    pub target: Option<Vec3>,
    /// Facing direction of the target (for stalkers that freeze while watched).
    pub target_facing: Vec3,
    /// Horizontal distance and direction to it (far away when there's none).
    pub dist: f32,
    pub toward: Vec3,
    pub to_home: Vec3,
    pub me: Vec3,
}

/// A player as the brains see one.
pub type PlayerView<'a> = (&'a Transform, &'a CharacterAction, &'a Team);

impl Senses {
    /// The nearest living player of `players` to `tf`.
    pub fn gather<'a>(players: impl Iterator<Item = PlayerView<'a>>, tf: &Transform, enemy: &Enemy) -> Self {
        let nearest = players
            .filter(|(_, a, team)| **team == Team::Player && !matches!(a.current, Some(a) if a.kind == ActionKind::Dead))
            .map(|(t, _, _)| (t.translation, t.rotation * Vec3::NEG_Z))
            .min_by(|a, b| a.0.distance_squared(tf.translation).total_cmp(&b.0.distance_squared(tf.translation)));
        let me = tf.translation;
        let to = nearest.map_or(Vec3::Z * 1000.0, |(p, _)| (p - me).with_y(0.0));
        Self {
            target: nearest.map(|(p, _)| p),
            target_facing: nearest.map_or(Vec3::NEG_Z, |(_, f)| f),
            dist: to.length(),
            toward: to.normalize_or_zero(),
            to_home: (enemy.home - me).with_y(0.0),
            me,
        }
    }

    pub fn alive(&self) -> bool {
        self.target.is_some()
    }

    /// The target is visible within `range`: a clear line from `eye` (height above my centre) to
    /// its chest, and not a whole floor apart. `skip` = entities that don't block the view
    /// (characters, myself).
    pub fn sees(&self, spatial: &SpatialQuery, range: f32, eye: f32, skip: &dyn Fn(Entity) -> bool) -> bool {
        let Some(p) = self.target else { return false };
        self.dist < range
            && (p.y - self.me.y).abs() < 2.5
            && crate::level::line_of_sight(spatial, self.me + Vec3::Y * eye, p + Vec3::Y * 0.6, &|e| !skip(e))
    }

    /// The target is looking at me (within `cos_half_angle` of its facing).
    pub fn watched(&self, cos_half_angle: f32) -> bool {
        self.alive() && self.target_facing.with_y(0.0).normalize_or_zero().dot(-self.toward) > cos_half_angle
    }

    /// Sideways around the target (`side` = ±1).
    pub fn around(&self, side: f32) -> Vec3 {
        Vec3::new(-self.toward.z, 0.0, self.toward.x) * side
    }
}

/// Movement habits shared by every brain.
#[derive(Debug, Clone, Copy)]
pub struct Steering {
    /// Circling direction, and the side to step around obstacles (±1).
    pub strafe: f32,
    /// Seconds spent walking into something (wants to move, barely moves).
    stuck: f32,
    /// Seconds left of sidestepping around an obstacle.
    detour: f32,
    /// Patrolling: heading for the patrol point (else home).
    outbound: bool,
}

impl Default for Steering {
    fn default() -> Self {
        Self { strafe: 1.0, stuck: 0.0, detour: 0.0, outbound: true }
    }
}

impl Steering {
    /// Paces between home and the enemy's patrol point (`None` when it has none).
    pub fn patrol(&mut self, enemy: &Enemy, me: Vec3, factor: f32) -> Option<Vec3> {
        let far = enemy.patrol?;
        let goal = if self.outbound { far } else { enemy.home };
        let to = (goal - me).with_y(0.0);
        if to.length() < 0.4 {
            self.outbound = !self.outbound;
            return Some(Vec3::ZERO);
        }
        Some(to.normalize() * factor)
    }

    /// Walking into something (rubble, a pillar, a door frame): sidestep around it for `detour`
    /// seconds. Call after deciding `movement.dir`.
    pub fn unstick(&mut self, movement: &mut MoveIntent, ctrl: &CharacterController, dt: f32, rng: &mut Rng, detour: f32) {
        let wants = movement.dir.length() * WALK_SPEED;
        let moved = Vec2::new(ctrl.moved.x, ctrl.moved.z).length();
        self.stuck = if wants > 1.0 && moved < wants * 0.3 { self.stuck + dt } else { 0.0 };
        if self.stuck > 0.4 {
            self.stuck = 0.0;
            self.detour = detour;
            if rng.roll() < 0.5 {
                self.strafe = -self.strafe;
            }
        }
        if self.detour > 0.0 && movement.dir.length_squared() > 0.01 {
            self.detour -= dt;
            let d = movement.dir;
            let side = Vec3::new(-d.z, 0.0, d.x) * self.strafe;
            movement.dir = (side * 0.8 + d * 0.3).normalize_or_zero() * d.length();
        }
    }
}

/// Keeps a light combo going: with swings left, queue the next one just before the cancel window
/// (the input buffer is short). A hit or a parry ends the combo. Returns the swings still left.
pub fn chain_combo(action: &CharacterAction, intent: &mut ActionIntent, combo_left: u8) -> u8 {
    let Some(cur) = action.current else { return combo_left };
    if matches!(cur.kind, ActionKind::Hurt | ActionKind::Stunned) {
        intent.attack_held = false;
        intent.buffered = None;
        return 0;
    }
    if matches!(cur.kind, ActionKind::LightAttack(_)) && combo_left > 0 && cur.elapsed >= cur.def.cancel - 0.1 {
        intent.buffer(ActionRequest::LightAttack(AttackHand::Right));
        return combo_left - 1;
    }
    combo_left
}

/// Turns `facing` toward `target` by at most `rate × dt` (rad).
pub fn turn_toward(facing: &mut f32, from: Vec3, target: Vec3, rate: f32, dt: f32) {
    let to = (target - from).with_y(0.0);
    if to.length_squared() < 1e-4 {
        return;
    }
    let want = f32::atan2(-to.x, -to.z);
    let d = (want - *facing + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
    let step = rate * dt;
    *facing += d.clamp(-step, step);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rng_is_deterministic_and_in_range() {
        let (mut a, mut b) = (Rng::new(7), Rng::new(7));
        for _ in 0..100 {
            let x = a.roll();
            assert_eq!(x, b.roll());
            assert!((0.0..1.0).contains(&x));
        }
        assert!((2.0..3.0).contains(&a.range((2.0, 3.0))));
    }

    #[test]
    fn watched_means_facing_me() {
        let enemy = Enemy { home: Vec3::ZERO, patrol: None, kind: super::super::EnemyKind::Hollow };
        // The player stands 5 m north (-Z) of the enemy, looking south (+Z) at it.
        let player = Transform::from_xyz(0.0, 0.0, -5.0).looking_to(Vec3::Z, Vec3::Y);
        let action = CharacterAction::default();
        let s = Senses::gather([(&player, &action, &Team::Player)].into_iter(), &Transform::IDENTITY, &enemy);
        assert!((s.dist - 5.0).abs() < 1e-4);
        assert!(s.watched(0.5));
        let away = Transform::from_xyz(0.0, 0.0, -5.0).looking_to(Vec3::NEG_Z, Vec3::Y);
        let s = Senses::gather([(&away, &action, &Team::Player)].into_iter(), &Transform::IDENTITY, &enemy);
        assert!(!s.watched(0.5), "its back is turned");
    }
}
