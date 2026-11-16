//! Chat. `T` (rebindable) opens a line at the bottom left; type, Enter sends, Esc throws it away.
//! While typing the character gets no input (like a menu).
//!
//! A message shows in the notification feed, the usual way (slides in, fades): the speaker's little
//! face in their tunic colour and "Name: what they said". Online it reaches everyone: a guest sends
//! it to the host, the host shows it and passes it to every other guest (the host's world is the
//! hub, like everything else). Offline you just see your own words.
//!
//! The name follows `Settings::name` (see `net/names.rs`); changing it online tells the host.

use bevy::input::keyboard::KeyboardInput;
use bevy::prelude::*;

use crate::input::{InputSystems, MenuInput};
use crate::net::NetSession;
use crate::net::names::{default_name, sanitize};
use crate::net::protocol::{NetId, PLAYER_ID_BASE, Reliable, encode};
use crate::humanoid::skins::Skin;
use crate::net::sync::{Puppet, tunic};
use crate::net::transport::{HOST_PEER, NetCmd, NetLink};
use crate::net::{NetInbox, Role};
use crate::settings::{Bind, Input, Settings};
use crate::tuning::*;
use crate::ui::icons::NoticeIcon;
use crate::ui::notifications::Notify;
use crate::ui::text_input::{Edit, any_char, type_into};
use crate::ui::{DISPLAY_FONT, strings};

pub struct ChatPlugin;

impl Plugin for ChatPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChatLine>()
            .add_message::<Said>()
            .add_systems(Startup, spawn_line)
            .add_systems(Update, chat_keys.in_set(InputSystems::Menus))
            .add_systems(Update, (chat_inbox, show_said, share_name, draw_line).chain().after(InputSystems::Menus));
    }
}

/// The line being typed.
#[derive(Resource, Default, Debug)]
pub struct ChatLine {
    pub open: bool,
    pub draft: String,
}

/// Somebody (maybe you) said `text`: show it.
#[derive(Message, Debug, Clone)]
pub struct Said {
    pub who: NetId,
    pub text: String,
}

/// A message as it may be sent: no control characters, trimmed, at most `CHAT_MAX_CHARS`.
pub fn clean_message(raw: &str) -> String {
    raw.chars().filter(|c| !c.is_control()).take(CHAT_MAX_CHARS).collect::<String>().trim().to_string()
}

/// How long a message of `text` stays on screen.
pub fn message_secs(text: &str) -> f32 {
    (CHAT_BASE_SECS + CHAT_SECS_PER_CHAR * text.chars().count() as f32).min(CHAT_MAX_SECS)
}

/// Your own name: the host's list online, else the settings' (or Player1).
pub fn my_name(session: &NetSession, settings: &Settings) -> String {
    if session.online() {
        return session.name_of(session.my_id);
    }
    match sanitize(&settings.name) {
        n if n.is_empty() => default_name(session.my_id, false),
        n => n,
    }
}

fn pressed(input: Input, keys: &ButtonInput<KeyCode>, mouse: &ButtonInput<MouseButton>) -> bool {
    match input {
        Input::Key(k) => keys.just_pressed(k),
        Input::Mouse(m) => mouse.just_pressed(m),
    }
}

/// Opens the line (not over a menu), types into it, sends on Enter.
#[allow(clippy::too_many_arguments)]
fn chat_keys(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut typed: MessageReader<KeyboardInput>,
    settings: Res<Settings>,
    session: Res<NetSession>,
    link: Res<NetLink>,
    mut line: ResMut<ChatLine>,
    mut menus: ResMut<MenuInput>,
    mut said: MessageWriter<Said>,
) {
    if !line.open {
        // The opening key itself (and anything typed before) isn't part of the message.
        typed.clear();
        let other_menu = menus.inventory || menus.pause || menus.online;
        if !other_menu && pressed(settings.bindings.get(Bind::Chat), &keys, &mouse) {
            line.open = true;
            line.draft.clear();
        }
        menus.chat = line.open;
        return;
    }
    let mut draft = std::mem::take(&mut line.draft);
    match type_into(&mut draft, typed.read(), CHAT_MAX_CHARS, any_char) {
        Edit::Typing => line.draft = draft,
        Edit::Cancel => line.open = false,
        Edit::Submit => {
            line.open = false;
            let text = clean_message(&draft);
            if !text.is_empty() {
                send(&session, &link, &text);
                said.write(Said { who: session.my_id, text });
            }
        }
    }
    menus.chat = line.open;
}

/// Puts my message on the wire: a guest tells the host, the host tells every guest.
fn send(session: &NetSession, link: &NetLink, text: &str) {
    match session.role {
        Role::Client => link.send(NetCmd::Reliable { to: Some(HOST_PEER), bytes: encode(&Reliable::Chat { text: text.into() }) }),
        Role::Hosting => link.send(NetCmd::Reliable { to: None, bytes: encode(&Reliable::Said { who: session.my_id, text: text.into() }) }),
        _ => {}
    }
}

/// Host: a guest's message goes to everyone else. Guest: the host passes on someone's message.
fn chat_inbox(mut inbox: MessageReader<NetInbox>, session: Res<NetSession>, link: Res<NetLink>, mut said: MessageWriter<Said>) {
    for msg in inbox.read() {
        match msg {
            NetInbox::Reliable(peer, Reliable::Chat { text }) if session.is_host() => {
                let Some(&who) = session.peers.get(peer) else { continue };
                let text = clean_message(text);
                if text.is_empty() {
                    continue;
                }
                let bytes = encode(&Reliable::Said { who, text: text.clone() });
                for (&other, _) in session.peers.iter().filter(|(p, _)| *p != peer) {
                    link.send(NetCmd::Reliable { to: Some(other), bytes: bytes.clone() });
                }
                said.write(Said { who, text });
            }
            NetInbox::Reliable(HOST_PEER, Reliable::Said { who, text }) if session.role == Role::Client && *who != session.my_id => {
                said.write(Said { who: *who, text: clean_message(text) });
            }
            _ => {}
        }
    }
}

/// A message becomes a notification: the speaker's face (in their skin) + "Name: text".
fn show_said(
    mut said: MessageReader<Said>,
    session: Res<NetSession>,
    settings: Res<Settings>,
    puppets: Query<(&Puppet, &Skin)>,
    mut notify: MessageWriter<Notify>,
) {
    for Said { who, text } in said.read() {
        let mine = *who == session.my_id;
        let name = if mine { my_name(&session, &settings) } else { session.name_of(*who) };
        let skin = if mine { settings.skin } else { puppets.iter().find(|(p, _)| p.id == *who).map_or(Skin::Monobrow, |(_, s)| *s) };
        let slot = if session.online() { *who } else { PLAYER_ID_BASE };
        info!("chat: {name} ({}): {text}", skin.name());
        notify.write(Notify::new(NoticeIcon::Face(skin, tunic(slot)), strings::said(&name, text)).lasting(message_secs(text)));
    }
}

/// The name changed in the settings: online, tell the host (or, hosting, take it and tell all).
fn share_name(settings: Res<Settings>, mut last: Local<Option<String>>, mut session: ResMut<NetSession>, link: Res<NetLink>) {
    let current = &settings.name;
    if last.as_ref() == Some(current) {
        return;
    }
    let first = last.is_none();
    *last = Some(current.clone());
    if first {
        return; // the name at start goes out with Hello / hosting
    }
    match session.role {
        Role::Client => link.send(NetCmd::Reliable { to: Some(HOST_PEER), bytes: encode(&Reliable::Rename { name: current.clone() }) }),
        Role::Hosting => {
            let id = session.my_id;
            session.assign_name(id, current);
            session.share_names(&link);
        }
        _ => {}
    }
}

// ── The typing line ──────────────────────────────────────────────────────

#[derive(Component)]
struct LineRoot;
#[derive(Component)]
struct LineText;

fn spawn_line(mut commands: Commands, assets: Res<AssetServer>) {
    let root = commands
        .spawn((
            Name::new("Chat line"),
            LineRoot,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(24.0),
                bottom: Val::Px(240.0),
                width: Val::Percent(40.0),
                min_width: Val::Px(320.0),
                padding: UiRect::axes(Val::Px(10.0), Val::Px(6.0)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.62)),
            BorderColor::all(Color::srgba(0.85, 0.7, 0.4, 0.7)),
            GlobalZIndex(9),
            Visibility::Hidden,
        ))
        .id();
    commands.spawn((
        LineText,
        Text::new(""),
        TextFont { font: assets.load(DISPLAY_FONT).into(), font_size: FontSize::Px(18.0), ..default() },
        TextColor(Color::srgb(0.95, 0.92, 0.84)),
        ChildOf(root),
    ));
}

fn draw_line(
    time: Res<Time<Real>>,
    line: Res<ChatLine>,
    mut root: Query<&mut Visibility, With<LineRoot>>,
    mut text: Query<&mut Text, With<LineText>>,
) {
    for mut v in &mut root {
        v.set_if_neq(if line.open { Visibility::Inherited } else { Visibility::Hidden });
    }
    if !line.open {
        return;
    }
    let caret = if (time.elapsed_secs() * 2.0) as u32 % 2 == 0 { "|" } else { " " };
    let shown = format!("{} {}{caret}", strings::CHAT_PROMPT, line.draft);
    for mut t in &mut text {
        if t.0 != shown {
            t.0 = shown.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_are_cleaned_and_last_by_length() {
        assert_eq!(clean_message("  hi\u{7}  "), "hi");
        assert_eq!(clean_message(&"x".repeat(500)).len(), CHAT_MAX_CHARS);
        assert!(message_secs("I said something!") > CHAT_BASE_SECS);
        assert_eq!(message_secs(&"x".repeat(CHAT_MAX_CHARS * 10)), CHAT_MAX_SECS);
    }

    #[test]
    fn offline_you_are_player1_or_your_name() {
        let session = NetSession::default();
        let mut settings = Settings::default();
        assert_eq!(my_name(&session, &settings), "Player1");
        settings.name = " gordelius ".into();
        assert_eq!(my_name(&session, &settings), "gordelius");
    }
}
