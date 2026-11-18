//! The Womb's flesh (level 4): the walls themselves pulse with a heartbeat (the theme's materials
//! glow brighter on every beat, `pulse_flesh`), and its props:
//! - **Membranes**: a wet curtain of flesh across a passage; cut it open (any hit tears it).
//! - **Bile pools**: acid on the floor that burns whoever stands in it (bodies this machine owns).
//! - **Sinews**: cords hanging from the ceiling, swaying. **Ova**: clusters of glowing eggs (the
//!   only light down here). **Flesh pillars**: ribbed columns. **The Heart**: a giant heart hanging
//!   in the boss's chamber, beating.

use std::f32::consts::TAU;

use avian3d::prelude::*;
use bevy::prelude::*;

use super::layout::{Layout, PropDef, PropKind};
use crate::character::Remote;
use crate::character::stats::{DamageMessage, DamageTaken, Health};
use crate::combat::Hurtbox;
use crate::textures::{self, Tex};
use crate::tuning::*;
use crate::ui::icons::NoticeIcon;
use crate::ui::notifications::Notify;

pub struct WombPlugin;

impl Plugin for WombPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (pulse_flesh, burn_in_bile, tear_membranes, sway_sinews, beat_hearts));
    }
}

/// The flesh materials that throb with the heartbeat (the theme's floor, walls and ceiling,
/// registered by `spawn_level`), with their resting glow.
#[derive(Resource, Default)]
pub struct FleshMaterials(pub Vec<(Handle<StandardMaterial>, LinearRgba)>);

/// Lub-dub: 0..1, two beats a cycle (`WOMB_HEARTBEAT` seconds).
pub fn heartbeat(t: f32) -> f32 {
    let p = (t / WOMB_HEARTBEAT).fract();
    let beat = |at: f32, w: f32| (-((p - at) / w).powi(2)).exp();
    (beat(0.05, 0.035) + 0.6 * beat(0.2, 0.04)).min(1.0)
}

fn pulse_flesh(time: Res<Time>, flesh: Option<Res<FleshMaterials>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let Some(flesh) = flesh else { return };
    let k = 1.0 + 2.5 * heartbeat(time.elapsed_secs());
    for (h, base) in &flesh.0 {
        if let Some(mut m) = materials.get_mut(h) {
            m.emissive = *base * k;
        }
    }
}

/// A wet curtain of flesh: any hit tears it.
#[derive(Component)]
pub struct Membrane;

/// A pool of bile (its rectangle on the floor, world x/z, and the floor height).
#[derive(Component)]
pub struct BilePool {
    min: Vec2,
    max: Vec2,
    floor: f32,
}

#[derive(Component)]
struct Sinew {
    phase: f32,
}

#[derive(Component)]
struct BeatingHeart;

pub struct WombMats {
    flesh: Handle<StandardMaterial>,
    cord: Handle<StandardMaterial>,
    bone: Handle<StandardMaterial>,
    membrane: Handle<StandardMaterial>,
    bile: Handle<StandardMaterial>,
    ova: Handle<StandardMaterial>,
    heart: Handle<StandardMaterial>,
}

impl WombMats {
    pub fn new(materials: &mut Assets<StandardMaterial>) -> Self {
        let mut flesh = |c: Color, glow: LinearRgba, tex: Tex, alpha: Option<f32>| {
            materials.add(StandardMaterial {
                base_color: alpha.map_or(c, |a| c.with_alpha(a)),
                base_color_texture: textures::get(tex),
                emissive: glow,
                alpha_mode: if alpha.is_some() { AlphaMode::Blend } else { AlphaMode::Opaque },
                double_sided: alpha.is_some(),
                cull_mode: if alpha.is_some() { None } else { Some(bevy::render::render_resource::Face::Back) },
                perceptual_roughness: 0.35,
                ..default()
            })
        };
        let flesh_ = flesh(Color::srgb(0.5, 0.18, 0.18), LinearRgba::rgb(0.05, 0.0, 0.0), Tex::Flesh, None);
        let cord = flesh(Color::srgb(0.42, 0.1, 0.12), LinearRgba::rgb(0.04, 0.0, 0.0), Tex::Flesh, None);
        let bone = flesh(Color::srgb(0.85, 0.8, 0.72), LinearRgba::BLACK, Tex::Bone, None);
        let membrane = flesh(Color::srgb(0.7, 0.2, 0.22), LinearRgba::rgb(0.5, 0.05, 0.05), Tex::Flesh, Some(0.75));
        let heart = flesh(Color::srgb(0.55, 0.08, 0.1), LinearRgba::rgb(0.6, 0.02, 0.02), Tex::Flesh, None);
        Self {
            flesh: flesh_,
            cord,
            bone,
            membrane,
            heart,
            bile: materials.add(StandardMaterial {
                base_color: Color::srgba(0.18, 0.24, 0.03, 0.9),
                emissive: LinearRgba::rgb(0.08, 0.13, 0.01),
                alpha_mode: AlphaMode::Blend,
                perceptual_roughness: 0.05,
                ..default()
            }),
            ova: materials.add(StandardMaterial {
                base_color: Color::srgba(1.0, 0.75, 0.55, 0.7),
                emissive: LinearRgba::rgb(3.0, 1.2, 0.4),
                alpha_mode: AlphaMode::Blend,
                perceptual_roughness: 0.15,
                ..default()
            }),
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn spawn_womb_prop(commands: &mut Commands, meshes: &mut Assets<Mesh>, m: &WombMats, layout: &Layout, prop: &PropDef, n: usize, root: Entity, frame: Transform) {
    let floor = frame.translation;
    let height = layout.ceiling_at(floor.xz()) - floor.y;
    fn piece(commands: &mut Commands, meshes: &mut Assets<Mesh>, parent: Entity, mesh: Mesh, mat: &Handle<StandardMaterial>, t: Transform) -> Entity {
        commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(mat.clone()), t, ChildOf(parent))).id()
    }
    let t = Transform::from_xyz;
    match prop.kind {
        PropKind::Membrane => {
            let (w, h) = (prop.size.0.max(1.0), prop.size.1.max(2.5));
            let at = floor + Vec3::Y * h * 0.5;
            commands.spawn((
                Name::new("Membrane"),
                Membrane,
                Health::new(1.0),
                Hurtbox,
                Mesh3d(meshes.add(Cuboid::new(w, h, 0.12))),
                MeshMaterial3d(m.membrane.clone()),
                RigidBody::Static,
                Collider::cuboid(w, h, 0.3),
                Transform::from_translation(at).with_rotation(Quat::from_rotation_y(prop.facing.yaw())),
            ));
        }
        PropKind::BilePool => {
            let (w, d) = (prop.size.0.max(1.0), prop.size.1.max(1.0));
            piece(commands, meshes, root, Plane3d::default().mesh().size(w, d).into(), &m.bile, t(0.0, 0.05, 0.0));
            // Its rectangle in the world (props are axis-aligned by `facing`).
            let half = frame.rotation * Vec3::new(w * 0.5, 0.0, d * 0.5);
            let (a, b) = (floor.xz() - half.xz().abs(), floor.xz() + half.xz().abs());
            commands.entity(root).insert(BilePool { min: a, max: b, floor: floor.y });
            commands.spawn((PointLight { color: Color::srgb(0.7, 1.0, 0.2), intensity: 4_000.0, range: (w.max(d)).min(6.0), ..default() }, t(0.0, 0.6, 0.0), ChildOf(root)));
        }
        PropKind::Sinews => {
            // A bunch of cords from the ceiling, some reaching the floor.
            for k in 0..5u32 {
                let a = k as f32 * 2.3 + n as f32;
                let off = Vec3::new(a.cos() * 0.5, 0.0, a.sin() * 0.5);
                let len = (height * (0.45 + 0.13 * (k % 4) as f32)).min(height - 0.2);
                let cord = commands
                    .spawn((Sinew { phase: a }, Transform::from_translation(off + Vec3::Y * height), Visibility::default(), ChildOf(root)))
                    .id();
                piece(commands, meshes, cord, Capsule3d::new(0.045 + 0.015 * (k % 2) as f32, len).into(), &m.cord, t(0.0, -len * 0.5, 0.0));
            }
        }
        PropKind::Ova => {
            for k in 0..7u32 {
                let a = k as f32 * 2.1;
                let r = 0.2 + 0.25 * (k % 3) as f32;
                let s = 0.18 + 0.06 * (k % 4) as f32;
                piece(commands, meshes, root, Sphere::new(s).mesh().ico(2).unwrap(), &m.ova, t(a.cos() * r, s * 0.8, a.sin() * r).with_scale(Vec3::new(1.0, 1.2, 1.0)));
            }
            commands.spawn((PointLight { color: Color::srgb(1.0, 0.55, 0.3), intensity: 16_000.0, range: 7.0, ..default() }, t(0.0, 0.8, 0.0), ChildOf(root)));
        }
        PropKind::FleshPillar => {
            let r = if prop.size.0 > 0.0 { prop.size.0 } else { 0.7 };
            piece(commands, meshes, root, Cylinder::new(r, height).into(), &m.flesh, t(0.0, height * 0.5, 0.0));
            // Ribs arching round it.
            for k in 0..((height / 0.6) as u32) {
                piece(commands, meshes, root, Torus::new(r * 0.95, r * 1.12).into(), &m.bone, t(0.0, 0.4 + 0.6 * k as f32, 0.0).with_rotation(Quat::from_rotation_x(0.15 * (k % 2) as f32 - 0.07)));
            }
            commands.spawn((RigidBody::Static, Collider::cylinder(r, height), t(0.0, height * 0.5, 0.0), ChildOf(root)));
        }
        PropKind::Heart => {
            // Hanging from vessels, beating (scale), glowing on every beat.
            let y = (height * 0.62).max(5.0);
            let heart = commands.spawn((BeatingHeart, t(0.0, y, 0.0), Visibility::default(), ChildOf(root))).id();
            piece(commands, meshes, heart, Sphere::new(2.0).mesh().ico(3).unwrap(), &m.heart, Transform::default().with_scale(Vec3::new(1.0, 1.25, 0.9)));
            piece(commands, meshes, heart, Sphere::new(1.4).mesh().ico(2).unwrap(), &m.heart, t(1.1, 0.9, 0.2));
            for k in 0..4 {
                let a = k as f32 / 4.0 * TAU;
                let top = Vec3::new(a.cos() * 3.0, height - y, a.sin() * 3.0);
                let mid = top * 0.5;
                piece(commands, meshes, heart, Cylinder::new(0.25, top.length()).into(), &m.cord, Transform::from_translation(mid).with_rotation(Quat::from_rotation_arc(Vec3::Y, top.normalize())));
            }
            commands.spawn((PointLight { color: Color::srgb(1.0, 0.25, 0.2), intensity: 400_000.0, range: 30.0, ..default() }, Transform::default(), ChildOf(heart)));
        }
        _ => {}
    }
}

/// Bile burns whoever stands in it (bodies this machine owns), a little every moment.
#[allow(clippy::type_complexity)]
fn burn_in_bile(
    time: Res<Time>,
    mut tick: Local<f32>,
    pools: Query<&BilePool>,
    bodies: Query<(Entity, &Transform, &crate::character::BodySize), (With<Hurtbox>, With<Health>, Without<Remote>)>,
    mut damage: MessageWriter<DamageMessage>,
) {
    *tick += time.delta_secs();
    if *tick < BILE_TICK {
        return;
    }
    *tick -= BILE_TICK;
    for (e, tf, size) in &bodies {
        let feet = tf.translation - Vec3::Y * size.half_height();
        let inside = pools.iter().any(|p| {
            let q = feet.xz();
            q.x > p.min.x && q.x < p.max.x && q.y > p.min.y && q.y < p.max.y && (feet.y - p.floor).abs() < 0.5
        });
        if inside {
            damage.write(DamageMessage { target: e, amount: BILE_DAMAGE, from: None, attacker: None, stance: 0.0 });
        }
    }
}

fn tear_membranes(mut commands: Commands, mut taken: MessageReader<DamageTaken>, membranes: Query<&Transform, With<Membrane>>, mut notify: MessageWriter<Notify>, mut sounds: MessageWriter<crate::sound::PlaySound>) {
    for t in taken.read() {
        if let Ok(tf) = membranes.get(t.target) {
            sounds.write(crate::sound::PlaySound { sound: crate::sound::Sound::Critical, pos: tf.translation, volume: 0.6 });
            commands.entity(t.target).despawn();
            notify.write(Notify::new(NoticeIcon::Door, crate::ui::strings::MEMBRANE_TORN.to_string()));
            info!("a membrane tears");
        }
    }
}

fn sway_sinews(time: Res<Time>, mut q: Query<(&Sinew, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (s, mut tf) in &mut q {
        tf.rotation = Quat::from_rotation_x(0.06 * (t * 0.7 + s.phase).sin()) * Quat::from_rotation_z(0.05 * (t * 0.5 + s.phase * 1.3).cos());
    }
}

fn beat_hearts(time: Res<Time>, mut q: Query<&mut Transform, With<BeatingHeart>>) {
    let k = 1.0 + 0.08 * heartbeat(time.elapsed_secs());
    for mut tf in &mut q {
        tf.scale = Vec3::splat(k);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_heart_beats_twice_a_cycle() {
        let samples: Vec<f32> = (0..400).map(|i| heartbeat(i as f32 / 400.0 * WOMB_HEARTBEAT)).collect();
        let peaks = samples.windows(3).filter(|w| w[1] > w[0] && w[1] >= w[2] && w[1] > 0.3).count();
        assert_eq!(peaks, 2, "lub-dub");
        assert!(samples.iter().all(|s| (0.0..=1.0).contains(s)));
    }
}
