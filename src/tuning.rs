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
