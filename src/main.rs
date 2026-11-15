//! FlowerBox — a Soulslike prototype. See ROADMAP.md.

mod arena;
mod camera;
mod character;
mod combat;
mod debug;
mod enemy;
mod healing;
mod hearth;
mod humanoid;
mod input;
mod interact;
mod items;
mod lockon;
mod net;
mod player;
mod pvp;
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
            lockon::LockOnPlugin,
            character::CharacterPlugin,
            humanoid::HumanoidPlugin,
            weapon::WeaponPlugin,
            player::PlayerPlugin,
        ))
        .add_plugins((
            enemy::EnemyPlugin,
            pvp::PvpPlugin,
            healing::HealingPlugin,
            hearth::HearthPlugin,
            ui::UiPlugin,
            debug::DebugPlugin,
            net::NetPlugin,
        ))
        .run();
}
