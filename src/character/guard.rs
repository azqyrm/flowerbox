//! Guarding with a shield, and the parry window at the end of raising it.
//!
//! - A character **guards** while `ActionIntent::guard_held` is set, it has a guard
//!   (`Guard::strength > 0`, i.e. a shield) and it isn't busy with another action. Holding the
//!   button raises the guard; letting go lowers it. Not raised = no blocking at all.
//! - **Raising** takes `GUARD_RAISE_TIME`. The shield already blocks while it comes up; the **last
//!   third** of the raise (`PARRY_WINDOW_START..1` of it) is the **parry window**.
//! - A frontal hit (within `GUARD_ANGLE` of the facing) on a raised guard is:
//!   - **parried** inside the window: no damage, no stamina, a [`Parried`] message (the attacker
//!     gets stunned once enemies exist, see ROADMAP N19);
//!   - **blocked** otherwise: `strength` of the damage is stopped, the guard costs stamina
//!     (`GUARD_STAMINA_PER_DAMAGE` × the hit). Without enough stamina the guard **breaks**: stamina
//!     goes to 0, the character staggers and takes the damage that got through.
//! - Only hits from the front (within `GUARD_ANGLE` of the facing) are guarded; hits from the
//!   sides, from behind or without a direction ignore the guard. Movement while guarding is slowed, no sprinting, and
//!   stamina regenerates more slowly.

use bevy::prelude::*;

use super::ActionIntent;
use super::action::CharacterAction;
use crate::tuning::*;

/// A character's guard. `strength` comes from its equipment (shield block value).
#[derive(Component, Debug, Clone, Default)]
pub struct Guard {
    /// Fraction of damage stopped while raised (0 = nothing to guard with).
    pub strength: f32,
    /// Seconds since the guard started rising; `None` = lowered.
    pub raised_for: Option<f32>,
}

impl Guard {
    pub fn is_up(&self) -> bool {
        self.raised_for.is_some()
    }

    /// 0 → 1 while the shield comes up.
    pub fn raise_progress(&self) -> f32 {
        self.raised_for.map_or(0.0, |t| (t / GUARD_RAISE_TIME).min(1.0))
    }

    pub fn in_parry_window(&self) -> bool {
        matches!(self.raised_for, Some(t) if (GUARD_RAISE_TIME * PARRY_WINDOW_START..=GUARD_RAISE_TIME).contains(&t))
    }

    /// What a hit of `amount` does to this guard. `facing` = the defender's yaw, `to_attacker` =
    /// direction toward the attacker (`None` = unknown: can't be guarded), `stamina` = what the
    /// defender has left.
    pub fn outcome(&self, facing: f32, to_attacker: Option<Vec3>, amount: f32, stamina: f32) -> GuardOutcome {
        if !self.is_up() || self.strength <= 0.0 {
            return GuardOutcome::Unguarded;
        }
        // Only hits from the front: the attacker must be within GUARD_ANGLE of the facing.
        let Some(dir) = to_attacker.map(|d| d.with_y(0.0)).filter(|d| d.length_squared() > 1e-6) else {
            return GuardOutcome::Unguarded; // no direction (falls, debug damage): can't be blocked
        };
        let forward = Vec3::new(-facing.sin(), 0.0, -facing.cos());
        if forward.dot(dir.normalize()) < GUARD_ANGLE.cos() {
            return GuardOutcome::Unguarded; // from the side or behind
        }
        if self.in_parry_window() {
            return GuardOutcome::Parried;
        }
        let cost = amount * GUARD_STAMINA_PER_DAMAGE;
        let through = amount * (1.0 - self.strength);
        if cost > stamina {
            GuardOutcome::Broken { through }
        } else {
            GuardOutcome::Blocked { through, stamina_cost: cost }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GuardOutcome {
    /// Guard down, no guard, or hit from outside the guarded arc: full damage.
    Unguarded,
    /// Negated in the parry window.
    Parried,
    /// Stopped by the shield: `through` still hurts, `stamina_cost` is spent.
    Blocked { through: f32, stamina_cost: f32 },
    /// Not enough stamina to hold: the guard breaks, `through` hurts, the defender staggers.
    Broken { through: f32 },
}

/// A guard parried an attack. `attacker` is the entity that swung, when known.
#[derive(Message, Debug, Clone, Copy)]
pub struct Parried {
    pub defender: Entity,
    pub attacker: Option<Entity>,
}

/// Raises/lowers guards from the intent. Runs after the action state machine, so starting a roll
/// or an attack lowers the guard; holding the button afterwards raises it again (from the start).
pub(super) fn update_guard(time: Res<Time>, mut q: Query<(&ActionIntent, &CharacterAction, &mut Guard)>) {
    let dt = time.delta_secs();
    for (intent, action, mut guard) in &mut q {
        let wants = intent.guard_held && guard.strength > 0.0 && action.current.is_none();
        let next = match (wants, guard.raised_for) {
            (false, _) => None,
            (true, None) => Some(0.0),
            (true, Some(t)) => Some(t + dt),
        };
        if guard.raised_for != next {
            guard.raised_for = next;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRONT: Option<Vec3> = Some(Vec3::NEG_Z);

    fn raised(t: f32) -> Guard {
        Guard { strength: 0.6, raised_for: Some(t) }
    }

    #[test]
    fn lowered_or_without_shield_does_not_block() {
        assert_eq!(Guard { strength: 0.6, raised_for: None }.outcome(0.0, FRONT, 20.0, 100.0), GuardOutcome::Unguarded);
        assert_eq!(Guard { strength: 0.0, raised_for: Some(1.0) }.outcome(0.0, FRONT, 20.0, 100.0), GuardOutcome::Unguarded);
    }

    #[test]
    fn parry_window_is_the_last_third_of_the_raise() {
        let r = GUARD_RAISE_TIME;
        assert!(!raised(r * 0.5).in_parry_window());
        assert!(raised(r * 0.8).in_parry_window());
        assert!(!raised(r * 1.2).in_parry_window());
        assert_eq!(raised(r * 0.8).outcome(0.0, FRONT, 20.0, 100.0), GuardOutcome::Parried);
        // Before the window the shield already blocks.
        assert!(matches!(raised(r * 0.2).outcome(0.0, FRONT, 20.0, 100.0), GuardOutcome::Blocked { .. }));
    }

    #[test]
    fn block_stops_its_share_and_costs_stamina() {
        let GuardOutcome::Blocked { through, stamina_cost } = raised(1.0).outcome(0.0, FRONT, 20.0, 100.0) else { panic!() };
        assert!((through - 8.0).abs() < 1e-4);
        assert!((stamina_cost - 20.0 * GUARD_STAMINA_PER_DAMAGE).abs() < 1e-4);
        assert!(matches!(raised(1.0).outcome(0.0, FRONT, 20.0, 1.0), GuardOutcome::Broken { .. }), "too tired to hold");
    }

    #[test]
    fn only_frontal_hits_are_guarded() {
        // Facing 0 looks along -Z.
        assert!(matches!(raised(1.0).outcome(0.0, Some(Vec3::NEG_Z), 20.0, 100.0), GuardOutcome::Blocked { .. }));
        assert_eq!(raised(1.0).outcome(0.0, Some(Vec3::Z), 20.0, 100.0), GuardOutcome::Unguarded);
        assert_eq!(raised(1.0).outcome(0.0, Some(Vec3::X), 20.0, 100.0), GuardOutcome::Unguarded);
        // 45° off the front is still guarded, 70° isn't.
        let dir = |deg: f32| Some(Vec3::new(-deg.to_radians().sin(), 0.0, -deg.to_radians().cos()));
        assert!(matches!(raised(1.0).outcome(0.0, dir(45.0), 20.0, 100.0), GuardOutcome::Blocked { .. }));
        assert_eq!(raised(1.0).outcome(0.0, dir(70.0), 20.0, 100.0), GuardOutcome::Unguarded);
        // No direction: can't be blocked.
        assert_eq!(raised(1.0).outcome(0.0, None, 20.0, 100.0), GuardOutcome::Unguarded);
        // Turned around (facing +Z), the same attacker on -Z is behind.
        assert_eq!(raised(1.0).outcome(std::f32::consts::PI, Some(Vec3::NEG_Z), 20.0, 100.0), GuardOutcome::Unguarded);
    }
}
