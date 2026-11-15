//! Beast AI: the behaviour of four-legged predators, parameterised by a [`BeastProfile`] so new
//! beasts (a hound, a boar…) only need a profile, a body (`crate::quadruped::QuadrupedSpec`) and a
//! natural weapon with a moveset (`crate::weapon`). Like the Hollow it only writes `MoveIntent` /
//! `ActionIntent`; the shared character systems do the rest.
//!
//! Compared with the Hollow a beast is aggressive: it notices you from further away, runs you down
//! (sprinting when far), rests only briefly between attacks and circles close.
//!
//! States: **Idle** (or pacing a patrol) → sees a player (line of sight) or gets hit → **Chase** →
//! close: **Bite** (a combo of quick bites) / mid range, off cooldown: **Pounce** (the grab,
//! `crate::grab`; during the wind-up it keeps turning toward its target, [`pounce_tracking`]) →
//! **Recover** (circles, briefly) → Chase … **Home** when the player dies or it's dragged past
//! its leash.

use avian3d::prelude::*;
use bevy::prelude::*;

use super::Enemy;
use crate::character::action::{ActionKind, ActionPhase, ActionRequest, CharacterAction};
use crate::character::controller::CharacterController;
use crate::character::stats::Health;
use crate::character::{ActionIntent, MoveIntent};
use crate::combat::Team;
use crate::tuning::*;

/// How a kind of beast behaves.
#[derive(Clone, Copy, Debug)]
pub struct BeastProfile {
    pub name: &'static str,
    /// Notices a visible player within this distance.
    pub aggro_range: f32,
    /// Gives up beyond this distance from home.
    pub leash: f32,
    /// Chasing speed (fraction of the walk speed), and the distance beyond which it sprints.
    pub chase_factor: f32,
    pub sprint_beyond: f32,
    /// Bites within this distance; a combo of `combo.0..=combo.1` bites.
    pub bite_range: f32,
    pub combo: (u8, u8),
    /// Pounces when the target is between these distances, with this chance per decision, at
    /// most once per `pounce_cooldown` seconds.
    pub pounce_range: (f32, f32),
    pub pounce_chance: f32,
    pub pounce_cooldown: f32,
    /// How fast it turns toward its target while winding up a pounce (rad/s).
    pub windup_turn: f32,
    /// Pause after attacking (min, max seconds), spent circling at `circle_distance`.
    pub recover_time: (f32, f32),
    pub circle_distance: f32,
    pub patrol_factor: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BeastState {
    Idle,
    Chase,
    Bite,
    Pounce,
    Recover,
    Home,
}

#[derive(Component, Debug)]
pub struct BeastAi {
    pub profile: &'static BeastProfile,
    pub state: BeastState,
    /// Where its target was last tick (the pounce wind-up turns toward it).
    pub target: Option<Vec3>,
    timer: f32,
    pounce_cooldown: f32,
    /// Added to the pounce chance after each declined chance (reset by a pounce), so a pounce
    /// always comes within a few openings.
    pounce_urge: f32,
    combo_left: u8,
    strafe: f32,
    stuck: f32,
    detour: f32,
    outbound: bool,
    rng: u32,
}

impl BeastAi {
    pub fn new(profile: &'static BeastProfile, seed: u32) -> Self {
        Self {
            profile,
            state: BeastState::Idle,
            target: None,
            timer: 0.0,
            // Not straight away: a first pounce comes after a moment of chasing.
            pounce_cooldown: 1.5,
            pounce_urge: 0.0,
            combo_left: 0,
            strafe: 1.0,
            stuck: 0.0,
            detour: 0.0,
            outbound: true,
            rng: seed.max(1),
        }
    }

    /// xorshift: deterministic, so autopilot runs repeat.
    fn roll(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x & 0xFFFF) as f32 / 65536.0
    }

    fn set(&mut self, state: BeastState) {
        if self.state != state {
            debug!("{}: {:?} -> {:?}", self.profile.name, self.state, state);
            self.state = state;
        }
    }
}

#[allow(clippy::type_complexity)]
pub fn beast_ai(
    time: Res<Time>,
    spatial: SpatialQuery,
    players: Query<(&Transform, &CharacterAction, &Team), Without<BeastAi>>,
    characters: Query<(), With<CharacterController>>,
    mut q: Query<(Entity, &Enemy, &mut BeastAi, &Transform, &CharacterController, &CharacterAction, &Health, &mut MoveIntent, &mut ActionIntent)>,
) {
    let dt = time.delta_secs();
    for (entity, enemy, mut ai, tf, ctrl, action, health, mut movement, mut intent) in &mut q {
        let p = *ai.profile;
        let nearest = players
            .iter()
            .filter(|(_, a, team)| **team == Team::Player && !matches!(a.current, Some(a) if a.kind == ActionKind::Dead))
            .map(|(t, _, _)| t.translation)
            .min_by(|a, b| a.distance_squared(tf.translation).total_cmp(&b.distance_squared(tf.translation)));
        let player_alive = nearest.is_some();
        let ppos = nearest.unwrap_or(tf.translation + Vec3::Z * 1000.0);
        ai.target = nearest;
        intent.tick(dt);
        movement.dir = Vec3::ZERO;
        movement.sprint = false;
        movement.face = None;
        if health.is_dead() {
            continue;
        }
        let to_player = (ppos - tf.translation).with_y(0.0);
        let dist = to_player.length();
        let toward = to_player.normalize_or_zero();
        let to_home = (enemy.home - tf.translation).with_y(0.0);
        let busy = action.current.is_some();
        let sees = |range: f32| {
            player_alive
                && dist < range
                && (ppos.y - tf.translation.y).abs() < 2.5
                && crate::level::line_of_sight(&spatial, tf.translation + Vec3::Y * 0.3, ppos + Vec3::Y * 0.6, &|e| {
                    e != entity && !characters.contains(e)
                })
        };
        ai.timer -= dt;
        ai.pounce_cooldown -= dt;

        if ai.state == BeastState::Idle && health.current < health.max && player_alive {
            ai.set(BeastState::Chase);
        }
        if matches!(ai.state, BeastState::Chase | BeastState::Bite | BeastState::Recover) && (!player_alive || to_home.length() > p.leash) {
            ai.set(BeastState::Home);
        }

        match ai.state {
            BeastState::Idle => {
                if sees(p.aggro_range) {
                    info!("a {} notices you", p.name);
                    ai.set(BeastState::Chase);
                } else if let Some(far) = enemy.patrol {
                    let goal = if ai.outbound { far } else { enemy.home };
                    let to = (goal - tf.translation).with_y(0.0);
                    if to.length() < 0.4 {
                        ai.outbound = !ai.outbound;
                    } else {
                        movement.dir = to.normalize() * p.patrol_factor;
                    }
                }
            }
            BeastState::Chase => {
                movement.face = Some(toward);
                if dist > p.bite_range * 0.8 {
                    movement.dir = toward * p.chase_factor;
                    movement.sprint = dist > p.sprint_beyond;
                }
                if !busy {
                    if try_pounce(&mut ai, &mut intent, dist, &sees) {
                        // crouching
                    } else if dist < p.bite_range && ai.timer <= 0.0 {
                        intent.buffer(ActionRequest::LightAttack);
                        let (a, b) = p.combo;
                        ai.combo_left = a.saturating_sub(1) + (ai.roll() * (b - a + 1) as f32) as u8;
                        ai.set(BeastState::Bite);
                    }
                }
            }
            BeastState::Bite => {
                movement.face = Some(toward);
                if let Some(cur) = action.current {
                    let chaining = matches!(cur.kind, ActionKind::LightAttack(_)) && ai.combo_left > 0 && cur.elapsed >= cur.def.cancel - 0.1;
                    if chaining {
                        ai.combo_left -= 1;
                        intent.buffer(ActionRequest::LightAttack);
                    }
                    if matches!(cur.kind, ActionKind::Hurt | ActionKind::Stunned) {
                        intent.buffered = None;
                        ai.combo_left = 0;
                    }
                } else if intent.buffered.is_none() {
                    ai.timer = p.recover_time.0 + (p.recover_time.1 - p.recover_time.0) * ai.roll();
                    ai.strafe = if ai.roll() < 0.5 { -1.0 } else { 1.0 };
                    ai.set(BeastState::Recover);
                }
            }
            BeastState::Pounce => {
                movement.face = Some(toward);
                // Done (or interrupted, or it never started): circle, then come again.
                let pouncing = action.current.is_some_and(|a| a.kind == ActionKind::Pounce);
                if !pouncing && intent.buffered.is_none() {
                    ai.timer = p.recover_time.0;
                    ai.set(BeastState::Recover);
                }
            }
            BeastState::Recover => {
                movement.face = Some(toward);
                // Backing off after a combo puts it at pounce distance: a good moment to leap.
                if !busy && try_pounce(&mut ai, &mut intent, dist, &sees) {
                    continue;
                }
                // With the pounce ready, it backs off to leaping distance first.
                let make_room = ai.pounce_cooldown <= 0.0 && dist < p.pounce_range.0 + 0.4;
                if !busy {
                    let side = Vec3::new(-toward.z, 0.0, toward.x) * ai.strafe;
                    movement.dir = if make_room {
                        (-toward * 0.85 + side * 0.25).normalize_or_zero() * 0.8
                    } else if dist < p.circle_distance * 0.6 {
                        (-toward * 0.6 + side * 0.4).normalize_or_zero() * 0.6
                    } else if dist < p.circle_distance * 1.4 {
                        side * 0.6
                    } else {
                        toward * p.chase_factor
                    };
                }
                if ai.timer <= 0.0 && !(make_room && ai.timer > -1.5) {
                    ai.set(BeastState::Chase);
                }
            }
            BeastState::Home => {
                if to_home.length() > 0.4 {
                    movement.dir = to_home.normalize() * p.chase_factor * 0.6;
                } else {
                    ai.set(BeastState::Idle);
                }
                if sees(p.aggro_range * 0.6) && to_home.length() < p.leash * 0.7 {
                    ai.set(BeastState::Chase);
                }
            }
        }

        // Walking into something: sidestep around it for a moment.
        let wants = movement.dir.length() * WALK_SPEED;
        let moved = Vec2::new(ctrl.moved.x, ctrl.moved.z).length();
        ai.stuck = if wants > 1.0 && moved < wants * 0.3 { ai.stuck + dt } else { 0.0 };
        if ai.stuck > 0.4 {
            ai.stuck = 0.0;
            ai.detour = 0.6;
            if ai.roll() < 0.5 {
                ai.strafe = -ai.strafe;
            }
        }
        if ai.detour > 0.0 && movement.dir.length_squared() > 0.01 {
            ai.detour -= dt;
            let d = movement.dir;
            let side = Vec3::new(-d.z, 0.0, d.x) * ai.strafe;
            movement.dir = (side * 0.8 + d * 0.3).normalize_or_zero() * d.length();
        }
    }
}

/// At pounce distance with the cooldown over, decides once: pounce now (with the profile's
/// chance), or not for a while. True if it pounces.
fn try_pounce(ai: &mut BeastAi, intent: &mut ActionIntent, dist: f32, sees: &dyn Fn(f32) -> bool) -> bool {
    let p = ai.profile;
    let (lo, hi) = p.pounce_range;
    if ai.pounce_cooldown > 0.0 || !(lo..hi).contains(&dist) || !sees(hi + 1.0) {
        return false;
    }
    ai.pounce_cooldown = p.pounce_cooldown * (0.3 + 0.4 * ai.roll());
    let roll = ai.roll();
    let chance = p.pounce_chance + ai.pounce_urge;
    debug!("{}: pounce decision at {dist:.1} m (chance {chance:.2}): {}", p.name, if roll < chance { "yes" } else { "not now" });
    if roll >= chance {
        ai.pounce_urge += 0.2;
        return false;
    }
    ai.pounce_urge = 0.0;
    info!("the {} crouches to pounce", p.name);
    intent.buffer(ActionRequest::Pounce);
    ai.pounce_cooldown = p.pounce_cooldown;
    ai.set(BeastState::Pounce);
    true
}

/// While winding up a pounce the beast keeps turning toward its target (so stepping aside doesn't
/// dodge it, a roll through the leap does); the leap goes where it faces when the wind-up ends.
pub fn pounce_tracking(time: Res<Time>, mut q: Query<(&BeastAi, &Transform, &mut CharacterController, &mut CharacterAction)>) {
    let dt = time.delta_secs();
    for (ai, tf, mut ctrl, mut action) in &mut q {
        let Some(cur) = action.current.as_mut().filter(|a| a.kind == ActionKind::Pounce && a.phase() == ActionPhase::Startup) else {
            continue;
        };
        let Some(target) = ai.target else { continue };
        let to = (target - tf.translation).with_y(0.0);
        if to.length_squared() < 1e-4 {
            continue;
        }
        let want = f32::atan2(-to.x, -to.z);
        let d = (want - ctrl.facing + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
        let step = ai.profile.windup_turn * dt;
        ctrl.facing += d.clamp(-step, step);
        cur.dir = Vec3::new(-ctrl.facing.sin(), 0.0, -ctrl.facing.cos());
    }
}
