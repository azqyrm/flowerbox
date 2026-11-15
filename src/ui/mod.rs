//! Screen UI. `hud` = bars, Healing Ale, equipment quadrant, interaction prompt, banners and
//! the death screen. The inventory UI comes in Phase 7.

use bevy::prelude::*;

pub mod hud;
pub mod strings;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Banner>().add_plugins(hud::HudPlugin);
    }
}

/// A big centred announcement that fades in and out (e.g. `strings::HEARTH_REKINDLED`).
#[derive(Message, Debug, Clone, Copy)]
pub struct Banner(pub &'static str);

/// Serif display font for banners and the death screen (PT Serif, see its license file).
pub const DISPLAY_FONT: &str = "fonts/PTSerif-Regular.ttf";
