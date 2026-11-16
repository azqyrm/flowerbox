//! The warg: a wolf the size of a pony. A quadruped body (`QuadrupedSpec::warg`), its fangs as a
//! natural weapon (`weapon::warg_fangs`: quick bites and the pounce grab) and the beast AI with
//! [`PROFILE`]. Spawned like every enemy from its `EnemyDef` (`super::kinds`).

use super::beast::BeastProfile;

pub const PROFILE: BeastProfile = BeastProfile {
    name: "warg",
    aggro_range: 13.0,
    leash: 24.0,
    chase_factor: 0.95,
    sprint_beyond: 6.0,
    bite_range: 2.1,
    combo: (1, 3),
    pounce_range: (2.2, 4.8),
    pounce_chance: 0.55,
    pounce_cooldown: 5.0,
    windup_turn: 2.6,
    recover_time: (0.35, 0.9),
    circle_distance: 3.2,
    patrol_factor: 0.3,
};

/// Lock-on aim and enemy bar: a little above the capsule centre (the back).
pub const WARG_TARGET_HEIGHT: f32 = 0.2;
