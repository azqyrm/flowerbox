//! The Storm Isle's weather and its closing ring (battle-royale mechanics, the Souls way):
//! - **The storm ring** ([`Storm`], placed by a level's `StormEye` prop): once you step inside its
//!   starting circle it begins to close on the eye in stages — hold, shrink, hold, shrink… down to
//!   the final circle where the Stormcaller waits. Outside it the storm burns (`STORM_DAMAGE` every
//!   `STORM_TICK` s, no stance damage). A wall of churning mist marks it. It opens again when the
//!   world resets (rest, death) or when you leave the isle.
//! - **Lightning** flashes over the isle (with thunder) and **rain** falls round the player while
//!   on it (the ambience is `Storm`).
//! - **Falling** off any edge into the void (below `KILL_Y`) kills you.
//!
//! Each machine runs the ring for its own player (like traps): a guest's ring starts when the
//! guest steps in.

use std::f32::consts::TAU;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use crate::ambience::{Ambience, CurrentAmbience};
use crate::character::Remote;
use crate::character::stats::{DamageMessage, Health};
use crate::hearth::WorldReset;
use crate::player::Player;
use crate::tuning::*;

pub struct StormPlugin;

impl Plugin for StormPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_flash).add_systems(Update, (run_storm, lightning, rain, fall_to_death));
    }
}

/// The closing ring round `centre`.
#[derive(Component, Debug)]
pub struct Storm {
    pub centre: Vec3,
    pub start: f32,
    pub end: f32,
    pub radius: f32,
    /// Seconds since it began closing (`None` = waiting for someone to step in).
    pub t: Option<f32>,
    tick: f32,
}

impl Storm {
    pub fn new(centre: Vec3, start: f32, end: f32) -> Self {
        Self { centre, start, end, radius: start, t: None, tick: 0.0 }
    }
}

/// The ring's radius `t` seconds after it began closing from `start` to `end`: a hold, a shrink,
/// a hold, a shrink… (`STORM_STAGES`: hold seconds, shrink seconds, fraction of the way closed).
pub fn ring_radius(start: f32, end: f32, t: f32) -> f32 {
    let mut at = 0.0;
    let mut closed = 0.0;
    for (hold, shrink, to) in STORM_STAGES {
        at += hold;
        if t < at {
            break;
        }
        let k = ((t - at) / shrink).min(1.0);
        let eased = k * k * (3.0 - 2.0 * k);
        let now = closed + (to - closed) * eased;
        if t < at + shrink {
            return start + (end - start) * now;
        }
        at += shrink;
        closed = to;
    }
    start + (end - start) * closed
}

/// The mist wall: an open tube, scaled to the radius.
#[derive(Component)]
struct StormWall;

/// Builds an open tube of radius 1, from y 0 to `height`, for the storm wall.
pub fn tube(height: f32) -> Mesh {
    let n = 72;
    let mut pos = vec![];
    let mut uv = vec![];
    let mut normals = vec![];
    for row in 0..2 {
        for i in 0..=n {
            let a = i as f32 / n as f32 * TAU;
            pos.push([a.cos(), row as f32 * height, a.sin()]);
            normals.push([-a.cos(), 0.0, -a.sin()]);
            uv.push([i as f32 / n as f32 * 8.0, row as f32]);
        }
    }
    let w = n as u32 + 1;
    let mut idx = vec![];
    for i in 0..n as u32 {
        idx.extend([i, i + w, i + 1, i + 1, i + w, i + 1 + w]);
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
        .with_inserted_indices(Indices::U32(idx))
}

/// Spawns a storm ring (called by the level builder for a `StormEye` prop).
pub fn spawn_storm(commands: &mut Commands, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>, centre: Vec3, start: f32, end: f32) {
    let mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.45, 0.55, 0.95, 0.22),
        base_color_texture: crate::textures::get(crate::textures::Tex::Cloth),
        emissive: LinearRgba::rgb(0.6, 0.8, 2.2),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    commands
        .spawn((Name::new("Storm"), Storm::new(centre, start, end), Transform::from_translation(centre), Visibility::default()))
        .with_child((StormWall, Mesh3d(meshes.add(tube(40.0))), MeshMaterial3d(mat), Transform::from_xyz(0.0, -12.0, 0.0).with_scale(Vec3::new(start, 1.0, start))));
}

#[allow(clippy::type_complexity)]
fn run_storm(
    time: Res<Time>,
    mut resets: MessageReader<WorldReset>,
    mut storms: Query<(&mut Storm, &Children)>,
    mut walls: Query<&mut Transform, With<StormWall>>,
    player: Query<(Entity, &Transform, &Health), (With<Player>, Without<Remote>, Without<StormWall>)>,
    mut damage: MessageWriter<DamageMessage>,
    mut notify: MessageWriter<crate::ui::notifications::Notify>,
) {
    let reset = resets.read().count() > 0;
    let dt = time.delta_secs();
    let Ok((me, ptf, hp)) = player.single() else { return };
    for (mut s, children) in &mut storms {
        let d = (ptf.translation - s.centre).with_y(0.0).length();
        let on_isle = (ptf.translation.y - s.centre.y).abs() < 12.0;
        if reset || !on_isle || d > s.start + 25.0 {
            if s.t.is_some() {
                info!("the storm opens again");
            }
            s.t = None;
            s.radius = s.start;
        } else if s.t.is_none() && d < s.start - 1.0 && !hp.is_dead() {
            s.t = Some(0.0);
            info!("the storm begins to close");
            notify.write(crate::ui::notifications::Notify::new(crate::ui::icons::NoticeIcon::Died, crate::ui::strings::STORM_CLOSING.to_string()));
        }
        if let Some(t) = s.t.as_mut() {
            *t += dt;
            let t = *t;
            s.radius = ring_radius(s.start, s.end, t);
            // Outside: burn.
            s.tick += dt;
            if s.tick >= STORM_TICK {
                s.tick -= STORM_TICK;
                if d > s.radius && !hp.is_dead() {
                    damage.write(DamageMessage { target: me, amount: STORM_DAMAGE, from: None, attacker: None, stance: 0.0 });
                }
            }
        }
        for c in children.iter() {
            if let Ok(mut tf) = walls.get_mut(c) {
                tf.scale = Vec3::new(s.radius, 1.0, s.radius);
                tf.rotation = Quat::from_rotation_y(time.elapsed_secs() * 0.05);
            }
        }
    }
}

/// A flash of lightning: a directional light that's dark except for a moment.
#[derive(Component)]
struct Flash {
    left: f32,
    next: f32,
    seed: u32,
}

fn spawn_flash(mut commands: Commands) {
    commands.spawn((
        Name::new("Lightning"),
        Flash { left: 0.0, next: 3.0, seed: 0x5EED_1234 },
        DirectionalLight { illuminance: 0.0, color: Color::srgb(0.75, 0.82, 1.0), shadow_maps_enabled: false, ..default() },
        Transform::from_xyz(0.0, 50.0, 0.0).looking_at(Vec3::new(10.0, 0.0, 6.0), Vec3::Y),
    ));
}

fn lightning(
    time: Res<Time>,
    current: Res<CurrentAmbience>,
    mut flashes: Query<&mut Flash>,
    mut lights: Query<&mut DirectionalLight, With<Flash>>,
    player: Query<&Transform, With<Player>>,
    mut sounds: MessageWriter<crate::sound::PlaySound>,
) {
    let dt = time.delta_secs();
    let on_isle = current.0 == Some(Ambience::Storm);
    let (Ok(mut f), Ok(mut light)) = (flashes.single_mut(), lights.single_mut()) else { return };
    f.left = (f.left - dt).max(0.0);
    f.next -= dt;
    if on_isle && f.next <= 0.0 {
        f.seed ^= f.seed << 13;
        f.seed ^= f.seed >> 17;
        f.seed ^= f.seed << 5;
        let r = (f.seed % 1000) as f32 / 1000.0;
        f.next = 4.0 + 8.0 * r;
        f.left = 0.18;
        if let Ok(p) = player.single() {
            let far = p.translation + Vec3::new(30.0 * (r * TAU).cos(), 20.0, 30.0 * (r * TAU).sin());
            sounds.write(crate::sound::PlaySound { sound: crate::sound::Sound::Shockwave, pos: far, volume: 0.6 });
        }
    }
    // Two quick strobes.
    let flicker = if f.left > 0.12 || (f.left > 0.0 && f.left < 0.06) { 1.0 } else { 0.0 };
    light.illuminance = if on_isle { 25_000.0 * flicker } else { 0.0 };
}

/// A streak of rain.
#[derive(Component)]
struct Drop {
    speed: f32,
}

fn rain(
    mut commands: Commands,
    time: Res<Time>,
    current: Res<CurrentAmbience>,
    mut assets: Local<Option<(Handle<Mesh>, Handle<StandardMaterial>)>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut seed: Local<u32>,
    player: Query<&Transform, (With<Player>, Without<Drop>)>,
    mut drops: Query<(Entity, &Drop, &mut Transform)>,
) {
    let on_isle = current.0 == Some(Ambience::Storm);
    if !on_isle {
        for (e, _, _) in &drops {
            commands.entity(e).despawn();
        }
        return;
    }
    let Ok(p) = player.single() else { return };
    let mut rng = || {
        *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (*seed >> 8) as f32 / (u32::MAX >> 8) as f32
    };
    let (mesh, mat) = assets
        .get_or_insert_with(|| {
            (
                meshes.add(Cuboid::new(0.012, 0.55, 0.012)),
                materials.add(StandardMaterial { base_color: Color::srgba(0.7, 0.75, 0.85, 0.35), alpha_mode: AlphaMode::Blend, unlit: true, ..default() }),
            )
        })
        .clone();
    let count = drops.iter().count();
    for _ in count..RAIN_DROPS {
        let at = p.translation + Vec3::new((rng() - 0.5) * 30.0, rng() * 16.0, (rng() - 0.5) * 30.0);
        commands.spawn((Drop { speed: 16.0 + 6.0 * rng() }, Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), Transform::from_translation(at).with_rotation(Quat::from_rotation_z(0.12))));
    }
    let dt = time.delta_secs();
    for (_, d, mut tf) in &mut drops {
        tf.translation += Vec3::new(1.8, -d.speed, 0.0) * dt;
        let rel = tf.translation - p.translation;
        if rel.y < -3.0 || rel.x.abs() > 16.0 || rel.z.abs() > 16.0 {
            tf.translation = p.translation + Vec3::new((rng() - 0.5) * 30.0, 10.0 + rng() * 6.0, (rng() - 0.5) * 30.0);
        }
    }
}

/// Off an edge, into the void: dead.
fn fall_to_death(player: Query<(Entity, &Transform, &Health), (With<Player>, Without<Remote>)>, mut damage: MessageWriter<DamageMessage>) {
    let Ok((e, tf, hp)) = player.single() else { return };
    if tf.translation.y < KILL_Y && !hp.is_dead() {
        info!("fell into the void");
        damage.write(DamageMessage { target: e, amount: f32::MAX, from: None, attacker: None, stance: 0.0 });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ring_holds_then_closes_in_stages() {
        let (start, end) = (40.0, 9.0);
        assert_eq!(ring_radius(start, end, 0.0), start);
        let first_hold = STORM_STAGES[0].0;
        assert_eq!(ring_radius(start, end, first_hold - 0.1), start, "holds first");
        let mut last = start;
        for i in 0..2000 {
            let r = ring_radius(start, end, i as f32 * 0.1);
            assert!(r <= last + 1e-4, "never opens up again");
            last = r;
        }
        assert!((last - end).abs() < 1e-3, "ends at the final circle ({last})");
    }
}
