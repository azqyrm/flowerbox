//! Third-person orbit camera that follows the player and never clips through walls.
//! Mouse = look (only while the cursor is grabbed).
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

use crate::input::{Action, cursor_grabbed};
use crate::player::Player;
use crate::tuning::*;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_camera)
            .add_systems(PostUpdate, follow_camera.before(TransformSystems::Propagate));
    }
}

#[derive(Component)]
pub struct ThirdPersonCamera {
    pub yaw: f32,
    pub pitch: f32,
    /// Desired distance from the focus point.
    pub distance: f32,
    /// Actual distance this frame (shorter when a wall is in the way).
    current_distance: f32,
}

fn spawn_camera(mut commands: Commands) {
    let mut cam = ThirdPersonCamera { yaw: 0.0, pitch: 0.35, distance: CAM_DISTANCE, current_distance: CAM_DISTANCE };
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

fn follow_camera(
    time: Res<Time>,
    spatial: SpatialQuery,
    cursor: Single<&CursorOptions, With<PrimaryWindow>>,
    player: Single<(Entity, &Transform, &ActionState<Action>), (With<Player>, Without<ThirdPersonCamera>)>,
    mut cams: Query<(&mut ThirdPersonCamera, &mut Transform)>,
) {
    let (player_entity, player_tf, actions) = player.into_inner();
    for (mut cam, mut tf) in &mut cams {
        if cursor_grabbed(&cursor) {
            let look = actions.axis_pair(&Action::Look);
            cam.yaw -= look.x * CAM_SENSITIVITY;
            cam.pitch = (cam.pitch + look.y * CAM_SENSITIVITY).clamp(CAM_PITCH_MIN, CAM_PITCH_MAX);
        }

        // Focus slightly above the capsule center (≈ shoulder height).
        let focus = player_tf.translation + Vec3::Y * (CAM_FOCUS_HEIGHT - PLAYER_HALF_HEIGHT);
        let rot = Quat::from_euler(EulerRot::YXZ, cam.yaw, -cam.pitch, 0.0);
        let dir = Dir3::new(rot * Vec3::Z).unwrap_or(Dir3::Z);

        // Pull in when geometry is between player and camera; ease back out smoothly.
        let blocked = spatial
            .cast_shape(
                &Collider::sphere(CAM_COLLISION_RADIUS),
                focus,
                Quat::IDENTITY,
                dir,
                &ShapeCastConfig::from_max_distance(cam.distance),
                &SpatialQueryFilter::from_excluded_entities([player_entity]),
            )
            .map(|hit| hit.distance.max(0.3));
        let target = blocked.unwrap_or(cam.distance);
        cam.current_distance = if target < cam.current_distance {
            target // snap in immediately so we never see through a wall
        } else {
            cam.current_distance + (target - cam.current_distance) * (1.0 - (-6.0 * time.delta_secs()).exp())
        };

        *tf = Transform::from_translation(focus + dir * cam.current_distance).looking_at(focus, Vec3::Y);
    }
}
