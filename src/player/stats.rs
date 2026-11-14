//! Health, stamina, invulnerability and damage application.

use bevy::prelude::*;

use super::action::{ActionKind, PlayerAction};
use super::controller::CharacterController;
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

/// Present while the entity ignores damage (roll i-frames, etc.).
#[derive(Component, Debug)]
pub struct Invulnerable;

/// Request to damage an entity. Sent by combat, the debug key, enemies later.
#[derive(Message, Debug, Clone)]
pub struct DamageMessage {
    pub target: Entity,
    pub amount: f32,
    /// Where the hit came from (attacker position), for knockback / reactions.
    pub from: Option<Vec3>,
}

/// Damage that was actually applied (not blocked by i-frames). For hit reactions.
#[derive(Message, Debug, Clone)]
pub struct DamageTaken {
    pub target: Entity,
    pub amount: f32,
    pub from: Option<Vec3>,
    pub killed: bool,
}

/// Applies damage to anything with `Health`. Players additionally flinch (Hurt) or die.
pub fn apply_damage(
    mut messages: MessageReader<DamageMessage>,
    mut taken: MessageWriter<DamageTaken>,
    mut q: Query<(&mut Health, Option<&mut PlayerAction>, Has<Invulnerable>)>,
) {
    for msg in messages.read() {
        let Ok((mut health, action, invulnerable)) = q.get_mut(msg.target) else { continue };
        if invulnerable || health.is_dead() {
            info!("damage {} ignored (invulnerable={invulnerable})", msg.amount);
            continue;
        }
        health.current = (health.current - msg.amount).max(0.0);
        let killed = health.is_dead();
        if let Some(mut action) = action {
            action.force(if killed { ActionKind::Dead } else { ActionKind::Hurt }, Vec3::ZERO);
        }
        taken.write(DamageTaken { target: msg.target, amount: msg.amount, from: msg.from, killed });
    }
}

pub(super) fn update_stamina(
    time: Res<Time>,
    mut q: Query<(&mut Stamina, &PlayerAction, &CharacterController)>,
) {
    let dt = time.delta_secs();
    for (mut stamina, action, ctrl) in &mut q {
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
