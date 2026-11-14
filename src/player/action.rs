//! The player's action state machine — the heart of the Soulslike feel.
//!
//! An *action* (roll, attack, drink, hurt…) runs through three phases:
//! **startup → active → recovery**. While an action runs it decides how the character
//! may move ([`ActionMovement`]). New actions are requested through a short **input buffer**
//! ([`PlayerInput::buffered`]) and only start when the current action reaches its
//! **cancel window** (`ActionDef::cancel`), or when nothing is running.
//! Locomotion (idle / walk / sprint / airborne) is simply "no action running".

use bevy::prelude::*;

use super::controller::CharacterController;
use super::stats::{Health, Invulnerable, Stamina};
use crate::input::PlayerInput;
use crate::tuning::*;

/// What the player asked for (from input). Turned into an [`ActionKind`] by [`resolve_request`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionRequest {
    Roll,
    Backstep,
    LightAttack,
    HeavyAttack,
    JumpAttack,
    DrinkAle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionKind {
    Roll,
    Backstep,
    /// Combo index, 0-based.
    LightAttack(u8),
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
        },
        LightAttack(_) => ActionDef {
            startup: LIGHT_STARTUP,
            active: LIGHT_ACTIVE,
            recovery: LIGHT_RECOVERY,
            cancel: LIGHT_CANCEL,
            stamina_cost: LIGHT_COST,
            iframes: None,
            movement: ActionMovement::Locked,
            ground: GroundRule::Ground,
        },
        HeavyAttack => ActionDef {
            startup: HEAVY_STARTUP,
            active: HEAVY_ACTIVE,
            recovery: HEAVY_RECOVERY,
            cancel: HEAVY_CANCEL,
            stamina_cost: HEAVY_COST,
            iframes: None,
            movement: ActionMovement::Locked,
            ground: GroundRule::Ground,
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
                movement: ActionMovement::Slowed(0.3),
                ground: GroundRule::Air,
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
}

impl ActiveAction {
    pub fn new(kind: ActionKind, dir: Vec3) -> Self {
        Self { kind, def: action_def(kind), elapsed: 0.0, dir }
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

/// Current action of the player; `None` = free locomotion.
#[derive(Component, Debug, Default)]
pub struct PlayerAction {
    pub current: Option<ActiveAction>,
}

impl PlayerAction {
    /// Starts an action immediately, interrupting anything (used for Hurt / Dead).
    pub fn force(&mut self, kind: ActionKind, dir: Vec3) {
        if matches!(self.current, Some(a) if a.kind == ActionKind::Dead) {
            return; // nothing interrupts death
        }
        self.current = Some(ActiveAction::new(kind, dir));
    }

    pub fn movement(&self) -> ActionMovement {
        self.current.map_or(ActionMovement::Free, |a| a.def.movement)
    }
}

/// Decides whether `request` can start now. `None` = not yet (keep it buffered) or never.
pub fn resolve_request(
    current: Option<&ActiveAction>,
    request: ActionRequest,
    grounded: bool,
    stamina: &Stamina,
) -> Option<ActionKind> {
    if let Some(cur) = current {
        if !cur.can_be_cancelled() {
            return None;
        }
    }
    let kind = match request {
        ActionRequest::Roll => ActionKind::Roll,
        ActionRequest::Backstep => ActionKind::Backstep,
        ActionRequest::LightAttack => match current.map(|a| a.kind) {
            Some(ActionKind::LightAttack(n)) if n + 1 < LIGHT_COMBO_MAX => ActionKind::LightAttack(n + 1),
            _ => ActionKind::LightAttack(0),
        },
        ActionRequest::HeavyAttack => ActionKind::HeavyAttack,
        ActionRequest::JumpAttack => ActionKind::JumpAttack,
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
    mut q: Query<(&mut PlayerAction, &mut PlayerInput, &mut Stamina, &CharacterController)>,
) {
    for (mut action, mut input, mut stamina, ctrl) in &mut q {
        let Some(request) = input.buffered else { continue };
        let Some(kind) = resolve_request(action.current.as_ref(), request, ctrl.grounded, &stamina) else {
            continue;
        };
        // Direction: input direction if any, otherwise where the character faces.
        let facing = Vec3::new(-ctrl.facing.sin(), 0.0, -ctrl.facing.cos());
        let dir = match kind {
            ActionKind::Backstep => -facing,
            _ if input.move_dir.length_squared() > 0.01 => input.move_dir.normalize(),
            _ => facing,
        };
        stamina.spend(action_def(kind).stamina_cost);
        action.current = Some(ActiveAction::new(kind, dir));
        input.buffered = None;
    }
}

/// Advances the running action, toggles i-frames, ends finished actions.
pub(super) fn tick_action(
    time: Res<Time>,
    mut commands: Commands,
    mut q: Query<(Entity, &mut PlayerAction, Has<Invulnerable>, &mut Health, &mut Stamina, &mut Transform, &mut CharacterController)>,
) {
    let dt = time.delta_secs();
    for (entity, mut action, invulnerable, mut health, mut stamina, mut tf, mut ctrl) in &mut q {
        let mut iframes = false;
        if let Some(cur) = action.current.as_mut() {
            cur.elapsed += dt;
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
        assert_eq!(resolve_request(Some(&early), ActionRequest::Roll, true, &s), None);
        let late = at(ActionKind::LightAttack(0), LIGHT_CANCEL);
        assert_eq!(resolve_request(Some(&late), ActionRequest::Roll, true, &s), Some(ActionKind::Roll));
    }

    #[test]
    fn light_attacks_chain_and_wrap() {
        let s = Stamina::new(100.0);
        let mut kind = resolve_request(None, ActionRequest::LightAttack, true, &s).unwrap();
        let mut seen = vec![kind];
        for _ in 0..LIGHT_COMBO_MAX {
            kind = resolve_request(Some(&at(kind, LIGHT_CANCEL)), ActionRequest::LightAttack, true, &s).unwrap();
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
        assert_eq!(resolve_request(None, ActionRequest::Roll, false, &full), None, "no roll in the air");
        assert_eq!(resolve_request(None, ActionRequest::JumpAttack, true, &full), None, "jump attack needs air");
        assert_eq!(resolve_request(None, ActionRequest::JumpAttack, false, &full), Some(ActionKind::JumpAttack));
        assert_eq!(resolve_request(None, ActionRequest::Roll, true, &empty), None, "no stamina");
        assert_eq!(resolve_request(None, ActionRequest::DrinkAle, true, &empty), Some(ActionKind::DrinkAle));
    }

    #[test]
    fn drink_and_death_cannot_be_cancelled() {
        let s = Stamina::new(100.0);
        let d = action_def(ActionKind::DrinkAle);
        let drinking = at(ActionKind::DrinkAle, d.total() - 0.01);
        assert_eq!(resolve_request(Some(&drinking), ActionRequest::Roll, true, &s), None);
        let mut pa = PlayerAction { current: Some(at(ActionKind::Dead, 0.0)) };
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
