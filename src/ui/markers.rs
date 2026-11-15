//! Screen-space markers that follow things in the world: the lock-on dot on the target, the
//! health bars over enemies (shown once hurt, or while locked on; hidden when dead or off-screen)
//! and, online, a health bar over every other player's head that shows for `PLAYER_BAR_SHOW`
//! seconds whenever their HP changes (hurt or healed), then fades away.

use bevy::prelude::*;
use bevy::transform::TransformSystems;

use super::hud::FRAME;
use crate::character::Remote;
use crate::character::stats::Health;
use crate::combat::Team;
use crate::enemy::Enemy;
use crate::lockon::{LockOn, Targetable};
use crate::player::Player;

pub struct MarkersPlugin;

impl Plugin for MarkersPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_lock_marker)
            .add_systems(
                PostUpdate,
                (spawn_enemy_bars, update_lock_marker, update_enemy_bars, spawn_player_bars, update_player_bars).after(TransformSystems::Propagate),
            );
    }
}

const BAR_WIDTH: f32 = 84.0;
/// Bar height above the enemy's origin (the capsule centre), metres.
const BAR_HEIGHT: f32 = 1.25;

#[derive(Component)]
struct LockMarker;

/// Health bar of `0` (an enemy entity).
#[derive(Component)]
struct EnemyBar(Entity);
#[derive(Component)]
struct EnemyBarFill;

fn spawn_lock_marker(mut commands: Commands) {
    commands.spawn((
        Name::new("Lock-on Marker"),
        LockMarker,
        Node {
            position_type: PositionType::Absolute,
            width: Val::Px(14.0),
            height: Val::Px(14.0),
            border: UiRect::all(Val::Px(2.0)),
            border_radius: BorderRadius::MAX,
            ..default()
        },
        BackgroundColor(Color::srgba(1.0, 0.95, 0.85, 0.85)),
        BorderColor::all(Color::srgba(0.2, 0.1, 0.05, 0.9)),
        Visibility::Hidden,
    ));
}

/// World point → UI position (logical px), if on screen and in front of the camera.
fn to_screen(camera: &Camera, cam_tf: &GlobalTransform, world: Vec3) -> Option<Vec2> {
    camera.world_to_viewport(cam_tf, world).ok()
}

fn update_lock_marker(
    cams: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    player: Query<&LockOn, With<Player>>,
    targets: Query<(&GlobalTransform, &Targetable)>,
    mut marker: Query<(&mut Node, &mut Visibility), With<LockMarker>>,
) {
    let Ok((mut node, mut vis)) = marker.single_mut() else { return };
    let Ok((camera, cam_tf)) = cams.single() else { return };
    let spot = player
        .single()
        .ok()
        .and_then(|l| l.target)
        .and_then(|t| targets.get(t).ok())
        .and_then(|(tf, t)| to_screen(camera, cam_tf, tf.translation() + Vec3::Y * t.height));
    match spot {
        Some(p) => {
            node.left = Val::Px(p.x - 7.0);
            node.top = Val::Px(p.y - 7.0);
            vis.set_if_neq(Visibility::Inherited);
        }
        None => {
            vis.set_if_neq(Visibility::Hidden);
        }
    }
}

fn spawn_enemy_bars(mut commands: Commands, enemies: Query<Entity, Added<Enemy>>) {
    for enemy in &enemies {
        let bar = commands
            .spawn((
                Name::new("Enemy HP Bar"),
                EnemyBar(enemy),
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Px(BAR_WIDTH),
                    height: Val::Px(7.0),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)),
                BorderColor::all(FRAME),
                Visibility::Hidden,
            ))
            .id();
        commands.spawn((
            EnemyBarFill,
            Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() },
            BackgroundColor(Color::srgb(0.62, 0.08, 0.06)),
            ChildOf(bar),
        ));
    }
}

#[allow(clippy::type_complexity)]
fn update_enemy_bars(
    mut commands: Commands,
    cams: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    player: Query<&LockOn, With<Player>>,
    enemies: Query<(&GlobalTransform, &Health), With<Enemy>>,
    mut bars: Query<(Entity, &EnemyBar, &mut Node, &mut Visibility, &Children), Without<EnemyBarFill>>,
    mut fills: Query<&mut Node, With<EnemyBarFill>>,
) {
    let Ok((camera, cam_tf)) = cams.single() else { return };
    let locked = player.single().ok().and_then(|l| l.target);
    for (bar, EnemyBar(enemy), mut node, mut vis, children) in &mut bars {
        let Ok((tf, health)) = enemies.get(*enemy) else {
            commands.entity(bar).despawn();
            continue;
        };
        let wanted = !health.is_dead() && (health.current < health.max || locked == Some(*enemy));
        let spot = wanted.then(|| to_screen(camera, cam_tf, tf.translation() + Vec3::Y * BAR_HEIGHT)).flatten();
        let Some(p) = spot else {
            vis.set_if_neq(Visibility::Hidden);
            continue;
        };
        vis.set_if_neq(Visibility::Inherited);
        node.left = Val::Px(p.x - BAR_WIDTH * 0.5);
        node.top = Val::Px(p.y);
        for child in children.iter() {
            if let Ok(mut fill) = fills.get_mut(child) {
                fill.width = Val::Percent((health.current / health.max).clamp(0.0, 1.0) * 100.0);
            }
        }
    }
}

// ── Other players' health bars (online) ──────────────────────────────────

/// Seconds a player's bar stays after their HP changed, then how long it takes to fade.
const PLAYER_BAR_SHOW: f32 = 3.0;
const PLAYER_BAR_FADE: f32 = 0.6;
const PLAYER_BAR_WIDTH: f32 = 70.0;
/// Above the head (metres above the capsule centre).
const PLAYER_BAR_HEIGHT: f32 = 1.2;

/// Health bar over another player (`player`, a remote puppet).
#[derive(Component)]
struct PlayerBar {
    player: Entity,
    last_hp: f32,
    /// Seconds since the HP last changed.
    since: f32,
}
#[derive(Component)]
struct PlayerBarFill;

fn spawn_player_bars(mut commands: Commands, players: Query<(Entity, &Health, Option<&Team>), Added<Remote>>) {
    for (player, health, team) in &players {
        if team != Some(&Team::Player) {
            continue;
        }
        let bar = commands
            .spawn((
                Name::new("Player HP Bar"),
                PlayerBar { player, last_hp: health.current, since: f32::INFINITY },
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Px(PLAYER_BAR_WIDTH),
                    height: Val::Px(6.0),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(Color::NONE),
                BorderColor::all(Color::NONE),
                Visibility::Hidden,
            ))
            .id();
        commands.spawn((
            PlayerBarFill,
            Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() },
            BackgroundColor(Color::NONE),
            ChildOf(bar),
        ));
    }
}

/// Opacity of a player's bar `since` seconds after their HP changed.
fn player_bar_alpha(since: f32) -> f32 {
    if since <= PLAYER_BAR_SHOW {
        1.0
    } else if since >= PLAYER_BAR_SHOW + PLAYER_BAR_FADE {
        0.0
    } else {
        1.0 - (since - PLAYER_BAR_SHOW) / PLAYER_BAR_FADE
    }
}

#[allow(clippy::type_complexity)]
fn update_player_bars(
    time: Res<Time>,
    mut commands: Commands,
    cams: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    players: Query<(&GlobalTransform, &Health)>,
    mut bars: Query<(Entity, &mut PlayerBar, &mut Node, &mut Visibility, &mut BackgroundColor, &mut BorderColor, &Children), Without<PlayerBarFill>>,
    mut fills: Query<(&mut Node, &mut BackgroundColor), With<PlayerBarFill>>,
) {
    let Ok((camera, cam_tf)) = cams.single() else { return };
    for (bar_entity, mut bar, mut node, mut vis, mut bg, mut border, children) in &mut bars {
        let Ok((tf, health)) = players.get(bar.player) else {
            commands.entity(bar_entity).despawn(); // the player left
            continue;
        };
        if health.current != bar.last_hp {
            bar.last_hp = health.current;
            bar.since = 0.0;
        } else {
            bar.since += time.delta_secs();
        }
        let alpha = player_bar_alpha(bar.since);
        let spot = (alpha > 0.0).then(|| to_screen(camera, cam_tf, tf.translation() + Vec3::Y * PLAYER_BAR_HEIGHT)).flatten();
        let Some(p) = spot else {
            vis.set_if_neq(Visibility::Hidden);
            continue;
        };
        vis.set_if_neq(Visibility::Inherited);
        node.left = Val::Px(p.x - PLAYER_BAR_WIDTH * 0.5);
        node.top = Val::Px(p.y);
        bg.0 = Color::srgba(0.0, 0.0, 0.0, 0.7 * alpha);
        *border = BorderColor::all(FRAME.with_alpha(alpha));
        for child in children.iter() {
            if let Ok((mut fill, mut color)) = fills.get_mut(child) {
                fill.width = Val::Percent((health.current / health.max).clamp(0.0, 1.0) * 100.0);
                color.0 = Color::srgba(0.75, 0.12, 0.08, alpha);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_bars_show_then_fade() {
        assert_eq!(player_bar_alpha(0.0), 1.0);
        assert_eq!(player_bar_alpha(PLAYER_BAR_SHOW), 1.0, "fully shown for the whole three seconds");
        let mid = player_bar_alpha(PLAYER_BAR_SHOW + PLAYER_BAR_FADE * 0.5);
        assert!(mid > 0.4 && mid < 0.6, "{mid}");
        assert_eq!(player_bar_alpha(PLAYER_BAR_SHOW + PLAYER_BAR_FADE), 0.0);
        assert_eq!(player_bar_alpha(f32::INFINITY), 0.0, "hidden until the HP changes");
    }
}
