//! Warg fangs: a beast's natural weapon. Quick snapping bites (a short combo with little wind-up,
//! so wargs feel aggressive) and the **pounce** grab. The bites are ordinary attacks (blockable,
//! parryable); their hitbox is the jaws (`crate::combat::bite_hits`). Heavy, running and jump
//! attacks are never used by the beast AI and reuse the light sword's numbers.
//!
//! Swing keys and the guard stance are for humanoid arms and mean nothing to a quadruped; the
//! quadruped animator (`crate::quadruped::anim`) poses bites from the timings alone.

use super::{AttackSpec, ModelSource, Moveset, PounceSpec, Stance, SwingKey, WeaponClass, WeaponDef, light_sword};

const fn bite(startup: f32, lunge: f32, damage_mult: f32) -> AttackSpec {
    AttackSpec {
        startup,
        active: 0.12,
        recovery: 0.32,
        cancel: startup + 0.3,
        stamina_cost: 14.0,
        lunge,
        damage_mult,
        windup: SwingKey::new(0.0, 0.0, 0.0, 0.0),
        strike: SwingKey::new(0.0, 0.0, 0.0, 0.0),
        stance: Stance::Lunge,
    }
}

/// Snap, snap, a lunging bite.
const COMBO: [AttackSpec; 3] = [bite(0.32, 0.6, 1.0), bite(0.26, 0.5, 1.0), bite(0.42, 1.4, 1.35)];

/// Lean back for a second, leap 3 m, bite and shake.
pub const POUNCE: PounceSpec = PounceSpec { windup: 1.0, leap: 0.42, recovery: 1.5, distance: 3.0, stamina_cost: 40.0, damage: 120.0 };

pub const MOVESET: Moveset = Moveset { light: &COMBO, pounce: Some(POUNCE), ability: None, ..light_sword::MOVESET };

/// The warg's jaws.
pub static WARG_FANGS: WeaponDef = WeaponDef {
    name: "Warg Fangs",
    class: WeaponClass::WargFangs,
    base_damage: 17.0,
    blade: (0.0, 0.0),
    model: ModelSource::Parts(&[]),
    guard_block: 0.0,
};
