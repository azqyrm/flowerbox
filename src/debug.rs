//! Dev helpers for manual and automated checks (see ROADMAP notes N5, N7, N9).
//!
//! - **F3**: toggle the debug overlay (action state, timers, buffer, stats).
//! - Debug builds only: **H** = take 15 damage; **1–4** = request Roll (backstep when standing still),
//!   Light (jump attack when airborne), Heavy, Drink (until Phases 4–6 bind the real inputs).
//! - `FLOWERBOX_SCREENSHOT=/path/shot.png` → saves a screenshot after a warm-up, then exits.
//!   `FLOWERBOX_SHOT_FRAME=n` changes the capture frame (default 120). `FLOWERBOX_OVERLAY=1` shows the overlay at start.
//! - `FLOWERBOX_AUTOPILOT=1` → walks forward, jumps every 2 s and runs a scripted action/damage
//!   timeline (see `autopilot_script`); position and action changes are logged.

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use bevy::time::common_conditions::on_timer;

use crate::input::{Autopilot, PlayerInput};
use crate::player::Player;
use crate::player::action::{ActionKind, ActionRequest, PlayerAction};
use crate::player::controller::CharacterController;
use crate::player::stats::{DamageMessage, Health, Invulnerable, Stamina};

pub struct DebugPlugin;

impl Plugin for DebugPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_overlay)
            .add_systems(Update, (toggle_overlay, update_overlay, log_action_changes));
        if cfg!(debug_assertions) {
            app.add_systems(Update, debug_keys);
        }
        if let Ok(path) = std::env::var("FLOWERBOX_SCREENSHOT") {
            let at = std::env::var("FLOWERBOX_SHOT_FRAME").ok().and_then(|s| s.parse().ok()).unwrap_or(120);
            app.insert_resource(AutoScreenshot { path, frame: 0, at })
                .add_systems(Update, auto_screenshot);
        }
        if std::env::var("FLOWERBOX_AUTOPILOT").is_ok() {
            app.insert_resource(Autopilot)
                .init_resource::<AirborneTicks>()
                .add_systems(FixedLast, count_airborne)
                .add_systems(
                    Update,
                    (log_player.run_if(on_timer(std::time::Duration::from_millis(500))), autopilot_script),
                );
        }
    }
}

// ── Overlay ──────────────────────────────────────────────────────────────

#[derive(Component)]
struct DebugOverlay;

fn spawn_overlay(mut commands: Commands) {
    let visible = std::env::var("FLOWERBOX_OVERLAY").is_ok();
    commands.spawn((
        Name::new("Debug Overlay"),
        DebugOverlay,
        Text::new(""),
        TextFont::from_font_size(15.0),
        TextColor(Color::srgb(0.85, 0.95, 0.85)),
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(10.0),
            right: Val::Px(10.0),
            padding: UiRect::all(Val::Px(8.0)),
            ..default()
        },
        if visible { Visibility::Visible } else { Visibility::Hidden },
    ));
}

fn toggle_overlay(keys: Res<ButtonInput<KeyCode>>, mut q: Query<&mut Visibility, With<DebugOverlay>>) {
    if keys.just_pressed(KeyCode::F3) {
        for mut v in &mut q {
            *v = if *v == Visibility::Hidden { Visibility::Visible } else { Visibility::Hidden };
        }
    }
}

fn update_overlay(
    player: Query<(&PlayerAction, &PlayerInput, &Health, &Stamina, &CharacterController, Has<Invulnerable>), With<Player>>,
    mut text: Query<(&mut Text, &Visibility), With<DebugOverlay>>,
) {
    let Ok((action, input, hp, st, ctrl, invuln)) = player.single() else { return };
    for (mut text, vis) in &mut text {
        if *vis == Visibility::Hidden {
            continue;
        }
        let action_line = match &action.current {
            Some(a) => format!(
                "action: {:?}  {:?}\n  {:.2} / {:.2}s  cancel@{:.2}",
                a.kind,
                a.phase(),
                a.elapsed,
                a.def.total(),
                a.def.cancel
            ),
            None => {
                let loco = if !ctrl.grounded {
                    "Airborne"
                } else if ctrl.sprinting {
                    "Sprint"
                } else if Vec2::new(ctrl.velocity.x, ctrl.velocity.z).length() > 0.2 {
                    "Walk"
                } else {
                    "Idle"
                };
                format!("action: - ({loco})")
            }
        };
        let buffer = match input.buffered {
            Some(r) => format!("{r:?} ({:.2}s)", input.buffer_left),
            None => "-".into(),
        };
        text.0 = format!(
            "[F3] DEBUG\n{action_line}\nbuffer: {buffer}\nHP {:.0}/{:.0}   ST {:.0}/{:.0}{}\n{}grounded {}  speed {:.1} m/s",
            hp.current,
            hp.max,
            st.current,
            st.max,
            if st.regen_delay > 0.0 { format!(" (delay {:.1})", st.regen_delay) } else { String::new() },
            if invuln { "** INVULNERABLE **\n" } else { "" },
            ctrl.grounded,
            Vec2::new(ctrl.velocity.x, ctrl.velocity.z).length(),
        );
    }
}

// ── Debug keys (debug builds only) ───────────────────────────────────────

fn debug_keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut damage: MessageWriter<DamageMessage>,
    mut player: Query<(Entity, &mut PlayerInput), With<Player>>,
) {
    let Ok((entity, mut input)) = player.single_mut() else { return };
    if keys.just_pressed(KeyCode::KeyH) {
        damage.write(DamageMessage { target: entity, amount: 15.0 });
    }
    let requests = [
        (KeyCode::Digit1, ActionRequest::Roll),
        (KeyCode::Digit2, ActionRequest::LightAttack),
        (KeyCode::Digit3, ActionRequest::HeavyAttack),
        (KeyCode::Digit4, ActionRequest::DrinkAle),
    ];
    for (key, request) in requests {
        if keys.just_pressed(key) {
            input.buffer(request);
        }
    }
}

// ── Automated checks ─────────────────────────────────────────────────────

#[derive(Resource)]
struct AutoScreenshot {
    path: String,
    frame: u32,
    at: u32,
}

fn auto_screenshot(mut commands: Commands, mut shot: ResMut<AutoScreenshot>, mut exit: MessageWriter<AppExit>) {
    shot.frame += 1;
    if shot.frame == shot.at {
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(shot.path.clone()));
    } else if shot.frame == shot.at + 60 {
        exit.write(AppExit::Success);
    }
}

/// Timeline that exercises the action system. Expected log (N9): roll → damage ignored →
/// hurt (HP 90) → light combo 0,1,2 → jump attack → death → respawn at spawn with full HP.
fn autopilot_script(
    time: Res<Time>,
    mut last: Local<f32>,
    mut damage: MessageWriter<DamageMessage>,
    mut player: Query<(Entity, &mut PlayerInput), With<Player>>,
) {
    let Ok((entity, mut input)) = player.single_mut() else { return };
    let (prev, now) = (*last, time.elapsed_secs());
    *last = now;
    let crossed = |t: f32| prev < t && now >= t;
    if crossed(1.0) {
        input.buffer(ActionRequest::Roll);
    }
    if crossed(1.2) {
        info!("autopilot: 10 damage during roll");
        damage.write(DamageMessage { target: entity, amount: 10.0 });
    }
    if crossed(2.5) {
        info!("autopilot: 10 damage while free");
        damage.write(DamageMessage { target: entity, amount: 10.0 });
    }
    for t in [2.9, 3.25, 3.6] {
        if crossed(t) {
            input.buffer(ActionRequest::LightAttack);
        }
    }
    // Autopilot jumps at t=6.0; a light attack shortly after becomes a jump attack.
    if crossed(6.15) {
        input.buffer(ActionRequest::LightAttack);
    }
    if crossed(7.5) {
        info!("autopilot: lethal damage");
        damage.write(DamageMessage { target: entity, amount: 500.0 });
    }
}

fn log_action_changes(
    mut last: Local<Option<ActionKind>>,
    q: Query<(&PlayerAction, &Health, &Stamina), With<Player>>,
) {
    let Ok((action, hp, st)) = q.single() else { return };
    let now = action.current.map(|a| a.kind);
    if now != *last {
        info!("action {:?} -> {:?}  (HP {:.0}, ST {:.0})", *last, now, hp.current, st.current);
        *last = now;
    }
}

/// Fixed ticks spent airborne since the last log line (should be ~0 unless jumping).
#[derive(Resource, Default)]
struct AirborneTicks(u32);

fn count_airborne(mut ticks: ResMut<AirborneTicks>, q: Query<&CharacterController, With<Player>>) {
    if q.iter().any(|c| !c.grounded) {
        ticks.0 += 1;
    }
}

fn log_player(mut ticks: ResMut<AirborneTicks>, q: Query<(&Transform, &CharacterController), With<Player>>) {
    for (tf, c) in &q {
        info!(
            "player pos={:.2} vel={:.2} grounded={} airborne_ticks={} r={:.2}",
            tf.translation,
            c.velocity,
            c.grounded,
            ticks.0,
            Vec2::new(tf.translation.x, tf.translation.z).length()
        );
    }
    ticks.0 = 0;
}
