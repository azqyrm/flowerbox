//! The menu on Esc (gamepad: Select). Like the inventory, the world keeps running (no pausing in a
//! Soulslike), but the player gets no gameplay input and the cursor is free.
//!
//! Pages: **main** (Resume, Settings, Controls, Delete save, Quit), **settings** (your name —
//! Enter, type, Enter —, skin, mouse sensitivity, invert Y, stick speed, volume, notification corner) and **controls** (rebind:
//! select an action, press the new key or mouse button; Esc cancels; a key already in use swaps).
//! Keys: Up/Down (W/S, D-pad) select, Left/Right (A/D) change, Enter/Space (A) activate,
//! Esc (B) back / close. Mouse: hover selects, click activates, `<` `>` change values.

use bevy::prelude::*;

use super::hud::{FRAME, GOLD_TEXT};
use super::notifications::Corner;
use super::{DISPLAY_FONT, strings};
use bevy::input::keyboard::KeyboardInput;

use super::text_input::{Edit, type_into};
use crate::input::{InputSystems, MenuInput};
use crate::net::NetSession;
use crate::net::names::{name_char, sanitize};
use crate::tuning::MAX_NAME_CHARS;
use crate::save::DeleteSave;
use crate::settings::{Bind, Input, Settings};

pub struct PauseMenuPlugin;

impl Plugin for PauseMenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PauseMenu>()
            .add_systems(Startup, spawn_root)
            .add_systems(Update, (toggle_menu, capture_binding, edit_name, menu_input, rebuild).chain().in_set(InputSystems::Menus));
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Page {
    #[default]
    Main,
    Settings,
    Controls,
}

#[derive(Resource, Default, Debug)]
pub struct PauseMenu {
    pub open: bool,
    page: Page,
    cursor: usize,
    /// Waiting for the new input of this action (armed after one quiet frame).
    capturing: Option<(Bind, bool)>,
    /// Typing a new name (the draft).
    editing_name: Option<String>,
    dirty: bool,
    /// A row clicked with the mouse: (row, direction: 0 = activate, ±1 = change).
    clicked: Option<(usize, i32)>,
}

impl PauseMenu {
    /// Opens the menu on `page` (debug scripts; the player uses Esc).
    pub fn show(&mut self, page: Page) {
        self.open = true;
        self.page = page;
        self.cursor = 0;
        self.dirty = true;
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Row {
    Resume,
    OpenSettings,
    OpenControls,
    DeleteSave,
    Quit,
    Back,
    PlayerName,
    Skin,
    Sensitivity,
    InvertY,
    StickSpeed,
    Volume,
    NoticeCorner,
    Rebind(Bind),
    ResetBindings,
}

fn rows(page: Page) -> Vec<Row> {
    match page {
        Page::Main => vec![Row::Resume, Row::OpenSettings, Row::OpenControls, Row::DeleteSave, Row::Quit],
        Page::Settings => vec![Row::PlayerName, Row::Skin, Row::Sensitivity, Row::InvertY, Row::StickSpeed, Row::Volume, Row::NoticeCorner, Row::Back],
        Page::Controls => {
            let mut r: Vec<Row> = Bind::ALL.iter().map(|b| Row::Rebind(*b)).collect();
            r.push(Row::ResetBindings);
            r.push(Row::Back);
            r
        }
    }
}

fn corner_label(c: Corner) -> &'static str {
    match c {
        Corner::TopLeft => "Top left",
        Corner::TopRight => "Top right",
        Corner::BottomLeft => "Bottom left",
        Corner::BottomRight => "Bottom right",
    }
}

fn next_corner(c: Corner, dir: i32) -> Corner {
    const ALL: [Corner; 4] = [Corner::BottomRight, Corner::BottomLeft, Corner::TopLeft, Corner::TopRight];
    let i = ALL.iter().position(|x| *x == c).unwrap_or(0) as i32;
    ALL[(i + dir).rem_euclid(4) as usize]
}

/// (label, value) of a row.
fn row_text(row: Row, s: &Settings, capturing: Option<Bind>, editing: Option<&str>, shown_as: Option<&str>) -> (String, Option<String>) {
    let pct = |v: f32| format!("{:.0}%", v * 100.0);
    match row {
        Row::Resume => ("Resume".into(), None),
        Row::OpenSettings => ("Settings".into(), None),
        Row::OpenControls => ("Controls".into(), None),
        Row::DeleteSave => ("Delete save (the next start is a new game)".into(), None),
        Row::Quit => ("Quit".into(), None),
        Row::Back => ("Back".into(), None),
        Row::PlayerName => {
            let value = match (editing, shown_as) {
                (Some(draft), _) => format!("{draft}_"),
                (None, _) if s.name.is_empty() => strings::NAME_NONE.into(),
                // Online the host may have told a twin apart.
                (None, Some(shown)) if shown != s.name => format!("{}  (shown as {shown})", s.name),
                (None, _) => s.name.clone(),
            };
            (strings::NAME_LABEL.into(), Some(value))
        }
        Row::Skin => ("Skin".into(), Some(s.skin.name().into())),
        Row::Sensitivity => ("Mouse sensitivity".into(), Some(pct(s.mouse_sensitivity))),
        Row::InvertY => ("Invert camera Y".into(), Some(if s.invert_y { "On" } else { "Off" }.into())),
        Row::StickSpeed => ("Gamepad camera speed".into(), Some(pct(s.stick_speed))),
        Row::Volume => ("Volume".into(), Some(pct(s.volume))),
        Row::NoticeCorner => ("Notifications".into(), Some(corner_label(s.notification_corner).into())),
        Row::Rebind(b) => {
            let value = if capturing == Some(b) { "Press a key...  (Esc cancels)".into() } else { s.bindings.get(b).label() };
            (b.label().into(), Some(value))
        }
        Row::ResetBindings => ("Reset to defaults".into(), None),
    }
}

#[derive(Component)]
struct MenuRoot;

fn spawn_root(mut commands: Commands) {
    commands.spawn((
        Name::new("Pause Menu"),
        MenuRoot,
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
        GlobalZIndex(7),
        Visibility::Hidden,
    ));
}

#[allow(clippy::too_many_arguments)]
fn toggle_menu(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    mut menu: ResMut<PauseMenu>,
    mut menus: ResMut<MenuInput>,
    mut was_blocked: Local<bool>,
    mut root: Query<&mut Visibility, With<MenuRoot>>,
) {
    let pad = |b: GamepadButton| pads.iter().any(|g| g.just_pressed(b));
    let esc = keys.just_pressed(KeyCode::Escape) || pad(GamepadButton::East);
    let select = pad(GamepadButton::Select);
    let mut set_open = |menu: &mut PauseMenu, open: bool| {
        menu.open = open;
        menu.page = Page::Main;
        menu.cursor = 0;
        menu.capturing = None;
        menu.editing_name = None;
        menu.dirty = true;
        for mut v in &mut root {
            *v = if open { Visibility::Inherited } else { Visibility::Hidden };
        }
    };
    if menu.open {
        if menu.capturing.is_some() || menu.editing_name.is_some() {
            // Esc cancels the capture / the name (handled in `capture_binding` / `edit_name`).
        } else if select || (esc && menu.page == Page::Main) {
            set_open(&mut menu, false);
        } else if esc {
            menu.page = Page::Main;
            menu.cursor = 0;
            menu.dirty = true;
        }
    } else if (keys.just_pressed(KeyCode::Escape) || select) && !*was_blocked && !menus.any() {
        // Only when no other menu had the input (Esc closing the inventory doesn't open this).
        set_open(&mut menu, true);
    }
    // Also covers opening from elsewhere (`show`) and closing via "Resume".
    menus.pause = menu.open;
    for mut v in &mut root {
        v.set_if_neq(if menu.open { Visibility::Inherited } else { Visibility::Hidden });
    }
    *was_blocked = menus.any();
}

/// Waiting for a new binding: the next key or mouse button (after one quiet frame) is it.
fn capture_binding(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut menu: ResMut<PauseMenu>,
    mut settings: ResMut<Settings>,
) {
    let Some((bind, armed)) = menu.capturing else { return };
    let any_key = keys.get_just_pressed().next().copied();
    let any_mouse = mouse.get_just_pressed().next().copied();
    if !armed {
        if any_key.is_none() && any_mouse.is_none() {
            menu.capturing = Some((bind, true));
        }
        return;
    }
    let input = match (any_key, any_mouse) {
        (Some(KeyCode::Escape), _) => {
            menu.capturing = None;
            menu.dirty = true;
            return;
        }
        (Some(k), _) => Input::Key(k),
        (None, Some(m)) => Input::Mouse(m),
        (None, None) => return,
    };
    let movement = matches!(bind, Bind::MoveForward | Bind::MoveBack | Bind::MoveLeft | Bind::MoveRight);
    if movement && matches!(input, Input::Mouse(_)) {
        return; // movement needs keys
    }
    settings.bindings.set(bind, input);
    menu.capturing = None;
    menu.dirty = true;
}

/// Typing the name: Enter keeps it (cleaned up), Esc leaves the old one.
fn edit_name(mut typed: MessageReader<KeyboardInput>, mut menu: ResMut<PauseMenu>, mut settings: ResMut<Settings>) {
    let Some(mut draft) = menu.editing_name.take() else {
        typed.clear(); // nothing typed before editing counts (the Enter that started it)
        return;
    };
    let before = draft.clone();
    match type_into(&mut draft, typed.read(), MAX_NAME_CHARS, name_char) {
        Edit::Typing => {
            if draft != before {
                menu.dirty = true;
            }
            menu.editing_name = Some(draft);
        }
        Edit::Submit => {
            let name = sanitize(&draft);
            if settings.name != name {
                settings.name = name;
            }
            menu.dirty = true;
        }
        Edit::Cancel => menu.dirty = true,
    }
}

#[allow(clippy::too_many_arguments)]
fn menu_input(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    mut menu: ResMut<PauseMenu>,
    mut settings: ResMut<Settings>,
    mut menus: ResMut<MenuInput>,
    mut root: Query<&mut Visibility, With<MenuRoot>>,
    mut exit: MessageWriter<AppExit>,
    mut delete: MessageWriter<DeleteSave>,
) {
    if !menu.open || menu.capturing.is_some() || menu.editing_name.is_some() {
        return;
    }
    let pressed = |ks: &[KeyCode]| ks.iter().any(|k| keys.just_pressed(*k));
    let pad = |b: GamepadButton| pads.iter().any(|g| g.just_pressed(b));
    use KeyCode::*;
    let list = rows(menu.page);
    let mut cursor_pos = menu.cursor.min(list.len() - 1);
    if pressed(&[ArrowUp, KeyW]) || pad(GamepadButton::DPadUp) {
        cursor_pos = (cursor_pos + list.len() - 1) % list.len();
    }
    if pressed(&[ArrowDown, KeyS]) || pad(GamepadButton::DPadDown) {
        cursor_pos = (cursor_pos + 1) % list.len();
    }
    let mut action: Option<(usize, i32)> = menu.clicked.take();
    if pressed(&[Enter, NumpadEnter, Space]) || pad(GamepadButton::South) {
        action = Some((cursor_pos, 0));
    }
    if pressed(&[ArrowLeft, KeyA]) || pad(GamepadButton::DPadLeft) {
        action = Some((cursor_pos, -1));
    }
    if pressed(&[ArrowRight, KeyD]) || pad(GamepadButton::DPadRight) {
        action = Some((cursor_pos, 1));
    }
    if cursor_pos != menu.cursor {
        menu.cursor = cursor_pos;
        menu.dirty = true;
    }
    let Some((index, dir)) = action else { return };
    let Some(&row) = list.get(index) else { return };
    menu.cursor = index;
    menu.dirty = true;
    let step = |v: f32, d: i32, lo: f32, hi: f32| (v + 0.1 * if d == 0 { 1.0 } else { d as f32 }).clamp(lo, hi);
    match row {
        Row::Resume => {
            menu.open = false;
            menus.pause = false;
            for mut v in &mut root {
                *v = Visibility::Hidden;
            }
        }
        Row::OpenSettings => (menu.page, menu.cursor) = (Page::Settings, 0),
        Row::OpenControls => (menu.page, menu.cursor) = (Page::Controls, 0),
        Row::Back => (menu.page, menu.cursor) = (Page::Main, 0),
        Row::DeleteSave if dir == 0 => {
            delete.write(DeleteSave);
        }
        Row::Quit if dir == 0 => {
            exit.write(AppExit::Success);
        }
        Row::Sensitivity => settings.mouse_sensitivity = step(settings.mouse_sensitivity, dir, 0.1, 3.0),
        Row::StickSpeed => settings.stick_speed = step(settings.stick_speed, dir, 0.2, 3.0),
        Row::Volume => settings.volume = step(settings.volume, if dir == 0 { 1 } else { dir }, 0.0, 1.0),
        Row::InvertY => settings.invert_y = !settings.invert_y,
        Row::Skin => settings.skin = settings.skin.cycle(if dir == 0 { 1 } else { dir }),
        Row::NoticeCorner => settings.notification_corner = next_corner(settings.notification_corner, if dir == 0 { 1 } else { dir }),
        Row::Rebind(b) if dir == 0 => menu.capturing = Some((b, false)),
        Row::PlayerName if dir == 0 => menu.editing_name = Some(settings.name.clone()),
        Row::ResetBindings if dir == 0 => settings.bindings = default(),
        _ => {}
    }
}

#[allow(clippy::type_complexity)]
fn rebuild(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut menu: ResMut<PauseMenu>,
    settings: Res<Settings>,
    session: Res<NetSession>,
    root: Single<Entity, With<MenuRoot>>,
) {
    if !menu.open || !(menu.dirty || settings.is_changed() || session.is_changed()) {
        return;
    }
    menu.dirty = false;
    let root = *root;
    commands.entity(root).despawn_children();
    let panel = commands
        .spawn((
            Node {
                width: Val::Px(560.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(4.0),
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
    let title = match menu.page {
        Page::Main => strings::MENU_TITLE,
        Page::Settings => "Settings",
        Page::Controls => "Controls",
    };
    commands.spawn((
        Text::new(title.to_uppercase()),
        TextFont { font: assets.load(DISPLAY_FONT).into(), font_size: FontSize::Px(32.0), ..default() },
        TextColor(GOLD_TEXT),
        Node { margin: UiRect::bottom(Val::Px(10.0)), ..default() },
        ChildOf(panel),
    ));
    let capturing = menu.capturing.map(|(b, _)| b);
    for (i, row) in rows(menu.page).into_iter().enumerate() {
        let selected = i == menu.cursor;
        let shown = session.online().then(|| session.name_of(session.my_id));
        let (label, value) = row_text(row, &settings, capturing, menu.editing_name.as_deref(), shown.as_deref());
        let r = commands
            .spawn((
                Node {
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    padding: UiRect::axes(Val::Px(10.0), Val::Px(5.0)),
                    border: UiRect::left(Val::Px(3.0)),
                    ..default()
                },
                BackgroundColor(if selected { Color::srgba(0.35, 0.28, 0.14, 0.5) } else { Color::NONE }),
                BorderColor::all(if selected { GOLD_TEXT } else { Color::NONE }),
                ChildOf(panel),
            ))
            .observe(move |_: On<PointerOver>, mut menu: ResMut<PauseMenu>| {
                if menu.cursor != i && menu.capturing.is_none() && menu.editing_name.is_none() {
                    menu.cursor = i;
                    menu.dirty = true;
                }
            })
            .observe(move |_: On<PointerClick>, mut menu: ResMut<PauseMenu>| {
                if menu.capturing.is_none() && menu.editing_name.is_none() {
                    menu.clicked = Some((i, 0));
                }
            })
            .id();
        let color = if selected { GOLD_TEXT } else { Color::srgb(0.9, 0.88, 0.82) };
        commands.spawn((Text::new(label), TextFont::from_font_size(17.0), TextColor(color), ChildOf(r)));
        if let Some(value) = value {
            let vbox = commands.spawn((Node { column_gap: Val::Px(10.0), align_items: AlignItems::Center, ..default() }, ChildOf(r))).id();
            let adjustable = !matches!(row, Row::Rebind(_) | Row::PlayerName);
            let arrow = |commands: &mut Commands, s: &str, dir: i32| {
                commands
                    .spawn((Text::new(s), TextFont::from_font_size(17.0), TextColor(Color::srgb(0.6, 0.56, 0.48)), ChildOf(vbox)))
                    .observe(move |mut click: On<PointerClick>, mut menu: ResMut<PauseMenu>| {
                        click.propagate(false);
                        menu.clicked = Some((i, dir));
                    });
            };
            if adjustable {
                arrow(&mut commands, "<", -1);
            }
            // The name in the serif font (it has umlauts).
            let font = if row == Row::PlayerName { TextFont { font: assets.load(DISPLAY_FONT).into(), font_size: FontSize::Px(18.0), ..default() } } else { TextFont::from_font_size(17.0) };
            commands.spawn((Text::new(value), font, TextColor(color), ChildOf(vbox)));
            if adjustable {
                arrow(&mut commands, ">", 1);
            }
        }
    }
    commands.spawn((
        Text::new(if menu.editing_name.is_some() { strings::NAME_EDITING_HINT } else { strings::MENU_HINTS }),
        TextFont::from_font_size(13.0),
        TextColor(Color::srgb(0.55, 0.52, 0.47)),
        Node { margin: UiRect::top(Val::Px(12.0)), ..default() },
        ChildOf(panel),
    ));
}
