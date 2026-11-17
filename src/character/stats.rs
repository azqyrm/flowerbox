//! Health, stamina, invulnerability and damage application.

use bevy::prelude::*;

use super::action::{ActionKind, CharacterAction};
use super::controller::CharacterController;
use super::guard::{Blocked, Guard, GuardOutcome, Parried};
use crate::tuning::*;

#[derive(Component, Debug, Clone)]
pub struct Health {
    pub current: f32,
    pub max: f32,
}

impl Health {
    pub fn new(max: f32) -> Self {
        Self { current: max, max }
    }
    pub fn is_dead(&self) -> bool {
        self.current <= 0.0
    }
}

#[derive(Component, Debug, Clone)]
pub struct Stamina {
    pub current: f32,
    pub max: f32,
    /// Seconds until regeneration resumes.
    pub regen_delay: f32,
}

impl Stamina {
    pub fn new(max: f32) -> Self {
        Self { current: max, max, regen_delay: 0.0 }
    }

    /// Souls rule: you may act with any positive stamina; the bar then bottoms out at 0.
    pub fn can_spend(&self) -> bool {
        self.current > 0.0
    }

    pub fn spend(&mut self, amount: f32) {
        if amount > 0.0 {
            self.current = (self.current - amount).max(0.0);
            self.regen_delay = STAMINA_REGEN_DELAY;
        }
    }

    /// Advances regeneration. `blocked` = an action or sprint is in progress.
    pub fn tick(&mut self, dt: f32, blocked: bool) {
        if blocked {
            return;
        }
        if self.regen_delay > 0.0 {
            self.regen_delay = (self.regen_delay - dt).max(0.0);
            return;
        }
        self.current = (self.current + STAMINA_REGEN * dt).min(self.max);
    }
}

/// Players only: seconds during which further damage is ignored after a hit (`HIT_IMMUNITY`).
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct HitImmunity(pub f32);

/// Stance (optional), Elden Ring style: a bar that fills with the stance damage of every hit
/// (`DamageMessage::stance`). While it isn't full, hits don't flinch; the hit that fills it
/// **breaks the stance**: an enemy reels (`Stunned`, open to a critical riposte), a player
/// staggers (`Hurt`). Then it empties. Stance damage drains after `STANCE_DRAIN_DELAY` without
/// hits. Without this component every hit flinches (the dummy).
#[derive(Component, Debug, Clone, Copy)]
pub struct Stance {
    /// How much stance damage breaks it (creature base, plus armour for players).
    pub max: f32,
    /// Stance damage taken so far.
    pub damage: f32,
    pub since_hit: f32,
    /// A break stuns (enemies: riposte window) instead of a plain stagger (players).
    pub break_stuns: bool,
}

impl Stance {
    pub fn new(max: f32, break_stuns: bool) -> Self {
        Self { max, damage: 0.0, since_hit: 0.0, break_stuns }
    }

    /// Takes `amount` stance damage; true if that breaks the stance (the bar empties again).
    pub fn hit(&mut self, amount: f32) -> bool {
        if amount <= 0.0 {
            return false;
        }
        self.since_hit = 0.0;
        self.damage += amount;
        if self.damage < self.max {
            return false;
        }
        self.damage = 0.0;
        true
    }

    /// How full the bar is, 0..=1.
    pub fn fraction(&self) -> f32 {
        if self.max <= 0.0 { 0.0 } else { (self.damage / self.max).clamp(0.0, 1.0) }
    }

    /// Drains after a while without hits.
    pub fn tick(&mut self, dt: f32) {
        self.since_hit += dt;
        if self.since_hit > STANCE_DRAIN_DELAY {
            self.damage = (self.damage - self.max * STANCE_DRAIN_RATE * dt).max(0.0);
        }
    }
}

/// Fraction of incoming damage this character shrugs off (armour). Optional.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct DamageAbsorption(pub f32);

/// Present while the entity ignores damage (roll i-frames, etc.).
#[derive(Component, Debug)]
pub struct Invulnerable;

/// Request to damage an entity. Sent by combat, the debug key, enemies later.
#[derive(Message, Debug, Clone)]
pub struct DamageMessage {
    pub target: Entity,
    pub amount: f32,
    /// Where the hit came from (attacker position), for knockback / reactions and guarding.
    pub from: Option<Vec3>,
    /// Who dealt it (a parry stuns them), if anyone.
    pub attacker: Option<Entity>,
    /// Stance damage (see [`Stance`]).
    pub stance: f32,
}

impl DamageMessage {
    /// A hit whose stance damage equals its damage (traps, shockwaves, debug keys).
    pub fn plain(target: Entity, amount: f32, from: Option<Vec3>, attacker: Option<Entity>) -> Self {
        Self { target, amount, from, attacker, stance: amount }
    }
}

/// Damage that was actually applied (not blocked by i-frames). For hit reactions.
#[derive(Message, Debug, Clone)]
pub struct DamageTaken {
    pub target: Entity,
    pub amount: f32,
    pub from: Option<Vec3>,
    pub killed: bool,
    /// Who dealt it, if known (`DamageMessage::attacker`).
    pub attacker: Option<Entity>,
}

/// Applies damage to anything with `Health`: i-frames, then the guard (parry / block / guard
/// break, see [`super::guard`]), then armour absorption. Characters flinch (Hurt) or die.
#[allow(clippy::type_complexity)]
pub fn apply_damage(
    mut messages: MessageReader<DamageMessage>,
    mut taken: MessageWriter<DamageTaken>,
    mut parried: MessageWriter<Parried>,
    mut blocked: MessageWriter<Blocked>,
    mut q: Query<(
        &mut Health,
        Option<&mut CharacterAction>,
        Has<Invulnerable>,
        Option<&DamageAbsorption>,
        Option<(&mut Guard, &mut Stamina, &CharacterController, &Transform)>,
        Option<&mut HitImmunity>,
        Option<&mut Stance>,
    ), Without<super::Remote>>,
) {
    for msg in messages.read() {
        let Ok((mut health, action, invulnerable, absorption, guard, immunity, stance)) = q.get_mut(msg.target) else { continue };
        if invulnerable || health.is_dead() {
            info!("damage {} ignored (invulnerable={invulnerable})", msg.amount);
            continue;
        }
        if immunity.as_ref().is_some_and(|i| i.0 > 0.0) {
            info!("damage {} ignored (just hit)", msg.amount);
            continue;
        }
        let mut amount = msg.amount;
        let mut stagger = true;
        if let Some((mut guard, mut stamina, ctrl, tf)) = guard {
            let to_attacker = msg.from.map(|p| p - tf.translation);
            match guard.outcome(ctrl.facing, to_attacker, amount, stamina.current) {
                GuardOutcome::Unguarded => {}
                GuardOutcome::Parried => {
                    info!("parried {} damage", msg.amount);
                    parried.write(Parried { defender: msg.target, attacker: msg.attacker });
                    continue;
                }
                GuardOutcome::Blocked { through, stamina_cost } => {
                    info!("blocked {} damage ({through:.0} through, {stamina_cost:.0} stamina)", msg.amount);
                    stamina.spend(stamina_cost);
                    blocked.write(Blocked { defender: msg.target });
                    amount = through;
                    stagger = false;
                }
                GuardOutcome::Broken { through } => {
                    info!("guard broken by {} damage ({through:.0} through)", msg.amount);
                    stamina.current = 0.0;
                    stamina.regen_delay = STAMINA_REGEN_DELAY;
                    guard.raised_for = None;
                    blocked.write(Blocked { defender: msg.target });
                    amount = through;
                }
            }
        }
        if let Some(a) = absorption.filter(|a| a.0 > 0.0) {
            amount *= 1.0 - a.0;
        }
        let amount = if amount.is_finite() { amount.round() } else { amount };
        health.current = (health.current - amount).max(0.0);
        if let Some(mut i) = immunity {
            i.0 = HIT_IMMUNITY;
        }
        let killed = health.is_dead();
        // Pinned on a blade (a critical): nothing knocks it off but death.
        if action.as_ref().is_some_and(|a| matches!(a.current.map(|c| c.kind), Some(ActionKind::Pinned(_)))) {
            stagger = false;
        }
        // The stance holds: no flinch (a grab or a death still lands). A break on an enemy stuns.
        let mut flinch = ActionKind::Hurt;
        if stagger && let Some(mut st) = stance {
            if st.hit(msg.stance) {
                info!("stance broken");
                if st.break_stuns {
                    flinch = ActionKind::Stunned;
                }
            } else {
                stagger = false;
            }
        }
        if let Some(mut action) = action
            && (killed || stagger)
        {
            action.force(if killed { ActionKind::Dead } else { flinch }, Vec3::ZERO);
        }
        taken.write(DamageTaken { target: msg.target, amount, from: msg.from, killed, attacker: msg.attacker });
    }
}

pub(super) fn drain_stance(time: Res<Time>, mut q: Query<&mut Stance>) {
    for mut st in &mut q {
        st.tick(time.delta_secs());
    }
}

pub(super) fn tick_hit_immunity(time: Res<Time>, mut q: Query<&mut HitImmunity>) {
    for mut i in &mut q {
        if i.0 > 0.0 {
            i.0 = (i.0 - time.delta_secs()).max(0.0);
        }
    }
}

pub(super) fn update_stamina(
    time: Res<Time>,
    mut q: Query<(&mut Stamina, &CharacterAction, &CharacterController, Option<&Guard>)>,
) {
    let dt = time.delta_secs();
    for (mut stamina, action, ctrl, guard) in &mut q {
        // Holding a guard up: stamina comes back more slowly.
        let dt = if guard.is_some_and(Guard::is_up) { dt * GUARD_REGEN_FACTOR } else { dt };
        if ctrl.sprinting {
            stamina.current = (stamina.current - SPRINT_STAMINA_PER_SEC * dt).max(0.0);
            stamina.regen_delay = STAMINA_REGEN_DELAY;
        }
        stamina.tick(dt, action.current.is_some() || ctrl.sprinting);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn third_sword_hit_breaks_a_hollows_stance_and_it_drains() {
        // The user's example: a sword deals 10 stance damage, a Hollow has 25.
        let mut st = Stance::new(25.0, true);
        assert!(!st.hit(10.0) && !st.hit(10.0), "two hits hold");
        assert!(st.hit(10.0), "the third breaks it");
        assert_eq!(st.damage, 0.0, "and the bar empties");
        st.hit(10.0);
        st.tick(STANCE_DRAIN_DELAY * 0.5);
        assert_eq!(st.damage, 10.0, "no drain right after a hit");
        st.tick(STANCE_DRAIN_DELAY);
        st.tick(1.0 / STANCE_DRAIN_RATE);
        assert_eq!(st.damage, 0.0, "drained");
        assert!(!st.hit(0.0), "no stance damage, no break");
    }

    #[test]
    fn stamina_waits_then_regens() {
        let mut s = Stamina::new(100.0);
        s.spend(50.0);
        assert_eq!(s.current, 50.0);
        s.tick(STAMINA_REGEN_DELAY - 0.01, false);
        assert_eq!(s.current, 50.0, "still in delay");
        s.tick(0.01, false);
        s.tick(1.0, false);
        assert!((s.current - (50.0 + STAMINA_REGEN)).abs() < 1e-3);
    }

    #[test]
    fn stamina_blocked_does_not_regen_and_clamps() {
        let mut s = Stamina::new(100.0);
        s.spend(150.0);
        assert_eq!(s.current, 0.0);
        assert!(!s.can_spend());
        s.tick(10.0, true);
        assert_eq!(s.current, 0.0);
    }
}
