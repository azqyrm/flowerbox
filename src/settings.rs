//! Player settings: camera feel, volume, notification corner and key bindings. Saved as RON in
//! the OS config folder (`~/.config/flowerbox/settings.ron` on Linux) whenever they change, and
//! loaded at start. Edited in the pause menu (`ui/pause_menu.rs`).
//!
//! Autopilot runs (tests) use the defaults and never touch the user's file, unless
//! `FLOWERBOX_CONFIG_DIR` points somewhere.

use std::path::PathBuf;

use bevy::audio::{GlobalVolume, Volume};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::ui::notifications::{Corner, NotificationSettings};

pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(load())
            .add_systems(Update, (apply_settings, save_settings).chain());
    }
}

/// A key or mouse button the player can bind.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    Key(KeyCode),
    Mouse(MouseButton),
}

impl Input {
    pub fn label(self) -> String {
        match self {
            Input::Key(k) => {
                let s = format!("{k:?}");
                for (side, rest) in [("Left", "Left"), ("Right", "Right")] {
                    for (code, name) in [("Shift", "Shift"), ("Control", "Ctrl"), ("Alt", "Alt"), ("Super", "Super")] {
                        if s == format!("{code}{side}") {
                            return format!("{rest} {name}");
                        }
                    }
                }
                if let Some(dir) = s.strip_prefix("Arrow") {
                    return format!("{dir} arrow");
                }
                s.strip_prefix("Key").or_else(|| s.strip_prefix("Digit")).unwrap_or(&s).to_string()
            }
            Input::Mouse(MouseButton::Left) => "Left mouse".into(),
            Input::Mouse(MouseButton::Right) => "Right mouse".into(),
            Input::Mouse(MouseButton::Middle) => "Middle mouse".into(),
            Input::Mouse(m) => format!("Mouse {m:?}"),
        }
    }
}

/// Every rebindable action.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Bind {
    MoveForward,
    MoveBack,
    MoveLeft,
    MoveRight,
    Jump,
    DodgeSprint,
    Attack,
    Block,
    DrinkAle,
    Interact,
    LockOn,
    Inventory,
}

impl Bind {
    pub const ALL: [Bind; 12] = [
        Bind::MoveForward,
        Bind::MoveBack,
        Bind::MoveLeft,
        Bind::MoveRight,
        Bind::Jump,
        Bind::DodgeSprint,
        Bind::Attack,
        Bind::Block,
        Bind::DrinkAle,
        Bind::Interact,
        Bind::LockOn,
        Bind::Inventory,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Bind::MoveForward => "Move forward",
            Bind::MoveBack => "Move back",
            Bind::MoveLeft => "Move left",
            Bind::MoveRight => "Move right",
            Bind::Jump => "Jump",
            Bind::DodgeSprint => "Roll (tap) / sprint (hold)",
            Bind::Attack => "Attack (tap light, hold heavy)",
            Bind::Block => "Block / parry",
            Bind::DrinkAle => "Drink Healing Ale",
            Bind::Interact => "Interact / rest",
            Bind::LockOn => "Lock on",
            Bind::Inventory => "Inventory",
        }
    }

    fn default_input(self) -> Input {
        use KeyCode::*;
        match self {
            Bind::MoveForward => Input::Key(KeyW),
            Bind::MoveBack => Input::Key(KeyS),
            Bind::MoveLeft => Input::Key(KeyA),
            Bind::MoveRight => Input::Key(KeyD),
            Bind::Jump => Input::Key(Space),
            Bind::DodgeSprint => Input::Key(ShiftLeft),
            Bind::Attack => Input::Mouse(MouseButton::Left),
            Bind::Block => Input::Mouse(MouseButton::Right),
            Bind::DrinkAle => Input::Key(KeyR),
            Bind::Interact => Input::Key(KeyE),
            Bind::LockOn => Input::Key(KeyQ),
            Bind::Inventory => Input::Key(Tab),
        }
    }
}

/// Key bindings (one input per action; gamepad buttons are fixed, see `input.rs`).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Bindings(pub Vec<(Bind, Input)>);

impl Default for Bindings {
    fn default() -> Self {
        Self(Bind::ALL.iter().map(|b| (*b, b.default_input())).collect())
    }
}

impl Bindings {
    pub fn get(&self, bind: Bind) -> Input {
        self.0.iter().find(|(b, _)| *b == bind).map_or(bind.default_input(), |(_, i)| *i)
    }

    /// Binds `input` to `bind`; an action that had `input` before gets `bind`'s old input (swap),
    /// so nothing is ever left unbound.
    pub fn set(&mut self, bind: Bind, input: Input) {
        let old = self.get(bind);
        for (b, i) in &mut self.0 {
            if *i == input && *b != bind {
                *i = old;
            }
        }
        match self.0.iter_mut().find(|(b, _)| *b == bind) {
            Some((_, i)) => *i = input,
            None => self.0.push((bind, input)),
        }
    }
}

#[derive(Resource, Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// Multiplier on the mouse look speed.
    pub mouse_sensitivity: f32,
    pub invert_y: bool,
    /// Multiplier on the gamepad right-stick camera speed.
    pub stick_speed: f32,
    /// 0..1.
    pub volume: f32,
    pub notification_corner: Corner,
    pub bindings: Bindings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            mouse_sensitivity: 1.0,
            invert_y: false,
            stick_speed: 1.0,
            volume: 0.8,
            notification_corner: Corner::BottomRight,
            bindings: Bindings::default(),
        }
    }
}

/// Where settings live, or `None` when they shouldn't be persisted (autopilot test runs).
fn settings_path() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("FLOWERBOX_CONFIG_DIR") {
        return Some(PathBuf::from(dir).join("settings.ron"));
    }
    if std::env::var("FLOWERBOX_AUTOPILOT").is_ok() {
        return None;
    }
    dirs::config_dir().map(|d| d.join("flowerbox").join("settings.ron"))
}

fn load() -> Settings {
    let Some(path) = settings_path() else { return Settings::default() };
    match std::fs::read_to_string(&path) {
        Ok(text) => ron::from_str(&text).unwrap_or_else(|e| {
            warn!("settings file {} unreadable ({e}); using defaults", path.display());
            Settings::default()
        }),
        Err(_) => Settings::default(),
    }
}

fn apply_settings(settings: Res<Settings>, mut volume: ResMut<GlobalVolume>, mut notes: ResMut<NotificationSettings>) {
    if !settings.is_changed() {
        return;
    }
    volume.volume = Volume::Linear(settings.volume.clamp(0.0, 1.0));
    notes.corner = settings.notification_corner;
}

/// Writes the file whenever the settings change (not for the initial load).
fn save_settings(settings: Res<Settings>) {
    if !settings.is_changed() || settings.is_added() {
        return;
    }
    let Some(path) = settings_path() else { return };
    let text = match ron::ser::to_string_pretty(&*settings, ron::ser::PrettyConfig::default()) {
        Ok(t) => t,
        Err(e) => return warn!("couldn't encode settings: {e}"),
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(e) = std::fs::write(&path, text) {
        warn!("couldn't save settings to {}: {e}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rebinding_swaps_instead_of_leaving_an_action_unbound() {
        let mut b = Bindings::default();
        b.set(Bind::Jump, Input::Key(KeyCode::KeyE)); // E was Interact
        assert_eq!(b.get(Bind::Jump), Input::Key(KeyCode::KeyE));
        assert_eq!(b.get(Bind::Interact), Input::Key(KeyCode::Space), "Interact takes Jump's old key");
    }

    #[test]
    fn key_labels_are_readable() {
        assert_eq!(Input::Key(KeyCode::ShiftLeft).label(), "Left Shift");
        assert_eq!(Input::Key(KeyCode::KeyW).label(), "W");
        assert_eq!(Input::Key(KeyCode::ArrowUp).label(), "Up arrow");
        assert_eq!(Input::Mouse(MouseButton::Right).label(), "Right mouse");
    }

    #[test]
    fn settings_round_trip_through_ron() {
        let mut s = Settings { invert_y: true, volume: 0.3, ..default() };
        s.bindings.set(Bind::Attack, Input::Key(KeyCode::KeyJ));
        let text = ron::ser::to_string_pretty(&s, ron::ser::PrettyConfig::default()).unwrap();
        let back: Settings = ron::from_str(&text).unwrap();
        assert_eq!(back, s);
        // Missing fields fall back to defaults (older files keep working).
        let partial: Settings = ron::from_str("(volume: 0.5)").unwrap();
        assert_eq!(partial.volume, 0.5);
        assert_eq!(partial.bindings, Bindings::default());
    }
}
