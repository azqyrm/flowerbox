//! Claws class: a horror's long hooked fingers (the Mourner). A natural weapon: nothing in the
//! hand, the reach is the fingers (the hitbox runs from the palm to past the fingertips).
//! Quick, raking flurries with very short pauses — the danger is the rhythm, not one big hit.

use super::{AttackSpec, ModelSource, Moveset, Stance, SwingKey, WeaponClass, WeaponDef, light_sword};

const fn rake(startup: f32, windup: SwingKey, strike: SwingKey) -> AttackSpec {
    AttackSpec {
        startup,
        active: 0.12,
        recovery: 0.22,
        cancel: startup + 0.22,
        stamina_cost: 8.0,
        lunge: 0.7,
        damage_mult: 0.9,
        windup,
        strike,
        stance: Stance::Lunge,
    }
}

/// Four rakes: wide right, back-hand left, low gut rake, a high downward tear.
const FLURRY: [AttackSpec; 4] = [
    rake(0.42, SwingKey::new(-0.7, 0.2, -1.3, 1.9), SwingKey::new(0.6, -0.1, 0.9, 1.2)),
    rake(0.24, SwingKey::new(0.6, 0.1, 0.9, 1.6), SwingKey::new(-0.6, 0.0, -1.0, 1.3)),
    rake(0.28, SwingKey::new(-0.3, 0.35, -0.5, 0.6), SwingKey::new(0.4, 0.2, 0.6, 1.7)),
    rake(0.36, SwingKey::new(0.0, -0.3, 0.1, 3.0), SwingKey::new(0.0, 0.45, 0.0, 0.8)),
];

pub const MOVESET: Moveset = Moveset {
    light: &FLURRY,
    heavy: AttackSpec {
        startup: 0.3,
        active: 0.18,
        recovery: 0.6,
        cancel: 1.0,
        stamina_cost: 0.0,
        lunge: 1.6,
        damage_mult: 1.7,
        // Both arms flung up behind the head, then torn down through the target.
        windup: SwingKey::new(0.0, -0.4, 0.0, 3.1),
        strike: SwingKey::new(0.0, 0.55, 0.0, 0.7),
        stance: Stance::Lunge,
    },
    ability: None,
    ..light_sword::MOVESET
};

/// The Mourner's fingers (no model: the claws are part of its body).
pub static MOURNER_CLAWS: WeaponDef = WeaponDef {
    name: "Mourner's Claws",
    class: WeaponClass::Claws,
    base_damage: 18.0,
    blade: (0.0, 0.5),
    model: ModelSource::Parts(&[]),
    guard_block: 0.0,
};
