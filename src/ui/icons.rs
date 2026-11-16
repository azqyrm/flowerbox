//! Item icons drawn from UI nodes (no image assets yet). Each icon fits a ~56 px box at scale 1.

use bevy::prelude::*;

use crate::humanoid::skins::Skin;

use crate::items::ItemIcon;

fn px(v: f32) -> Val {
    Val::Px(v)
}

const STEEL: Color = Color::srgb(0.78, 0.8, 0.83);
const RUST: Color = Color::srgb(0.6, 0.38, 0.24);
const IRON: Color = Color::srgb(0.45, 0.42, 0.38);
const GRIP: Color = Color::srgb(0.25, 0.15, 0.08);
const OAK: Color = Color::srgb(0.5, 0.32, 0.15);
const LEATHER: Color = Color::srgb(0.42, 0.27, 0.15);
const BRASS: Color = Color::srgb(0.85, 0.66, 0.3);

/// Spawns `icon` under `parent`; returns the icon's root node.
pub fn spawn_icon(commands: &mut Commands, parent: Entity, icon: ItemIcon, scale: f32) -> Entity {
    let s = |v: f32| px(v * scale);
    let r = |v: f32| BorderRadius::all(s(v));
    let root = commands.spawn((Node { width: s(56.0), height: s(56.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }, ChildOf(parent))).id();
    // A child node with a fill colour and rounded corners.
    let block = |commands: &mut Commands, parent: Entity, node: Node, color: Color| {
        commands.spawn((node, BackgroundColor(color), ChildOf(parent))).id()
    };
    match icon {
        ItemIcon::Sword | ItemIcon::ShortSword | ItemIcon::FineSword => {
            let (len, metal) = match icon {
                ItemIcon::Sword => (34.0, STEEL),
                ItemIcon::FineSword => (36.0, Color::srgb(0.88, 0.9, 0.95)),
                _ => (24.0, RUST),
            };
            let guard = if icon == ItemIcon::FineSword { BRASS } else { IRON };
            let sword = commands
                .spawn((
                    Node { width: s(16.0), height: s(len + 20.0), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, ..default() },
                    UiTransform { rotation: Rot2::degrees(45.0), ..UiTransform::IDENTITY },
                    ChildOf(root),
                ))
                .id();
            let parts = [(5.0, len, metal, 2.0), (16.0, 4.0, guard, 1.0), (4.0, 10.0, GRIP, 1.0), (7.0, 6.0, guard, 3.0)];
            for (w, h, c, rad) in parts {
                block(commands, sword, Node { width: s(w), height: s(h), border_radius: r(rad), ..default() }, c);
            }
        }
        ItemIcon::Greatsword => {
            // A broad dark blade with a glowing red fuller and a thorned guard.
            let sword = commands
                .spawn((
                    Node { width: s(20.0), height: s(58.0), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, ..default() },
                    UiTransform { rotation: Rot2::degrees(45.0), ..UiTransform::IDENTITY },
                    ChildOf(root),
                ))
                .id();
            let blade = block(commands, sword, Node { width: s(9.0), height: s(36.0), border_radius: BorderRadius::new(s(4.0), s(4.0), s(0.0), s(0.0)), justify_content: JustifyContent::Center, ..default() }, Color::srgb(0.45, 0.45, 0.48));
            block(commands, blade, Node { width: s(2.0), height: s(30.0), margin: UiRect::top(s(4.0)), ..default() }, Color::srgb(0.95, 0.2, 0.1));
            block(commands, sword, Node { width: s(20.0), height: s(4.0), border_radius: r(1.0), ..default() }, Color::srgb(0.3, 0.28, 0.3));
            block(commands, sword, Node { width: s(4.0), height: s(13.0), ..default() }, GRIP);
            block(commands, sword, Node { width: s(7.0), height: s(5.0), border_radius: r(3.0), ..default() }, IRON);
        }
        ItemIcon::Mail => {
            // A mail shirt: body, short sleeves, a ring texture of dots.
            let shirt = block(commands, root, Node { width: s(28.0), height: s(36.0), border_radius: BorderRadius::new(s(4.0), s(4.0), s(2.0), s(2.0)), flex_wrap: FlexWrap::Wrap, column_gap: s(3.0), row_gap: s(3.0), padding: UiRect::all(s(3.0)), ..default() }, Color::srgb(0.5, 0.5, 0.53));
            for _ in 0..20 {
                block(commands, shirt, Node { width: s(2.5), height: s(2.5), border_radius: BorderRadius::MAX, ..default() }, Color::srgb(0.25, 0.25, 0.27));
            }
            for x in [-17.0, 17.0] {
                block(commands, root, Node { position_type: PositionType::Absolute, left: s(28.0 + x - 6.0), top: s(10.0), width: s(12.0), height: s(14.0), border_radius: r(3.0), ..default() }, Color::srgb(0.46, 0.46, 0.49));
            }
            block(commands, root, Node { position_type: PositionType::Absolute, top: s(28.0), width: s(30.0), height: s(4.0), ..default() }, Color::srgb(0.45, 0.06, 0.07));
        }
        ItemIcon::Gauntlet => {
            let g = block(commands, root, Node { width: s(22.0), height: s(36.0), border_radius: BorderRadius::new(s(9.0), s(9.0), s(3.0), s(3.0)), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, justify_content: JustifyContent::SpaceEvenly, ..default() }, Color::srgb(0.32, 0.3, 0.32));
            for _ in 0..4 {
                block(commands, g, Node { width: s(18.0), height: s(2.0), ..default() }, Color::srgb(0.55, 0.38, 0.22));
            }
            block(commands, root, Node { position_type: PositionType::Absolute, left: s(12.0), top: s(18.0), width: s(7.0), height: s(14.0), border_radius: r(3.0), ..default() }, Color::srgb(0.3, 0.28, 0.3));
        }
        ItemIcon::Greaves => {
            for x in [-8.0, 8.0] {
                let leg = block(commands, root, Node { position_type: PositionType::Absolute, left: s(28.0 + x - 6.0), top: s(6.0), width: s(12.0), height: s(40.0), border_radius: BorderRadius::new(s(5.0), s(5.0), s(2.0), s(2.0)), flex_direction: FlexDirection::Column, justify_content: JustifyContent::SpaceEvenly, ..default() }, Color::srgb(0.34, 0.32, 0.33));
                block(commands, leg, Node { width: s(12.0), height: s(3.0), ..default() }, Color::srgb(0.55, 0.38, 0.22));
                block(commands, root, Node { position_type: PositionType::Absolute, left: s(28.0 + x - 7.0), top: s(44.0), width: s(14.0), height: s(6.0), border_radius: r(2.0), ..default() }, Color::srgb(0.2, 0.15, 0.1));
            }
        }
        ItemIcon::BellRing => {
            let ring = commands
                .spawn((
                    Node { width: s(30.0), height: s(30.0), border: UiRect::all(s(5.0)), border_radius: BorderRadius::MAX, justify_content: JustifyContent::Center, ..default() },
                    BorderColor::all(Color::srgb(0.62, 0.42, 0.2)),
                    ChildOf(root),
                ))
                .id();
            // A tiny bell instead of a stone.
            block(commands, ring, Node { position_type: PositionType::Absolute, top: s(-14.0), width: s(12.0), height: s(12.0), border_radius: BorderRadius::new(s(6.0), s(6.0), s(1.0), s(1.0)), ..default() }, Color::srgb(0.8, 0.6, 0.25));
        }
        ItemIcon::Axe => {
            let axe = commands
                .spawn((
                    Node { width: s(30.0), height: s(52.0), ..default() },
                    UiTransform { rotation: Rot2::degrees(35.0), ..UiTransform::IDENTITY },
                    ChildOf(root),
                ))
                .id();
            let abs = |l: f32, t: f32, w: f32, h: f32, rad: BorderRadius| Node {
                position_type: PositionType::Absolute,
                left: s(l),
                top: s(t),
                width: s(w),
                height: s(h),
                border_radius: rad,
                ..default()
            };
            block(commands, axe, abs(13.0, 2.0, 4.0, 50.0, r(1.5)), Color::srgb(0.38, 0.25, 0.13));
            block(commands, axe, abs(16.0, 4.0, 13.0, 15.0, BorderRadius::new(s(1.0), s(7.0), s(7.0), s(1.0))), RUST);
            block(commands, axe, abs(9.0, 7.0, 5.0, 8.0, r(1.0)), RUST);
        }
        ItemIcon::Fist => {
            let fist = block(commands, root, Node { width: s(24.0), height: s(22.0), border_radius: r(7.0), flex_direction: FlexDirection::Column, justify_content: JustifyContent::SpaceEvenly, ..default() }, Color::srgb(0.78, 0.6, 0.48));
            for _ in 0..3 {
                block(commands, fist, Node { width: s(16.0), height: s(1.5), margin: UiRect::left(s(4.0)), ..default() }, Color::srgb(0.55, 0.4, 0.3));
            }
        }
        ItemIcon::RoundShield | ItemIcon::Buckler => {
            let (d, face, rim) = if icon == ItemIcon::RoundShield { (44.0, OAK, IRON) } else { (32.0, Color::srgb(0.5, 0.5, 0.52), BRASS) };
            let shield = commands
                .spawn((
                    Node { width: s(d), height: s(d), border: UiRect::all(s(3.0)), border_radius: BorderRadius::MAX, justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
                    BackgroundColor(face),
                    BorderColor::all(rim),
                    ChildOf(root),
                ))
                .id();
            if icon == ItemIcon::RoundShield {
                for x in [-9.0, 9.0] {
                    block(commands, shield, Node { position_type: PositionType::Absolute, left: s(19.0 + x - 3.0), top: s(2.0), width: s(1.5), height: s(34.0), ..default() }, Color::srgb(0.3, 0.18, 0.08));
                }
            }
            block(commands, shield, Node { width: s(d * 0.25), height: s(d * 0.25), border_radius: BorderRadius::MAX, ..default() }, Color::srgb(0.62, 0.6, 0.58));
        }
        ItemIcon::Hood => {
            let hood = block(
                commands,
                root,
                Node { width: s(36.0), height: s(40.0), border_radius: BorderRadius::new(s(18.0), s(18.0), s(6.0), s(6.0)), justify_content: JustifyContent::Center, ..default() },
                LEATHER,
            );
            block(commands, hood, Node { position_type: PositionType::Absolute, top: s(14.0), width: s(20.0), height: s(22.0), border_radius: BorderRadius::new(s(10.0), s(10.0), s(4.0), s(4.0)), ..default() }, Color::srgb(0.08, 0.06, 0.05));
        }
        ItemIcon::Ring | ItemIcon::ThornRing => {
            let (band, gem) = if icon == ItemIcon::Ring {
                (BRASS, Color::srgb(0.35, 0.25, 0.12))
            } else {
                (Color::srgb(0.38, 0.27, 0.16), Color::srgb(0.2, 0.32, 0.14))
            };
            let ring = commands
                .spawn((
                    Node { width: s(30.0), height: s(30.0), border: UiRect::all(s(5.0)), border_radius: BorderRadius::MAX, justify_content: JustifyContent::Center, ..default() },
                    BorderColor::all(band),
                    ChildOf(root),
                ))
                .id();
            block(commands, ring, Node { position_type: PositionType::Absolute, top: s(-11.0), width: s(10.0), height: s(10.0), border_radius: BorderRadius::MAX, ..default() }, gem);
            if icon == ItemIcon::ThornRing {
                // Thorns sticking out around the band.
                for i in 0..8 {
                    let a = i as f32 / 8.0 * std::f32::consts::TAU + 0.4;
                    commands.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: s(28.0 + a.cos() * 17.0 - 2.0),
                            top: s(28.0 + a.sin() * 17.0 - 2.0),
                            width: s(4.0),
                            height: s(4.0),
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.55, 0.42, 0.25)),
                        UiTransform { rotation: Rot2::radians(a + 0.785), ..UiTransform::IDENTITY },
                        ChildOf(root),
                    ));
                }
            }
        }
        ItemIcon::Key => {
            // A big iron key: a ring bow, a shaft, two bits.
            let iron = Color::srgb(0.42, 0.4, 0.38);
            commands.spawn((
                Node { position_type: PositionType::Absolute, left: s(10.0), top: s(18.0), width: s(18.0), height: s(18.0), border: UiRect::all(s(4.0)), border_radius: BorderRadius::MAX, ..default() },
                BorderColor::all(iron),
                ChildOf(root),
            ));
            block(commands, root, Node { position_type: PositionType::Absolute, left: s(27.0), top: s(25.0), width: s(22.0), height: s(4.0), ..default() }, iron);
            block(commands, root, Node { position_type: PositionType::Absolute, left: s(40.0), top: s(29.0), width: s(4.0), height: s(8.0), ..default() }, iron);
            block(commands, root, Node { position_type: PositionType::Absolute, left: s(45.0), top: s(29.0), width: s(4.0), height: s(6.0), ..default() }, iron);
        }
        ItemIcon::HopCone => {
            // Overlapping green scales, widest in the middle, a stalk on top.
            block(commands, root, Node { position_type: PositionType::Absolute, left: s(27.0), top: s(6.0), width: s(3.0), height: s(8.0), ..default() }, Color::srgb(0.3, 0.25, 0.12));
            let rows: [(f32, f32, usize); 5] = [(12.0, 14.0, 2), (19.0, 20.0, 3), (26.0, 22.0, 3), (33.0, 18.0, 3), (40.0, 12.0, 2)];
            for (r, &(top, width, n)) in rows.iter().enumerate() {
                for k in 0..n {
                    let w = width / n as f32 + 4.0;
                    let left = 28.5 - width * 0.5 + k as f32 * (width / n as f32) - 2.0;
                    let shade = 0.5 + 0.08 * ((r + k) % 2) as f32;
                    commands.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: s(left),
                            top: s(top),
                            width: s(w),
                            height: s(10.0),
                            border: UiRect::all(s(1.0)),
                            border_radius: BorderRadius::new(s(2.0), s(2.0), s(6.0), s(6.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.42 * shade + 0.1, shade + 0.12, 0.2 * shade)),
                        BorderColor::all(Color::srgb(0.18, 0.3, 0.1)),
                        ChildOf(root),
                    ));
                }
            }
        }
        ItemIcon::Poppy | ItemIcon::Camomile => {
            let (petal, centre) = if icon == ItemIcon::Poppy {
                (Color::srgb(0.85, 0.1, 0.08), Color::srgb(0.08, 0.06, 0.05))
            } else {
                (Color::srgb(0.95, 0.94, 0.88), Color::srgb(0.95, 0.75, 0.15))
            };
            flower(commands, root, s(1.0), petal, centre, if icon == ItemIcon::Poppy { 4 } else { 9 });
        }
        ItemIcon::Tankard => {
            let mug = commands.spawn((Node { width: s(26.0), height: s(30.0), ..default() }, ChildOf(root))).id();
            commands.spawn((
                Node { left: s(15.0), top: s(9.0), width: s(10.0), height: s(13.0), border: UiRect::all(s(3.0)), border_radius: r(4.0), position_type: PositionType::Absolute, ..default() },
                BorderColor::all(Color::srgb(0.38, 0.22, 0.1)),
                ChildOf(mug),
            ));
            let body = block(
                commands,
                mug,
                Node { left: s(2.0), top: s(6.0), width: s(17.0), height: s(23.0), border_radius: BorderRadius::new(s(2.0), s(2.0), s(4.0), s(4.0)), position_type: PositionType::Absolute, ..default() },
                Color::srgb(0.5, 0.3, 0.13),
            );
            for top in [4.0, 16.0] {
                block(commands, body, Node { top: s(top), width: Val::Percent(100.0), height: s(3.0), position_type: PositionType::Absolute, ..default() }, Color::srgb(0.25, 0.25, 0.27));
            }
            block(commands, mug, Node { left: s(0.0), top: s(1.0), width: s(21.0), height: s(7.0), border_radius: r(4.0), position_type: PositionType::Absolute, ..default() }, Color::srgb(0.95, 0.9, 0.74));
        }
    }
    root
}

/// A flower seen from above: `petals` round petals around a centre, with a green stem below.
fn flower(commands: &mut Commands, root: Entity, unit: Val, petal: Color, centre: Color, petals: usize) {
    let u = match unit {
        Val::Px(v) => v,
        _ => 1.0,
    };
    let px_ = |v: f32| Val::Px(v * u);
    commands.spawn((
        Node { position_type: PositionType::Absolute, left: px_(27.0), top: px_(30.0), width: px_(3.0), height: px_(22.0), ..default() },
        BackgroundColor(Color::srgb(0.2, 0.42, 0.15)),
        ChildOf(root),
    ));
    let size = if petals <= 5 { 17.0 } else { 11.0 };
    let ring = if petals <= 5 { 8.0 } else { 10.0 };
    for i in 0..petals {
        let a = i as f32 / petals as f32 * std::f32::consts::TAU;
        let (x, y) = (28.0 + a.cos() * ring - size * 0.5, 22.0 + a.sin() * ring - size * 0.5);
        commands.spawn((
            Node { position_type: PositionType::Absolute, left: px_(x), top: px_(y), width: px_(size), height: px_(size), border_radius: BorderRadius::MAX, ..default() },
            BackgroundColor(petal),
            BorderColor::all(Color::srgba(0.0, 0.0, 0.0, 0.5)),
            ChildOf(root),
        ));
    }
    commands.spawn((
        Node { position_type: PositionType::Absolute, left: px_(23.0), top: px_(17.0), width: px_(10.0), height: px_(10.0), border_radius: BorderRadius::MAX, ..default() },
        BackgroundColor(centre),
        ChildOf(root),
    ));
}

/// Icons for notifications: an item, or one of the world events.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NoticeIcon {
    Item(ItemIcon),
    /// The sunflower was revived.
    Sunflower,
    Joined,
    Left,
    /// Somebody was composted.
    Died,
    /// A door (locked, or opens from the other side).
    Door,
    /// A player's little face (chat): their skin, and their slot's tunic colour (Monobrows wear it).
    Face(Skin, Color),
}

/// Spawns a notification icon (same ~56 px box at scale 1); returns its root.
pub fn spawn_notice_icon(commands: &mut Commands, parent: Entity, icon: NoticeIcon, scale: f32) -> Entity {
    let s = |v: f32| px(v * scale);
    match icon {
        NoticeIcon::Item(item) => spawn_icon(commands, parent, item, scale),
        NoticeIcon::Sunflower => {
            let root = commands.spawn((Node { width: s(56.0), height: s(56.0), ..default() }, ChildOf(parent))).id();
            flower(commands, root, s(1.0), Color::srgb(1.0, 0.78, 0.1), Color::srgb(0.32, 0.18, 0.06), 10);
            root
        }
        NoticeIcon::Door => {
            // An arched plank door with two iron bands and a ring.
            let root = commands
                .spawn((Node { width: s(56.0), height: s(56.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }, ChildOf(parent)))
                .id();
            let door = commands
                .spawn((
                    Node {
                        width: s(28.0),
                        height: s(38.0),
                        border: UiRect::all(s(2.0)),
                        border_radius: BorderRadius::new(s(14.0), s(14.0), s(1.0), s(1.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.42, 0.26, 0.13)),
                    BorderColor::all(Color::srgb(0.08, 0.06, 0.05)),
                    ChildOf(root),
                ))
                .id();
            for top in [10.0, 24.0] {
                commands.spawn((
                    Node { position_type: PositionType::Absolute, top: s(top), width: Val::Percent(100.0), height: s(3.0), ..default() },
                    BackgroundColor(Color::srgb(0.15, 0.15, 0.16)),
                    ChildOf(door),
                ));
            }
            commands.spawn((
                Node { position_type: PositionType::Absolute, left: s(16.0), top: s(15.0), width: s(5.0), height: s(5.0), border_radius: BorderRadius::MAX, ..default() },
                BackgroundColor(Color::srgb(0.75, 0.6, 0.3)),
                ChildOf(door),
            ));
            root
        }
        NoticeIcon::Face(skin, tunic) => {
            let root = commands
                .spawn((Node { width: s(56.0), height: s(56.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }, ChildOf(parent)))
                .id();
            spawn_face(commands, root, skin, tunic, scale);
            root
        }
        NoticeIcon::Joined | NoticeIcon::Left | NoticeIcon::Died => {
            let (fill, mark) = match icon {
                NoticeIcon::Joined => (Color::srgb(0.25, 0.55, 0.25), "+"),
                NoticeIcon::Left => (Color::srgb(0.4, 0.4, 0.42), "-"),
                _ => (Color::srgb(0.32, 0.2, 0.1), "x"),
            };
            let root = commands
                .spawn((Node { width: s(56.0), height: s(56.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }, ChildOf(parent)))
                .id();
            let disc = commands
                .spawn((
                    Node {
                        width: s(34.0),
                        height: s(34.0),
                        border: UiRect::all(s(2.5)),
                        border_radius: BorderRadius::MAX,
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    BackgroundColor(fill),
                    BorderColor::all(Color::srgb(0.05, 0.04, 0.03)),
                    ChildOf(root),
                ))
                .id();
            commands.spawn((Text::new(mark), TextFont::from_font_size(24.0 * scale), TextColor(Color::srgb(0.95, 0.93, 0.88)), ChildOf(disc)));
            root
        }
    }
}

/// A player's bust in their skin: head and features drawn from boxes and circles over a collar.
fn spawn_face(commands: &mut Commands, parent: Entity, skin: Skin, tunic: Color, scale: f32) {
    let s = |v: f32| px(v * scale);
    let look = skin.spec().look;
    let dark = Color::srgb(0.06, 0.04, 0.03);
    let at = |l: f32, t: f32, w: f32, h: f32| Node { position_type: PositionType::Absolute, left: s(l), top: s(t), width: s(w), height: s(h), ..default() };
    let round = |l: f32, t: f32, w: f32, h: f32| Node { border_radius: BorderRadius::MAX, ..at(l, t, w, h) };
    let bust = commands.spawn((Node { width: s(38.0), height: s(44.0), ..default() }, ChildOf(parent))).id();
    let dot = |commands: &mut Commands, parent: Entity, node: Node, color: Color| {
        commands.spawn((node, BackgroundColor(color), ChildOf(parent)));
    };
    // Collar (what they wear on the chest).
    let collar = match skin {
        Skin::Monobrow => tunic,
        Skin::Bruin => look.bottom,
        Skin::Rattlebones => look.skin,
        _ => look.top,
    };
    commands.spawn((
        Node { border: UiRect::all(s(2.0)), border_radius: BorderRadius::new(s(14.0), s(14.0), s(3.0), s(3.0)), ..at(1.0, 30.0, 36.0, 14.0) },
        BackgroundColor(collar),
        BorderColor::all(dark),
        ChildOf(bust),
    ));
    // Behind the head: ears.
    match skin {
        Skin::Tabby => {
            for l in [7.0, 21.0] {
                commands.spawn((
                    Node { border: UiRect::all(s(2.0)), ..at(l, 1.0, 10.0, 10.0) },
                    UiTransform::from_rotation(Rot2::degrees(45.0)),
                    BackgroundColor(look.skin),
                    BorderColor::all(dark),
                    ChildOf(bust),
                ));
            }
        }
        Skin::Bruin => {
            for l in [4.0, 24.0] {
                let ear = commands
                    .spawn((Node { border: UiRect::all(s(2.0)), border_radius: BorderRadius::MAX, ..at(l, 0.0, 11.0, 11.0) }, BackgroundColor(look.skin), BorderColor::all(dark), ChildOf(bust)))
                    .id();
                dot(commands, ear, round(2.0, 2.0, 3.5, 3.5), Color::srgb(0.66, 0.5, 0.32));
            }
        }
        _ => {}
    }
    let radius = if skin == Skin::Blockhead { BorderRadius::all(s(2.0)) } else { BorderRadius::MAX };
    let head = commands
        .spawn((Node { border: UiRect::all(s(2.0)), border_radius: radius, ..at(6.0, 2.0, 26.0, 30.0) }, BackgroundColor(look.skin), BorderColor::all(dark), ChildOf(bust)))
        .id();
    // Inside the head (22 × 26 inside the border).
    match skin {
        Skin::Monobrow => {
            dot(commands, head, Node { border_radius: BorderRadius::new(s(12.0), s(12.0), s(2.0), s(2.0)), ..at(0.0, 0.0, 22.0, 8.0) }, look.hair);
            // The one long brow.
            dot(commands, head, Node { border_radius: BorderRadius::all(s(1.0)), ..at(3.0, 11.0, 16.0, 3.5) }, dark);
            dot(commands, head, Node { border_radius: BorderRadius::all(s(1.5)), ..at(7.0, 19.0, 8.0, 2.5) }, Color::srgb(0.45, 0.16, 0.12));
        }
        Skin::Blockhead => {
            dot(commands, head, at(0.0, 0.0, 22.0, 7.0), look.hair);
            for (l, pupil) in [(2.5, 5.5), (13.5, 13.5)] {
                dot(commands, head, at(l, 10.0, 6.0, 3.5), Color::srgb(0.95, 0.95, 0.93));
                dot(commands, head, at(pupil, 10.0, 3.0, 3.5), Color::srgb(0.2, 0.25, 0.6));
            }
            dot(commands, head, at(9.0, 14.0, 4.0, 3.5), Color::srgb(0.6, 0.42, 0.3));
            dot(commands, head, at(5.0, 19.0, 12.0, 4.0), Color::srgb(0.32, 0.2, 0.12));
        }
        Skin::Tabby => {
            dot(commands, head, round(6.0, 14.0, 10.0, 7.0), Color::srgb(0.95, 0.88, 0.75));
            for l in [4.0, 14.0] {
                dot(commands, head, round(l, 9.0, 4.0, 5.0), Color::srgb(0.35, 0.8, 0.2));
                dot(commands, head, round(l + 1.3, 9.5, 1.4, 4.0), dark);
            }
            dot(commands, head, round(9.5, 14.0, 3.0, 2.2), Color::srgb(0.9, 0.5, 0.55));
            for (l, t) in [(-2.0, 17.0), (-2.0, 19.5), (17.0, 17.0), (17.0, 19.5)] {
                dot(commands, head, at(l, t, 7.0, 0.8), dark);
            }
        }
        Skin::Bruin => {
            dot(commands, head, round(5.0, 13.0, 12.0, 9.0), Color::srgb(0.66, 0.5, 0.32));
            dot(commands, head, round(8.5, 13.5, 5.0, 3.5), dark);
            for l in [4.5, 14.0] {
                dot(commands, head, round(l, 9.0, 3.5, 3.5), dark);
            }
        }
        Skin::Rattlebones => {
            for l in [2.0, 12.5] {
                let socket = commands.spawn((Node { border_radius: BorderRadius::MAX, ..at(l, 7.0, 7.5, 7.5) }, BackgroundColor(dark), ChildOf(head))).id();
                dot(commands, socket, round(2.5, 2.5, 2.5, 2.5), Color::srgb(0.45, 0.75, 1.0));
            }
            commands.spawn((at(9.5, 15.0, 3.0, 3.0), UiTransform::from_rotation(Rot2::degrees(45.0)), BackgroundColor(dark), ChildOf(head)));
            let teeth = commands.spawn((at(5.0, 20.0, 12.0, 4.0), BackgroundColor(Color::srgb(0.96, 0.94, 0.86)), ChildOf(head))).id();
            for l in [2.5, 5.5, 8.5] {
                dot(commands, teeth, at(l, 0.0, 0.8, 4.0), dark);
            }
        }
        Skin::Toadstool => {
            for l in [5.0, 14.0] {
                dot(commands, head, round(l, 11.0, 3.5, 4.0), dark);
            }
            for l in [1.5, 16.5] {
                dot(commands, head, round(l, 15.5, 4.5, 3.5), Color::srgb(0.95, 0.55, 0.5));
            }
            dot(commands, head, Node { border_radius: BorderRadius::new(s(1.0), s(1.0), s(4.0), s(4.0)), ..at(8.0, 19.0, 6.0, 3.0) }, Color::srgb(0.45, 0.16, 0.12));
        }
    }
    // Over the head: the mushroom's cap.
    if skin == Skin::Toadstool {
        let cap = commands
            .spawn((
                Node { border: UiRect::all(s(2.0)), border_radius: BorderRadius::new(s(19.0), s(19.0), s(4.0), s(4.0)), ..at(0.0, -3.0, 38.0, 15.0) },
                BackgroundColor(Color::srgb(0.78, 0.1, 0.07)),
                BorderColor::all(dark),
                ChildOf(bust),
            ))
            .id();
        for (l, t, d) in [(6.0, 4.0, 4.0), (15.0, 1.0, 5.0), (25.0, 4.5, 4.0), (11.0, 7.0, 3.0), (21.0, 7.5, 3.0)] {
            dot(commands, cap, round(l, t, d, d), Color::srgb(0.96, 0.94, 0.9));
        }
    }
}
