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

// ── Levels (see level/) ─────────────────────────────────────────────────
/// Grid the level's rooms are rasterised onto (m); walls stand on its lines.
pub const LEVEL_CELL: f32 = 0.25;
pub const LEVEL_WALL_THICKNESS: f32 = 0.5;
/// Level walls reach from below the deepest floor to the arena wall's height.
pub const LEVEL_BOTTOM: f32 = -5.0;
pub const LEVEL_WALL_TOP: f32 = WALL_HEIGHT;
/// One ceiling height for every room (deeper rooms are taller).
pub const LEVEL_CEILING: f32 = 5.0;
/// Height of a stair step's rise (m); the step count follows from the drop.
pub const STAIR_RISE: f32 = 0.18;
pub const LEVEL_TORCH_INTENSITY: f32 = 300_000.0;
pub const LEVEL_TORCH_RANGE: f32 = 13.0;
pub const DOOR_HEIGHT: f32 = 3.2;
/// How close (m, horizontally, from the door's middle) the player must be to open it.
pub const DOOR_RANGE: f32 = 1.9;
pub const DOOR_OPEN_TIME: f32 = 1.4;
/// The player's push: reach for the door, push (the door starts swinging), step back into stance.
pub const DOOR_PUSH_STARTUP: f32 = 0.4;
pub const DOOR_PUSH_ACTIVE: f32 = 0.15;
pub const DOOR_PUSH_RECOVERY: f32 = 0.5;

// ── Lighting ─────────────────────────────────────────────────────────────
pub const TORCH_INTENSITY: f32 = 600_000.0; // lumens
pub const TORCH_RANGE: f32 = 20.0;
pub const TORCH_HEIGHT: f32 = 2.8;
/// How many torches (the nearest to the camera) cast shadows.
/// (RX 6800 XT, level 1: 3 → 60 FPS (vsync) everywhere, 4 → dips to 43, 6 → 32–57.)
pub const TORCH_SHADOW_LIMIT: usize = 3;
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
pub const REST_RECOVERY: f32 = 0.8;

// ── Combat ───────────────────────────────────────────────────────────────
/// A dual attack (both same-class weapons at once) costs this × a normal attack's stamina.
pub const DUAL_STAMINA_MULT: f32 = 1.5;
/// Attacks with a weapon held in both hands deal this × damage.
pub const TWO_HAND_DAMAGE_MULT: f32 = 1.2;
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
/// How hard a hit swings the dummy: tilt velocity per point of damage, and the cap.
pub const DUMMY_PUSH_PER_DAMAGE: f32 = 0.17;
pub const DUMMY_PUSH_MAX: f32 = 11.0;

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
/// Hits from within this angle either side of the facing direction are guarded (radians, ≈ 57°,
/// a 115° arc in front). Hits from the sides and behind, or with no known direction, aren't.
pub const GUARD_ANGLE: f32 = 1.0;
/// Stamina a blocked hit costs, per point of the hit's damage.
pub const GUARD_STAMINA_PER_DAMAGE: f32 = 0.6;
/// Walking speed multiplier while guarding.
pub const GUARD_MOVE_FACTOR: f32 = 0.55;
/// Stamina regeneration multiplier while the guard is up.
pub const GUARD_REGEN_FACTOR: f32 = 0.35;

/// How long a parried attacker reels (open to a riposte).
pub const PARRY_STUN_TIME: f32 = 1.8;
/// Damage multiplier for hitting a stunned (parried) character: the riposte.
pub const RIPOSTE_MULT: f32 = 2.5;

// ── Lock-on and gamepad camera ─────────────────────────────────────────
/// Max distance to lock onto a target, and the cone (radians either side of the view) to look in.
pub const LOCK_RANGE: f32 = 15.0;
pub const LOCK_CONE: f32 = 1.1;
/// The lock breaks beyond this distance.
pub const LOCK_BREAK_RANGE: f32 = 20.0;
/// How fast the camera turns toward the locked target (1/s), and its pitch while locked.
pub const CAM_LOCK_TURN: f32 = 8.0;
pub const CAM_LOCK_PITCH: f32 = 0.3;
/// Right-stick camera speed (rad/s at full tilt).
pub const CAM_STICK_SPEED: f32 = 3.0;

// ── Enemies (the Hollow, see enemy/mod.rs) ───────────────────────────────
/// Where Hollows stand (floor x, z): across the arena from the player's start, past the dummy.
pub const HOLLOW_SPAWNS: [(f32, f32); 1] = [(-4.0, -10.0)];
pub const HOLLOW_HP: f32 = 70.0;
pub const HOLLOW_STAMINA: f32 = 90.0;
/// Notices the player within this distance (if it can see them); gives up beyond `HOLLOW_LEASH`
/// from home.
pub const HOLLOW_AGGRO_RANGE: f32 = 9.0;
/// Walking speed (fraction of the player's) while patrolling.
pub const HOLLOW_PATROL_FACTOR: f32 = 0.3;
pub const HOLLOW_LEASH: f32 = 16.0;
/// Starts an attack within this distance.
pub const HOLLOW_ATTACK_RANGE: f32 = 2.0;
/// Walking speed as a fraction of the player's.
pub const HOLLOW_MOVE_FACTOR: f32 = 0.6;
/// Chance an attack is a charged heavy instead of a light combo.
pub const HOLLOW_HEAVY_CHANCE: f32 = 0.25;
/// Pause between attacks (min, max seconds), spent circling.
pub const HOLLOW_RECOVER_TIME: (f32, f32) = (1.0, 2.0);
// ── Grabs (see grab.rs; the warg's pounce timings are in weapon/warg_fangs.rs) ─────────
/// How long a grabbed character is held (from the moment it's caught), and the stagger after.
pub const GRAB_HOLD: f32 = 1.3;
pub const GRAB_RELEASE: f32 = 0.35;
/// The bite lands this long after being caught (the shake). It can't be blocked: you're held.
pub const GRAB_BITE_AT: f32 = 0.6;
/// Caught when within this distance (horizontal, m) of the leaping creature's jaws.
pub const GRAB_REACH: f32 = 0.85;
/// A held victim is kept this far in front of the creature's centre.
pub const GRAB_HOLD_DISTANCE: f32 = 1.2;

// ── The warg (a quadruped beast, see enemy/beast.rs + enemy/warg.rs) ─────
pub const WARG_HP: f32 = 160.0;
pub const WARG_STAMINA: f32 = 140.0;

/// A dead Hollow lies there this long before vanishing.
pub const CORPSE_TIME: f32 = 4.0;

// ── Consumables and PvP (see pvp.rs) ────────────────────────────────────
/// Eating a Poppy / Camomile: hand to mouth, the effect lands at the start of the active phase.
pub const USE_ITEM_STARTUP: f32 = 0.45;
pub const USE_ITEM_ACTIVE: f32 = 0.2;
pub const USE_ITEM_RECOVERY: f32 = 0.45;
/// After taking a hit, players ignore further damage this long (two hits landing in the same
/// instant — two attackers, bunched network messages — don't stack). Souls games have no real
/// post-hit invulnerability; keep this tiny.
pub const HIT_IMMUNITY: f32 = 0.1;

// ── Items ────────────────────────────────────────────────────────────────
/// How close (m) the player must be to pick an item up.
pub const PICKUP_RANGE: f32 = 1.4;
/// Equip load the player can carry; the load class is the worn weight / this.
pub const PLAYER_MAX_LOAD: f32 = 30.0;
/// Load ratio up to which the load class is Light / Medium (above Medium: Heavy, over 1: Overloaded).
pub const LOAD_LIGHT: f32 = 0.3;
pub const LOAD_MEDIUM: f32 = 0.7;

/// Thrown-out items land this far in front of the player (m).
pub const THROW_OUT_DISTANCE: f32 = 0.9;
