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
// which a buffered action may interrupt it. See `player::action::action_def`.
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

pub const LIGHT_STARTUP: f32 = 0.15;
pub const LIGHT_ACTIVE: f32 = 0.12;
pub const LIGHT_RECOVERY: f32 = 0.25;
pub const LIGHT_CANCEL: f32 = 0.35;
pub const LIGHT_COST: f32 = 15.0;
pub const LIGHT_COMBO_MAX: u8 = 3;

pub const HEAVY_STARTUP: f32 = 0.35;
pub const HEAVY_ACTIVE: f32 = 0.15;
pub const HEAVY_RECOVERY: f32 = 0.40;
pub const HEAVY_CANCEL: f32 = 0.70;
pub const HEAVY_COST: f32 = 30.0;

pub const JUMP_ATTACK_STARTUP: f32 = 0.20;
pub const JUMP_ATTACK_ACTIVE: f32 = 0.25;
pub const JUMP_ATTACK_RECOVERY: f32 = 0.35;
pub const JUMP_ATTACK_COST: f32 = 25.0;

pub const DRINK_STARTUP: f32 = 0.50;
pub const DRINK_ACTIVE: f32 = 0.20;
pub const DRINK_RECOVERY: f32 = 0.40;
pub const DRINK_MOVE_FACTOR: f32 = 0.35;

pub const HURT_DURATION: f32 = 0.45;
/// Placeholder until Phase 6 (Hearth respawn): time on the floor before respawning.
pub const DEAD_DURATION: f32 = 3.0;
