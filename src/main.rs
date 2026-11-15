//! FlowerBox — a Soulslike prototype. See ROADMAP.md.

mod arena;
mod camera;
mod character;
mod combat;
mod debug;
mod healing;
mod hearth;
mod humanoid;
mod input;
mod interact;
mod items;
mod player;
mod tuning;
mod ui;
mod weapon;

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
            interact::InteractPlugin,
            items::ItemsPlugin,
            character::CharacterPlugin,
            humanoid::HumanoidPlugin,
            weapon::WeaponPlugin,
            player::PlayerPlugin,
            healing::HealingPlugin,
            hearth::HearthPlugin,
            ui::UiPlugin,
            debug::DebugPlugin,
        ))
        .run();
}
