//! Sound, synthesised in code at start-up (no audio files, no licensing): short one-shots
//! (footsteps, swings, hits, block clang, parry ring, drink gulp) and loops (sunflower hum, torch
//! crackle). Everything plays at its place in the world (spatial audio, the listener is the
//! camera); the master volume is in the settings.
//!
//! Triggers read the same components and messages as the rest of the game, so online puppets and
//! enemies make their sounds too.

use std::collections::HashMap;
use std::f32::consts::TAU;

use bevy::audio::{AudioSinkPlayback, GlobalVolume, SpatialAudioSink, SpatialListener, Volume};
use bevy::prelude::*;

use crate::arena::Torch;
use crate::camera::ThirdPersonCamera;
use crate::character::action::{ActionKind, ActionPhase, CharacterAction};
use crate::character::controller::CharacterController;
use crate::character::guard::{Blocked, Parried};
use crate::character::stats::DamageMessage;
use crate::combat::{Dummy, DummyStruck};
use crate::hearth::Hearth;

pub struct SoundPlugin;

impl Plugin for SoundPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<PlaySound>()
            .add_systems(Startup, build_sounds)
            .add_systems(Update, (add_listener, start_loops, footsteps, action_sounds, impact_sounds, (hum_volume, loop_volume).chain(), play_requests));
    }
}

const RATE: u32 = 44_100;

// ── Synthesis ────────────────────────────────────────────────────────────

/// Deterministic white noise.
struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

/// A one-shot anyone can ask for (`PlaySound::at`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sound {
    /// A big bronze bell.
    Bell,
    /// A slam's shockwave: a deep boom.
    Shockwave,
    /// A horror's shriek.
    Shriek,
    /// A blade run through a body: a wet crunch.
    Critical,
    /// A trap's darts: a sharp hiss of air.
    Dart,
    /// A pressure plate sinking: a stony click.
    Click,
}

#[derive(Message, Debug, Clone, Copy)]
pub struct PlaySound {
    pub sound: Sound,
    pub pos: Vec3,
    pub volume: f32,
}

impl PlaySound {
    pub fn at(sound: Sound, pos: Vec3) -> Self {
        Self { sound, pos, volume: 1.0 }
    }
}

/// A church bell: inharmonic partials (hum, prime, tierce, quint, nominal) ringing out slowly.
pub fn bell(fundamental: f32) -> Vec<f32> {
    let partials = [(0.5, 0.5, 0.35), (1.0, 0.6, 0.6), (1.2, 0.35, 0.9), (1.5, 0.25, 1.2), (2.0, 0.4, 1.6), (2.6, 0.15, 2.4)];
    samples(3.2, |t| {
        let strike = (-t * 60.0).exp() * 0.25 * (TAU * fundamental * 4.1 * t).sin();
        partials.iter().map(|(ratio, amp, decay)| (TAU * fundamental * ratio * t).sin() * amp * (-t * decay).exp()).sum::<f32>() * 0.45 + strike
    })
}

/// A deep boom with rumble (shockwaves).
pub fn boom(seed: u32) -> Vec<f32> {
    let (mut n, mut lp) = (Noise(seed.max(1)), LowPass { y: 0.0 });
    samples(1.1, |t| {
        let body = (TAU * (55.0 - 25.0 * t) * t).sin() * (-t * 4.0).exp() * 0.8;
        let rumble = lp.step(n.next(), 220.0) * (-t * 3.0).exp() * 1.4;
        (body + rumble).clamp(-1.0, 1.0)
    })
}

/// A shriek: two detuned high voices with vibrato over breath noise, swelling and dying.
pub fn shriek(seed: u32) -> Vec<f32> {
    let (mut n, mut lp) = (Noise(seed.max(1)), LowPass { y: 0.0 });
    samples(1.3, |t| {
        let env = (t * 8.0).min(1.0) * (-((t - 0.3).max(0.0)) * 2.6).exp();
        let vib = (TAU * 7.0 * t).sin() * 18.0;
        let a = (TAU * (1180.0 + vib) * t).sin();
        let b = (TAU * (1255.0 - vib) * t + 1.3).sin();
        let breath = lp.step(n.next(), 3000.0);
        ((a + b) * 0.28 + breath * 0.35) * env
    })
}

/// One-pole low-pass at `cutoff` Hz.
struct LowPass {
    y: f32,
}

impl LowPass {
    fn step(&mut self, x: f32, cutoff: f32) -> f32 {
        let a = 1.0 - (-TAU * cutoff / RATE as f32).exp();
        self.y += a * (x - self.y);
        self.y
    }
}

fn samples(seconds: f32, mut f: impl FnMut(f32) -> f32) -> Vec<f32> {
    (0..(seconds * RATE as f32) as usize).map(|i| f(i as f32 / RATE as f32)).collect()
}

/// A dull step: a low thump and a little grit.
pub fn footstep(seed: u32) -> Vec<f32> {
    let (mut n, mut lp) = (Noise(seed.max(1)), LowPass { y: 0.0 });
    samples(0.14, |t| {
        let grit = lp.step(n.next(), 900.0) * (-t * 40.0).exp() * 0.9;
        let thump = (TAU * 70.0 * t).sin() * (-t * 32.0).exp() * 0.7;
        grit + thump
    })
}

/// Air whoosh: band-limited noise swept up then down, swelling and fading.
pub fn whoosh(seconds: f32, low: f32, high: f32, seed: u32) -> Vec<f32> {
    let (mut n, mut lp1, mut lp2) = (Noise(seed.max(1)), LowPass { y: 0.0 }, LowPass { y: 0.0 });
    samples(seconds, |t| {
        let p = t / seconds;
        let cutoff = low + (high - low) * (std::f32::consts::PI * p).sin();
        let x = n.next();
        let band = lp1.step(x, cutoff) - lp2.step(x, cutoff * 0.35);
        band * (std::f32::consts::PI * p).sin().powi(2) * 2.2
    })
}

/// A body hit: a pitched-down thud with a short crack on top.
pub fn hit(pitch: f32, seed: u32) -> Vec<f32> {
    let (mut n, mut lp) = (Noise(seed.max(1)), LowPass { y: 0.0 });
    let mut phase = 0.0;
    samples(0.3, |t| {
        phase += TAU * pitch * (1.0 + 1.5 * (-t * 25.0).exp()) / RATE as f32;
        let thud = phase.sin() * (-t * 14.0).exp() * 0.9;
        let crack = lp.step(n.next(), 3000.0) * (-t * 90.0).exp() * 0.8;
        thud + crack
    })
}

/// A critical: a deep thud, a wet tearing squelch and a crack of bone.
pub fn critical(seed: u32) -> Vec<f32> {
    let (mut n, mut lp, mut lp2) = (Noise(seed.max(1)), LowPass { y: 0.0 }, LowPass { y: 0.0 });
    let mut phase = 0.0;
    samples(0.7, |t| {
        phase += TAU * 70.0 * (1.0 + 2.0 * (-t * 18.0).exp()) / RATE as f32;
        let thud = phase.sin() * (-t * 9.0).exp() * 0.9;
        let x = n.next();
        // Squelch: noise through a sweeping low-pass, wobbling.
        let cutoff = 400.0 + 1600.0 * (-t * 6.0).exp() * (1.0 + 0.5 * (t * 60.0).sin());
        let squelch = lp.step(x, cutoff) * (t * 30.0).min(1.0) * (-t * 5.0).exp() * 1.6;
        let crack = lp2.step(x, 5000.0) * (-((t - 0.04).abs()) * 160.0).exp() * 0.9;
        (thud + squelch + crack).clamp(-1.0, 1.0)
    })
}

/// A stance breaking: a heavy, dull crack and a short grunt-like groan.
pub fn stagger(seed: u32) -> Vec<f32> {
    let (mut n, mut lp) = (Noise(seed.max(1)), LowPass { y: 0.0 });
    let mut phase = 0.0;
    samples(0.5, |t| {
        phase += TAU * (110.0 - 40.0 * t) / RATE as f32;
        let groan = (phase.sin() + 0.4 * (phase * 2.0).sin()) * (t * 20.0).min(1.0) * (-t * 7.0).exp() * 0.5;
        let crack = lp.step(n.next(), 2200.0) * (-t * 35.0).exp() * 1.2;
        (groan + crack).clamp(-1.0, 1.0)
    })
}

/// Struck metal: inharmonic partials ringing out (`ring` = how long).
pub fn metal(partials: &[(f32, f32)], ring: f32, seed: u32) -> Vec<f32> {
    let mut n = Noise(seed.max(1));
    samples(ring * 1.2, |t| {
        let tone: f32 = partials.iter().map(|(f, a)| (TAU * f * t).sin() * a * (-t * (1.0 + f / 900.0) / ring).exp()).sum();
        tone + n.next() * (-t * 120.0).exp() * 0.5
    })
}

/// Two swallows: little falling "gloop" tones.
pub fn gulp() -> Vec<f32> {
    let mut phase = 0.0;
    samples(0.42, |t| {
        let (local, on) = if t < 0.16 { (t, true) } else if (0.22..0.38).contains(&t) { (t - 0.22, true) } else { (0.0, false) };
        if !on {
            return 0.0;
        }
        let f = 260.0 - 150.0 * (local / 0.16);
        phase += TAU * f / RATE as f32;
        phase.sin() * (std::f32::consts::PI * local / 0.16).sin() * 0.8
    })
}

/// The sunflower's hum: a soft open chord with a slow swell. Loops seamlessly (all frequencies
/// complete whole cycles in 4 s).
pub fn hum_loop() -> Vec<f32> {
    samples(4.0, |t| {
        let swell = 0.75 + 0.25 * (TAU * 0.5 * t).sin();
        let chord = (TAU * 110.0 * t).sin() * 0.5 + (TAU * 165.0 * t).sin() * 0.3 + (TAU * 220.0 * t).sin() * 0.2 + (TAU * 330.25 * t).sin() * 0.08;
        chord * swell * 0.5
    })
}

/// A torch: low hiss with random crackles. 3 s, crackles kept away from the loop seam.
pub fn crackle_loop(seed: u32) -> Vec<f32> {
    let (mut n, mut lp, mut pops) = (Noise(seed.max(1)), LowPass { y: 0.0 }, Noise(seed.wrapping_mul(7) | 1));
    let len = (3.0 * RATE as f32) as usize;
    let mut out: Vec<f32> = (0..len).map(|_| lp.step(n.next(), 600.0) * 0.25).collect();
    for _ in 0..22 {
        let at = ((pops.next() * 0.5 + 0.5) * (len as f32 - 0.1 * RATE as f32)) as usize;
        let amp = 0.3 + 0.5 * (pops.next() * 0.5 + 0.5);
        for i in 0..(0.02 * RATE as f32) as usize {
            let t = i as f32 / RATE as f32;
            if let Some(s) = out.get_mut(at + i) {
                *s += n.next() * amp * (-t * 300.0).exp();
            }
        }
    }
    out
}

/// 16-bit mono PCM WAV.
pub fn wav(samples: &[f32]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut b = Vec::with_capacity(44 + data_len as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes()); // PCM
    b.extend_from_slice(&1u16.to_le_bytes()); // mono
    b.extend_from_slice(&RATE.to_le_bytes());
    b.extend_from_slice(&(RATE * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        b.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32000.0) as i16).to_le_bytes());
    }
    b
}

// ── Playback ─────────────────────────────────────────────────────────────

#[derive(Resource)]
struct Sounds {
    steps: Vec<Handle<AudioSource>>,
    swing: Handle<AudioSource>,
    heavy_swing: Handle<AudioSource>,
    roll: Handle<AudioSource>,
    hit: Handle<AudioSource>,
    straw: Handle<AudioSource>,
    block: Handle<AudioSource>,
    parry: Handle<AudioSource>,
    gulp: Handle<AudioSource>,
    hum: Handle<AudioSource>,
    crackle: Vec<Handle<AudioSource>>,
    bell: Handle<AudioSource>,
    boom: Handle<AudioSource>,
    shriek: Handle<AudioSource>,
    critical: Handle<AudioSource>,
    stagger: Handle<AudioSource>,
    dart: Handle<AudioSource>,
    click: Handle<AudioSource>,
}

fn build_sounds(mut commands: Commands, mut sources: ResMut<Assets<AudioSource>>) {
    let mut add = |s: Vec<f32>| sources.add(AudioSource { bytes: wav(&s).into() });
    let sounds = Sounds {
        steps: (1..=4).map(|i| add(footstep(i * 977))).collect(),
        swing: add(whoosh(0.26, 500.0, 2200.0, 11)),
        heavy_swing: add(whoosh(0.42, 250.0, 1300.0, 12)),
        roll: add(whoosh(0.45, 150.0, 700.0, 13)),
        hit: add(hit(95.0, 21)),
        straw: add(hit(140.0, 22)),
        block: add(metal(&[(540.0, 0.5), (1290.0, 0.35), (2310.0, 0.2), (3170.0, 0.12)], 0.35, 31)),
        parry: add(metal(&[(880.0, 0.45), (1777.0, 0.35), (2640.0, 0.25), (3960.0, 0.15), (5280.0, 0.08)], 0.8, 32)),
        gulp: add(gulp()),
        hum: add(hum_loop()),
        crackle: (1..=3).map(|i| add(crackle_loop(i * 4099))).collect(),
        bell: add(bell(196.0)),
        boom: add(boom(41)),
        shriek: add(shriek(43)),
        critical: add(critical(47)),
        stagger: add(stagger(53)),
        dart: add(whoosh(0.18, 1800.0, 5000.0, 59)),
        click: add(metal(&[(820.0, 0.4), (1460.0, 0.25)], 0.08, 61)),
    };
    commands.insert_resource(sounds);
}

fn add_listener(mut commands: Commands, cams: Query<Entity, (With<ThirdPersonCamera>, Without<SpatialListener>)>) {
    for cam in &cams {
        commands.entity(cam).insert(SpatialListener::new(0.3));
    }
}

/// A one-shot at `pos`.
fn play(commands: &mut Commands, sound: &Handle<AudioSource>, pos: Vec3, volume: f32) {
    commands.spawn((
        AudioPlayer::new(sound.clone()),
        PlaybackSettings { spatial: true, volume: Volume::Linear(volume), ..PlaybackSettings::DESPAWN },
        Transform::from_translation(pos),
    ));
}

#[derive(Component)]
struct HumLoop;

/// A looping sound's own loudness. Bevy only applies the master volume when a sound starts, so
/// loops (which start at level load) follow it here: sink = base × master, every frame.
#[derive(Component)]
struct LoopVolume(f32);

/// Loops on the sunflower and the torches (once they exist).
fn start_loops(
    mut commands: Commands,
    sounds: Option<Res<Sounds>>,
    hearths: Query<Entity, (With<Hearth>, Without<HumLoop>)>,
    torches: Query<Entity, (With<Torch>, Without<HumLoop>)>,
) {
    let Some(sounds) = sounds else { return };
    for e in &hearths {
        commands.entity(e).insert(HumLoop);
        commands.spawn((
            HumLoop,
            LoopVolume(0.15),
            AudioPlayer::new(sounds.hum.clone()),
            PlaybackSettings { spatial: true, volume: Volume::Linear(0.15), ..PlaybackSettings::LOOP },
            Transform::from_xyz(0.0, 1.5, 0.0),
            ChildOf(e),
        ));
    }
    for (i, e) in torches.iter().enumerate() {
        commands.entity(e).insert(HumLoop);
        commands.spawn((
            LoopVolume(0.22),
            AudioPlayer::new(sounds.crackle[i % sounds.crackle.len()].clone()),
            PlaybackSettings { spatial: true, volume: Volume::Linear(0.22), ..PlaybackSettings::LOOP },
            Transform::default(),
            ChildOf(e),
        ));
    }
}

/// A revived sunflower hums louder.
fn hum_volume(hearths: Query<&Hearth>, mut loops: Query<(&mut LoopVolume, &ChildOf), With<HumLoop>>) {
    for (mut base, parent) in &mut loops {
        let Ok(hearth) = hearths.get(parent.parent()) else { continue };
        base.0 = if hearth.kindled { 0.55 } else { 0.15 };
    }
}

/// Loops (torches, the sunflower) follow the master volume setting while they play.
fn loop_volume(master: Res<GlobalVolume>, mut sinks: Query<(&mut SpatialAudioSink, &LoopVolume)>) {
    let master = master.volume.to_linear();
    for (mut sink, base) in &mut sinks {
        sink.set_volume(Volume::Linear(base.0 * master));
    }
}

/// Distance walked since the last footstep, per character.
#[derive(Default)]
struct Strides(HashMap<Entity, f32>);

fn footsteps(
    mut commands: Commands,
    time: Res<Time>,
    sounds: Option<Res<Sounds>>,
    mut strides: Local<Strides>,
    mut n: Local<usize>,
    q: Query<(Entity, &Transform, &CharacterController, &CharacterAction)>,
) {
    let Some(sounds) = sounds else { return };
    let dt = time.delta_secs();
    for (e, tf, ctrl, action) in &q {
        let speed = Vec2::new(ctrl.moved.x, ctrl.moved.z).length();
        let walking = ctrl.grounded && speed > 0.6 && action.current.is_none_or(|a| a.kind != ActionKind::Dead);
        let acc = strides.0.entry(e).or_default();
        if !walking {
            *acc = 0.0;
            continue;
        }
        *acc += speed * dt;
        let stride = 0.62 + speed * 0.07;
        if *acc >= stride {
            *acc -= stride;
            *n += 1;
            let volume = (0.18 + speed * 0.06).min(0.6);
            play(&mut commands, &sounds.steps[*n % sounds.steps.len()], tf.translation - Vec3::Y * 0.85, volume);
        }
    }
    strides.0.retain(|e, _| q.contains(*e));
}

/// Swings, rolls and drinks, when the action reaches the moment.
fn action_sounds(
    mut commands: Commands,
    sounds: Option<Res<Sounds>>,
    mut played: Local<HashMap<Entity, (u32, ActionKind)>>,
    q: Query<(Entity, &Transform, &CharacterAction)>,
) {
    let Some(sounds) = sounds else { return };
    for (e, tf, action) in &q {
        let Some(cur) = action.current else { continue };
        let key = (action.serial, cur.kind);
        let sound = match cur.kind {
            ActionKind::LightAttack(_) | ActionKind::RunningAttack | ActionKind::JumpAttack if cur.phase() == ActionPhase::Active => &sounds.swing,
            ActionKind::HeavyAttack if cur.phase() == ActionPhase::Active => &sounds.heavy_swing,
            ActionKind::Roll | ActionKind::Backstep => &sounds.roll,
            ActionKind::DrinkAle | ActionKind::UseItem if cur.phase() == ActionPhase::Active => &sounds.gulp,
            // Reeling (parried or stance broken): the stagger's crack. Works for puppets too.
            ActionKind::Stunned => &sounds.stagger,
            _ => continue,
        };
        if played.get(&e) == Some(&key) {
            continue;
        }
        played.insert(e, key);
        play(&mut commands, sound, tf.translation + Vec3::Y * 0.4, 0.7);
    }
    played.retain(|e, _| q.contains(*e));
}

fn play_requests(mut commands: Commands, sounds: Option<Res<Sounds>>, mut requests: MessageReader<PlaySound>) {
    let Some(sounds) = sounds else { return };
    for r in requests.read() {
        let s = match r.sound {
            Sound::Bell => &sounds.bell,
            Sound::Shockwave => &sounds.boom,
            Sound::Shriek => &sounds.shriek,
            Sound::Critical => &sounds.critical,
            Sound::Dart => &sounds.dart,
            Sound::Click => &sounds.click,
        };
        play(&mut commands, s, r.pos, r.volume);
    }
}

/// Weapon hits (detected on this machine), dummy hits (from anyone), blocks and parries.
#[allow(clippy::too_many_arguments)]
fn impact_sounds(
    mut commands: Commands,
    sounds: Option<Res<Sounds>>,
    mut damage: MessageReader<DamageMessage>,
    mut struck: MessageReader<DummyStruck>,
    mut blocked: MessageReader<Blocked>,
    mut parried: MessageReader<Parried>,
    positions: Query<&GlobalTransform>,
    dummies: Query<(Entity, &GlobalTransform), With<Dummy>>,
) {
    let Some(sounds) = sounds else { return };
    let at = |e: Entity| positions.get(e).map(|g| g.translation() + Vec3::Y * 0.4).ok();
    for d in damage.read() {
        // Weapon hits only (they have an attacker); the dummy sounds via `DummyStruck`.
        if d.attacker.is_none() || dummies.contains(d.target) {
            continue;
        }
        if let Some(p) = at(d.target) {
            play(&mut commands, &sounds.hit, p, (0.45 + d.amount * 0.012).min(1.0));
        }
    }
    for s in struck.read() {
        if let Ok((_, g)) = dummies.single() {
            play(&mut commands, &sounds.straw, g.translation() + Vec3::Y * 0.5, (0.4 + s.amount * 0.012).min(1.0));
        }
    }
    for b in blocked.read() {
        if let Some(p) = at(b.defender) {
            play(&mut commands, &sounds.block, p, 0.8);
        }
    }
    for p in parried.read() {
        if let Some(pos) = at(p.defender) {
            play(&mut commands, &sounds.parry, pos, 1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rms(s: &[f32]) -> f32 {
        (s.iter().map(|x| x * x).sum::<f32>() / s.len() as f32).sqrt()
    }

    #[test]
    fn sounds_are_audible_and_dont_clip() {
        for (name, s) in [
            ("footstep", footstep(1)),
            ("swing", whoosh(0.26, 500.0, 2200.0, 11)),
            ("hit", hit(95.0, 21)),
            ("block", metal(&[(540.0, 0.5), (1290.0, 0.35)], 0.35, 31)),
            ("gulp", gulp()),
            ("hum", hum_loop()),
            ("crackle", crackle_loop(3)),
            ("bell", bell(196.0)),
            ("boom", boom(41)),
            ("shriek", shriek(43)),
        ] {
            let peak = s.iter().fold(0.0f32, |m, x| m.max(x.abs()));
            assert!(rms(&s) > 0.01, "{name} is silent");
            assert!(peak <= 1.2, "{name} clips: {peak}");
        }
    }

    #[test]
    fn hum_loops_seamlessly_and_wav_header_is_valid() {
        let s = hum_loop();
        let (first, last) = (s[0], *s.last().unwrap());
        assert!((first - last).abs() < 0.02, "seam jump {first} → {last}");
        let w = wav(&s);
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(&w[8..16], b"WAVEfmt ");
        assert_eq!(u32::from_le_bytes(w[40..44].try_into().unwrap()) as usize, s.len() * 2);
        // Bevy/rodio can decode it.
        use bevy::audio::Decodable;
        let source = AudioSource { bytes: w.into() };
        assert!(source.decoder().count() > 1000);
    }
}
