//! The Light counter, bottom right (Souls' rune / soul count): a small glowing sun and the amount.
//! A gain shows "+N" above it for a while (gains in quick succession add up) and the number counts
//! up to the new total.

use bevy::prelude::*;

use super::hud::{FRAME, GOLD_TEXT};
use crate::light::{LightGained, LightPurse};
use crate::player::Player;

pub struct LightCounterPlugin;

impl Plugin for LightCounterPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_counter).add_systems(Update, update_counter.after(crate::light::LightSystems));
    }
}

/// How long "+N" stays, then fades (s).
const GAIN_SHOW: f32 = 2.5;
const GAIN_FADE: f32 = 0.6;
/// The shown number catches up in about this long (s).
const COUNT_UP: f32 = 0.8;

#[derive(Component)]
struct LightAmount;
#[derive(Component)]
struct LightGain;

/// The counter's animation state.
#[derive(Resource, Default)]
struct Shown {
    value: f32,
    gain: u32,
    since_gain: f32,
}

fn spawn_counter(mut commands: Commands, assets: Res<AssetServer>) {
    let font: Handle<Font> = assets.load(super::DISPLAY_FONT);
    commands.init_resource::<Shown>();
    let root = commands
        .spawn((
            Name::new("HUD Light"),
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(32.0),
                bottom: Val::Px(32.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::FlexEnd,
                row_gap: Val::Px(2.0),
                ..default()
            },
        ))
        .id();
    commands.spawn((
        LightGain,
        Text::new(""),
        TextFont { font: font.clone().into(), font_size: FontSize::Px(18.0), ..default() },
        TextColor(GOLD_TEXT.with_alpha(0.0)),
        TextShadow { offset: Vec2::splat(2.0), color: Color::BLACK },
        ChildOf(root),
    ));
    let row = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(10.0),
                padding: UiRect::axes(Val::Px(12.0), Val::Px(5.0)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                min_width: Val::Px(130.0),
                justify_content: JustifyContent::SpaceBetween,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
            BorderColor::all(FRAME),
            ChildOf(root),
        ))
        .id();
    // The sun: a warm disk in a fainter halo.
    let halo = commands
        .spawn((
            Node {
                width: Val::Px(20.0),
                height: Val::Px(20.0),
                border_radius: BorderRadius::MAX,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(1.0, 0.8, 0.35, 0.3)),
            ChildOf(row),
        ))
        .id();
    commands.spawn((
        Node { width: Val::Px(11.0), height: Val::Px(11.0), border_radius: BorderRadius::MAX, ..default() },
        BackgroundColor(Color::srgb(1.0, 0.9, 0.6)),
        ChildOf(halo),
    ));
    commands.spawn((
        LightAmount,
        Text::new("0"),
        TextFont { font: font.into(), font_size: FontSize::Px(20.0), ..default() },
        TextColor(GOLD_TEXT),
        TextShadow { offset: Vec2::splat(2.0), color: Color::BLACK },
        ChildOf(row),
    ));
}

fn update_counter(
    time: Res<Time>,
    mut shown: ResMut<Shown>,
    mut gains: MessageReader<LightGained>,
    purse: Query<&LightPurse, With<Player>>,
    mut amount: Query<&mut Text, (With<LightAmount>, Without<LightGain>)>,
    mut gain: Query<(&mut Text, &mut TextColor), (With<LightGain>, Without<LightAmount>)>,
) {
    let dt = time.delta_secs();
    let total: u32 = gains.read().map(|g| g.amount).sum();
    if total > 0 {
        // Gains close together add up into one "+N".
        shown.gain = if shown.since_gain < GAIN_SHOW { shown.gain + total } else { total };
        shown.since_gain = 0.0;
    } else {
        shown.since_gain += dt;
    }
    let target = purse.single().map_or(0, |p| p.0) as f32;
    // Count up toward the purse (a load or a spend snaps).
    if target < shown.value || (target - shown.value) < 1.0 {
        shown.value = target;
    } else {
        let rate = ((target - shown.value) / COUNT_UP).max(30.0);
        shown.value = (shown.value + rate * dt).min(target);
    }
    if let Ok(mut t) = amount.single_mut() {
        let s = format!("{}", shown.value.round() as u32);
        if t.0 != s {
            t.0 = s;
        }
    }
    if let Ok((mut t, mut color)) = gain.single_mut() {
        let alpha = if shown.gain == 0 { 0.0 } else { (1.0 - (shown.since_gain - GAIN_SHOW) / GAIN_FADE).clamp(0.0, 1.0) };
        let s = format!("+{}", shown.gain);
        if t.0 != s {
            t.0 = s;
        }
        color.0 = GOLD_TEXT.with_alpha(alpha);
    }
}
