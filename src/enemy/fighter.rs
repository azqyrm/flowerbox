//! The humanoid brain: anything that fights with a weapon on two legs — the Hollow, the Ashen
//! Acolyte, the Mourner, the Thorned Saint. One system ([`fighter_ai`]) and a data
//! [`FighterProfile`] per kind; like every brain it only writes `MoveIntent` / `ActionIntent`.
//!
//! States: **Idle** at home (or pacing a patrol) → notices a player (line of sight, or being hit)
//! → **Chase** → close: **Attack** (a light combo of a few swings, or a charged heavy), or a
//! **special move** when one is ready ([`Special`]: an ability — a spin, a dash, a slam, a shriek —
//! with its own range, chance and cooldown) → **Recover** (circles, keeps its distance) → Chase…
//! It walks **Home** when the players are dead or it was dragged past its leash.
//!
//! Variations, all data:
//! - **Stalker** (`stalker`): while the target looks at it from further than a few metres, it
//!   stands frozen like a statue; the moment you look away it glides at you.
//! - **Enrage** (`enrage`): below a share of its health it roars once (a special), then moves faster,
//!   recovers sooner and unlocks its phase-two specials — the boss's second phase.

use avian3d::prelude::*;
use bevy::prelude::*;

use super::Enemy;
use super::ai::{Rng, Senses, Steering, chain_combo};
use crate::ability::AbilityId;
use crate::character::action::{ActionKind, ActionRequest, CharacterAction};
use crate::character::controller::CharacterController;
use crate::character::stats::Health;
use crate::character::{ActionIntent, MoveIntent};
use crate::combat::Team;
use crate::weapon::AttackHand;

/// A special move: an ability the fighter may use when its target is in `range`.
#[derive(Clone, Copy, Debug)]
pub struct Special {
    pub ability: AbilityId,
    pub range: (f32, f32),
    /// Chance per decision once ready.
    pub chance: f32,
    /// Seconds before it can be used again.
    pub cooldown: f32,
    /// Only once enraged (the boss's second phase).
    pub enraged_only: bool,
}

impl Special {
    pub const fn new(ability: AbilityId, range: (f32, f32), chance: f32, cooldown: f32) -> Self {
        Self { ability, range, chance, cooldown, enraged_only: false }
    }

    pub const fn enraged(self) -> Self {
        Self { enraged_only: true, ..self }
    }
}

/// The second phase.
#[derive(Clone, Copy, Debug)]
pub struct Enrage {
    /// Below this share of its health.
    pub below: f32,
    /// The roar that marks the change (performed at once).
    pub roar: Option<AbilityId>,
    /// Movement and recovery-time multipliers from then on.
    pub speed: f32,
    pub recover: f32,
}

/// How a kind of fighter behaves.
#[derive(Clone, Copy, Debug)]
pub struct FighterProfile {
    pub name: &'static str,
    pub aggro_range: f32,
    pub leash: f32,
    /// Chasing speed (fraction of the walk speed); sprints when further than `sprint_beyond`.
    pub move_factor: f32,
    pub sprint_beyond: f32,
    pub attack_range: f32,
    pub heavy_chance: f32,
    /// Extra swings chained onto a light attack (min, max).
    pub extra_swings: (u8, u8),
    pub recover_time: (f32, f32),
    pub patrol_factor: f32,
    pub specials: &'static [Special],
    pub stalker: bool,
    pub enrage: Option<Enrage>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FighterState {
    Idle,
    Chase,
    Attack,
    Recover,
    Home,
    /// A stalker caught in the target's gaze.
    Frozen,
}

const MAX_SPECIALS: usize = 6;

#[derive(Component, Debug)]
pub struct FighterAi {
    pub profile: &'static FighterProfile,
    pub state: FighterState,
    /// Recover: seconds left; Attack: seconds the heavy is still held.
    timer: f32,
    combo_left: u8,
    steering: Steering,
    rng: Rng,
    /// Seconds until each special is ready again.
    cooldowns: [f32; MAX_SPECIALS],
    pub enraged: bool,
}

impl FighterAi {
    pub fn new(profile: &'static FighterProfile, seed: u32) -> Self {
        // Specials start partly cooled down: not all at once the moment it notices you.
        let mut rng = Rng::new(seed);
        let mut cooldowns = [0.0; MAX_SPECIALS];
        for (c, s) in cooldowns.iter_mut().zip(profile.specials) {
            *c = s.cooldown * rng.range((0.3, 0.7));
        }
        Self { profile, state: FighterState::Idle, timer: 0.0, combo_left: 0, steering: Steering::default(), rng, cooldowns, enraged: false }
    }

    fn set(&mut self, state: FighterState) {
        if self.state != state {
            debug!("{}: {:?} -> {:?}", self.profile.name, self.state, state);
            self.state = state;
        }
    }

    /// A ready special for this distance (rolls its chance); starts it.
    fn try_special(&mut self, intent: &mut ActionIntent, dist: f32) -> bool {
        for (i, s) in self.profile.specials.iter().enumerate().take(MAX_SPECIALS) {
            if self.cooldowns[i] > 0.0 || (s.enraged_only && !self.enraged) || !(s.range.0..s.range.1).contains(&dist) {
                continue;
            }
            // Declined chances wait a little before the next roll.
            if self.rng.roll() >= s.chance {
                self.cooldowns[i] = 0.8;
                continue;
            }
            self.cooldowns[i] = s.cooldown;
            info!("the {} uses {}", self.profile.name, s.ability.def().name);
            intent.buffer(ActionRequest::Special(s.ability));
            self.set(FighterState::Attack);
            return true;
        }
        false
    }

    fn recover_time(&mut self) -> f32 {
        let t = self.rng.range(self.profile.recover_time);
        if self.enraged { t * self.profile.enrage.map_or(1.0, |e| e.recover) } else { t }
    }
}

#[allow(clippy::type_complexity)]
pub fn fighter_ai(
    time: Res<Time>,
    spatial: SpatialQuery,
    players: Query<(&Transform, &CharacterAction, &Team), Without<FighterAi>>,
    characters: Query<(), With<CharacterController>>,
    mut q: Query<(Entity, &Enemy, &mut FighterAi, &Transform, &CharacterController, &CharacterAction, &Health, &mut MoveIntent, &mut ActionIntent), Without<super::Dormant>>,
) {
    let dt = time.delta_secs();
    for (entity, enemy, mut ai, tf, ctrl, action, health, mut movement, mut intent) in &mut q {
        let p = *ai.profile;
        let senses = Senses::gather(players.iter(), tf, enemy);
        intent.tick(dt);
        movement.dir = Vec3::ZERO;
        movement.sprint = false;
        movement.face = None;
        if health.is_dead() {
            intent.attack_held = false;
            continue;
        }
        let skip = |e: Entity| e == entity || characters.contains(e);
        let sees = |range: f32| senses.sees(&spatial, range, 0.6, &skip);
        let (dist, toward) = (senses.dist, senses.toward);
        let busy = action.current.is_some();
        ai.timer -= dt;
        for c in &mut ai.cooldowns {
            *c -= dt;
        }
        let speed = if ai.enraged { p.enrage.map_or(1.0, |e| e.speed) } else { 1.0 };

        // Second phase: roar once, then fight harder.
        if let Some(e) = p.enrage
            && !ai.enraged
            && health.current < health.max * e.below
        {
            ai.enraged = true;
            info!("the {} is enraged", p.name);
            if let Some(roar) = e.roar {
                intent.buffer(ActionRequest::Special(roar));
                ai.set(FighterState::Attack);
            }
        }
        // Hit while idle: turn on the attacker.
        if matches!(ai.state, FighterState::Idle | FighterState::Frozen) && health.current < health.max && senses.alive() {
            ai.set(FighterState::Chase);
        }
        if matches!(ai.state, FighterState::Chase | FighterState::Attack | FighterState::Recover | FighterState::Frozen)
            && (!senses.alive() || senses.to_home.length() > p.leash)
        {
            intent.attack_held = false;
            ai.set(FighterState::Home);
        }

        match ai.state {
            FighterState::Idle => {
                if sees(p.aggro_range) {
                    info!("a {} notices you", p.name);
                    ai.set(FighterState::Chase);
                } else if let Some(dir) = ai.steering.patrol(enemy, tf.translation, p.patrol_factor) {
                    movement.dir = dir;
                }
            }
            FighterState::Frozen => {
                // Caught in the gaze: perfectly still, facing you.
                movement.face = Some(toward);
                if !senses.watched(STALKER_GAZE) || dist < STALKER_NEAR {
                    ai.set(FighterState::Chase);
                }
            }
            FighterState::Chase => {
                movement.face = Some(toward);
                if p.stalker && !busy && dist > STALKER_NEAR && senses.watched(STALKER_GAZE) && sees(p.leash) {
                    ai.set(FighterState::Frozen);
                    continue;
                }
                if dist > p.attack_range * 0.8 {
                    movement.dir = toward * p.move_factor * speed;
                    movement.sprint = dist > p.sprint_beyond;
                }
                if !busy && ai.timer <= 0.0 && !ai.try_special(&mut intent, dist) && dist < p.attack_range {
                    if ai.rng.roll() < p.heavy_chance {
                        intent.buffer(ActionRequest::HeavyAttack(AttackHand::Right));
                        intent.attack_held = true;
                        ai.timer = ai.rng.range((0.35, 0.65));
                        ai.combo_left = 0;
                    } else {
                        intent.buffer(ActionRequest::LightAttack(AttackHand::Right));
                        let (a, b) = p.extra_swings;
                        ai.combo_left = a + (ai.rng.roll() * (b - a + 1) as f32) as u8;
                    }
                    ai.set(FighterState::Attack);
                }
            }
            FighterState::Attack => {
                movement.face = Some(toward);
                intent.attack_held = intent.attack_held && ai.timer > 0.0;
                if action.current.is_some() {
                    ai.combo_left = chain_combo(action, &mut intent, ai.combo_left);
                } else if intent.buffered.is_none() {
                    ai.timer = ai.recover_time();
                    ai.steering.strafe = ai.rng.sign();
                    ai.set(FighterState::Recover);
                }
            }
            FighterState::Recover => {
                movement.face = Some(toward);
                // A gap-closer (a dash from afar) doesn't wait for the pause to end.
                if !busy && dist > p.attack_range * 1.5 && ai.try_special(&mut intent, dist) {
                    continue;
                }
                if !busy {
                    movement.dir = if dist < 1.6 {
                        -toward * 0.5
                    } else if dist < 3.5 {
                        senses.around(ai.steering.strafe) * 0.4
                    } else {
                        toward * p.move_factor * speed
                    };
                }
                if ai.timer <= 0.0 {
                    ai.set(FighterState::Chase);
                }
            }
            FighterState::Home => {
                if senses.to_home.length() > 0.4 {
                    movement.dir = senses.to_home.normalize() * p.move_factor;
                } else {
                    ai.set(FighterState::Idle);
                }
                if sees(p.aggro_range * 0.6) && senses.to_home.length() < p.leash * 0.7 {
                    ai.set(FighterState::Chase);
                }
            }
        }
        // Cancel a stale heavy hold if the swing got interrupted.
        if !matches!(action.current.map(|a| a.kind), Some(ActionKind::HeavyCharge)) && ai.state != FighterState::Attack {
            intent.attack_held = false;
        }
        let FighterAi { steering, rng, .. } = &mut *ai;
        steering.unstick(&mut movement, ctrl, dt, rng, 0.7);
    }
}

/// A stalker freezes while its target's facing is within ~50° of it…
const STALKER_GAZE: f32 = 0.64;
/// …unless it's already this close (then it just attacks).
const STALKER_NEAR: f32 = 3.2;
