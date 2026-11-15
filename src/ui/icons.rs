//! Item icons drawn from UI nodes (no image assets yet). Each icon fits a ~56 px box at scale 1.

use bevy::prelude::*;

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
        ItemIcon::Sword | ItemIcon::ShortSword => {
            let (len, metal) = if icon == ItemIcon::Sword { (34.0, STEEL) } else { (24.0, RUST) };
            let sword = commands
                .spawn((
                    Node { width: s(16.0), height: s(len + 20.0), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, ..default() },
                    UiTransform { rotation: Rot2::degrees(45.0), ..UiTransform::IDENTITY },
                    ChildOf(root),
                ))
                .id();
            let parts = [(5.0, len, metal, 2.0), (16.0, 4.0, IRON, 1.0), (4.0, 10.0, GRIP, 1.0), (7.0, 6.0, IRON, 3.0)];
            for (w, h, c, rad) in parts {
                block(commands, sword, Node { width: s(w), height: s(h), border_radius: r(rad), ..default() }, c);
            }
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
        ItemIcon::Ring => {
            let ring = commands
                .spawn((
                    Node { width: s(30.0), height: s(30.0), border: UiRect::all(s(5.0)), border_radius: BorderRadius::MAX, justify_content: JustifyContent::Center, ..default() },
                    BorderColor::all(BRASS),
                    ChildOf(root),
                ))
                .id();
            block(commands, ring, Node { position_type: PositionType::Absolute, top: s(-11.0), width: s(10.0), height: s(10.0), border_radius: BorderRadius::MAX, ..default() }, Color::srgb(0.35, 0.25, 0.12));
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
