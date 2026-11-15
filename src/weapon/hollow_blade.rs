//! Hollow blade class: an enemy's crude one-handed swings. Same shape as the light sword, but
//! with long, readable wind-ups (the arm hauled far back) so the player can roll, block or time a
//! parry. Heavy, running and jump attacks reuse the light sword's.

use super::{AttackSpec, ModelSource, Moveset, Stance, SwingKey, WeaponClass, WeaponDef, light_sword};

const fn slow(windup: SwingKey, strike: SwingKey) -> AttackSpec {
    AttackSpec {
        startup: 0.55,
        active: 0.14,
        recovery: 0.45,
        cancel: 0.75,
        stamina_cost: 12.0,
        lunge: 0.6,
        damage_mult: 1.0,
        windup,
        strike,
        stance: Stance::Lunge,
    }
}

/// Wide right → left haymaker, then a clumsy overhead chop.
const COMBO: [AttackSpec; 2] = [
    slow(SwingKey::new(-0.75, 0.1, -1.35, 1.85), SwingKey::new(0.6, -0.1, 0.8, 1.3)),
    slow(SwingKey::new(0.1, 0.25, 0.05, 3.0), SwingKey::new(0.0, -0.4, 0.0, 0.9)),
];

pub const MOVESET: Moveset = Moveset { light: &COMBO, ..light_sword::MOVESET };

/// What the Hollows carry: a rusted blade swung with the hollow moveset.
pub static HOLLOW_SWORD: WeaponDef = WeaponDef {
    name: "Hollow's Rusted Blade",
    class: WeaponClass::HollowBlade,
    base_damage: 14.0,
    blade: (0.12, 0.73),
    model: ModelSource::Parts(&light_sword::SHORTSWORD_PARTS),
};
