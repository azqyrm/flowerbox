//! Dev helpers.
//!
//! `FLOWERBOX_SCREENSHOT=/path/shot.png cargo run` → saves a screenshot after a short warm-up and exits.
//! Lets an AI session verify visuals without a human looking at the window.

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};

pub struct DebugPlugin;

impl Plugin for DebugPlugin {
    fn build(&self, app: &mut App) {
        if let Ok(path) = std::env::var("FLOWERBOX_SCREENSHOT") {
            app.insert_resource(AutoScreenshot { path, frame: 0 })
                .add_systems(Update, auto_screenshot);
        }
    }
}

#[derive(Resource)]
struct AutoScreenshot {
    path: String,
    frame: u32,
}

fn auto_screenshot(mut commands: Commands, mut shot: ResMut<AutoScreenshot>, mut exit: MessageWriter<AppExit>) {
    shot.frame += 1;
    match shot.frame {
        120 => {
            commands.spawn(Screenshot::primary_window()).observe(save_to_disk(shot.path.clone()));
        }
        180 => {
            exit.write(AppExit::Success);
        }
        _ => {}
    }
}
