//! All gameplay / presentation constants in one place. Tweak game feel here.

// ── Arena ────────────────────────────────────────────────────────────────
/// Radius of the walkable circular floor (m).
pub const ARENA_RADIUS: f32 = 18.0;
pub const FLOOR_THICKNESS: f32 = 0.5;
pub const WALL_HEIGHT: f32 = 6.0;
pub const WALL_THICKNESS: f32 = 1.0;
/// Number of box segments that approximate the ring wall.
pub const WALL_SEGMENTS: usize = 40;
pub const PILLAR_COUNT: usize = 8;
pub const PILLAR_RADIUS: f32 = 0.7;
/// Distance of pillars from the arena center.
pub const PILLAR_RING: f32 = 14.0;

// ── Lighting ─────────────────────────────────────────────────────────────
pub const TORCH_INTENSITY: f32 = 600_000.0; // lumens
pub const TORCH_RANGE: f32 = 20.0;
pub const TORCH_HEIGHT: f32 = 2.8;
pub const TORCH_COLOR: (f32, f32, f32) = (1.0, 0.55, 0.22);
pub const MOON_INTENSITY: f32 = 30_000_000.0; // lumens
pub const MOON_COLOR: (f32, f32, f32) = (0.62, 0.72, 1.0);
pub const AMBIENT_BRIGHTNESS: f32 = 30.0;
pub const FOG_START: f32 = 12.0;
pub const FOG_END: f32 = 55.0;

// ── Player body ──────────────────────────────────────────────────────────
pub const PLAYER_RADIUS: f32 = 0.35;
/// Capsule cylinder length (total height = this + 2 × radius = 1.8 m).
pub const PLAYER_CAPSULE_LENGTH: f32 = 1.1;
pub const PLAYER_HALF_HEIGHT: f32 = PLAYER_CAPSULE_LENGTH * 0.5 + PLAYER_RADIUS;
pub const PLAYER_SPAWN: (f32, f32, f32) = (0.0, 1.0, 8.0);

// ── Movement ─────────────────────────────────────────────────────────────
pub const WALK_SPEED: f32 = 4.5;
pub const SPRINT_SPEED: f32 = 7.0;
/// Shift must be held this long before sprint kicks in (a shorter tap = roll, Phase 4).
pub const SPRINT_HOLD_THRESHOLD: f32 = 0.25;
pub const GROUND_ACCEL: f32 = 45.0;
pub const AIR_ACCEL: f32 = 6.0;
/// How fast the character turns toward the move direction (rad/s).
pub const TURN_SPEED: f32 = 14.0;
pub const GRAVITY: f32 = 20.0;
pub const JUMP_HEIGHT: f32 = 1.2;
pub const COYOTE_TIME: f32 = 0.1;
pub const JUMP_BUFFER: f32 = 0.15;
/// Surfaces whose normal has a y component above this count as ground (~45°).
pub const GROUND_NORMAL_MIN_Y: f32 = 0.7;
/// How far below the capsule the ground probe looks (also the max snap-down per tick).
pub const GROUND_PROBE_DISTANCE: f32 = 0.08;
/// The ground probe starts this far above the capsule so it never begins in contact.
pub const GROUND_PROBE_LIFT: f32 = 0.05;
/// Ground probe sphere radius relative to the capsule radius (thinner, so walls don't touch it).
pub const GROUND_PROBE_RADIUS_FACTOR: f32 = 0.8;
/// Gap kept between capsule and floor after snapping (matches avian's default skin width).
pub const GROUND_SKIN: f32 = 0.01;

// ── Camera ───────────────────────────────────────────────────────────────
pub const CAM_DISTANCE: f32 = 4.5;
pub const CAM_FOCUS_HEIGHT: f32 = 1.5;
pub const CAM_SENSITIVITY: f32 = 0.0025;
pub const CAM_PITCH_MIN: f32 = -0.6;
pub const CAM_PITCH_MAX: f32 = 1.2;
/// Radius of the sphere cast that keeps the camera from clipping through walls.
pub const CAM_COLLISION_RADIUS: f32 = 0.25;

// ── Stats ────────────────────────────────────────────────────────────────
pub const PLAYER_MAX_HP: f32 = 100.0;
pub const PLAYER_MAX_STAMINA: f32 = 100.0;
pub const STAMINA_REGEN: f32 = 35.0; // per second
/// Regen pause after any stamina spend.
pub const STAMINA_REGEN_DELAY: f32 = 0.6;
pub const SPRINT_STAMINA_PER_SEC: f32 = 12.0;

// ── Action timings (seconds) ─────────────────────────────────────────────
// Each action = startup → active → recovery. `cancel` = time since start after
// which a buffered action may interrupt it. See `character::action::action_def`.
pub const INPUT_BUFFER: f32 = 0.2;

pub const ROLL_STARTUP: f32 = 0.05;
pub const ROLL_ACTIVE: f32 = 0.35; // = i-frames 0.05..0.40
pub const ROLL_RECOVERY: f32 = 0.20;
pub const ROLL_CANCEL: f32 = 0.45;
pub const ROLL_COST: f32 = 20.0;
pub const ROLL_DISTANCE: f32 = 3.5;

pub const BACKSTEP_STARTUP: f32 = 0.05;
pub const BACKSTEP_ACTIVE: f32 = 0.20;
pub const BACKSTEP_RECOVERY: f32 = 0.15;
pub const BACKSTEP_CANCEL: f32 = 0.30;
pub const BACKSTEP_COST: f32 = 15.0;
pub const BACKSTEP_DISTANCE: f32 = 1.6;

// Attack timings, costs, lunges and damage are per weapon class: see `weapon/<class>.rs`.

pub const DRINK_STARTUP: f32 = 0.50;
pub const DRINK_ACTIVE: f32 = 0.20;
pub const DRINK_RECOVERY: f32 = 0.40;
pub const DRINK_MOVE_FACTOR: f32 = 0.35;

pub const HURT_DURATION: f32 = 0.45;

/// Resting at a Hearth: kneel down, rest (restores everything at its start), stand up.
pub const REST_STARTUP: f32 = 0.7;
pub const REST_ACTIVE: f32 = 0.6;
pub const REST_RECOVERY: f32 = 0.8;

// ── Combat ───────────────────────────────────────────────────────────────
/// LMB held at least this long = heavy attack (charging starts), shorter = light attack.
pub const ATTACK_HOLD_THRESHOLD: f32 = 0.25;
pub const HEAVY_CHARGE_MOVE_FACTOR: f32 = 0.3;
/// Downward speed of the jump attack plunge (m/s).
pub const PLUNGE_SPEED: f32 = 14.0;
/// After landing, the jump attack's hitbox stays active this long (the slam).
pub const JUMP_SLAM_TIME: f32 = 0.1;
/// Radius of the capsule swept along the blade for hit detection (a little forgiving).
pub const BLADE_HIT_RADIUS: f32 = 0.12;
/// Game time is slowed to almost zero for this long (real seconds) when a hit lands.
pub const HITSTOP_LIGHT: f32 = 0.06;
pub const HITSTOP_HEAVY: f32 = 0.1;
pub const HITSTOP_TIME_SCALE: f32 = 0.05;
pub const SHAKE_LIGHT: f32 = 0.35;
pub const SHAKE_HEAVY: f32 = 0.6;

// ── Training dummy ───────────────────────────────────────────────────────
pub const DUMMY_HP: f32 = 120.0;
pub const DUMMY_POSITION: (f32, f32) = (0.0, 0.0);
/// Seconds the broken dummy lies on the floor before it resets.
pub const DUMMY_RESET_TIME: f32 = 3.0;

// ── Healing Ale ──────────────────────────────────────────────────────────
pub const ALE_CHARGES: u32 = 3;
/// Fraction of max HP one drink restores, spread over `ALE_HEAL_TIME` from the swallow.
pub const ALE_HEAL_FRACTION: f32 = 0.4;
pub const ALE_HEAL_TIME: f32 = 0.5;

// ── Hearth ───────────────────────────────────────────────────────────────
/// Hearth position on the floor (x, z): in front of the pillar behind the player's start.
pub const HEARTH_POSITION: (f32, f32) = (0.0, 12.0);
/// How close (m, from the Hearth's centre) the player must be to rest.
pub const HEARTH_RANGE: f32 = 2.3;
/// Respawn spot: this far from the Hearth toward the arena centre, facing the centre.
pub const HEARTH_RESPAWN_OFFSET: f32 = 1.6;

// ── Death screen (seconds since death) ───────────────────────────────────
pub const DEATH_TEXT_DELAY: f32 = 1.1;
pub const DEATH_TEXT_FADE: f32 = 0.7;
/// Fade to black starts here; the player respawns when it's fully black.
pub const DEATH_FADE_OUT_AT: f32 = 3.6;
pub const DEATH_FADE_OUT: f32 = 0.9;
/// Fade back in after respawning.
pub const RESPAWN_FADE_IN: f32 = 0.9;

// ── Guard and parry (see character/guard.rs) ──────────────────────────────
/// Time to raise the shield. It blocks while rising; the last part is the parry window.
pub const GUARD_RAISE_TIME: f32 = 0.3;
/// The parry window starts at this fraction of the raise (2/3 → the last third).
pub const PARRY_WINDOW_START: f32 = 2.0 / 3.0;
/// Hits within this angle of the facing direction are guarded (radians, ≈ 80°).
pub const GUARD_ANGLE: f32 = 1.4;
/// Stamina a blocked hit costs, per point of the hit's damage.
pub const GUARD_STAMINA_PER_DAMAGE: f32 = 0.6;
/// Walking speed multiplier while guarding.
pub const GUARD_MOVE_FACTOR: f32 = 0.55;
/// Stamina regeneration multiplier while the guard is up.
pub const GUARD_REGEN_FACTOR: f32 = 0.35;

// ── Items ────────────────────────────────────────────────────────────────
/// How close (m) the player must be to pick an item up.
pub const PICKUP_RANGE: f32 = 1.4;
/// Equip load the player can carry; the load class is the worn weight / this.
pub const PLAYER_MAX_LOAD: f32 = 30.0;
/// Load ratio up to which the load class is Light / Medium (above Medium: Heavy, over 1: Overloaded).
pub const LOAD_LIGHT: f32 = 0.3;
pub const LOAD_MEDIUM: f32 = 0.7;
