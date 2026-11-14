//! FlowerBox — a Soulslike prototype. See ROADMAP.md.

mod arena;
mod camera;
mod character;
mod combat;
mod debug;
mod humanoid;
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
            character::CharacterPlugin,
            humanoid::HumanoidPlugin,
            player::PlayerPlugin,
            debug::DebugPlugin,
        ))
        .run();
}
