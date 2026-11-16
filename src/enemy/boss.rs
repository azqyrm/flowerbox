//! Boss fights, the Souls way:
//! - **Fog gate** ([`FogGate`], placed by a level as a prop): a wall of glowing mist across the
//!   arena's entrance. From outside, E steps you through; from inside there is no way back while
//!   the boss lives. Gone once it's defeated.
//! - **Boss bar**: the boss's name and a long health bar at the bottom of the screen while you're
//!   near it (with the lagging "damage chunk" like the player's bar).
//! - **Defeat**: a banner for everyone who sees it fall; it stays dead for good
//!   ([`DefeatedBosses`], saved; resting doesn't bring it back; online the host's list counts and
//!   joiners get it).

use avian3d::prelude::*;
use bevy::prelude::*;

use super::kinds::Boss;
use super::{Enemy, EnemyKind};
use crate::character::action::{ActionKind, CharacterAction};
use crate::character::controller::CharacterController;
use crate::character::stats::Health;
use crate::hearth::DeathSequence;
use crate::input::{GameplayBlocked, PlayerInput};
use crate::interact::{InteractPrompt, InteractSystems};
use crate::player::Player;
use crate::tuning::*;
use crate::ui::hud::{FRAME, GOLD_TEXT};
use crate::ui::{Banner, DISPLAY_FONT, strings};

pub struct BossPlugin;

impl Plugin for BossPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DefeatedBosses>()
            .add_systems(Startup, spawn_boss_bar)
            .add_systems(Update, (boss_deaths, lift_fog, fog_interaction.in_set(InteractSystems::Doors), animate_fog, boss_bar).chain());
    }
}

/// Bosses defeated in this world (saved): they don't respawn.
#[derive(Resource, Debug, Default, Clone, PartialEq)]
pub struct DefeatedBosses(pub Vec<EnemyKind>);

/// A wall of mist across a boss arena's entrance. `into` points into the arena.
#[derive(Component, Debug)]
pub struct FogGate {
    pub boss: EnemyKind,
    pub into: Vec3,
    pub half_width: f32,
}

/// A boss fell: banner, and remember it.
fn boss_deaths(
    mut defeated: ResMut<DefeatedBosses>,
    mut banner: MessageWriter<Banner>,
    bosses: Query<(&Enemy, &CharacterAction), (With<Boss>, Changed<CharacterAction>)>,
) {
    for (enemy, action) in &bosses {
        if matches!(action.current, Some(a) if a.kind == ActionKind::Dead) && !defeated.0.contains(&enemy.kind) {
            defeated.0.push(enemy.kind);
            banner.write(Banner(strings::BOSS_DEFEATED));
            info!("{} defeated", enemy.kind.def().name);
        }
    }
}

/// The gate's mist clears once its boss is defeated.
fn lift_fog(mut commands: Commands, defeated: Res<DefeatedBosses>, gates: Query<(Entity, &FogGate)>) {
    if !defeated.is_changed() {
        return;
    }
    for (e, gate) in &gates {
        if defeated.0.contains(&gate.boss) {
            commands.entity(e).despawn();
            info!("the mist clears");
        }
    }
}

/// From outside: E steps through the mist (into the arena). From inside: nothing (it holds you in).
#[allow(clippy::type_complexity)]
fn fog_interaction(
    mut prompt: ResMut<InteractPrompt>,
    blocked: Res<GameplayBlocked>,
    gates: Query<(&FogGate, &GlobalTransform)>,
    mut player: Query<(&mut Transform, &CharacterAction, &mut CharacterController, &mut PlayerInput, Has<DeathSequence>), With<Player>>,
) {
    let Ok((mut tf, action, mut ctrl, mut input, dying)) = player.single_mut() else { return };
    if prompt.0.is_some() || blocked.0 || dying || action.current.is_some() {
        return;
    }
    for (gate, gtf) in &gates {
        let c = gtf.translation();
        let d = tf.translation - c;
        let across = Vec3::new(-gate.into.z, 0.0, gate.into.x);
        let outside = d.dot(gate.into) < 0.0;
        if !outside || d.with_y(0.0).length() > FOG_RANGE + gate.half_width || d.dot(across).abs() > gate.half_width || d.y.abs() > 2.5 {
            continue;
        }
        prompt.0 = Some(strings::PROMPT_FOG);
        if std::mem::take(&mut input.interact) {
            tf.translation = Vec3::new(c.x, tf.translation.y, c.z) + gate.into * FOG_STEP;
            ctrl.facing = f32::atan2(-gate.into.x, -gate.into.z);
            ctrl.velocity = Vec3::ZERO;
            info!("through the mist");
        }
        return;
    }
}

/// Spawns a fog gate across an opening `width` wide at `centre` (floor point).
pub fn spawn_fog_gate(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    boss: EnemyKind,
    centre: Vec3,
    into: Vec3,
    width: f32,
    height: f32,
) {
    let yaw = f32::atan2(into.x, into.z);
    let mist = materials.add(StandardMaterial {
        base_color: Color::srgba(0.7, 0.78, 0.95, 0.1),
        emissive: LinearRgba::new(0.12, 0.16, 0.32, 0.1),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    let gate = commands
        .spawn((
            Name::new("Fog gate"),
            FogGate { boss, into, half_width: width * 0.5 },
            Transform::from_translation(centre).with_rotation(Quat::from_rotation_y(yaw)),
            Visibility::default(),
        ))
        .id();
    // The blocker: a thin wall across the opening.
    commands.spawn((RigidBody::Static, Collider::cuboid(width, height, 0.3), Transform::from_xyz(0.0, height * 0.5, 0.0), ChildOf(gate)));
    // Three drifting sheets of mist.
    for (i, z) in [-0.12, 0.0, 0.12].into_iter().enumerate() {
        commands.spawn((
            FogSheet(i as f32 * 2.1),
            Mesh3d(meshes.add(Rectangle::new(width, height))),
            MeshMaterial3d(mist.clone()),
            Transform::from_xyz(0.0, height * 0.5, z),
            ChildOf(gate),
        ));
    }
    commands.spawn((
        PointLight { color: Color::srgb(0.7, 0.8, 1.0), intensity: 120_000.0, range: 7.0, ..default() },
        Transform::from_xyz(0.0, 1.2, -0.6),
        ChildOf(gate),
    ));
}

#[derive(Component)]
struct FogSheet(f32);

fn animate_fog(time: Res<Time>, mut sheets: Query<(&FogSheet, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (sheet, mut tf) in &mut sheets {
        let w = (t * 0.7 + sheet.0).sin();
        tf.scale = Vec3::new(1.0 + 0.03 * w, 1.0 + 0.02 * (t * 0.9 + sheet.0).cos(), 1.0);
        tf.translation.x = 0.08 * w;
    }
}

// ── The boss bar ─────────────────────────────────────────────────────────

#[derive(Component)]
struct BossBar;
#[derive(Component)]
struct BossName;
#[derive(Component)]
struct BossFill;
#[derive(Component)]
struct BossChunk;

const BAR_WIDTH: f32 = 720.0;

fn spawn_boss_bar(mut commands: Commands, assets: Res<AssetServer>) {
    let root = commands
        .spawn((
            Name::new("Boss bar"),
            BossBar,
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(70.0),
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(4.0),
                ..default()
            },
            Visibility::Hidden,
        ))
        .id();
    commands.spawn((
        BossName,
        Text::new(""),
        TextFont { font: assets.load(DISPLAY_FONT).into(), font_size: FontSize::Px(24.0), ..default() },
        TextColor(GOLD_TEXT),
        TextShadow::default(),
        ChildOf(root),
    ));
    let frame = commands
        .spawn((
            Node { width: Val::Px(BAR_WIDTH), height: Val::Px(12.0), border: UiRect::all(Val::Px(1.0)), ..default() },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
            BorderColor::all(FRAME),
            ChildOf(root),
        ))
        .id();
    commands.spawn((
        BossChunk,
        Node { position_type: PositionType::Absolute, height: Val::Percent(100.0), width: Val::Percent(100.0), ..default() },
        BackgroundColor(Color::srgb(0.85, 0.62, 0.25)),
        ChildOf(frame),
    ));
    commands.spawn((
        BossFill,
        Node { position_type: PositionType::Absolute, height: Val::Percent(100.0), width: Val::Percent(100.0), ..default() },
        BackgroundColor(Color::srgb(0.55, 0.06, 0.05)),
        ChildOf(frame),
    ));
}

/// Shows the nearest living boss within `BOSS_BAR_RANGE` of the player (same floor).
#[allow(clippy::type_complexity)]
fn boss_bar(
    time: Res<Time>,
    mut chunk_frac: Local<f32>,
    player: Query<&Transform, With<Player>>,
    bosses: Query<(&Boss, &Health, &Transform)>,
    mut bar: Query<&mut Visibility, With<BossBar>>,
    mut name: Query<&mut Text, With<BossName>>,
    mut fill: Query<&mut Node, (With<BossFill>, Without<BossChunk>)>,
    mut chunk: Query<&mut Node, (With<BossChunk>, Without<BossFill>)>,
) {
    let Ok(ptf) = player.single() else { return };
    let near = bosses
        .iter()
        .filter(|(_, h, tf)| !h.is_dead() && tf.translation.distance(ptf.translation) < BOSS_BAR_RANGE && (tf.translation.y - ptf.translation.y).abs() < 5.0)
        .min_by(|a, b| a.2.translation.distance(ptf.translation).total_cmp(&b.2.translation.distance(ptf.translation)));
    let Ok(mut vis) = bar.single_mut() else { return };
    let Some((boss, health, _)) = near else {
        *vis = Visibility::Hidden;
        return;
    };
    *vis = Visibility::Inherited;
    if let Ok(mut t) = name.single_mut()
        && t.0 != boss.name {
            t.0 = boss.name.to_string();
        }
    let frac = (health.current / health.max).clamp(0.0, 1.0);
    *chunk_frac = if *chunk_frac < frac { frac } else { (*chunk_frac - 0.25 * time.delta_secs()).max(frac) };
    if let Ok(mut n) = fill.single_mut() {
        n.width = Val::Percent(frac * 100.0);
    }
    if let Ok(mut n) = chunk.single_mut() {
        n.width = Val::Percent(*chunk_frac * 100.0);
    }
}
