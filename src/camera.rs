//! Camera. Phase 1: a temporary orbit camera (RMB drag to rotate, wheel to zoom).
//! Phase 2 replaces this with the third-person player camera.

use bevy::camera::Hdr;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;

use crate::tuning::*;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_camera)
            .add_systems(Update, orbit_camera);
    }
}

#[derive(Component)]
pub struct OrbitCamera {
    pub focus: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
}

impl OrbitCamera {
    fn transform(&self) -> Transform {
        let rot = Quat::from_euler(EulerRot::YXZ, self.yaw, -self.pitch, 0.0);
        let pos = self.focus + rot * Vec3::new(0.0, 0.0, self.distance);
        Transform::from_translation(pos).looking_at(self.focus, Vec3::Y)
    }
}

fn spawn_camera(mut commands: Commands) {
    // FLOWERBOX_CAM="yaw,pitch,distance" (radians, meters) overrides the start pose — handy for screenshots.
    let mut orbit = OrbitCamera { focus: Vec3::new(0.0, 1.0, 0.0), yaw: 0.6, pitch: 0.55, distance: 15.0 };
    if let Ok(s) = std::env::var("FLOWERBOX_CAM") {
        let v: Vec<f32> = s.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        if let [y, p, d] = v[..] {
            (orbit.yaw, orbit.pitch, orbit.distance) = (y, p, d);
        }
    }
    commands.spawn((
        Camera3d::default(),
        Hdr,
        Tonemapping::TonyMcMapface,
        Bloom::NATURAL,
        DistanceFog {
            color: Color::srgb(0.02, 0.022, 0.035),
            falloff: FogFalloff::Linear { start: FOG_START, end: FOG_END },
            ..default()
        },
        orbit.transform(),
        orbit,
    ));
}

fn orbit_camera(
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    mut cams: Query<(&mut OrbitCamera, &mut Transform)>,
) {
    for (mut orbit, mut tf) in &mut cams {
        if mouse_buttons.pressed(MouseButton::Right) {
            orbit.yaw -= motion.delta.x * 0.005;
            orbit.pitch = (orbit.pitch + motion.delta.y * 0.005).clamp(-0.2, 1.5);
        }
        orbit.distance = (orbit.distance - scroll.delta.y * 1.0).clamp(2.0, 45.0);
        *tf = orbit.transform();
    }
}
