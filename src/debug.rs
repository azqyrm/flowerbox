//! Dev helpers for automated checks (see ROADMAP notes N5 / N7).
//!
//! - `FLOWERBOX_SCREENSHOT=/path/shot.png` → saves a screenshot after a warm-up, then exits.
//!   `FLOWERBOX_SHOT_FRAME=n` changes the capture frame (default 120).
//! - `FLOWERBOX_AUTOPILOT=1` → the player walks forward and jumps every 2 s; position is logged.

use bevy::prelude::*;
use bevy::time::common_conditions::on_timer;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};

use crate::input::Autopilot;
use crate::player::Player;
use crate::player::controller::CharacterController;

pub struct DebugPlugin;

impl Plugin for DebugPlugin {
    fn build(&self, app: &mut App) {
        if let Ok(path) = std::env::var("FLOWERBOX_SCREENSHOT") {
            let at = std::env::var("FLOWERBOX_SHOT_FRAME").ok().and_then(|s| s.parse().ok()).unwrap_or(120);
            app.insert_resource(AutoScreenshot { path, frame: 0, at })
                .add_systems(Update, auto_screenshot);
        }
        if std::env::var("FLOWERBOX_AUTOPILOT").is_ok() {
            app.insert_resource(Autopilot)
                .add_systems(Update, log_player.run_if(on_timer(std::time::Duration::from_millis(500))));
        }
    }
}

#[derive(Resource)]
struct AutoScreenshot {
    path: String,
    frame: u32,
    at: u32,
}

fn auto_screenshot(mut commands: Commands, mut shot: ResMut<AutoScreenshot>, mut exit: MessageWriter<AppExit>) {
    shot.frame += 1;
    if shot.frame == shot.at {
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(shot.path.clone()));
    } else if shot.frame == shot.at + 60 {
        exit.write(AppExit::Success);
    }
}

fn log_player(q: Query<(&Transform, &CharacterController), With<Player>>) {
    for (tf, c) in &q {
        info!(
            "player pos={:.2} vel={:.2} grounded={} r={:.2}",
            tf.translation,
            c.velocity,
            c.grounded,
            Vec2::new(tf.translation.x, tf.translation.z).length()
        );
    }
}
