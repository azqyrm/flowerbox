//! Screen UI. `hud` = bars, Healing Ale, equipment quadrant, interaction prompt, banners and
//! the death screen. `inventory_ui` = the Tab/I inventory and equipment screen. `markers` = lock-on dot and
//! enemy health bars. `icons` = item icons drawn from nodes. `strings` = all player-facing wording.

use bevy::prelude::*;

pub mod damage_numbers;
pub mod hud;
pub mod icons;
pub mod inventory_ui;
pub mod markers;
pub mod notifications;
pub mod pause_menu;
pub mod strings;
pub mod text_input;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Banner>().add_plugins((hud::HudPlugin, inventory_ui::InventoryUiPlugin, markers::MarkersPlugin, notifications::NotificationsPlugin, damage_numbers::DamageNumbersPlugin, pause_menu::PauseMenuPlugin));
    }
}

/// A big centred announcement that fades in and out (e.g. `strings::HEARTH_REKINDLED`).
#[derive(Message, Debug, Clone, Copy)]
pub struct Banner(pub &'static str);

/// Serif display font for banners and the death screen (PT Serif, see its license file).
pub const DISPLAY_FONT: &str = "fonts/PTSerif-Regular.ttf";
