//! The player's action state machine — the heart of the Soulslike feel.
//!
//! An *action* (roll, attack, drink, hurt…) runs through three phases:
//! **startup → active → recovery**. While an action runs it decides how the character
//! may move ([`ActionMovement`]). New actions are requested through a short **input buffer**
//! ([`crate::character::ActionIntent::buffered`]) and only start when the current action reaches its
//! **cancel window** (`ActionDef::cancel`), or when nothing is running.
//! Locomotion (idle / walk / sprint / airborne) is simply "no action running".

use bevy::prelude::*;

use super::controller::CharacterController;
use super::stats::{Health, Invulnerable, Stamina};
use super::{ActionIntent, MoveIntent};
use crate::tuning::*;

/// What the player asked for (from input). Turned into an [`ActionKind`] by [`resolve_request`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionRequest {
    /// Becomes a backstep when there's no movement input.
    Roll,
    /// Becomes a jump attack while airborne.
    LightAttack,
    HeavyAttack,
    DrinkAle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionKind {
    Roll,
    Backstep,
    /// Combo index, 0-based.
    LightAttack(u8),
    /// Light attack out of a sprint: a leaping slash.
    RunningAttack,
    /// Holding LMB: the heavy attack winds up until release or full charge.
    HeavyCharge,
    HeavyAttack,
    JumpAttack,
    DrinkAle,
    Hurt,
    Dead,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionPhase {
    Startup,
    Active,
    Recovery,
    Done,
}

/// How the controller treats movement while an action runs.
#[derive(Clone, Copy, Debug)]
pub enum ActionMovement {
    /// Normal input movement.
    Free,
    /// Input movement scaled by the factor; no sprint, no jump.
    Slowed(f32),
    /// No input movement; the character decelerates.
    Locked,
    /// The action drives the horizontal velocity: `dir * speed(elapsed)`.
    Scripted(fn(f32) -> f32),
    /// Jump attack: hang in the air during startup, then dive at `PLUNGE_SPEED`.
    Plunge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroundRule {
    Ground,
    Air,
    Any,
}

#[derive(Clone, Copy, Debug)]
pub struct ActionDef {
    pub startup: f32,
    pub active: f32,
    pub recovery: f32,
    /// Seconds after start from which a buffered action may cancel this one.
    pub cancel: f32,
    pub stamina_cost: f32,
    /// Invulnerable between these times (seconds since start).
    pub iframes: Option<(f32, f32)>,
    pub movement: ActionMovement,
    pub ground: GroundRule,
    /// Upward velocity given when the action starts (the running attack's leap).
    pub hop: f32,
}

impl ActionDef {
    pub fn total(&self) -> f32 {
        self.startup + self.active + self.recovery
    }
}

/// Constant speed that covers `distance` over the active window, eased out in recovery.
fn dash_speed(elapsed: f32, startup: f32, active: f32, recovery: f32, distance: f32) -> f32 {
    // Distance split: 85% during active, 15% bleeding off in recovery.
    let v = distance * 0.85 / active;
    if elapsed < startup {
        v * 0.5
    } else if elapsed < startup + active {
        v
    } else {
        let t = ((elapsed - startup - active) / recovery).clamp(0.0, 1.0);
        v * 0.6 * (1.0 - t)
    }
}

/// Smooth forward step covering `distance` between `start` and `end` (seconds since action start).
fn lunge_speed(elapsed: f32, start: f32, end: f32, distance: f32) -> f32 {
    if elapsed < start || elapsed >= end {
        return 0.0;
    }
    // Sine bell integrates to `distance` over the window.
    let t = (elapsed - start) / (end - start);
    distance * std::f32::consts::PI / (2.0 * (end - start)) * (std::f32::consts::PI * t).sin()
}

fn light_lunge(e: f32) -> f32 {
    lunge_speed(e, LIGHT_STARTUP * 0.3, LIGHT_STARTUP + LIGHT_ACTIVE, LIGHT_LUNGE)
}
fn heavy_lunge(e: f32) -> f32 {
    lunge_speed(e, HEAVY_STARTUP * 0.5, HEAVY_STARTUP + HEAVY_ACTIVE, HEAVY_LUNGE)
}
fn running_attack_leap(e: f32) -> f32 {
    lunge_speed(e, 0.0, RUN_ATTACK_STARTUP + RUN_ATTACK_ACTIVE, RUN_ATTACK_DISTANCE)
}

fn roll_speed(e: f32) -> f32 {
    dash_speed(e, ROLL_STARTUP, ROLL_ACTIVE, ROLL_RECOVERY, ROLL_DISTANCE)
}
fn backstep_speed(e: f32) -> f32 {
    dash_speed(e, BACKSTEP_STARTUP, BACKSTEP_ACTIVE, BACKSTEP_RECOVERY, BACKSTEP_DISTANCE)
}

/// The data table for every action. Numbers live in `tuning.rs`.
pub fn action_def(kind: ActionKind) -> ActionDef {
    use ActionKind::*;
    match kind {
        Roll => ActionDef {
            startup: ROLL_STARTUP,
            active: ROLL_ACTIVE,
            recovery: ROLL_RECOVERY,
            cancel: ROLL_CANCEL,
            stamina_cost: ROLL_COST,
            iframes: Some((ROLL_STARTUP, ROLL_STARTUP + ROLL_ACTIVE)),
            movement: ActionMovement::Scripted(roll_speed),
            ground: GroundRule::Ground,
            hop: 0.0,
        },
        Backstep => ActionDef {
            startup: BACKSTEP_STARTUP,
            active: BACKSTEP_ACTIVE,
            recovery: BACKSTEP_RECOVERY,
            cancel: BACKSTEP_CANCEL,
            stamina_cost: BACKSTEP_COST,
            iframes: Some((BACKSTEP_STARTUP, BACKSTEP_STARTUP + BACKSTEP_ACTIVE)),
            movement: ActionMovement::Scripted(backstep_speed),
            ground: GroundRule::Ground,
            hop: 0.0,
        },
        LightAttack(_) => ActionDef {
            startup: LIGHT_STARTUP,
            active: LIGHT_ACTIVE,
            recovery: LIGHT_RECOVERY,
            cancel: LIGHT_CANCEL,
            stamina_cost: LIGHT_COST,
            iframes: None,
            movement: ActionMovement::Scripted(light_lunge),
            ground: GroundRule::Ground,
            hop: 0.0,
        },
        RunningAttack => ActionDef {
            startup: RUN_ATTACK_STARTUP,
            active: RUN_ATTACK_ACTIVE,
            recovery: RUN_ATTACK_RECOVERY,
            cancel: RUN_ATTACK_STARTUP + RUN_ATTACK_ACTIVE + RUN_ATTACK_RECOVERY * 0.6,
            stamina_cost: RUN_ATTACK_COST,
            iframes: None,
            movement: ActionMovement::Scripted(running_attack_leap),
            ground: GroundRule::Ground,
            hop: RUN_ATTACK_HOP,
        },
        HeavyCharge => ActionDef {
            startup: 0.0,
            // Never ends by itself: `release_heavy_charge` turns it into HeavyAttack.
            active: HEAVY_CHARGE_MAX + 10.0,
            recovery: 0.0,
            cancel: 0.0, // can roll out of a charge
            stamina_cost: HEAVY_COST,
            iframes: None,
            movement: ActionMovement::Slowed(HEAVY_CHARGE_MOVE_FACTOR),
            ground: GroundRule::Ground,
            hop: 0.0,
        },
        HeavyAttack => ActionDef {
            startup: HEAVY_STARTUP,
            active: HEAVY_ACTIVE,
            recovery: HEAVY_RECOVERY,
            cancel: HEAVY_CANCEL,
            stamina_cost: 0.0, // paid when the charge starts
            iframes: None,
            movement: ActionMovement::Scripted(heavy_lunge),
            ground: GroundRule::Ground,
            hop: 0.0,
        },
        JumpAttack => {
            let total = JUMP_ATTACK_STARTUP + JUMP_ATTACK_ACTIVE + JUMP_ATTACK_RECOVERY;
            ActionDef {
                startup: JUMP_ATTACK_STARTUP,
                active: JUMP_ATTACK_ACTIVE,
                recovery: JUMP_ATTACK_RECOVERY,
                cancel: total,
                stamina_cost: JUMP_ATTACK_COST,
                iframes: None,
                movement: ActionMovement::Plunge,
                ground: GroundRule::Air,
                hop: 0.0,
            }
        }
        DrinkAle => {
            let total = DRINK_STARTUP + DRINK_ACTIVE + DRINK_RECOVERY;
            ActionDef {
                startup: DRINK_STARTUP,
                active: DRINK_ACTIVE,
                recovery: DRINK_RECOVERY,
                cancel: total, // can't cancel a drink — that's the risk
                stamina_cost: 0.0,
                iframes: None,
                movement: ActionMovement::Slowed(DRINK_MOVE_FACTOR),
                ground: GroundRule::Ground,
                hop: 0.0,
            }
        }
        Hurt => ActionDef {
            startup: 0.0,
            active: 0.0,
            recovery: HURT_DURATION,
            cancel: HURT_DURATION,
            stamina_cost: 0.0,
            iframes: None,
            movement: ActionMovement::Locked,
            ground: GroundRule::Any,
            hop: 0.0,
        },
        Dead => ActionDef {
            startup: 0.0,
            active: 0.0,
            recovery: DEAD_DURATION,
            cancel: f32::INFINITY,
            stamina_cost: 0.0,
            iframes: None,
            movement: ActionMovement::Locked,
            ground: GroundRule::Any,
            hop: 0.0,
        },
    }
}

/// A running action.
#[derive(Clone, Copy, Debug)]
pub struct ActiveAction {
    pub kind: ActionKind,
    pub def: ActionDef,
    pub elapsed: f32,
    /// World-space horizontal direction locked in at start (roll direction, etc.).
    pub dir: Vec3,
    /// Heavy attack charge, 0..=1.
    pub charge: f32,
}

impl ActiveAction {
    pub fn new(kind: ActionKind, dir: Vec3) -> Self {
        Self { kind, def: action_def(kind), elapsed: 0.0, dir, charge: 0.0 }
    }

    pub fn phase(&self) -> ActionPhase {
        let d = &self.def;
        if self.elapsed < d.startup {
            ActionPhase::Startup
        } else if self.elapsed < d.startup + d.active {
            ActionPhase::Active
        } else if self.elapsed < d.total() {
            ActionPhase::Recovery
        } else {
            ActionPhase::Done
        }
    }

    pub fn in_iframes(&self) -> bool {
        self.def.iframes.is_some_and(|(a, b)| self.elapsed >= a && self.elapsed < b)
    }

    pub fn can_be_cancelled(&self) -> bool {
        self.elapsed >= self.def.cancel
    }
}

/// Current action of a character; `None` = free locomotion.
#[derive(Component, Debug, Default)]
pub struct CharacterAction {
    pub current: Option<ActiveAction>,
    /// Increments every time an action starts; lets combat tell swings apart.
    pub serial: u32,
}

impl CharacterAction {
    /// Starts `action` now, replacing whatever was running.
    pub fn start(&mut self, action: ActiveAction) {
        self.current = Some(action);
        self.serial = self.serial.wrapping_add(1);
    }

    /// Starts an action immediately, interrupting anything (used for Hurt / Dead).
    pub fn force(&mut self, kind: ActionKind, dir: Vec3) {
        if matches!(self.current, Some(a) if a.kind == ActionKind::Dead) {
            return; // nothing interrupts death
        }
        self.start(ActiveAction::new(kind, dir));
    }

    pub fn movement(&self) -> ActionMovement {
        self.current.map_or(ActionMovement::Free, |a| a.def.movement)
    }
}

/// The character's situation when a request is resolved.
#[derive(Clone, Copy, Debug, Default)]
pub struct RequestContext {
    pub grounded: bool,
    /// Has movement input (a roll instead of a backstep).
    pub moving: bool,
    /// Sprinting right now (a light attack becomes the running attack).
    pub sprinting: bool,
}

/// Decides whether `request` can start now. `None` = not yet (keep it buffered) or never.
pub fn resolve_request(
    current: Option<&ActiveAction>,
    request: ActionRequest,
    ctx: RequestContext,
    stamina: &Stamina,
) -> Option<ActionKind> {
    let RequestContext { grounded, moving, sprinting } = ctx;
    if let Some(cur) = current {
        if !cur.can_be_cancelled() {
            return None;
        }
    }
    let kind = match request {
        ActionRequest::Roll if moving => ActionKind::Roll,
        ActionRequest::Roll => ActionKind::Backstep,
        ActionRequest::LightAttack if !grounded => ActionKind::JumpAttack,
        ActionRequest::LightAttack if sprinting && current.is_none() => ActionKind::RunningAttack,
        ActionRequest::LightAttack => match current.map(|a| a.kind) {
            Some(ActionKind::LightAttack(n)) if n + 1 < LIGHT_COMBO_MAX => ActionKind::LightAttack(n + 1),
            _ => ActionKind::LightAttack(0),
        },
        ActionRequest::HeavyAttack => ActionKind::HeavyCharge,
        ActionRequest::DrinkAle => ActionKind::DrinkAle,
    };
    let def = action_def(kind);
    let ground_ok = match def.ground {
        GroundRule::Ground => grounded,
        GroundRule::Air => !grounded,
        GroundRule::Any => true,
    };
    if !ground_ok || (def.stamina_cost > 0.0 && !stamina.can_spend()) {
        return None;
    }
    Some(kind)
}

/// Starts a buffered action when allowed.
pub(super) fn start_buffered_action(
    mut q: Query<(&mut CharacterAction, &mut ActionIntent, &MoveIntent, &mut Stamina, &mut CharacterController)>,
) {
    for (mut action, mut intent, movement, mut stamina, mut ctrl) in &mut q {
        let Some(request) = intent.buffered else { continue };
        let moving = movement.dir.length_squared() > 0.01;
        let ctx = RequestContext { grounded: ctrl.grounded, moving, sprinting: ctrl.sprinting };
        let Some(kind) = resolve_request(action.current.as_ref(), request, ctx, &stamina) else {
            continue;
        };
        // Direction: input direction if any, otherwise where the character faces.
        let facing = Vec3::new(-ctrl.facing.sin(), 0.0, -ctrl.facing.cos());
        let dir = match kind {
            ActionKind::Backstep => -facing,
            _ if moving => movement.dir.normalize(),
            _ => facing,
        };
        // Turn instantly toward the action (roll / attack direction). A backstep keeps facing.
        if kind != ActionKind::Backstep {
            ctrl.facing = f32::atan2(-dir.x, -dir.z);
        }
        let def = action_def(kind);
        stamina.spend(def.stamina_cost);
        if def.hop > 0.0 {
            ctrl.velocity.y = def.hop;
            ctrl.grounded = false;
        }
        action.start(ActiveAction::new(kind, dir));
        intent.buffered = None;
    }
}

/// Turns a heavy charge into the heavy swing when LMB is released or the charge is full.
pub(super) fn release_heavy_charge(mut q: Query<(&mut CharacterAction, &ActionIntent)>) {
    for (mut action, intent) in &mut q {
        let Some(cur) = action.current else { continue };
        if cur.kind != ActionKind::HeavyCharge || (intent.attack_held && cur.elapsed < HEAVY_CHARGE_MAX) {
            continue;
        }
        let mut heavy = ActiveAction::new(ActionKind::HeavyAttack, cur.dir);
        heavy.charge = (cur.elapsed / HEAVY_CHARGE_MAX).clamp(0.0, 1.0);
        action.start(heavy);
    }
}

/// Advances the running action, toggles i-frames, ends finished actions.
pub(super) fn tick_action(
    time: Res<Time>,
    mut commands: Commands,
    mut q: Query<(Entity, &mut CharacterAction, Has<Invulnerable>, &mut Health, &mut Stamina, &mut Transform, &mut CharacterController)>,
) {
    let dt = time.delta_secs();
    for (entity, mut action, invulnerable, mut health, mut stamina, mut tf, mut ctrl) in &mut q {
        let mut iframes = false;
        if let Some(cur) = action.current.as_mut() {
            cur.elapsed += dt;
            // Jump attack: landing ends the plunge; the slam stays active briefly.
            // Never skip the swing itself (its hit sweep would be skipped too).
            let swing_done = cur.elapsed >= cur.def.startup + JUMP_ATTACK_STRIKE;
            if cur.kind == ActionKind::JumpAttack && cur.phase() == ActionPhase::Active && ctrl.grounded && swing_done {
                let slam_start = cur.def.startup + cur.def.active - JUMP_SLAM_TIME;
                cur.elapsed = cur.elapsed.max(slam_start);
            }
            iframes = cur.in_iframes();
            if cur.phase() == ActionPhase::Done {
                if cur.kind == ActionKind::Dead {
                    // Placeholder respawn until the Hearth exists (Phase 6).
                    let (x, y, z) = PLAYER_SPAWN;
                    tf.translation = Vec3::new(x, y, z);
                    ctrl.velocity = Vec3::ZERO;
                    *health = Health::new(health.max);
                    *stamina = Stamina::new(stamina.max);
                }
                action.current = None;
            }
        }
        match (iframes, invulnerable) {
            (true, false) => {
                commands.entity(entity).insert(Invulnerable);
            }
            (false, true) => {
                commands.entity(entity).remove::<Invulnerable>();
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(grounded: bool, moving: bool) -> RequestContext {
        RequestContext { grounded, moving, sprinting: false }
    }

    #[test]
    fn sprinting_light_attack_is_a_running_attack_with_a_leap() {
        let s = Stamina::new(100.0);
        let sprint = RequestContext { grounded: true, moving: true, sprinting: true };
        assert_eq!(resolve_request(None, ActionRequest::LightAttack, sprint, &s), Some(ActionKind::RunningAttack));
        let def = action_def(ActionKind::RunningAttack);
        assert!(def.hop > 0.0);
        let ActionMovement::Scripted(speed) = def.movement else { panic!() };
        let dt = 1.0 / 64.0;
        let dist: f32 = (0..(def.total() / dt) as usize).map(|i| speed(i as f32 * dt) * dt).sum();
        assert!((dist - RUN_ATTACK_DISTANCE).abs() < 0.15, "leapt {dist}");
    }

    #[test]
    fn light_attack_steps_forward() {
        let ActionMovement::Scripted(speed) = action_def(ActionKind::LightAttack(0)).movement else { panic!() };
        let dt = 1.0 / 64.0;
        let dist: f32 = (0..64).map(|i| speed(i as f32 * dt) * dt).sum();
        assert!((dist - LIGHT_LUNGE).abs() < 0.05, "stepped {dist}");
    }

    fn at(kind: ActionKind, elapsed: f32) -> ActiveAction {
        ActiveAction { elapsed, ..ActiveAction::new(kind, Vec3::Z) }
    }

    #[test]
    fn phases_follow_timings() {
        let d = action_def(ActionKind::Roll);
        assert_eq!(at(ActionKind::Roll, 0.0).phase(), ActionPhase::Startup);
        assert_eq!(at(ActionKind::Roll, d.startup + 0.01).phase(), ActionPhase::Active);
        assert_eq!(at(ActionKind::Roll, d.startup + d.active + 0.01).phase(), ActionPhase::Recovery);
        assert_eq!(at(ActionKind::Roll, d.total()).phase(), ActionPhase::Done);
    }

    #[test]
    fn roll_iframes_window() {
        assert!(!at(ActionKind::Roll, 0.02).in_iframes());
        assert!(at(ActionKind::Roll, 0.06).in_iframes());
        assert!(at(ActionKind::Roll, 0.39).in_iframes());
        assert!(!at(ActionKind::Roll, 0.41).in_iframes());
        assert!(!at(ActionKind::LightAttack(0), 0.2).in_iframes());
    }

    #[test]
    fn buffer_waits_for_cancel_window() {
        let s = Stamina::new(100.0);
        let early = at(ActionKind::LightAttack(0), LIGHT_CANCEL - 0.01);
        assert_eq!(resolve_request(Some(&early), ActionRequest::Roll, ctx(true, true), &s), None);
        let late = at(ActionKind::LightAttack(0), LIGHT_CANCEL);
        assert_eq!(resolve_request(Some(&late), ActionRequest::Roll, ctx(true, true), &s), Some(ActionKind::Roll));
    }

    #[test]
    fn light_attacks_chain_and_wrap() {
        let s = Stamina::new(100.0);
        let mut kind = resolve_request(None, ActionRequest::LightAttack, ctx(true, true), &s).unwrap();
        let mut seen = vec![kind];
        for _ in 0..LIGHT_COMBO_MAX {
            kind = resolve_request(Some(&at(kind, LIGHT_CANCEL)), ActionRequest::LightAttack, ctx(true, true), &s).unwrap();
            seen.push(kind);
        }
        assert_eq!(seen[..4], [
            ActionKind::LightAttack(0),
            ActionKind::LightAttack(1),
            ActionKind::LightAttack(2),
            ActionKind::LightAttack(0)
        ]);
    }

    #[test]
    fn ground_and_stamina_rules() {
        let full = Stamina::new(100.0);
        let mut empty = Stamina::new(100.0);
        empty.spend(100.0);
        assert_eq!(resolve_request(None, ActionRequest::Roll, ctx(false, true), &full), None, "no roll in the air");
        assert_eq!(resolve_request(None, ActionRequest::LightAttack, ctx(true, true), &full), Some(ActionKind::LightAttack(0)));
        assert_eq!(
            resolve_request(None, ActionRequest::LightAttack, ctx(false, true), &full),
            Some(ActionKind::JumpAttack),
            "light attack in the air = jump attack"
        );
        assert_eq!(resolve_request(None, ActionRequest::HeavyAttack, ctx(false, true), &full), None, "no heavy in the air");
        assert_eq!(resolve_request(None, ActionRequest::HeavyAttack, ctx(true, true), &full), Some(ActionKind::HeavyCharge));
        assert_eq!(resolve_request(None, ActionRequest::Roll, ctx(true, true), &empty), None, "no stamina");
        assert_eq!(resolve_request(None, ActionRequest::Roll, ctx(true, false), &full), Some(ActionKind::Backstep), "roll standing still = backstep");
        assert_eq!(resolve_request(None, ActionRequest::Roll, ctx(false, false), &full), None, "no backstep in the air");
        assert_eq!(resolve_request(None, ActionRequest::DrinkAle, ctx(true, true), &empty), Some(ActionKind::DrinkAle));
    }

    #[test]
    fn drink_and_death_cannot_be_cancelled() {
        let s = Stamina::new(100.0);
        let d = action_def(ActionKind::DrinkAle);
        let drinking = at(ActionKind::DrinkAle, d.total() - 0.01);
        assert_eq!(resolve_request(Some(&drinking), ActionRequest::Roll, ctx(true, true), &s), None);
        let mut pa = CharacterAction { current: Some(at(ActionKind::Dead, 0.0)), serial: 0 };
        pa.force(ActionKind::Hurt, Vec3::ZERO);
        assert_eq!(pa.current.unwrap().kind, ActionKind::Dead);
    }

    #[test]
    fn roll_covers_roughly_its_distance() {
        let d = action_def(ActionKind::Roll);
        let ActionMovement::Scripted(speed) = d.movement else { panic!() };
        let dt = 1.0 / 64.0;
        let dist: f32 = (0..(d.total() / dt) as usize).map(|i| speed(i as f32 * dt) * dt).sum();
        assert!((dist - ROLL_DISTANCE).abs() < 0.4, "rolled {dist}");
    }
}
