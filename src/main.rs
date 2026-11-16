//! FlowerBox — a Soulslike prototype. See ROADMAP.md.

mod ability;
mod arena;
mod camera;
mod character;
mod combat;
mod debug;
mod enemy;
mod grab;
mod healing;
mod hearth;
mod humanoid;
mod input;
mod interact;
mod items;
mod level;
mod lockon;
mod net;
mod player;
mod pvp;
mod quadruped;
mod save;
mod settings;
mod sound;
mod textures;
mod tuning;
mod ui;
mod weapon;

use avian3d::prelude::*;
use bevy::prelude::*;

/// Running an automated test (autopilot or screenshot), not a game.
fn test_run() -> bool {
    std::env::var("FLOWERBOX_AUTOPILOT").is_ok() || std::env::var("FLOWERBOX_SCREENSHOT").is_ok()
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "FlowerBox".into(),
                // Test runs (autopilots, screenshots) don't wait for vsync: a desktop that throttles
                // hidden windows (GNOME Wayland) otherwise slows them to a few frames per second.
                present_mode: if test_run() { bevy::window::PresentMode::AutoNoVsync } else { bevy::window::PresentMode::default() },
                ..default()
            }),
            ..default()
        }))
        .add_plugins(PhysicsPlugins::default())
        .add_plugins((
            settings::SettingsPlugin,
            arena::ArenaPlugin,
            camera::CameraPlugin,
            combat::CombatPlugin,
            input::InputPlugin,
            interact::InteractPlugin,
            items::ItemsPlugin,
            lockon::LockOnPlugin,
            character::CharacterPlugin,
            humanoid::HumanoidPlugin,
            quadruped::QuadrupedPlugin,
            weapon::WeaponPlugin,
            player::PlayerPlugin,
        ))
        .add_plugins((
            level::LevelPlugin,
            enemy::EnemyPlugin,
            grab::GrabPlugin,
            ability::AbilityPlugin,
            pvp::PvpPlugin,
            save::SavePlugin,
            sound::SoundPlugin,
            healing::HealingPlugin,
            hearth::HearthPlugin,
            ui::UiPlugin,
            debug::DebugPlugin,
            net::NetPlugin,
            textures::TexturesPlugin,
        ))
        .run();
}
