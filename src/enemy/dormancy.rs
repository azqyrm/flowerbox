//! Dormancy: enemies far from every player sleep — no brain, no movement, no animation, not drawn
//! ([`Dormant`]). They wake when a player comes within [`DORMANCY_WAKE`] and fall asleep again
//! beyond [`DORMANCY_SLEEP`] (the gap stops flickering at the edge).
//!
//! Why distance and not "behind a closed door": an enemy you can't reach can't see or hurt you
//! either, distance is cheap and robust (no room graph, works in open halls, online with any number
//! of players), and the measured cost is per enemy, not per room (ROADMAP N31). Online each machine
//! decides for itself: the host by every player (its world's enemies must keep fighting guests), a
//! guest by the players it sees (only drawing and animating the host's puppets).
//!
//! Dead enemies are left alone (their corpse timer hides them).

use bevy::prelude::*;

use super::{Dormant, Enemy};
use crate::character::stats::Health;
use crate::combat::Team;
use crate::tuning::{DORMANCY_SLEEP, DORMANCY_WAKE};

pub struct DormancyPlugin;

impl Plugin for DormancyPlugin {
    fn build(&self, app: &mut App) {
        // For measuring what dormancy saves (ROADMAP N31).
        if std::env::var("FLOWERBOX_NO_DORMANCY").is_ok() {
            return;
        }
        app.add_systems(Update, update_dormancy);
    }
}

/// Wake / sleep decision: `asleep` now, `nearest` player distance.
pub fn next_asleep(asleep: bool, nearest: f32) -> bool {
    if asleep { nearest > DORMANCY_WAKE } else { nearest > DORMANCY_SLEEP }
}

#[allow(clippy::type_complexity)]
fn update_dormancy(
    mut commands: Commands,
    players: Query<(&Transform, &Team), Without<Enemy>>,
    mut enemies: Query<(Entity, &Transform, &Health, &mut Visibility, Has<Dormant>), With<Enemy>>,
    mut log: Local<(usize, f32)>,
    time: Res<Time>,
) {
    let spots: Vec<Vec3> = players.iter().filter(|(_, t)| **t == Team::Player).map(|(tf, _)| tf.translation).collect();
    let mut sleeping = 0;
    for (entity, tf, health, mut vis, asleep) in &mut enemies {
        if health.is_dead() {
            continue;
        }
        let nearest = spots.iter().map(|p| p.distance(tf.translation)).fold(f32::INFINITY, f32::min);
        let next = next_asleep(asleep, nearest);
        sleeping += usize::from(next);
        if next == asleep {
            // Something else made a sleeper visible (a remote corpse revived): hide it again.
            if asleep && *vis != Visibility::Hidden {
                *vis = Visibility::Hidden;
            }
            continue;
        }
        if next {
            commands.entity(entity).insert(Dormant);
            *vis = Visibility::Hidden;
        } else {
            commands.entity(entity).remove::<Dormant>();
            *vis = Visibility::Inherited;
        }
    }
    // Every few seconds, how many sleep (for tuning and the autopilots).
    log.1 += time.delta_secs();
    if log.0 != sleeping && log.1 > 2.0 {
        log.1 = 0.0;
        log.0 = sleeping;
        debug!("dormancy: {sleeping} of {} enemies asleep", enemies.iter().count());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wakes_and_sleeps_with_a_gap() {
        assert!(!next_asleep(true, DORMANCY_WAKE - 1.0), "close: wakes");
        assert!(next_asleep(true, DORMANCY_WAKE + 1.0), "still far: keeps sleeping");
        assert!(!next_asleep(false, DORMANCY_SLEEP - 1.0), "awake inside the gap: stays awake");
        assert!(next_asleep(false, DORMANCY_SLEEP + 1.0), "far: sleeps");
    }
}
