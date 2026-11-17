//! Area ambience: a soundscape per area rather than a tune (drones, wind, water, a far choir,
//! a forge's rumble), synthesised at start-up like every other sound (`crate::sound`). Each level
//! room names its [`Ambience`] (`RoomDef::ambience`, default: its theme's); outside every room, in
//! the central arena, it's [`Ambience::Arena`]. Walking from one area into another cross-fades over
//! `AMBIENCE_FADE` seconds. Loudness follows the master volume.

use std::f32::consts::TAU;

use bevy::audio::{AudioSink, AudioSinkPlayback, GlobalVolume, Volume};
use bevy::prelude::*;
use serde::Deserialize;

use crate::level::Levels;
use crate::level::layout::Theme;
use crate::player::Player;
use crate::tuning::*;

pub struct AmbiencePlugin;

impl Plugin for AmbiencePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CurrentAmbience>().add_systems(Startup, synthesise).add_systems(Update, (pick_ambience, crossfade).chain());
    }
}

/// Beds synthesised on a background thread at start-up (WAV bytes), so entering an area never
/// stalls a frame.
#[derive(Resource, Default, Clone)]
struct BedBank(std::sync::Arc<std::sync::Mutex<Vec<(Ambience, Vec<u8>)>>>);

fn synthesise(mut commands: Commands) {
    let bank = BedBank::default();
    let out = bank.0.clone();
    std::thread::spawn(move || {
        for kind in Ambience::ALL {
            let bytes = crate::sound::wav(&bed(kind));
            if let Ok(mut v) = out.lock() {
                v.push((kind, bytes));
            }
        }
    });
    commands.insert_resource(bank);
}

/// A soundscape.
#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Ambience {
    /// The central arena: a hollow wind over a low, slow drone.
    Arena,
    /// Damp stone halls: dripping water, a cold drone.
    Halls,
    /// The forge: a deep fire rumble, a metallic hum, a far anvil now and then.
    Forge,
    /// The cathedral: a far, breathing choir and high wind.
    Cathedral,
    /// Ossuary and crypt: a deep drone, whispers.
    Crypt,
    /// A boss's hall: a dissonant pulse and slow booms.
    Boss,
}

impl Ambience {
    pub const ALL: [Ambience; 6] = [Ambience::Arena, Ambience::Halls, Ambience::Forge, Ambience::Cathedral, Ambience::Crypt, Ambience::Boss];

    /// The default of a level theme.
    pub fn of_theme(theme: Theme) -> Self {
        match theme {
            Theme::Sunken => Ambience::Halls,
            Theme::Gothic => Ambience::Cathedral,
        }
    }

    /// Its loudness (the beds differ in how dense they are).
    fn gain(self) -> f32 {
        match self {
            Ambience::Arena => 0.55,
            Ambience::Halls => 0.6,
            Ambience::Forge => 0.6,
            Ambience::Cathedral => 0.5,
            Ambience::Crypt => 0.6,
            Ambience::Boss => 0.55,
        }
    }
}

/// The area the player is in now (by room).
#[derive(Resource, Debug, Default)]
pub struct CurrentAmbience(pub Option<Ambience>);

/// A playing bed and how far it's faded in (0..1).
#[derive(Component)]
struct Bed {
    kind: Ambience,
    level: f32,
}

fn pick_ambience(levels: Option<Res<Levels>>, player: Query<&Transform, With<Player>>, mut current: ResMut<CurrentAmbience>) {
    let (Some(levels), Ok(tf)) = (levels, player.single()) else { return };
    let p = Vec2::new(tf.translation.x, tf.translation.z);
    let here = levels.0.iter().find_map(|layout| {
        let room = &layout.file.rooms[layout.room_at(p)?];
        Some(room.ambience.unwrap_or(Ambience::of_theme(layout.file.theme)))
    });
    let here = here.or_else(|| (p.length() < ARENA_RADIUS + 1.0).then_some(Ambience::Arena));
    // On stairs and in doorways (no room cell) the last area holds.
    if let Some(a) = here
        && current.0 != Some(a)
    {
        info!("ambience: {a:?}");
        current.0 = Some(a);
    }
}

fn crossfade(
    mut commands: Commands,
    time: Res<Time>,
    master: Res<GlobalVolume>,
    current: Res<CurrentAmbience>,
    mut sources: ResMut<Assets<AudioSource>>,
    bank: Option<Res<BedBank>>,
    mut cache: Local<Vec<(Ambience, Handle<AudioSource>)>>,
    mut beds: Query<(Entity, &mut Bed, Option<&mut AudioSink>)>,
) {
    let step = time.delta_secs() / AMBIENCE_FADE;
    let mut playing = false;
    for (e, mut bed, sink) in &mut beds {
        let wanted = current.0 == Some(bed.kind);
        playing |= wanted;
        bed.level = if wanted { (bed.level + step).min(1.0) } else { (bed.level - step).max(0.0) };
        if !wanted && bed.level <= 0.0 {
            commands.entity(e).despawn();
            continue;
        }
        if let Some(mut sink) = sink {
            // Ease the curve so the cross-fade keeps the loudness even.
            let level = (bed.level * std::f32::consts::FRAC_PI_2).sin();
            sink.set_volume(Volume::Linear(level * bed.kind.gain() * AMBIENCE_VOLUME * master.volume.to_linear()));
        }
    }
    let Some(kind) = current.0 else { return };
    if playing {
        return;
    }
    let handle = match cache.iter().find(|(k, _)| *k == kind) {
        Some((_, h)) => h.clone(),
        None => {
            // Not synthesised yet: try again next frame.
            let Some(bytes) = bank.and_then(|b| b.0.lock().ok().and_then(|v| v.iter().find(|(k, _)| *k == kind).map(|(_, b)| b.clone()))) else { return };
            let h = sources.add(AudioSource { bytes: bytes.into() });
            cache.push((kind, h.clone()));
            h
        }
    };
    commands.spawn((
        Name::new("Ambience"),
        Bed { kind, level: 0.0 },
        AudioPlayer::new(handle),
        PlaybackSettings { volume: Volume::Linear(0.0), ..PlaybackSettings::LOOP },
    ));
}

// ── Synthesis ────────────────────────────────────────────────────────────

const RATE: f32 = 44_100.0;
/// Length of every bed (s); the loop seam is cross-faded over `SEAM`.
const LENGTH: f32 = 24.0;
const SEAM: f32 = 2.0;

struct Rng(u32);

impl Rng {
    fn next(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
    fn unit(&mut self) -> f32 {
        self.next() * 0.5 + 0.5
    }
}

/// One-pole low-pass.
#[derive(Default)]
struct Lp(f32);

impl Lp {
    fn step(&mut self, x: f32, cutoff: f32) -> f32 {
        let a = 1.0 - (-TAU * cutoff / RATE).exp();
        self.0 += a * (x - self.0);
        self.0
    }
}

/// A resonant band-pass (state-variable filter).
#[derive(Default)]
struct Bp {
    low: f32,
    band: f32,
}

impl Bp {
    fn step(&mut self, x: f32, freq: f32, q: f32) -> f32 {
        let f = 2.0 * (std::f32::consts::PI * freq / RATE).sin();
        let high = x - self.low - self.band / q;
        self.band += f * high;
        self.low += f * self.band;
        self.band
    }
}

/// A sustained tone: partials with slow, independent wobble (sounds alive, not like a test tone).
fn drone(t: f32, f: f32, partials: &[(f32, f32)], wobble: f32) -> f32 {
    partials.iter().enumerate().map(|(i, (h, a))| (TAU * f * h * t + wobble * (t * (0.07 + i as f32 * 0.031)).sin()).sin() * a).sum()
}

/// Slow breathing envelope between `lo` and 1 with period `p`.
fn breathe(t: f32, p: f32, phase: f32, lo: f32) -> f32 {
    lo + (1.0 - lo) * (0.5 - 0.5 * (TAU * t / p + phase).cos())
}

/// The bed of `kind`: `LENGTH` seconds that loop seamlessly.
pub fn bed(kind: Ambience) -> Vec<f32> {
    let total = ((LENGTH + SEAM) * RATE) as usize;
    let mut rng = Rng(0x9E37_79B9 ^ ((kind as u32 + 1) * 7919));
    let (mut lp, mut lp2, mut bp, mut bp2) = (Lp::default(), Lp::default(), Bp::default(), Bp::default());
    // Sparse events (drips, clangs, booms, whispers): start times.
    let mut events: Vec<(f32, f32)> = vec![];
    let (rate, len) = match kind {
        Ambience::Halls => (1.3, 0.6),
        Ambience::Forge => (0.18, 2.5),
        Ambience::Crypt => (0.25, 1.6),
        Ambience::Boss => (0.5, 1.8),
        _ => (0.0, 0.0),
    };
    let mut at = 0.5;
    while rate > 0.0 && at < LENGTH - len {
        events.push((at, 0.6 + 0.8 * rng.unit()));
        at += (1.0 / rate) * (0.4 + 1.2 * rng.unit());
    }
    let mut out = Vec::with_capacity(total);
    for i in 0..total {
        let t = i as f32 / RATE;
        let n = rng.next();
        let s = match kind {
            Ambience::Arena => {
                // Wind: noise through a slowly sweeping band, plus a low fifth.
                let sweep = 280.0 + 220.0 * (t * 0.21).sin() + 90.0 * (t * 0.53).sin();
                let wind = bp.step(n, sweep, 3.0) * 0.5 * breathe(t, 8.0, 0.0, 0.35);
                let low = drone(t, 55.0, &[(1.0, 0.3), (1.5, 0.12), (2.0, 0.08)], 0.6) * breathe(t, 12.0, 1.0, 0.6);
                wind + low * 0.6
            }
            Ambience::Halls => {
                let air = lp.step(n, 160.0) * 1.4;
                let low = drone(t, 73.4, &[(1.0, 0.22), (2.0, 0.07), (2.99, 0.04)], 0.4) * breathe(t, 11.0, 0.5, 0.5);
                // Drips: a short, falling ping through a resonance.
                let drips: f32 = events
                    .iter()
                    .filter(|(s, _)| t >= *s && t < *s + 0.35)
                    .map(|(s, pitch)| {
                        let d = t - s;
                        (TAU * (900.0 + 700.0 * pitch) * d * (1.0 - d * 0.8)).sin() * (-d * 28.0).exp() * 0.22
                    })
                    .sum();
                air + low + drips
            }
            Ambience::Forge => {
                // Fire: brown-ish rumble with crackle; a hum; a far anvil clang now and then.
                let rumble = lp2.step(lp.step(n, 90.0), 60.0) * 6.0 * breathe(t, 5.0, 0.3, 0.6);
                let crackle = if rng.unit() > 0.9993 { rng.next() * 0.5 } else { 0.0 };
                let hum = drone(t, 49.0, &[(1.0, 0.15), (3.0, 0.05), (5.02, 0.03)], 0.3);
                let clangs: f32 = events
                    .iter()
                    .filter(|(s, _)| t >= *s && t < *s + 2.5)
                    .map(|(s, pitch)| {
                        let d = t - s;
                        [(1.0, 0.5), (2.76, 0.3), (5.4, 0.2)].iter().map(|(h, a)| (TAU * 310.0 * pitch * h * d).sin() * a).sum::<f32>() * (-d * 2.2).exp() * 0.08
                    })
                    .sum();
                rumble + crackle + hum + clangs
            }
            Ambience::Cathedral => {
                // A far choir: an A-minor chord of soft "ah"s (detuned voices through a vowel
                // formant), each voice breathing on its own; high wind through the vaults.
                let voices = [(110.0, 0.0), (130.8, 1.7), (164.8, 3.1), (220.0, 4.4), (261.6, 5.3)];
                let mut choir = 0.0;
                for (k, (f, ph)) in voices.iter().enumerate() {
                    let vib = 1.0 + 0.004 * (t * (4.7 + k as f32 * 0.3)).sin();
                    let saw = |ff: f32| ((ff * t) % 1.0) * 2.0 - 1.0;
                    choir += (saw(f * vib) + saw(f * vib * 1.003)) * breathe(t, 9.0 + k as f32 * 1.3, *ph, 0.15);
                }
                let ah = bp.step(choir, 700.0, 2.5) * 0.6 + bp2.step(choir, 1150.0, 4.0) * 0.3;
                let wind = lp.step(n, 900.0) * 0.25 * breathe(t, 7.0, 2.0, 0.3);
                ah * 0.08 + wind
            }
            Ambience::Crypt => {
                let low = drone(t, 36.7, &[(1.0, 0.35), (2.0, 0.12), (2.5, 0.05)], 0.8) * breathe(t, 14.0, 0.0, 0.55);
                let air = lp.step(n, 120.0) * 1.2;
                // Whispers: hissy noise bursts through a moving vowel.
                let whisper: f32 = events
                    .iter()
                    .filter(|(s, _)| t >= *s && t < *s + 1.6)
                    .map(|(s, pitch)| {
                        let d = (t - s) / 1.6;
                        (std::f32::consts::PI * d).sin().powi(2) * pitch
                    })
                    .sum();
                let hiss = bp.step(n, 2200.0 + 900.0 * (t * 3.0).sin(), 3.5) * whisper * 0.18;
                low + air + hiss
            }
            Ambience::Boss => {
                // A tritone cluster that throbs, and a deep boom every couple of seconds.
                let pulse = 0.6 + 0.4 * (TAU * t * 0.5).sin().max(0.0);
                let cluster = drone(t, 58.3, &[(1.0, 0.25), (1.414, 0.18), (2.0, 0.1), (2.83, 0.06)], 0.5) * pulse;
                let booms: f32 = events
                    .iter()
                    .filter(|(s, _)| t >= *s && t < *s + 1.8)
                    .map(|(s, _)| {
                        let d = t - s;
                        (TAU * 42.0 * d * (1.0 + 0.6 * (-d * 6.0).exp())).sin() * (-d * 2.5).exp() * 0.5
                    })
                    .sum();
                let air = lp.step(n, 300.0) * 0.8;
                cluster + booms + air
            }
        };
        out.push(s);
    }
    // Seamless loop: blend the extra tail into the head.
    let len = (LENGTH * RATE) as usize;
    let seam = (SEAM * RATE) as usize;
    for i in 0..seam {
        let k = i as f32 / seam as f32;
        out[i] = out[i] * k + out[len + i] * (1.0 - k);
    }
    out.truncate(len);
    // Normalise to a calm peak.
    let peak = out.iter().fold(0.0f32, |m, s| m.max(s.abs())).max(1e-6);
    out.iter_mut().for_each(|s| *s *= 0.7 / peak);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beds_loop_without_a_click_and_are_audible() {
        for kind in Ambience::ALL {
            let b = bed(kind);
            assert_eq!(b.len(), (LENGTH * RATE) as usize);
            let rms = (b.iter().map(|s| s * s).sum::<f32>() / b.len() as f32).sqrt();
            assert!(rms > 0.03, "{kind:?} too quiet ({rms})");
            assert!(b.iter().all(|s| s.is_finite() && s.abs() <= 0.71), "{kind:?}");
            // The seam: the last sample flows into the first.
            let jump = (b[b.len() - 1] - b[0]).abs();
            assert!(jump < 0.1, "{kind:?} clicks at the loop ({jump})");
        }
    }
}
