//! Procedural textures, generated in code at startup (no image assets, no licensing): value-noise
//! patterns for stone, cloth, metal, flesh and glass. They are greyscale-ish detail maps meant to
//! be tinted by a material's `base_color` — except [`Tex::StainedGlass`] and [`Tex::Runes`], which
//! carry their own colours / glow masks.
//!
//! Use: `textures::get(Tex::Bricks)` anywhere after `PreStartup` (a global registry, so spawners
//! don't need an `Assets<Image>` parameter). [`tiled_box`] builds a box mesh whose UVs follow world
//! metres, so a long wall repeats its bricks instead of stretching one.

use std::sync::OnceLock;

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

pub struct TexturesPlugin;

impl Plugin for TexturesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, generate);
    }
}

/// The procedural textures.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tex {
    /// Gothic ashlar: offset courses of rough blocks, dark mortar.
    Bricks,
    /// Big worn floor slabs.
    Flagstones,
    /// Interlocking rings (mail, the boss's hauberk).
    Chainmail,
    /// Coarse woven cloth (tunics, robes, banners).
    Cloth,
    /// Glowing rune bands (weapons, the sealed door, the riddle). Bright where runes are.
    Runes,
    /// Wet, veined flesh (horrors).
    Flesh,
    /// Pitted, stained bone.
    Bone,
    /// Lead-framed coloured glass panes (cathedral windows): carries its own colours.
    StainedGlass,
    /// Flaking rust over dark iron.
    Rust,
    /// Cracked, grained leather.
    Leather,
    /// Veined marble.
    Marble,
}

impl Tex {
    pub const ALL: [Tex; 11] =
        [Tex::Bricks, Tex::Flagstones, Tex::Chainmail, Tex::Cloth, Tex::Runes, Tex::Flesh, Tex::Bone, Tex::StainedGlass, Tex::Rust, Tex::Leather, Tex::Marble];
}

static REGISTRY: OnceLock<Vec<(Tex, Handle<Image>)>> = OnceLock::new();

/// The texture (after `PreStartup`; `None` before, or in tests).
pub fn get(tex: Tex) -> Option<Handle<Image>> {
    REGISTRY.get()?.iter().find(|(t, _)| *t == tex).map(|(_, h)| h.clone())
}

const SIZE: u32 = 256;

fn generate(mut images: ResMut<Assets<Image>>) {
    if REGISTRY.get().is_some() {
        return;
    }
    let list = Tex::ALL
        .iter()
        .map(|&t| {
            let mut data = Vec::with_capacity((SIZE * SIZE * 4) as usize);
            for y in 0..SIZE {
                for x in 0..SIZE {
                    let [r, g, b] = sample(t, x as f32 / SIZE as f32, y as f32 / SIZE as f32);
                    data.extend_from_slice(&[to_u8(r), to_u8(g), to_u8(b), 255]);
                }
            }
            let mut image = Image::new(
                Extent3d { width: SIZE, height: SIZE, depth_or_array_layers: 1 },
                TextureDimension::D2,
                data,
                TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::RENDER_WORLD,
            );
            image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                address_mode_u: ImageAddressMode::Repeat,
                address_mode_v: ImageAddressMode::Repeat,
                ..ImageSamplerDescriptor::linear()
            });
            (t, images.add(image))
        })
        .collect();
    let _ = REGISTRY.set(list);
}

fn to_u8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

// ── Noise ────────────────────────────────────────────────────────────────

fn hash(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(374_761_393) ^ (y as u32).wrapping_mul(668_265_263) ^ seed.wrapping_mul(2_246_822_519);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    h ^= h >> 16;
    (h & 0xFFFF) as f32 / 65535.0
}

/// Tileable value noise: `period` cells across the texture.
fn value(u: f32, v: f32, period: i32, seed: u32) -> f32 {
    let (x, y) = (u * period as f32, v * period as f32);
    let (x0, y0) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let s = |t: f32| t * t * (3.0 - 2.0 * t);
    let w = |a: i32| a.rem_euclid(period);
    let c = |dx: i32, dy: i32| hash(w(x0 + dx), w(y0 + dy), seed);
    let top = c(0, 0) + (c(1, 0) - c(0, 0)) * s(fx);
    let bottom = c(0, 1) + (c(1, 1) - c(0, 1)) * s(fx);
    top + (bottom - top) * s(fy)
}

/// Fractal noise, 0..1, tileable.
fn fbm(u: f32, v: f32, period: i32, octaves: u32, seed: u32) -> f32 {
    let (mut sum, mut amp, mut norm, mut p) = (0.0, 0.5, 0.0, period);
    for o in 0..octaves {
        sum += value(u, v, p, seed + o * 17) * amp;
        norm += amp;
        amp *= 0.5;
        p *= 2;
    }
    sum / norm
}

fn grey(v: f32) -> [f32; 3] {
    [v, v, v]
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// Distance to the nearest edge of a cell of a `cols × rows` grid with every other row shifted
/// by half a cell (masonry); also returns the cell's id for per-block variation.
fn masonry(u: f32, v: f32, cols: f32, rows: f32) -> (f32, (i32, i32)) {
    let row = (v * rows).floor();
    let shift = if row as i32 % 2 == 0 { 0.0 } else { 0.5 };
    let x = u * cols + shift;
    let col = x.floor();
    let (fx, fy) = (x - col, v * rows - row);
    let edge = fx.min(1.0 - fx) / cols * rows.max(cols) * 0.5;
    let edge = edge.min(fy.min(1.0 - fy) / rows * rows.max(cols) * 0.5);
    (edge, ((col as i32).rem_euclid(cols as i32), row as i32))
}

fn sample(t: Tex, u: f32, v: f32) -> [f32; 3] {
    match t {
        Tex::Bricks => {
            let (edge, (cx, cy)) = masonry(u, v, 4.0, 8.0);
            let block = 0.72 + 0.22 * hash(cx, cy, 3);
            let grit = fbm(u, v, 16, 4, 11);
            let chip = (fbm(u, v, 8, 3, 29) - 0.5) * 0.25;
            let mortar = smooth(0.02, 0.07, edge);
            grey((block * (0.75 + 0.35 * grit) + chip) * (0.25 + 0.75 * mortar))
        }
        Tex::Flagstones => {
            let (edge, (cx, cy)) = masonry(u, v, 3.0, 3.0);
            let tone = 0.6 + 0.3 * hash(cx, cy, 5);
            let wear = fbm(u, v, 8, 5, 41);
            let cracks = smooth(0.0, 0.04, (fbm(u, v, 6, 4, 7) - 0.5).abs());
            grey(tone * (0.7 + 0.4 * wear) * (0.5 + 0.5 * cracks) * (0.3 + 0.7 * smooth(0.01, 0.05, edge)))
        }
        Tex::Chainmail => {
            let n = 16.0;
            let (x, y) = (u * n, v * n * 1.4);
            let shift = if (y.floor() as i32) % 2 == 0 { 0.0 } else { 0.5 };
            let (fx, fy) = ((x + shift).fract() - 0.5, y.fract() - 0.5);
            let r = (fx * fx + fy * fy).sqrt();
            let ring = 1.0 - ((r - 0.32).abs() / 0.12).min(1.0);
            let shine = 0.35 + 0.65 * ring * (0.7 + 0.3 * (1.0 - fy));
            grey(shine * (0.85 + 0.15 * fbm(u, v, 8, 2, 9)))
        }
        Tex::Cloth => {
            let n = 64.0;
            let warp = ((u * n * std::f32::consts::TAU).sin() * 0.5 + 0.5) * ((v * n * 0.5).floor() % 2.0);
            let weft = ((v * n * std::f32::consts::TAU).sin() * 0.5 + 0.5) * (1.0 - (u * n * 0.5).floor() % 2.0);
            let thread = 0.65 + 0.25 * (warp + weft).min(1.0);
            grey(thread * (0.8 + 0.3 * fbm(u, v, 8, 4, 13)) * (0.9 + 0.1 * value(u, v, 4, 99)))
        }
        Tex::Runes => {
            // Bands of angular glyphs: strokes on a grid, bright on black.
            let rows = 4.0;
            let band = (v * rows).fract();
            let in_band = smooth(0.12, 0.2, band) * smooth(0.12, 0.2, 1.0 - band);
            let (gx, gy) = ((u * 16.0).floor() as i32, (v * rows).floor() as i32);
            let (fx, fy) = ((u * 16.0).fract(), band);
            let h = hash(gx, gy, 77);
            let vertical = (fx - 0.5).abs() < 0.08 && h > 0.25;
            let diagonal = ((fx - fy).abs() < 0.08 && h > 0.55) || ((fx + fy - 1.0).abs() < 0.08 && h < 0.45);
            let bar = (fy - 0.5).abs() < 0.06 && h > 0.7;
            let glyph = if vertical || diagonal || bar { 1.0 } else { 0.0 };
            let g = glyph * in_band;
            [g * 1.0, g * 0.55, g * 0.25]
        }
        Tex::Flesh => {
            let base = fbm(u, v, 6, 5, 21);
            let veins = 1.0 - smooth(0.0, 0.05, (fbm(u, v, 5, 4, 51) - 0.5).abs());
            let wet = fbm(u, v, 24, 2, 61);
            let c = mix([0.55, 0.12, 0.1], [0.85, 0.45, 0.4], base);
            let c = mix(c, [0.25, 0.02, 0.05], veins * 0.8);
            mix(c, [1.0, 0.8, 0.75], (wet - 0.7).max(0.0) * 1.5)
        }
        Tex::Bone => {
            let base = fbm(u, v, 8, 5, 31);
            let pits = smooth(0.62, 0.7, fbm(u, v, 32, 2, 33));
            let stain = fbm(u, v, 3, 3, 35);
            let c = mix([0.93, 0.88, 0.76], [0.6, 0.52, 0.38], stain * 0.8);
            let c = mix(c, [0.3, 0.25, 0.2], pits * 0.7);
            [c[0] * (0.85 + 0.2 * base), c[1] * (0.85 + 0.2 * base), c[2] * (0.85 + 0.2 * base)]
        }
        Tex::StainedGlass => {
            // Irregular panes (jittered grid), lead lines between, a rose of colours.
            let n = 6.0;
            let (x, y) = (u * n, v * n);
            let (cx, cy) = (x.floor() as i32, y.floor() as i32);
            let mut best = (f32::MAX, f32::MAX, 0.0);
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let (px, py) = (cx + dx, cy + dy);
                    let w = |a: i32| a.rem_euclid(n as i32);
                    let jx = px as f32 + 0.15 + 0.7 * hash(w(px), w(py), 1);
                    let jy = py as f32 + 0.15 + 0.7 * hash(w(px), w(py), 2);
                    let d = ((x - jx).powi(2) + (y - jy).powi(2)).sqrt();
                    if d < best.0 {
                        best = (d, best.0, hash(w(px), w(py), 3));
                    } else if d < best.1 {
                        best.1 = d;
                    }
                }
            }
            let lead = smooth(0.04, 0.1, best.1 - best.0);
            let palette = [[0.75, 0.08, 0.1], [0.1, 0.2, 0.75], [0.85, 0.6, 0.1], [0.15, 0.55, 0.25], [0.45, 0.1, 0.55], [0.8, 0.75, 0.6]];
            let c = palette[(best.2 * palette.len() as f32) as usize % palette.len()];
            let shade = 0.75 + 0.35 * fbm(u, v, 12, 3, 4);
            [c[0] * shade * lead, c[1] * shade * lead, c[2] * shade * lead]
        }
        Tex::Rust => {
            let flakes = fbm(u, v, 10, 5, 71);
            let iron = [0.22, 0.22, 0.24];
            let rust = mix([0.45, 0.18, 0.06], [0.7, 0.35, 0.12], fbm(u, v, 20, 3, 73));
            mix(iron, rust, smooth(0.4, 0.6, flakes))
        }
        Tex::Leather => {
            let grain = fbm(u, v, 32, 3, 81);
            let cracks = smooth(0.0, 0.03, (fbm(u, v, 10, 3, 83) - 0.5).abs());
            grey((0.65 + 0.3 * grain) * (0.6 + 0.4 * cracks))
        }
        Tex::Marble => {
            let turb = fbm(u, v, 4, 5, 91);
            let veins = ((u * 6.0 + turb * 6.0) * std::f32::consts::PI).sin().abs().powf(0.25);
            mix([0.35, 0.33, 0.32], [0.92, 0.9, 0.86], veins)
        }
    }
}

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A box of `size` centred at `centre` (world) whose UVs are world metres / `tile`: textures
/// continue across neighbouring boxes and repeat instead of stretching.
pub fn tiled_box(size: Vec3, centre: Vec3, tile: f32) -> Mesh {
    let mut mesh = Mesh::from(Cuboid::from_size(size));
    let positions: Vec<[f32; 3]> = match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
        Some(VertexAttributeValues::Float32x3(p)) => p.clone(),
        _ => return mesh,
    };
    let normals: Vec<[f32; 3]> = match mesh.attribute(Mesh::ATTRIBUTE_NORMAL) {
        Some(VertexAttributeValues::Float32x3(n)) => n.clone(),
        _ => return mesh,
    };
    let uvs: Vec<[f32; 2]> = positions
        .iter()
        .zip(&normals)
        .map(|(p, n)| {
            let w = Vec3::from_array(*p) + centre;
            let (a, b) = if n[0].abs() > 0.5 {
                (w.z, -w.y)
            } else if n[1].abs() > 0.5 {
                (w.x, w.z)
            } else {
                (w.x, -w.y)
            };
            [a / tile, b / tile]
        })
        .collect();
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns_stay_in_range_and_tile() {
        for t in Tex::ALL {
            for (u, v) in [(0.0, 0.0), (0.37, 0.81), (0.999, 0.5)] {
                for c in sample(t, u, v) {
                    assert!((0.0..=1.2).contains(&c), "{t:?} at ({u}, {v}): {c}");
                }
            }
        }
        // Noise wraps around (no seam where the texture repeats).
        let (a, b) = (fbm(0.0, 0.3, 8, 4, 1), fbm(1.0, 0.3, 8, 4, 1));
        assert!((a - b).abs() < 1e-4);
    }

    #[test]
    fn tiled_box_uvs_follow_world_metres() {
        let mesh = tiled_box(Vec3::new(4.0, 1.0, 1.0), Vec3::ZERO, 2.0);
        let Some(VertexAttributeValues::Float32x2(uv)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0) else { panic!() };
        let span = uv.iter().map(|u| u[0]).fold(f32::MIN, f32::max) - uv.iter().map(|u| u[0]).fold(f32::MAX, f32::min);
        assert!((span - 2.0).abs() < 1e-4, "4 m wide / 2 m tiles = 2 repeats, got {span}");
    }
}
