//! FlowerBox — a Soulslike prototype. See ROADMAP.md.

mod arena;
mod camera;
mod combat;
mod debug;
mod input;
mod player;
mod tuning;

use avian3d::prelude::*;
use bevy::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "FlowerBox".into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(PhysicsPlugins::default())
        .add_plugins((
            arena::ArenaPlugin,
            camera::CameraPlugin,
            combat::CombatPlugin,
            input::InputPlugin,
            player::PlayerPlugin,
            debug::DebugPlugin,
        ))
        .run();
}
