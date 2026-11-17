//! Spectating a boss fight (online): a player who falls while a boss is alive near them, with
//! another player still fighting it, isn't sent back to the sunflower yet. The death text fades and
//! the camera follows a survivor (Q / E or the lock-on switch to change who) until the fight ends:
//! the boss falls (back to the sunflower, it's over) or the last survivor falls too (a wipe:
//! everyone respawns, the boss and its bar reset with the world, `net::world::host_wipe_watch`).

use bevy::prelude::*;

use crate::camera::CameraFocus;
use crate::character::action::{ActionKind, CharacterAction};
use crate::combat::Team;
use crate::enemy::kinds::Boss;
use crate::hearth::{DeathSequence, SharedRest};
use crate::net::NetSession;
use crate::net::sync::Puppet;
use crate::player::Player;
use crate::tuning::*;
use crate::ui::hud::GOLD_TEXT;

pub struct SpectatePlugin;

impl Plugin for SpectatePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_label).add_systems(Update, spectate);
    }
}

#[derive(Component)]
struct SpectateLabel;

fn spawn_label(mut commands: Commands) {
    commands.spawn((
        SpectateLabel,
        Text::new(""),
        TextFont::from_font_size(18.0),
        TextColor(GOLD_TEXT),
        TextShadow { offset: Vec2::splat(2.0), color: Color::BLACK },
        Node { position_type: PositionType::Absolute, top: Val::Px(70.0), width: Val::Percent(100.0), justify_content: JustifyContent::Center, ..default() },
        TextLayout::justify(Justify::Center),
        Visibility::Hidden,
    ));
}

fn dead(a: &CharacterAction) -> bool {
    matches!(a.current, Some(c) if c.kind == ActionKind::Dead)
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn spectate(
    keys: Res<ButtonInput<KeyCode>>,
    shared: Res<SharedRest>,
    session: Res<NetSession>,
    mut focus: ResMut<CameraFocus>,
    mut watched: Local<usize>,
    mut me: Query<(&Transform, &mut DeathSequence), With<Player>>,
    bosses: Query<(&Transform, &CharacterAction), (With<Boss>, Without<Player>)>,
    players: Query<(Entity, &Puppet, &Transform, &CharacterAction, &Team), Without<Player>>,
    mut label: Query<(&mut Text, &mut Visibility), With<SpectateLabel>>,
) {
    let Ok((tf, mut seq)) = me.single_mut() else {
        focus.0 = None;
        if let Ok((_, mut v)) = label.single_mut() {
            *v = Visibility::Hidden;
        }
        return;
    };
    // The fight: a living boss near where we fell; the survivors: other players near it, alive.
    let boss = bosses.iter().find(|(b, a)| !dead(a) && b.translation.distance(tf.translation) < SPECTATE_RANGE);
    let mut survivors: Vec<(Entity, u32, Vec3)> = match boss {
        Some((b, _)) => players
            .iter()
            .filter(|(_, _, p, a, team)| **team == Team::Player && !dead(a) && p.translation.distance(b.translation) < SPECTATE_RANGE)
            .map(|(e, puppet, p, _, _)| (e, puppet.id, p.translation))
            .collect(),
        None => vec![],
    };
    survivors.sort_by_key(|s| s.1);
    let watch = shared.0 && !seq.respawned && !survivors.is_empty() && (seq.hold || seq.t >= SPECTATE_AT);
    if !watch {
        if seq.hold {
            info!("spectating over: back to the sunflower");
        }
        seq.hold = false;
        focus.0 = None;
        if let Ok((_, mut v)) = label.single_mut() {
            *v = Visibility::Hidden;
        }
        return;
    }
    if !seq.hold {
        info!("composted in a boss fight: watching {} survivor(s)", survivors.len());
    }
    seq.hold = true;
    let n = survivors.len();
    if keys.just_pressed(KeyCode::KeyE) {
        *watched += 1;
    }
    if keys.just_pressed(KeyCode::KeyQ) {
        *watched += n - 1;
    }
    *watched %= n;
    let (who, id, _) = survivors[*watched];
    focus.0 = Some(who);
    if let Ok((mut t, mut v)) = label.single_mut() {
        let text = crate::ui::strings::spectating(&session.name_of(id), n > 1);
        if t.0 != text {
            t.0 = text;
        }
        *v = Visibility::Inherited;
    }
}
