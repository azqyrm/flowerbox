//! In-game HUD (bevy_ui):
//! - top left: HP (red, with a lagging "damage taken" chunk), stamina (green), Healing Ale count
//! - bottom left: the equipment quadrant (spells / left hand / right hand / quick item)
//! - bottom centre: the interaction prompt (see `ui::strings`) and the "Picked up" toast
//! - centre: banners and the death screen (band text, fade to black)

use bevy::prelude::*;

use super::icons::spawn_icon;
use super::{Banner, DISPLAY_FONT, strings};
use crate::character::stats::{Health, Stamina};
use crate::healing::HealingAle;
use crate::hearth::{DeathSequence, death_screen};
use crate::interact::{InteractPrompt, InteractSystems};
use crate::items::inventory::{EquipSlot, Equipment, Inventory};
use crate::items::pickup::ItemPickedUp;
use crate::items::ItemIcon;
use crate::player::Player;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_hud)
            .add_systems(Update, (update_bars, update_ale.after(update_hands), update_hands, update_prompt.after(InteractSystems::Hearth), update_toast, update_banner, update_death_screen));
    }
}

/// Pixel width of the HP bar per point of max HP (the bar grows with max HP, Souls-style).
const HP_BAR_PX_PER_POINT: f32 = 3.2;
const STAMINA_BAR_PX_PER_POINT: f32 = 2.8;
/// The damage chunk waits this long, then drains at `LAG_DRAIN` of max HP per second.
const LAG_HOLD: f32 = 0.7;
const LAG_DRAIN: f32 = 0.8;
const BANNER_TIME: f32 = 3.4;
const TOAST_TIME: f32 = 3.0;

pub(super) const FRAME: Color = Color::srgb(0.42, 0.36, 0.25);
pub(super) const PANEL: Color = Color::srgba(0.03, 0.025, 0.02, 0.8);
const HP_RED: Color = Color::srgb(0.62, 0.08, 0.06);
const HP_LAG: Color = Color::srgb(0.85, 0.62, 0.25);
const STAMINA_GREEN: Color = Color::srgb(0.2, 0.55, 0.22);
pub(super) const GOLD_TEXT: Color = Color::srgb(0.93, 0.82, 0.55);

#[derive(Component)]
struct HpFrame;
#[derive(Component)]
struct HpFill;
#[derive(Component)]
struct HpLag {
    value: f32,
    hold: f32,
}
#[derive(Component)]
struct StaminaFrame;
#[derive(Component)]
struct StaminaFill;
/// Text showing the Ale charges; both the top-left counter and the quadrant slot.
#[derive(Component)]
struct AleCount;
/// Tankard icons, dimmed when empty.
#[derive(Component)]
struct AleIcon;
#[derive(Component)]
struct WeaponName;
/// A hand slot of the equipment quadrant; its icon follows the equipment.
#[derive(Component)]
struct HandSlot(EquipSlot);
#[derive(Component)]
struct ToastRoot {
    t: f32,
}
#[derive(Component)]
struct ToastIcon;
#[derive(Component)]
struct ToastText;
#[derive(Component)]
struct PromptRoot;
#[derive(Component)]
struct PromptText;
#[derive(Component)]
struct BannerRoot {
    t: f32,
}
#[derive(Component)]
struct BannerText;
#[derive(Component)]
struct DeathBand;
#[derive(Component)]
struct DeathText;
#[derive(Component)]
struct BlackFade;

fn px(v: f32) -> Val {
    Val::Px(v)
}

fn absolute() -> Node {
    Node { position_type: PositionType::Absolute, ..default() }
}

fn spawn_hud(mut commands: Commands, assets: Res<AssetServer>) {
    let display: Handle<Font> = assets.load(DISPLAY_FONT);

    // ── Top left: bars and Ale ──
    let top_left = commands
        .spawn((
            Name::new("HUD Status"),
            Node {
                left: px(28.0),
                top: px(24.0),
                flex_direction: FlexDirection::Column,
                row_gap: px(6.0),
                ..absolute()
            },
        ))
        .id();
    let bar = |commands: &mut Commands, height: f32, fill: Color, frame_marker: bool| {
        let frame = commands
            .spawn((
                Node {
                    height: px(height),
                    border: UiRect::all(px(2.0)),
                    border_radius: BorderRadius::all(px(2.0)),
                    ..default()
                },
                BackgroundColor(PANEL),
                BorderColor::all(FRAME),
                ChildOf(top_left),
            ))
            .id();
        if frame_marker {
            commands.entity(frame).insert(HpFrame);
            commands.spawn((Node { height: Val::Percent(100.0), ..absolute() }, BackgroundColor(HP_LAG), HpLag { value: 1.0, hold: 0.0 }, ChildOf(frame)));
            commands.spawn((Node { height: Val::Percent(100.0), ..absolute() }, BackgroundColor(fill), HpFill, ChildOf(frame)));
        } else {
            commands.entity(frame).insert(StaminaFrame);
            commands.spawn((Node { height: Val::Percent(100.0), ..absolute() }, BackgroundColor(fill), StaminaFill, ChildOf(frame)));
        }
    };
    bar(&mut commands, 16.0, HP_RED, true);
    bar(&mut commands, 11.0, STAMINA_GREEN, false);
    let ale_row = commands
        .spawn((
            Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(6.0), margin: UiRect::top(px(4.0)), ..default() },
            ChildOf(top_left),
        ))
        .id();
    let tankard = spawn_icon(&mut commands, ale_row, ItemIcon::Tankard, 1.0);
    commands.entity(tankard).insert((
        AleIcon,
        Node { width: px(28.0), height: px(32.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
    ));
    commands.spawn((
        Text::new("3"),
        TextFont::from_font_size(20.0),
        TextColor(GOLD_TEXT),
        TextShadow { offset: Vec2::splat(2.0), color: Color::BLACK },
        AleCount,
        ChildOf(ale_row),
    ));

    // ── Bottom left: equipment quadrant ──
    let quad = commands
        .spawn((Name::new("HUD Equipment"), Node { left: px(32.0), bottom: px(32.0), width: px(196.0), height: px(196.0), ..absolute() }))
        .id();
    let slot = |commands: &mut Commands, left: f32, top: f32| {
        commands
            .spawn((
                Node {
                    left: px(left),
                    top: px(top),
                    width: px(62.0),
                    height: px(62.0),
                    border: UiRect::all(px(2.0)),
                    border_radius: BorderRadius::all(px(5.0)),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..absolute()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
                BorderColor::all(FRAME),
                ChildOf(quad),
            ))
            .id()
    };
    slot(&mut commands, 67.0, 0.0); // top: spells (none yet)
    let left = slot(&mut commands, 0.0, 67.0);
    let right = slot(&mut commands, 134.0, 67.0);
    let quick = slot(&mut commands, 67.0, 134.0);
    commands.entity(left).insert(HandSlot(EquipSlot::LeftHand));
    commands.entity(right).insert(HandSlot(EquipSlot::RightHand));
    let tankard = spawn_icon(&mut commands, quick, ItemIcon::Tankard, 1.25);
    commands.entity(tankard).insert(AleIcon);
    commands.spawn((
        Text::new("3"),
        TextFont::from_font_size(15.0),
        TextColor(GOLD_TEXT),
        TextShadow { offset: Vec2::splat(1.5), color: Color::BLACK },
        AleCount,
        Node { right: px(4.0), bottom: px(1.0), ..absolute() },
        ChildOf(quick),
    ));
    commands.spawn((
        Text::new(""),
        TextFont::from_font_size(15.0),
        TextColor(GOLD_TEXT),
        TextShadow { offset: Vec2::splat(1.5), color: Color::BLACK },
        WeaponName,
        Node { left: px(204.0), top: px(88.0), ..absolute() },
        ChildOf(quad),
    ));

    // ── Bottom centre: interaction prompt ──
    let prompt = commands
        .spawn((
            Name::new("HUD Prompt"),
            PromptRoot,
            Node { width: Val::Percent(100.0), bottom: Val::Percent(16.0), justify_content: JustifyContent::Center, ..absolute() },
            Visibility::Hidden,
        ))
        .id();
    let panel = commands
        .spawn((
            Node {
                padding: UiRect::axes(px(22.0), px(9.0)),
                border: UiRect::vertical(px(1.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.65)),
            BorderColor::all(FRAME),
            ChildOf(prompt),
        ))
        .id();
    commands.spawn((Text::new(""), TextFont::from_font_size(20.0), TextColor(Color::srgb(0.92, 0.9, 0.84)), PromptText, ChildOf(panel)));

    // ── Bottom centre, above the prompt: "Picked up: X" toast ──
    let toast = commands
        .spawn((
            Name::new("HUD Toast"),
            ToastRoot { t: TOAST_TIME },
            Node { width: Val::Percent(100.0), bottom: Val::Percent(24.0), justify_content: JustifyContent::Center, ..absolute() },
            Visibility::Hidden,
        ))
        .id();
    let panel = commands
        .spawn((
            Node {
                padding: UiRect::new(px(10.0), px(24.0), px(4.0), px(4.0)),
                border: UiRect::vertical(px(1.0)),
                align_items: AlignItems::Center,
                column_gap: px(8.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)),
            BorderColor::all(FRAME),
            ChildOf(toast),
        ))
        .id();
    commands.spawn((ToastIcon, Node { width: px(44.0), height: px(44.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }, ChildOf(panel)));
    commands.spawn((Text::new(""), TextFont::from_font_size(19.0), TextColor(Color::srgb(0.92, 0.9, 0.84)), ToastText, ChildOf(panel)));

    // ── Centre: banner ──
    let banner = commands
        .spawn((
            Name::new("HUD Banner"),
            BannerRoot { t: BANNER_TIME },
            Node {
                width: Val::Percent(100.0),
                height: px(130.0),
                top: Val::Percent(38.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..absolute()
            },
            BackgroundColor(Color::NONE),
        ))
        .id();
    commands.spawn((
        Text::new(""),
        TextFont { font: display.clone().into(), font_size: FontSize::Px(64.0), ..default() },
        TextColor(Color::NONE),
        BannerText,
        ChildOf(banner),
    ));

    // ── Death screen: band + text, and a full-screen black fade above everything ──
    let band = commands
        .spawn((
            Name::new("Death Screen"),
            DeathBand,
            Node {
                width: Val::Percent(100.0),
                height: px(170.0),
                top: Val::Percent(50.0),
                margin: UiRect::top(px(-85.0)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..absolute()
            },
            BackgroundColor(Color::NONE),
            GlobalZIndex(10),
        ))
        .id();
    commands.spawn((
        Text::new(super::strings::DEATH),
        TextFont { font: display.into(), font_size: FontSize::Px(118.0), ..default() },
        TextColor(Color::NONE),
        DeathText,
        UiTransform::IDENTITY,
        ChildOf(band),
    ));
    commands.spawn((
        Name::new("Black Fade"),
        BlackFade,
        Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..absolute() },
        BackgroundColor(Color::NONE),
        GlobalZIndex(20),
    ));
}

fn update_bars(
    time: Res<Time>,
    player: Query<(&Health, &Stamina), With<Player>>,
    mut nodes: ParamSet<(
        Query<&mut Node, With<HpFrame>>,
        Query<&mut Node, With<HpFill>>,
        Query<(&mut Node, &mut HpLag)>,
        Query<&mut Node, With<StaminaFrame>>,
        Query<&mut Node, With<StaminaFill>>,
    )>,
) {
    let Ok((hp, st)) = player.single() else { return };
    let dt = time.delta_secs();
    let hp_frac = (hp.current / hp.max).clamp(0.0, 1.0);
    for mut n in &mut nodes.p0() {
        n.width = px(hp.max * HP_BAR_PX_PER_POINT);
    }
    for mut n in &mut nodes.p1() {
        n.width = Val::Percent(hp_frac * 100.0);
    }
    for (mut n, mut lag) in &mut nodes.p2() {
        // The chunk of HP just lost lingers, then drains; heals pull it up instantly.
        if hp_frac >= lag.value {
            lag.value = hp_frac;
            lag.hold = LAG_HOLD;
        } else if lag.hold > 0.0 {
            lag.hold -= dt;
        } else {
            lag.value = (lag.value - LAG_DRAIN * dt).max(hp_frac);
        }
        n.width = Val::Percent(lag.value * 100.0);
    }
    for mut n in &mut nodes.p3() {
        n.width = px(st.max * STAMINA_BAR_PX_PER_POINT);
    }
    for mut n in &mut nodes.p4() {
        n.width = Val::Percent((st.current / st.max).clamp(0.0, 1.0) * 100.0);
    }
}

fn update_ale(
    player: Query<&HealingAle, (With<Player>, Changed<HealingAle>)>,
    mut texts: Query<(&mut Text, &mut TextColor), With<AleCount>>,
    icons: Query<&Children, With<AleIcon>>,
    mut colors: Query<(Option<&mut BackgroundColor>, Option<&mut BorderColor>)>,
    children: Query<&Children>,
) {
    let Ok(ale) = player.single() else { return };
    for (mut text, mut color) in &mut texts {
        text.0 = ale.charges.to_string();
        color.0 = if ale.charges == 0 { Color::srgb(0.5, 0.45, 0.4) } else { GOLD_TEXT };
    }
    // Empty: grey the tankard out.
    let alpha = if ale.charges == 0 { 0.3 } else { 1.0 };
    for icon_children in &icons {
        let mut stack: Vec<Entity> = icon_children.iter().collect();
        while let Some(e) = stack.pop() {
            if let Ok((bg, border)) = colors.get_mut(e) {
                if let Some(mut bg) = bg {
                    bg.0.set_alpha(alpha);
                }
                if let Some(mut border) = border {
                    let c = border.top.with_alpha(alpha);
                    border.set_all(c);
                }
            }
            if let Ok(c) = children.get(e) {
                stack.extend(c.iter());
            }
        }
    }
}

/// Hand slot icons and the weapon name follow the equipment (bare fist when the right hand is empty).
fn update_hands(
    mut commands: Commands,
    player: Query<(&Inventory, &Equipment), (With<Player>, Changed<Equipment>)>,
    slots: Query<(Entity, &HandSlot)>,
    mut text: Query<&mut Text, With<WeaponName>>,
) {
    let Ok((inventory, equipment)) = player.single() else { return };
    for (entity, slot) in &slots {
        commands.entity(entity).despawn_children();
        let icon = match (slot.0, equipment.item(slot.0, inventory)) {
            (_, Some(item)) => Some(item.def().icon),
            (EquipSlot::RightHand, None) => Some(ItemIcon::Fist),
            _ => None,
        };
        if let Some(icon) = icon {
            spawn_icon(&mut commands, entity, icon, 1.0);
        }
    }
    for mut t in &mut text {
        t.0 = equipment.weapon(inventory).name.to_string();
    }
}

fn update_toast(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut picked: MessageReader<ItemPickedUp>,
    mut root: Query<(&mut ToastRoot, &mut Visibility)>,
    icon: Query<Entity, With<ToastIcon>>,
    mut text: Query<&mut Text, With<ToastText>>,
) {
    let Ok((mut toast, mut vis)) = root.single_mut() else { return };
    if let Some(ItemPickedUp { item }) = picked.read().last() {
        toast.t = 0.0;
        for e in &icon {
            commands.entity(e).despawn_children();
            let i = spawn_icon(&mut commands, e, item.def().icon, 0.75);
            commands.entity(i).insert(Node { width: px(42.0), height: px(42.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() });
        }
        for mut t in &mut text {
            t.0 = format!("{}: {}", strings::PICKED_UP, item.def().name);
        }
    }
    toast.t = (toast.t + time.delta_secs()).min(TOAST_TIME);
    vis.set_if_neq(if toast.t < TOAST_TIME { Visibility::Inherited } else { Visibility::Hidden });
}

fn update_prompt(
    prompt: Res<InteractPrompt>,
    mut root: Query<&mut Visibility, With<PromptRoot>>,
    mut text: Query<&mut Text, With<PromptText>>,
) {
    for mut v in &mut root {
        v.set_if_neq(if prompt.0.is_some() { Visibility::Inherited } else { Visibility::Hidden });
    }
    if let Some(p) = prompt.0 {
        for mut t in &mut text {
            if t.0 != p {
                t.0 = p.to_string();
            }
        }
    }
}

/// Alpha of a banner `t` seconds after it appeared.
fn banner_alpha(t: f32) -> f32 {
    let fade_in = (t / 0.5).clamp(0.0, 1.0);
    let fade_out = ((BANNER_TIME - t) / 1.0).clamp(0.0, 1.0);
    fade_in.min(fade_out)
}

fn update_banner(
    time: Res<Time<Real>>,
    mut banners: MessageReader<Banner>,
    mut root: Query<(&mut BannerRoot, &mut BackgroundColor)>,
    mut text: Query<(&mut Text, &mut TextColor), With<BannerText>>,
) {
    let Ok((mut root, mut band)) = root.single_mut() else { return };
    let Ok((mut text, mut color)) = text.single_mut() else { return };
    if let Some(Banner(msg)) = banners.read().last() {
        text.0 = msg.to_string();
        root.t = 0.0;
    }
    root.t = (root.t + time.delta_secs()).min(BANNER_TIME);
    let a = banner_alpha(root.t);
    color.0 = GOLD_TEXT.with_alpha(a);
    band.0 = Color::srgba(0.0, 0.0, 0.0, 0.5 * a);
}

fn update_death_screen(
    player: Query<Option<&DeathSequence>, With<Player>>,
    mut band: Query<&mut BackgroundColor, (With<DeathBand>, Without<BlackFade>)>,
    mut text: Query<(&mut TextColor, &mut UiTransform), With<DeathText>>,
    mut black: Query<&mut BackgroundColor, (With<BlackFade>, Without<DeathBand>)>,
) {
    let Ok(seq) = player.single() else { return };
    let t = seq.map(|s| s.t);
    let (text_a, black_a) = t.map_or((0.0, 0.0), death_screen);
    for mut b in &mut band {
        b.0 = Color::srgba(0.0, 0.0, 0.0, 0.7 * text_a);
    }
    for (mut c, mut tf) in &mut text {
        c.0 = Color::srgba(0.62, 0.07, 0.05, text_a);
        // The words slowly swell while they're shown.
        tf.scale = Vec2::splat(1.0 + 0.06 * t.unwrap_or(0.0).min(5.0) / 5.0);
    }
    for mut b in &mut black {
        b.0 = Color::srgba(0.0, 0.0, 0.0, black_a);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn banner_fades_in_holds_and_out() {
        assert_eq!(banner_alpha(0.0), 0.0);
        assert_eq!(banner_alpha(1.5), 1.0);
        assert_eq!(banner_alpha(BANNER_TIME), 0.0);
    }
}
