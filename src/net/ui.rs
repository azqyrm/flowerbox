//! The online panel (`O`): host, join with a code from the clipboard, copy the code, leave.
//! While it's open the player gets no gameplay input and the cursor is free (like the inventory).
//! A small status line in the top-right corner shows the session while online.

use bevy::prelude::*;
use bevy::window::{CursorOptions, PrimaryWindow};

use super::transport::NetLink;
use super::{NetInbox, NetSession, Role, SystemClipboard};
use crate::input::{GameplayBlocked, set_grab, toggle_cursor};
use crate::ui::hud::{FRAME, GOLD_TEXT};
use crate::ui::{DISPLAY_FONT, strings};

pub struct NetUiPlugin;

impl Plugin for NetUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OnlinePanel>()
            .init_resource::<PanelPress>()
            .add_systems(Startup, spawn_panel)
            .add_systems(Update, (toggle_panel.after(toggle_cursor), panel_keys, refresh_panel, status_corner).chain());
    }
}

#[derive(Resource, Default)]
pub struct OnlinePanel {
    pub open: bool,
}

#[derive(Component)]
struct PanelRoot;
#[derive(Component)]
struct PanelStatus;
#[derive(Component)]
struct PanelCode;
#[derive(Component)]
struct CornerStatus;

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum PanelButton {
    Host,
    Join,
    Copy,
    Leave,
    Close,
}

const TEXT: Color = Color::srgb(0.9, 0.88, 0.82);
const DIM: Color = Color::srgb(0.55, 0.52, 0.47);

fn spawn_panel(mut commands: Commands, assets: Res<AssetServer>) {
    let root = commands
        .spawn((
            Name::new("Online Panel"),
            PanelRoot,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
            GlobalZIndex(6),
            Visibility::Hidden,
        ))
        .id();
    let panel = commands
        .spawn((
            Node {
                width: Val::Px(620.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(12.0),
                padding: UiRect::all(Val::Px(22.0)),
                border: UiRect::all(Val::Px(2.0)),
                border_radius: BorderRadius::all(Val::Px(6.0)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.045, 0.04, 0.035)),
            BorderColor::all(FRAME),
            ChildOf(root),
        ))
        .id();
    commands.spawn((
        Text::new(strings::ONLINE_TITLE.to_uppercase()),
        TextFont { font: assets.load(DISPLAY_FONT).into(), font_size: FontSize::Px(34.0), ..default() },
        TextColor(GOLD_TEXT),
        ChildOf(panel),
    ));
    commands.spawn((Text::new(strings::ONLINE_EXPLAINER), TextFont::from_font_size(14.0), TextColor(DIM), ChildOf(panel)));
    commands.spawn((Text::new(""), TextFont::from_font_size(16.0), TextColor(TEXT), PanelStatus, ChildOf(panel)));
    commands.spawn((
        Text::new(""),
        TextFont::from_font_size(12.0),
        TextColor(GOLD_TEXT),
        PanelCode,
        Node { padding: UiRect::all(Val::Px(6.0)), ..default() },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
        ChildOf(panel),
    ));
    let row = commands.spawn((Node { column_gap: Val::Px(8.0), flex_wrap: FlexWrap::Wrap, row_gap: Val::Px(8.0), ..default() }, ChildOf(panel))).id();
    for (button, label) in [
        (PanelButton::Host, "[H] Host a game"),
        (PanelButton::Join, "[J] Join (code from clipboard)"),
        (PanelButton::Copy, "[C] Copy code"),
        (PanelButton::Leave, "[L] Leave"),
        (PanelButton::Close, "[O] Close"),
    ] {
        let b = commands
            .spawn((
                button,
                Node {
                    padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    border_radius: BorderRadius::all(Val::Px(4.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.35, 0.28, 0.14, 0.55)),
                BorderColor::all(GOLD_TEXT),
                ChildOf(row),
            ))
            .observe(move |_: On<PointerClick>, mut pressed: ResMut<PanelPress>| pressed.0 = Some(button))
            .id();
        commands.spawn((Text::new(label), TextFont::from_font_size(15.0), TextColor(GOLD_TEXT), ChildOf(b)));
    }

    commands.spawn((
        Name::new("Online Status"),
        CornerStatus,
        Text::new(""),
        TextFont::from_font_size(14.0),
        TextColor(GOLD_TEXT),
        TextShadow { offset: Vec2::splat(1.5), color: Color::BLACK },
        Node { position_type: PositionType::Absolute, right: Val::Px(20.0), top: Val::Px(16.0), ..default() },
    ));
}

/// A clicked panel button, handled by `panel_keys` (with the keyboard shortcuts).
#[derive(Resource, Default)]
struct PanelPress(Option<PanelButton>);

fn toggle_panel(
    keys: Res<ButtonInput<KeyCode>>,
    mut panel: ResMut<OnlinePanel>,
    mut press: ResMut<PanelPress>,
    mut blocked: ResMut<GameplayBlocked>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
    mut root: Query<&mut Visibility, With<PanelRoot>>,
) {
    let close = panel.open && (keys.just_pressed(KeyCode::Escape) || press.0 == Some(PanelButton::Close));
    let open = !panel.open && !blocked.0 && keys.just_pressed(KeyCode::KeyO);
    let toggle = (panel.open && keys.just_pressed(KeyCode::KeyO)) || close || open;
    if !toggle {
        return;
    }
    if press.0 == Some(PanelButton::Close) {
        press.0 = None;
    }
    panel.open = !panel.open;
    blocked.0 = panel.open;
    set_grab(&mut cursor, !panel.open);
    for mut v in &mut root {
        *v = if panel.open { Visibility::Inherited } else { Visibility::Hidden };
    }
}

fn panel_keys(
    keys: Res<ButtonInput<KeyCode>>,
    panel: Res<OnlinePanel>,
    mut press: ResMut<PanelPress>,
    link: Res<NetLink>,
    clipboard: Res<SystemClipboard>,
    mut session: ResMut<NetSession>,
    mut inbox: MessageWriter<NetInbox>,
) {
    let clicked = press.0.take();
    if !panel.open {
        return;
    }
    let hit = |b: PanelButton, k: KeyCode| clicked == Some(b) || keys.just_pressed(k);
    if hit(PanelButton::Host, KeyCode::KeyH) {
        super::host(&link, &mut session);
    }
    if hit(PanelButton::Join, KeyCode::KeyJ) {
        match clipboard.paste() {
            Some(code) => super::join(&link, &mut session, &code),
            None => session.status = "The clipboard is empty: copy the host's code first.".into(),
        }
    }
    if hit(PanelButton::Copy, KeyCode::KeyC)
        && let Some(code) = session.code.clone()
    {
        session.status = if clipboard.copy(&code) { "Code copied." } else { "Couldn't reach the clipboard: copy the code by hand." }.into();
    }
    if hit(PanelButton::Leave, KeyCode::KeyL) {
        super::leave(&link, &mut session, &mut inbox);
    }
}

fn role_label(session: &NetSession) -> String {
    match session.role {
        Role::Offline => "Offline.".into(),
        Role::StartingHost => "Starting to host…".into(),
        Role::Hosting => format!("Hosting, {} guest(s) connected.", session.peers.len()),
        Role::Joining => "Joining…".into(),
        Role::Client => format!("In the host's world as player {}.", session.my_id),
    }
}

fn refresh_panel(
    panel: Res<OnlinePanel>,
    session: Res<NetSession>,
    mut status: Query<&mut Text, (With<PanelStatus>, Without<PanelCode>)>,
    mut code: Query<&mut Text, (With<PanelCode>, Without<PanelStatus>)>,
) {
    if !panel.open || !(panel.is_changed() || session.is_changed()) {
        return;
    }
    for mut t in &mut status {
        t.0 = format!("{}\n{}", role_label(&session), session.status);
    }
    for mut t in &mut code {
        t.0 = session.code.clone().unwrap_or_else(|| "(no code: host a game to get one)".into());
    }
}

fn status_corner(session: Res<NetSession>, mut corner: Query<&mut Text, With<CornerStatus>>) {
    if !session.is_changed() {
        return;
    }
    for mut t in &mut corner {
        t.0 = match session.role {
            Role::Offline => String::new(),
            _ => format!("Online: {}  [O]", role_label(&session)),
        };
    }
}
