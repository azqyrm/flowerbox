//! Third-person orbit camera that follows the player and never clips through walls.
//! Mouse = look (only while the cursor is grabbed), or the gamepad's right stick. Locked on
//! (`crate::lockon`), the camera turns to keep the target in front.
//! `FLOWERBOX_CAM="yaw,pitch,distance"` overrides the starting pose (for screenshots, see ROADMAP N5).

use avian3d::prelude::*;
use bevy::camera::Hdr;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use bevy::window::{CursorOptions, PrimaryWindow};
use leafwing_input_manager::prelude::*;

use crate::character::controller::CharacterController;
use crate::combat::{CameraShake, Hurtbox};
use crate::input::{Action, GameplayBlocked, cursor_grabbed};
use crate::lockon::{LockOn, Targetable};
use crate::settings::Settings;
use crate::player::Player;
use crate::tuning::*;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraFocus>()
            .add_systems(Startup, spawn_camera)
            .add_systems(PostUpdate, follow_camera.before(TransformSystems::Propagate));
    }
}

/// Someone else to follow instead of the player (spectating a boss fight, `crate::spectate`).
#[derive(Resource, Debug, Default)]
pub struct CameraFocus(pub Option<Entity>);

#[derive(Component)]
pub struct ThirdPersonCamera {
    pub yaw: f32,
    pub pitch: f32,
    /// Desired distance from the focus point.
    pub distance: f32,
    /// Actual distance this frame (shorter when a wall is in the way).
    current_distance: f32,
    /// Swing to this yaw (lock-on pressed with nothing to lock onto: look where the player faces).
    pub recenter: Option<f32>,
}

fn spawn_camera(mut commands: Commands) {
    let mut cam = ThirdPersonCamera { yaw: 0.0, pitch: 0.35, distance: CAM_DISTANCE, current_distance: CAM_DISTANCE, recenter: None };
    if let Ok(s) = std::env::var("FLOWERBOX_CAM") {
        let v: Vec<f32> = s.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        if let [y, p, d] = v[..] {
            (cam.yaw, cam.pitch, cam.distance) = (y, p, d);
        }
    }
    commands.spawn((
        Name::new("Camera"),
        Camera3d::default(),
        Hdr,
        Tonemapping::TonyMcMapface,
        Bloom::NATURAL,
        DistanceFog {
            color: Color::srgb(0.02, 0.022, 0.035),
            falloff: FogFalloff::Linear { start: FOG_START, end: FOG_END },
            ..default()
        },
        Transform::from_xyz(0.0, 3.0, 12.0),
        cam,
    ));
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn follow_camera(
    time: Res<Time>,
    real: Res<Time<Real>>,
    mut shake: ResMut<CameraShake>,
    spatial: SpatialQuery,
    cursor: Single<&CursorOptions, With<PrimaryWindow>>,
    blocked: Res<GameplayBlocked>,
    settings: Res<Settings>,
    player: Single<(Entity, &Transform, &ActionState<Action>, Option<&LockOn>), (With<Player>, Without<ThirdPersonCamera>)>,
    targets: Query<(&Transform, &Targetable), (Without<Player>, Without<ThirdPersonCamera>)>,
    mut cams: Query<(&mut ThirdPersonCamera, &mut Transform)>,
    characters: Query<(), Or<(With<CharacterController>, With<Hurtbox>)>>,
    (focus_on, others): (Res<CameraFocus>, Query<&Transform, (Without<Player>, Without<ThirdPersonCamera>)>),
) {
    let (player_entity, player_tf, actions, lock) = player.into_inner();
    let player_tf = focus_on.0.and_then(|e| others.get(e).ok()).unwrap_or(player_tf);
    let dt = time.delta_secs();
    let target = lock.and_then(|l| l.target).and_then(|t| targets.get(t).ok());
    for (mut cam, mut tf) in &mut cams {
        if let Some((ttf, _)) = target {
            // Locked on: look from behind the player toward the target.
            let d = (ttf.translation - player_tf.translation).with_y(0.0);
            if d.length_squared() > 1e-4 {
                let want = f32::atan2(-d.x, -d.z);
                cam.yaw += angle_diff(cam.yaw, want) * (1.0 - (-CAM_LOCK_TURN * dt).exp());
            }
            cam.pitch += (CAM_LOCK_PITCH - cam.pitch) * (1.0 - (-4.0 * dt).exp());
            cam.recenter = None;
        } else if let Some(want) = cam.recenter {
            let d = angle_diff(cam.yaw, want);
            cam.yaw += d * (1.0 - (-CAM_LOCK_TURN * dt).exp());
            if d.abs() < 0.02 {
                cam.recenter = None;
            }
        } else if !blocked.0 {
            if cursor_grabbed(&cursor) {
                let look = actions.axis_pair(&Action::Look);
                let sens = CAM_SENSITIVITY * settings.mouse_sensitivity;
                let invert = if settings.invert_y { -1.0 } else { 1.0 };
                cam.yaw -= look.x * sens;
                cam.pitch = (cam.pitch + look.y * sens * invert).clamp(CAM_PITCH_MIN, CAM_PITCH_MAX);
            }
            // Gamepad right stick (rate-based, so scaled by time).
            let stick = actions.clamped_axis_pair(&Action::LookStick);
            let speed = CAM_STICK_SPEED * settings.stick_speed;
            let invert = if settings.invert_y { -1.0 } else { 1.0 };
            cam.yaw -= stick.x * speed * dt;
            cam.pitch = (cam.pitch - stick.y * speed * 0.6 * dt * invert).clamp(CAM_PITCH_MIN, CAM_PITCH_MAX);
        }

        // Focus slightly above the capsule center (≈ shoulder height).
        let focus = player_tf.translation + Vec3::Y * (CAM_FOCUS_HEIGHT - PLAYER_HALF_HEIGHT);
        let rot = Quat::from_euler(EulerRot::YXZ, cam.yaw, -cam.pitch, 0.0);
        let dir = Dir3::new(rot * Vec3::Z).unwrap_or(Dir3::Z);

        // Pull in when geometry is between player and camera; ease back out smoothly.
        // Only the level pulls the camera in, not characters (other players, enemies, the dummy).
        let blocked = spatial
            .cast_shape_predicate(
                &Collider::sphere(CAM_COLLISION_RADIUS),
                focus,
                Quat::IDENTITY,
                dir,
                &ShapeCastConfig::from_max_distance(cam.distance),
                &SpatialQueryFilter::from_excluded_entities([player_entity]),
                &|e| !characters.contains(e),
            )
            .map(|hit| hit.distance.max(0.3));
        let target = blocked.unwrap_or(cam.distance);
        cam.current_distance = if target < cam.current_distance {
            target // snap in immediately so we never see through a wall
        } else {
            cam.current_distance + (target - cam.current_distance) * (1.0 - (-6.0 * time.delta_secs()).exp())
        };

        // Camera shake (trauma², smooth pseudo-noise); decays in real time so hitstop doesn't freeze it.
        let t = real.elapsed_secs();
        let k = shake.trauma * shake.trauma * 0.12;
        let jitter = Vec3::new((t * 47.0).sin() + (t * 31.0).cos() * 0.5, (t * 53.0).cos() + (t * 23.0).sin() * 0.5, 0.0) * k;
        shake.trauma = (shake.trauma - real.delta_secs() * 2.5).max(0.0);

        *tf = Transform::from_translation(focus + dir * cam.current_distance).looking_at(focus, Vec3::Y);
        let offset = tf.rotation * jitter;
        tf.translation += offset;
    }
}

/// Shortest signed angle `to - from`, in (-π, π].
fn angle_diff(from: f32, to: f32) -> f32 {
    let d = (to - from).rem_euclid(std::f32::consts::TAU);
    if d > std::f32::consts::PI { d - std::f32::consts::TAU } else { d }
}
