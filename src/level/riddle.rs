//! The bell riddle: a sealed door (`Door::sealed`) opens only when the level's bells are rung in
//! the right order (`RiddleDef { bells, door }`). An inscription on a lectern ([`Plaque`], E to
//! read) hints at the order; each bell names itself in its prompt notification.
//!
//! Ringing: E at a [`Bell`] swings it (everyone sees and hears it). Whoever owns the world decides
//! (offline: us; online: the host, like doors): the right next bell advances the rite; a wrong one
//! resets it (unless it's the first bell — then the rite starts over from there). Completing it
//! opens the door for everyone (`DoorOpened`, saved like any open door) with a banner.

use bevy::prelude::*;

use super::door::{Door, DoorOpened};
use super::layout::RiddleDef;
use crate::character::action::CharacterAction;
use crate::hearth::DeathSequence;
use crate::input::{GameplayBlocked, PlayerInput};
use crate::interact::{InteractPrompt, InteractSystems};
use crate::items::pickup::PickupAuthority;
use crate::player::Player;
use crate::tuning::*;
use crate::ui::icons::NoticeIcon;
use crate::ui::notifications::Notify;
use crate::ui::{Banner, strings};

pub struct RiddlePlugin;

impl Plugin for RiddlePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<BellRung>()
            .add_message::<RingRequest>()
            .init_resource::<Riddles>()
            .add_systems(Update, ((bell_interaction, plaque_interaction).in_set(InteractSystems::Doors), judge_rings, swing_bells).chain());
    }
}

/// A bell (on its prop root). `pivot` is the swinging part.
#[derive(Component, Debug)]
pub struct Bell {
    pub id: u32,
    pub name: String,
    pub pivot: Entity,
}

/// The swinging body of a bell: its current swing amplitude (rad), decaying.
#[derive(Component, Debug)]
pub struct BellBody {
    pub swing: f32,
}

/// A readable inscription.
#[derive(Component, Debug)]
pub struct Plaque {
    pub text: String,
}

/// Every riddle of the loaded levels and how far along each is.
#[derive(Resource, Debug, Default)]
pub struct Riddles(pub Vec<(RiddleDef, usize)>);

/// The local player rang bell `id` (the world's owner judges it; online it goes to the host).
#[derive(Message, Debug, Clone, Copy)]
pub struct RingRequest {
    pub id: u32,
}

/// Bell `id` was rung (swing it, play it); `by` = who, for others' notifications (None = us).
#[derive(Message, Debug, Clone, Copy)]
pub struct BellRung {
    pub id: u32,
}

/// Advances `progress` through `bells` with a ring of `bell`; true when the riddle is complete.
pub fn advance(bells: &[u32], progress: &mut usize, bell: u32) -> bool {
    if bells.get(*progress) == Some(&bell) {
        *progress += 1;
    } else {
        *progress = usize::from(bells.first() == Some(&bell));
    }
    *progress == bells.len()
}

#[allow(clippy::type_complexity)]
fn bell_interaction(
    mut prompt: ResMut<InteractPrompt>,
    blocked: Res<GameplayBlocked>,
    bells: Query<(&Bell, &GlobalTransform)>,
    mut ring: MessageWriter<RingRequest>,
    mut rung: MessageWriter<BellRung>,
    mut notify: MessageWriter<Notify>,
    mut player: Query<(&Transform, &CharacterAction, &mut PlayerInput, Has<DeathSequence>), With<Player>>,
) {
    let Ok((tf, action, mut input, dying)) = player.single_mut() else { return };
    if prompt.0.is_some() || blocked.0 || dying || action.current.is_some() {
        return;
    }
    let near = bells.iter().find(|(_, g)| {
        let d = g.translation() - tf.translation;
        d.with_y(0.0).length() < BELL_RANGE && d.y.abs() < 2.0
    });
    let Some((bell, _)) = near else { return };
    prompt.0 = Some(strings::PROMPT_RING);
    if std::mem::take(&mut input.interact) {
        info!("rang {}", bell.name);
        notify.write(Notify::new(NoticeIcon::Door, bell.name.clone()));
        rung.write(BellRung { id: bell.id });
        ring.write(RingRequest { id: bell.id });
    }
}

fn plaque_interaction(
    mut prompt: ResMut<InteractPrompt>,
    blocked: Res<GameplayBlocked>,
    plaques: Query<(&Plaque, &GlobalTransform)>,
    mut notify: MessageWriter<Notify>,
    mut player: Query<(&Transform, &CharacterAction, &mut PlayerInput), With<Player>>,
) {
    let Ok((tf, action, mut input)) = player.single_mut() else { return };
    if prompt.0.is_some() || blocked.0 || action.current.is_some() {
        return;
    }
    let near = plaques.iter().find(|(_, g)| {
        let d = g.translation() - tf.translation;
        d.with_y(0.0).length() < BELL_RANGE && d.y.abs() < 2.0
    });
    let Some((plaque, _)) = near else { return };
    prompt.0 = Some(strings::PROMPT_READ);
    if std::mem::take(&mut input.interact) {
        // Long inscriptions come as several notifications, one per line.
        for line in plaque.text.split('|').rev() {
            notify.write(Notify::new(NoticeIcon::Door, line.trim().to_string()).lasting(PLAQUE_NOTICE_TIME));
        }
        info!("read: {}", plaque.text);
    }
}

/// Offline / host: judge rings (our own, and guests' via `crate::net::world`, which writes
/// `RingRequest`s too) and open the sealed door when a riddle is complete.
pub fn judge_rings(
    authority: Res<PickupAuthority>,
    mut riddles: ResMut<Riddles>,
    mut requests: MessageReader<RingRequest>,
    mut doors: Query<&mut Door>,
    mut opened: MessageWriter<DoorOpened>,
    mut banner: MessageWriter<Banner>,
    mut notify: MessageWriter<Notify>,
) {
    if !authority.0 {
        requests.clear();
        return;
    }
    for req in requests.read() {
        for (riddle, progress) in &mut riddles.0 {
            if !riddle.bells.contains(&req.id) {
                continue;
            }
            let Some(mut door) = doors.iter_mut().find(|d| d.id == riddle.door) else { continue };
            if door.open {
                continue;
            }
            let before = *progress;
            if advance(&riddle.bells, progress, req.id) {
                door.sealed = false;
                door.open = true;
                opened.write(DoorOpened { id: door.id });
                banner.write(Banner(strings::RITE_SOLVED));
                info!("riddle {} solved: door {} opens", riddle.id, door.id);
            } else if *progress > before {
                notify.write(Notify::new(NoticeIcon::Door, strings::rite_progress(*progress, riddle.bells.len())));
            } else {
                notify.write(Notify::new(NoticeIcon::Door, strings::RITE_WRONG));
                info!("riddle {}: wrong bell, back to {}", riddle.id, progress);
            }
        }
    }
}

/// A rung bell swings and slowly settles.
fn swing_bells(
    time: Res<Time>,
    mut rung: MessageReader<BellRung>,
    bells: Query<&Bell>,
    mut bodies: Query<(&mut BellBody, &mut Transform)>,
    mut sounds: MessageWriter<crate::sound::PlaySound>,
) {
    for r in rung.read() {
        for bell in bells.iter().filter(|b| b.id == r.id) {
            if let Ok((mut body, tf)) = bodies.get_mut(bell.pivot) {
                body.swing = 0.6;
                sounds.write(crate::sound::PlaySound::at(crate::sound::Sound::Bell, tf.translation));
            }
        }
    }
    let t = time.elapsed_secs();
    for (mut body, mut tf) in &mut bodies {
        body.swing *= (-0.5 * time.delta_secs()).exp();
        tf.rotation = Quat::from_rotation_x(body.swing * (t * 3.2).sin());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rite_needs_the_right_order() {
        let bells = [3, 1, 2];
        let mut p = 0;
        assert!(!advance(&bells, &mut p, 3));
        assert!(!advance(&bells, &mut p, 2), "wrong: back to the start");
        assert_eq!(p, 0);
        assert!(!advance(&bells, &mut p, 3));
        assert!(!advance(&bells, &mut p, 3), "the first bell again: starts over from it");
        assert_eq!(p, 1);
        assert!(!advance(&bells, &mut p, 1));
        assert!(advance(&bells, &mut p, 2), "complete");
    }
}
