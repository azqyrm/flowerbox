//! The action state machine of any character — the heart of the Soulslike feel.
//!
//! An *action* (roll, attack, drink, hurt…) runs through three phases:
//! **startup → active → recovery**. While an action runs it decides how the character
//! may move ([`ActionMovement`]). New actions are requested through a short **input buffer**
//! ([`crate::character::ActionIntent::buffered`]) and only start when the current action reaches its
//! **cancel window** (`ActionDef::cancel`), or when nothing is running.
//! Locomotion (idle / walk / sprint / airborne) is simply "no action running".
//!
//! Attack timings, costs, lunges and damage come from the **moveset of the weapon class** held
//! when the action starts (`crate::weapon`); `ActiveAction::weapon` remembers it.

use bevy::prelude::*;

use super::controller::CharacterController;
use super::stats::{Invulnerable, Stamina};
use super::{ActionIntent, MoveIntent};
use crate::tuning::*;
use crate::ability::{AbilityEffect, AbilityId};
use crate::weapon::{AttackHand, AttackSpec, WeaponClass, Wielded};

/// What the player asked for (from input). Turned into an [`ActionKind`] by [`resolve_request`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionRequest {
    /// Becomes a backstep when there's no movement input.
    Roll,
    /// With the given hand(s); becomes a jump attack while airborne.
    LightAttack(AttackHand),
    HeavyAttack(AttackHand),
    /// The main weapon's ability (`crate::ability`).
    Ability,
    DrinkAle,
    /// Eat/use a consumable from the bag (what it does: `crate::pvp`).
    UseItem,
    /// Kneel at a Hearth (the Hearth decides when this is allowed and what resting does).
    Rest,
    /// A creature's grab leap (only movesets with a `pounce`, e.g. the warg).
    Pounce,
    /// A specific ability, whatever the weapon (enemies' special moves, `crate::enemy::fighter`).
    Special(AbilityId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
    /// Eating a consumable (Poppy, Camomile); its effect lands at the start of the active phase.
    UseItem,
    /// Kneeling at a Hearth and staying there (active never ends); starting it revives the
    /// sunflower and sets the respawn point; the rest *completes* separately (`hearth::RestComplete`).
    Rest,
    /// Getting up from a rest.
    StandUp,
    /// Pushing a door open (it swings at the start of the active phase, `level::door`).
    OpenDoor,
    Hurt,
    /// Parried: reeling, open to a riposte.
    Stunned,
    /// Lasts until something revives the character (the Hearth respawns the player).
    Dead,
    /// Grab leap: wind-up (leaning back), the leap (`active`, carries whoever it catches), then
    /// the bite and shake (`recovery`). See `crate::grab`.
    Pounce,
    /// An activated ability (`crate::ability`): a strike with its own swing, or a buff stance.
    Ability(AbilityId),
    /// Held in a creature's jaws: no control; bitten, then let go (`crate::grab`).
    Grabbed,
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
    /// The action drives the horizontal velocity: `dir * curve.speed(elapsed)`.
    Scripted(MotionCurve),
    /// Jump attack: hang in the air during startup, then dive at `PLUNGE_SPEED`.
    Plunge,
    /// Moved by someone else: the horizontal velocity is whatever was put in
    /// `CharacterController::velocity` (a grab carries its victim; walls still stop it).
    Held,
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

/// Speed profile of a scripted movement (data, so weapon classes can define their own).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MotionCurve {
    /// Dodge: half speed in startup, constant in active (85% of `distance`), easing out in recovery.
    Dash { startup: f32, active: f32, recovery: f32, distance: f32 },
    /// Smooth forward step (sine bell) covering `distance` between `start` and `end` seconds.
    Lunge { start: f32, end: f32, distance: f32 },
}

impl MotionCurve {
    pub fn speed(&self, elapsed: f32) -> f32 {
        match *self {
            MotionCurve::Dash { startup, active, recovery, distance } => {
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
            MotionCurve::Lunge { start, end, distance } => {
                if elapsed < start || elapsed >= end {
                    return 0.0;
                }
                let t = (elapsed - start) / (end - start);
                distance * std::f32::consts::PI / (2.0 * (end - start)) * (std::f32::consts::PI * t).sin()
            }
        }
    }
}

/// An attack from a weapon moveset as an action definition; it lunges forward from 30% of the
/// startup until the end of the active window.
fn attack_def(spec: &AttackSpec, movement: Option<ActionMovement>, ground: GroundRule, hop: f32) -> ActionDef {
    let lunge = MotionCurve::Lunge { start: spec.startup * 0.3, end: spec.startup + spec.active, distance: spec.lunge };
    ActionDef {
        startup: spec.startup,
        active: spec.active,
        recovery: spec.recovery,
        cancel: spec.cancel,
        stamina_cost: spec.stamina_cost,
        iframes: None,
        movement: movement.unwrap_or(if spec.lunge > 0.0 { ActionMovement::Scripted(lunge) } else { ActionMovement::Locked }),
        ground,
        hop,
    }
}

/// The data table for every action. Dodge/drink/hurt numbers live in `tuning.rs`, attacks come
/// from the `weapon` class's moveset.
pub fn action_def(kind: ActionKind, weapon: WeaponClass) -> ActionDef {
    use ActionKind::*;
    let moves = weapon.moveset();
    match kind {
        Roll => ActionDef {
            startup: ROLL_STARTUP,
            active: ROLL_ACTIVE,
            recovery: ROLL_RECOVERY,
            cancel: ROLL_CANCEL,
            stamina_cost: ROLL_COST,
            iframes: Some((ROLL_STARTUP, ROLL_STARTUP + ROLL_ACTIVE)),
            movement: ActionMovement::Scripted(MotionCurve::Dash {
                startup: ROLL_STARTUP,
                active: ROLL_ACTIVE,
                recovery: ROLL_RECOVERY,
                distance: ROLL_DISTANCE,
            }),
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
            movement: ActionMovement::Scripted(MotionCurve::Dash {
                startup: BACKSTEP_STARTUP,
                active: BACKSTEP_ACTIVE,
                recovery: BACKSTEP_RECOVERY,
                distance: BACKSTEP_DISTANCE,
            }),
            ground: GroundRule::Ground,
            hop: 0.0,
        },
        LightAttack(n) => attack_def(moves.light(n), None, GroundRule::Ground, 0.0),
        RunningAttack => attack_def(&moves.running, None, GroundRule::Ground, moves.running_hop),
        HeavyCharge => ActionDef {
            startup: 0.0,
            // Never ends by itself: `release_heavy_charge` turns it into HeavyAttack.
            active: moves.heavy_charge_max + 10.0,
            recovery: 0.0,
            cancel: 0.0, // can roll out of a charge
            stamina_cost: moves.heavy_charge_cost,
            iframes: None,
            movement: ActionMovement::Slowed(HEAVY_CHARGE_MOVE_FACTOR),
            ground: GroundRule::Ground,
            hop: 0.0,
        },
        HeavyAttack => attack_def(&moves.heavy, None, GroundRule::Ground, 0.0),
        JumpAttack => attack_def(&moves.jump, Some(ActionMovement::Plunge), GroundRule::Air, 0.0),
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
        UseItem => ActionDef {
            startup: USE_ITEM_STARTUP,
            active: USE_ITEM_ACTIVE,
            recovery: USE_ITEM_RECOVERY,
            cancel: USE_ITEM_STARTUP + USE_ITEM_ACTIVE + USE_ITEM_RECOVERY, // committed, like drinking
            stamina_cost: 0.0,
            iframes: None,
            movement: ActionMovement::Slowed(DRINK_MOVE_FACTOR),
            ground: GroundRule::Ground,
            hop: 0.0,
        },
        // Kneel, then stay resting until something stands you up (`StandUp`).
        Rest => ActionDef {
            startup: REST_STARTUP,
            active: f32::INFINITY,
            recovery: 0.0,
            cancel: f32::INFINITY,
            stamina_cost: 0.0,
            iframes: None,
            movement: ActionMovement::Locked,
            ground: GroundRule::Ground,
            hop: 0.0,
        },
        StandUp => ActionDef {
            startup: 0.0,
            active: 0.0,
            recovery: REST_RECOVERY,
            cancel: REST_RECOVERY,
            stamina_cost: 0.0,
            iframes: None,
            movement: ActionMovement::Locked,
            ground: GroundRule::Ground,
            hop: 0.0,
        },
        OpenDoor => ActionDef {
            startup: DOOR_PUSH_STARTUP,
            active: DOOR_PUSH_ACTIVE,
            recovery: DOOR_PUSH_RECOVERY,
            cancel: DOOR_PUSH_STARTUP + DOOR_PUSH_ACTIVE + DOOR_PUSH_RECOVERY * 0.5,
            stamina_cost: 0.0,
            iframes: None,
            movement: ActionMovement::Scripted(MotionCurve::Lunge {
                start: DOOR_PUSH_STARTUP * 0.6,
                end: DOOR_PUSH_STARTUP + DOOR_PUSH_ACTIVE,
                distance: 0.25,
            }),
            ground: GroundRule::Ground,
            hop: 0.0,
        },
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
        Stunned => ActionDef {
            startup: 0.0,
            active: 0.0,
            recovery: PARRY_STUN_TIME,
            cancel: PARRY_STUN_TIME,
            stamina_cost: 0.0,
            iframes: None,
            movement: ActionMovement::Locked,
            ground: GroundRule::Any,
            hop: 0.0,
        },
        Pounce => {
            let p = moves.pounce.unwrap_or(crate::weapon::warg_fangs::POUNCE);
            ActionDef {
                startup: p.windup,
                active: p.leap,
                recovery: p.recovery,
                cancel: p.windup + p.leap + p.recovery, // committed
                stamina_cost: p.stamina_cost,
                iframes: None,
                movement: ActionMovement::Scripted(MotionCurve::Lunge { start: p.windup, end: p.windup + p.leap, distance: p.distance }),
                ground: GroundRule::Ground,
                hop: 0.0,
            }
        }
        Ability(id) => match id.def().effect {
            AbilityEffect::Strike(spec) | AbilityEffect::Spin { swing: spec, .. } | AbilityEffect::Slam { swing: spec, .. } => {
                ActionDef { stamina_cost: id.def().stamina_cost, ..attack_def(&spec, None, GroundRule::Ground, 0.0) }
            }
            AbilityEffect::Buff { startup, active, recovery, .. } | AbilityEffect::Shriek { startup, active, recovery, .. } => ActionDef {
                startup,
                active,
                recovery,
                cancel: startup + active + recovery * 0.6,
                stamina_cost: id.def().stamina_cost,
                iframes: None,
                movement: ActionMovement::Locked,
                ground: GroundRule::Ground,
                hop: 0.0,
            },
        },
        Grabbed => ActionDef {
            startup: 0.0,
            active: GRAB_HOLD,
            recovery: GRAB_RELEASE,
            cancel: GRAB_HOLD + GRAB_RELEASE, // no escape: you had to dodge
            stamina_cost: 0.0,
            iframes: None,
            movement: ActionMovement::Held,
            ground: GroundRule::Any,
            hop: 0.0,
        },
        Dead => ActionDef {
            startup: 0.0,
            active: 0.0,
            recovery: f32::INFINITY, // never ends by itself
            cancel: f32::INFINITY,
            stamina_cost: 0.0,
            iframes: None,
            movement: ActionMovement::Locked,
            ground: GroundRule::Any,
            hop: 0.0,
        },
    }
}

/// Roll weight class from the equip load (Souls-style): light gear rolls fast and far, heavy
/// gear slowly, overloaded is a short "fat roll" with few i-frames. Backsteps don't change.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RollWeight {
    Light,
    #[default]
    Medium,
    Heavy,
    Overloaded,
}

impl RollWeight {
    /// From the equip load ratio (worn weight / max load).
    pub fn from_load(ratio: f32) -> Self {
        match ratio {
            r if r <= LOAD_LIGHT => RollWeight::Light,
            r if r <= LOAD_MEDIUM => RollWeight::Medium,
            r if r <= 1.0 => RollWeight::Heavy,
            _ => RollWeight::Overloaded,
        }
    }

    /// The roll for this weight class. Medium is the base roll (`ROLL_*`).
    pub fn roll_def(self) -> ActionDef {
        // (time scale, distance scale, i-frames as a fraction of the base roll's, extra recovery).
        // The fast roll keeps the full i-frames in a shorter roll.
        let (time, dist, iframes, extra) = match self {
            RollWeight::Light => (0.85, 1.1, 1.0, 0.0),
            RollWeight::Medium => (1.0, 1.0, 1.0, 0.0),
            RollWeight::Heavy => (1.2, 0.8, 0.8, 0.12),
            RollWeight::Overloaded => (1.4, 0.45, 0.45, 0.45),
        };
        let (startup, active, recovery) = (ROLL_STARTUP * time, ROLL_ACTIVE * time, ROLL_RECOVERY * time + extra);
        ActionDef {
            startup,
            active,
            recovery,
            cancel: ROLL_CANCEL * time + extra,
            stamina_cost: ROLL_COST,
            iframes: Some((startup, startup + ROLL_ACTIVE * iframes)),
            movement: ActionMovement::Scripted(MotionCurve::Dash { startup, active, recovery, distance: ROLL_DISTANCE * dist }),
            ground: GroundRule::Ground,
            hop: 0.0,
        }
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
    /// Weapon class held when the action started (its moveset drives attacks and their poses).
    pub weapon: WeaponClass,
    /// Which hand(s) an attack swings (the left mirrors the right's swing; both = the dual attack).
    pub hand: AttackHand,
}

impl ActiveAction {
    pub fn new(kind: ActionKind, dir: Vec3, weapon: WeaponClass) -> Self {
        Self { kind, def: action_def(kind, weapon), elapsed: 0.0, dir, charge: 0.0, weapon, hand: AttackHand::Right }
    }

    /// The same action swung with `hand`: a dual attack (both) costs `DUAL_STAMINA_MULT` × stamina.
    pub fn with_hand(mut self, hand: AttackHand) -> Self {
        if hand == AttackHand::Both && self.hand != AttackHand::Both {
            self.def.stamina_cost *= DUAL_STAMINA_MULT;
        }
        self.hand = hand;
        self
    }

    /// The moveset attack this action performs, if it's an attack.
    pub fn attack(&self) -> Option<&'static AttackSpec> {
        let moves = self.weapon.moveset();
        Some(match self.kind {
            ActionKind::LightAttack(n) => moves.light(n),
            ActionKind::RunningAttack => &moves.running,
            ActionKind::HeavyCharge | ActionKind::HeavyAttack => &moves.heavy,
            ActionKind::JumpAttack => &moves.jump,
            ActionKind::Ability(id) => return id.strike(),
            _ => return None,
        })
    }

    /// Damage multiplier if this action hits (charged heavies scale up to the full-charge value).
    pub fn damage_multiplier(&self) -> Option<f32> {
        let moves = self.weapon.moveset();
        match self.kind {
            ActionKind::HeavyCharge => None,
            ActionKind::HeavyAttack => {
                Some(moves.heavy.damage_mult + (moves.heavy_full_charge_mult - moves.heavy.damage_mult) * self.charge)
            }
            _ => self.attack().map(|a| a.damage_mult),
        }
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

    /// Ends any action, including Dead (respawning).
    pub fn revive(&mut self) {
        self.current = None;
        self.serial = self.serial.wrapping_add(1);
    }

    /// Starts an action immediately, interrupting anything (used for Hurt / Dead).
    pub fn force(&mut self, kind: ActionKind, dir: Vec3) {
        if matches!(self.current, Some(a) if a.kind == ActionKind::Dead) {
            return; // nothing interrupts death
        }
        let weapon = self.current.map_or_else(WeaponClass::default, |a| a.weapon);
        self.start(ActiveAction::new(kind, dir, weapon));
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
    /// Weapon class of the requested hand (combo length, attack availability, ability).
    pub weapon: WeaponClass,
}

/// Decides whether `request` can start now. `None` = not yet (keep it buffered) or never.
pub fn resolve_request(
    current: Option<&ActiveAction>,
    request: ActionRequest,
    ctx: RequestContext,
    stamina: &Stamina,
) -> Option<ActionKind> {
    let RequestContext { grounded, moving, sprinting, weapon } = ctx;
    if let Some(cur) = current
        && !cur.can_be_cancelled() {
            return None;
        }
    let kind = match request {
        ActionRequest::Roll if moving => ActionKind::Roll,
        ActionRequest::Roll => ActionKind::Backstep,
        ActionRequest::LightAttack(_) if !grounded => ActionKind::JumpAttack,
        ActionRequest::LightAttack(_) if sprinting && current.is_none() => ActionKind::RunningAttack,
        // The combo continues with the same hand(s).
        ActionRequest::LightAttack(hand) => match current.map(|a| (a.kind, a.hand)) {
            Some((ActionKind::LightAttack(n), h)) if h == hand && ((n + 1) as usize) < weapon.moveset().light.len() => {
                ActionKind::LightAttack(n + 1)
            }
            _ => ActionKind::LightAttack(0),
        },
        ActionRequest::HeavyAttack(_) => ActionKind::HeavyCharge,
        ActionRequest::Ability => ActionKind::Ability(weapon.moveset().ability?),
        ActionRequest::DrinkAle => ActionKind::DrinkAle,
        ActionRequest::UseItem => ActionKind::UseItem,
        ActionRequest::Rest => ActionKind::Rest,
        ActionRequest::Pounce if weapon.moveset().pounce.is_some() => ActionKind::Pounce,
        ActionRequest::Pounce => return None,
        ActionRequest::Special(id) => ActionKind::Ability(id),
    };
    let def = action_def(kind, weapon);
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
    mut q: Query<(
        &mut CharacterAction,
        &mut ActionIntent,
        &MoveIntent,
        &mut Stamina,
        &mut CharacterController,
        Option<&Wielded>,
        Option<&RollWeight>,
    )>,
) {
    for (mut action, mut intent, movement, mut stamina, mut ctrl, wielded, roll_weight) in &mut q {
        let Some(request) = intent.buffered else { continue };
        // Which hand(s) swing: the left only with a left weapon, both only with a matching pair.
        let hand = match request {
            ActionRequest::LightAttack(h) | ActionRequest::HeavyAttack(h) => h,
            _ => AttackHand::Right,
        };
        let usable = match hand {
            AttackHand::Right => true,
            AttackHand::Left => wielded.is_some_and(Wielded::left_attacks),
            AttackHand::Both => wielded.is_some_and(|w| w.dual_class().is_some()),
        };
        if !usable {
            intent.buffered = None;
            continue;
        }
        let weapon = wielded.map_or_else(WeaponClass::default, |w| w.class_for(hand));
        let moving = movement.dir.length_squared() > 0.01;
        let ctx = RequestContext { grounded: ctrl.grounded, moving, sprinting: ctrl.sprinting, weapon };
        let Some(kind) = resolve_request(action.current.as_ref(), request, ctx, &stamina) else {
            continue;
        };
        // Direction: input direction if any, otherwise where the character faces.
        let facing = Vec3::new(-ctrl.facing.sin(), 0.0, -ctrl.facing.cos());
        let is_attack = matches!(
            kind,
            ActionKind::LightAttack(_) | ActionKind::HeavyCharge | ActionKind::RunningAttack | ActionKind::JumpAttack | ActionKind::Ability(_)
        );
        let dir = match kind {
            ActionKind::Backstep => -facing,
            // Locked on: attacks go at the target, whatever the stick says.
            _ if is_attack && movement.face.is_some() => movement.face.unwrap_or(facing),
            _ if moving => movement.dir.normalize(),
            _ => facing,
        };
        // Turn instantly toward the action (roll / attack direction). A backstep keeps facing.
        if kind != ActionKind::Backstep {
            ctrl.facing = f32::atan2(-dir.x, -dir.z);
        }
        let mut started = ActiveAction::new(kind, dir, weapon);
        if is_attack && !matches!(kind, ActionKind::Ability(_)) {
            started = started.with_hand(hand);
        }
        if let (ActionKind::Roll, Some(w)) = (kind, roll_weight) {
            started.def = w.roll_def();
        }
        let def = started.def;
        stamina.spend(def.stamina_cost);
        if def.hop > 0.0 {
            ctrl.velocity.y = def.hop;
            ctrl.grounded = false;
        }
        action.start(started);
        intent.buffered = None;
    }
}

/// Turns a heavy charge into the heavy swing when LMB is released or the charge is full.
pub(super) fn release_heavy_charge(mut q: Query<(&mut CharacterAction, &ActionIntent)>) {
    for (mut action, intent) in &mut q {
        let Some(cur) = action.current else { continue };
        let charge_max = cur.weapon.moveset().heavy_charge_max;
        if cur.kind != ActionKind::HeavyCharge || (intent.attack_held && cur.elapsed < charge_max) {
            continue;
        }
        let mut heavy = ActiveAction::new(ActionKind::HeavyAttack, cur.dir, cur.weapon);
        heavy.hand = cur.hand; // its stamina was paid with the charge
        heavy.charge = (cur.elapsed / charge_max).clamp(0.0, 1.0);
        action.start(heavy);
    }
}

/// Advances the running action, toggles i-frames, ends finished actions.
pub(super) fn tick_action(
    time: Res<Time>,
    mut commands: Commands,
    mut q: Query<(Entity, &mut CharacterAction, Has<Invulnerable>, &CharacterController)>,
) {
    let dt = time.delta_secs();
    for (entity, mut action, invulnerable, ctrl) in &mut q {
        let mut iframes = false;
        if let Some(cur) = action.current.as_mut() {
            cur.elapsed += dt;
            // Jump attack: landing ends the plunge; the slam stays active briefly.
            // Never skip the swing itself (its hit sweep would be skipped too).
            let swing_done = cur.elapsed >= cur.def.startup + cur.weapon.moveset().jump_strike_time;
            if cur.kind == ActionKind::JumpAttack && cur.phase() == ActionPhase::Active && ctrl.grounded && swing_done {
                let slam_start = cur.def.startup + cur.def.active - JUMP_SLAM_TIME;
                cur.elapsed = cur.elapsed.max(slam_start);
            }
            iframes = cur.in_iframes();
            if cur.phase() == ActionPhase::Done {
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

    const W: WeaponClass = WeaponClass::LightSword;
    fn moves() -> &'static crate::weapon::Moveset {
        W.moveset()
    }

    fn ctx(grounded: bool, moving: bool) -> RequestContext {
        RequestContext { grounded, moving, sprinting: false, weapon: W }
    }

    #[test]
    fn dual_attacks_cost_one_and_a_half_times_the_stamina() {
        let one = ActiveAction::new(ActionKind::LightAttack(0), Vec3::NEG_Z, W);
        let both = ActiveAction::new(ActionKind::LightAttack(0), Vec3::NEG_Z, W).with_hand(AttackHand::Both);
        assert!((both.def.stamina_cost - one.def.stamina_cost * DUAL_STAMINA_MULT).abs() < 1e-4);
        assert_eq!(both.with_hand(AttackHand::Both).def.stamina_cost, both.def.stamina_cost, "not twice");
    }

    #[test]
    fn abilities_come_from_the_class() {
        let s = Stamina::new(100.0);
        let ctx = RequestContext { grounded: true, moving: false, sprinting: false, weapon: WeaponClass::Axe };
        assert_eq!(resolve_request(None, ActionRequest::Ability, ctx, &s), Some(ActionKind::Ability(AbilityId::Warcry)));
        let fangs = RequestContext { weapon: WeaponClass::WargFangs, ..ctx };
        assert_eq!(resolve_request(None, ActionRequest::Ability, fangs, &s), None, "no ability");
    }

    #[test]
    fn heavier_loads_roll_slower_and_shorter() {
        let dist = |w: RollWeight| match w.roll_def().movement {
            ActionMovement::Scripted(MotionCurve::Dash { distance, .. }) => distance,
            _ => panic!(),
        };
        let order = [RollWeight::Light, RollWeight::Medium, RollWeight::Heavy, RollWeight::Overloaded];
        for pair in order.windows(2) {
            let (a, b) = (pair[0].roll_def(), pair[1].roll_def());
            assert!(a.total() < b.total() && dist(pair[0]) > dist(pair[1]), "{pair:?}");
            let i = |d: ActionDef| d.iframes.map_or(0.0, |(s, e)| e - s);
            assert!(i(a) >= i(b), "{pair:?}");
        }
        assert_eq!(RollWeight::Medium.roll_def().total(), action_def(ActionKind::Roll, W).total(), "medium = base roll");
        assert_eq!(RollWeight::from_load(0.1), RollWeight::Light);
        assert_eq!(RollWeight::from_load(1.2), RollWeight::Overloaded);
    }

    #[test]
    fn sprinting_light_attack_is_a_running_attack_with_a_leap() {
        let s = Stamina::new(100.0);
        let sprint = RequestContext { grounded: true, moving: true, sprinting: true, weapon: W };
        assert_eq!(resolve_request(None, ActionRequest::LightAttack(AttackHand::Right), sprint, &s), Some(ActionKind::RunningAttack));
        let def = action_def(ActionKind::RunningAttack, W);
        assert!(def.hop > 0.0);
        let ActionMovement::Scripted(curve) = def.movement else { panic!() };
        let dt = 1.0 / 64.0;
        let dist: f32 = (0..(def.total() / dt) as usize).map(|i| curve.speed(i as f32 * dt) * dt).sum();
        assert!((dist - moves().running.lunge).abs() < 0.15, "leapt {dist}");
    }

    #[test]
    fn light_attack_steps_forward() {
        let ActionMovement::Scripted(curve) = action_def(ActionKind::LightAttack(0), W).movement else { panic!() };
        let dt = 1.0 / 64.0;
        let dist: f32 = (0..64).map(|i| curve.speed(i as f32 * dt) * dt).sum();
        assert!((dist - moves().light(0).lunge).abs() < 0.05, "stepped {dist}");
    }

    fn at(kind: ActionKind, elapsed: f32) -> ActiveAction {
        ActiveAction { elapsed, ..ActiveAction::new(kind, Vec3::Z, W) }
    }

    #[test]
    fn phases_follow_timings() {
        let d = action_def(ActionKind::Roll, W);
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
        let early = at(ActionKind::LightAttack(0), moves().light(0).cancel - 0.01);
        assert_eq!(resolve_request(Some(&early), ActionRequest::Roll, ctx(true, true), &s), None);
        let late = at(ActionKind::LightAttack(0), moves().light(0).cancel);
        assert_eq!(resolve_request(Some(&late), ActionRequest::Roll, ctx(true, true), &s), Some(ActionKind::Roll));
    }

    #[test]
    fn light_attacks_chain_and_wrap() {
        let s = Stamina::new(100.0);
        let mut kind = resolve_request(None, ActionRequest::LightAttack(AttackHand::Right), ctx(true, true), &s).unwrap();
        let mut seen = vec![kind];
        for _ in 0..moves().light.len() {
            kind = resolve_request(Some(&at(kind, moves().light(0).cancel)), ActionRequest::LightAttack(AttackHand::Right), ctx(true, true), &s).unwrap();
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
        assert_eq!(resolve_request(None, ActionRequest::LightAttack(AttackHand::Right), ctx(true, true), &full), Some(ActionKind::LightAttack(0)));
        assert_eq!(
            resolve_request(None, ActionRequest::LightAttack(AttackHand::Right), ctx(false, true), &full),
            Some(ActionKind::JumpAttack),
            "light attack in the air = jump attack"
        );
        assert_eq!(resolve_request(None, ActionRequest::HeavyAttack(AttackHand::Right), ctx(false, true), &full), None, "no heavy in the air");
        assert_eq!(resolve_request(None, ActionRequest::HeavyAttack(AttackHand::Right), ctx(true, true), &full), Some(ActionKind::HeavyCharge));
        assert_eq!(resolve_request(None, ActionRequest::Roll, ctx(true, true), &empty), None, "no stamina");
        assert_eq!(resolve_request(None, ActionRequest::Roll, ctx(true, false), &full), Some(ActionKind::Backstep), "roll standing still = backstep");
        assert_eq!(resolve_request(None, ActionRequest::Roll, ctx(false, false), &full), None, "no backstep in the air");
        assert_eq!(resolve_request(None, ActionRequest::DrinkAle, ctx(true, true), &empty), Some(ActionKind::DrinkAle));
    }

    #[test]
    fn drink_and_death_cannot_be_cancelled() {
        let s = Stamina::new(100.0);
        let d = action_def(ActionKind::DrinkAle, W);
        let drinking = at(ActionKind::DrinkAle, d.total() - 0.01);
        assert_eq!(resolve_request(Some(&drinking), ActionRequest::Roll, ctx(true, true), &s), None);
        let mut pa = CharacterAction { current: Some(at(ActionKind::Dead, 0.0)), serial: 0 };
        pa.force(ActionKind::Hurt, Vec3::ZERO);
        assert_eq!(pa.current.unwrap().kind, ActionKind::Dead);
        // Death never ends by itself; only `revive` ends it.
        let long_dead = at(ActionKind::Dead, 1000.0);
        assert_eq!(long_dead.phase(), ActionPhase::Recovery);
        pa.revive();
        assert!(pa.current.is_none());
    }

    #[test]
    fn rest_is_grounded_and_uninterruptible() {
        let s = Stamina::new(0.0);
        assert_eq!(resolve_request(None, ActionRequest::Rest, ctx(true, false), &s), Some(ActionKind::Rest), "no stamina needed");
        assert_eq!(resolve_request(None, ActionRequest::Rest, ctx(false, false), &s), None, "not in the air");
        let resting = at(ActionKind::Rest, 0.5);
        assert_eq!(resolve_request(Some(&resting), ActionRequest::Roll, ctx(true, true), &Stamina::new(100.0)), None);
    }

    #[test]
    fn roll_covers_roughly_its_distance() {
        let d = action_def(ActionKind::Roll, W);
        let ActionMovement::Scripted(curve) = d.movement else { panic!() };
        let dt = 1.0 / 64.0;
        let dist: f32 = (0..(d.total() / dt) as usize).map(|i| curve.speed(i as f32 * dt) * dt).sum();
        assert!((dist - ROLL_DISTANCE).abs() < 0.4, "rolled {dist}");
    }
}
