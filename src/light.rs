//! **Light**, the currency (souls / runes): every player carries a purse of it. Killing an enemy
//! always grants its `EnemyDef::light` (more for harder foes) at once, no pickup. Who gets it is
//! [`LIGHT_SHARE`]: everyone in the world (the current rule), only those who struck it, or only
//! those near it when it fell. The world's owner decides (offline: this machine; online: the
//! host, who tells each guest what it earned).
//!
//! Not yet: losing it on death (a bloodstain to win back) or spending it.

use std::collections::HashSet;

use bevy::prelude::*;

use crate::character::stats::DamageTaken;
use crate::character::Remote;
use crate::combat::Team;
use crate::enemy::{Corpse, Enemy};
use crate::net::NetSession;
use crate::net::protocol::NetId;
use crate::net::sync::Puppet;
use crate::player::Player;
use crate::tuning::*;

pub struct LightPlugin;

impl Plugin for LightPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<LightGained>()
            .add_message::<LightForGuest>()
            .add_systems(Update, (record_attackers, grant_light, gain_light).chain().in_set(LightSystems));
    }
}

/// Where Light is granted (the HUD reads the gains after it).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct LightSystems;

/// The player's Light.
#[derive(Component, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct LightPurse(pub u32);

/// Who gets the Light of a kill.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // the other rules are ready but not in use yet (user request: everyone, for now)
pub enum LightShare {
    /// Every player in the world.
    Everyone,
    /// Every player who hit it at least once.
    Attackers,
    /// Every player within `LIGHT_NEARBY_RANGE` of it when it fell.
    Nearby,
}

/// This machine's player earned Light (adds to the purse; the HUD shows it).
#[derive(Message, Debug, Clone, Copy)]
pub struct LightGained {
    pub amount: u32,
}

/// Host: a guest earned Light (sent by `net::sync`).
#[derive(Message, Debug, Clone, Copy)]
pub struct LightForGuest {
    pub guest: NetId,
    pub amount: u32,
}

/// The players who struck an enemy (local player or guest puppets), for `LightShare::Attackers`.
#[derive(Component, Debug, Default)]
pub struct StruckBy(pub HashSet<Entity>);

fn record_attackers(
    mut commands: Commands,
    mut taken: MessageReader<DamageTaken>,
    players: Query<&Team>,
    mut enemies: Query<Option<&mut StruckBy>, (With<Enemy>, Without<Remote>)>,
) {
    for t in taken.read() {
        let Some(attacker) = t.attacker.filter(|a| players.get(*a).is_ok_and(|team| *team == Team::Player)) else { continue };
        match enemies.get_mut(t.target) {
            Ok(Some(mut s)) => {
                s.0.insert(attacker);
            }
            Ok(None) => {
                commands.entity(t.target).insert(StruckBy(HashSet::from([attacker])));
            }
            Err(_) => {}
        }
    }
}

/// Whether a player gets the Light of a kill under `share`.
pub fn earns(share: LightShare, struck: bool, distance: f32) -> bool {
    match share {
        LightShare::Everyone => true,
        LightShare::Attackers => struck,
        LightShare::Nearby => distance <= LIGHT_NEARBY_RANGE,
    }
}

/// A fresh kill in this machine's world: hand out its Light.
#[allow(clippy::type_complexity)]
fn grant_light(
    session: Res<NetSession>,
    killed: Query<(&Enemy, &GlobalTransform, Option<&StruckBy>), (Added<Corpse>, Without<Remote>)>,
    me: Query<(Entity, &GlobalTransform), (With<Player>, Without<Remote>)>,
    guests: Query<(Entity, &Puppet, &GlobalTransform, &Team)>,
    mut gained: MessageWriter<LightGained>,
    mut for_guest: MessageWriter<LightForGuest>,
) {
    if session.is_client() {
        return; // the host's world: it tells us
    }
    for (enemy, at, struck) in &killed {
        let amount = enemy.kind.def().light;
        let struck_by = |e: Entity| struck.is_some_and(|s| s.0.contains(&e));
        if let Ok((e, tf)) = me.single()
            && earns(LIGHT_SHARE, struck_by(e), tf.translation().distance(at.translation()))
        {
            gained.write(LightGained { amount });
        }
        for (e, puppet, tf, team) in &guests {
            if *team == Team::Player && earns(LIGHT_SHARE, struck_by(e), tf.translation().distance(at.translation())) {
                for_guest.write(LightForGuest { guest: puppet.id, amount });
            }
        }
    }
}

fn gain_light(mut gained: MessageReader<LightGained>, mut purse: Query<&mut LightPurse, With<Player>>) {
    let Ok(mut purse) = purse.single_mut() else {
        gained.clear();
        return;
    };
    for g in gained.read() {
        purse.0 = purse.0.saturating_add(g.amount);
        info!("+{} Light ({} now)", g.amount, purse.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn share_rules() {
        assert!(earns(LightShare::Everyone, false, 1000.0));
        assert!(earns(LightShare::Attackers, true, 1000.0) && !earns(LightShare::Attackers, false, 1.0));
        assert!(earns(LightShare::Nearby, false, LIGHT_NEARBY_RANGE - 1.0) && !earns(LightShare::Nearby, true, LIGHT_NEARBY_RANGE + 1.0));
    }

    #[test]
    fn harder_enemies_give_more() {
        use crate::enemy::EnemyKind;
        let light = |k: EnemyKind| k.def().light;
        assert!(light(EnemyKind::Hollow) < light(EnemyKind::Acolyte));
        assert!(light(EnemyKind::Acolyte) < light(EnemyKind::Warg));
        assert!(light(EnemyKind::Warg) < light(EnemyKind::Mourner));
        let most_common = EnemyKind::ALL.iter().filter(|k| !k.def().boss).map(|k| light(*k)).max().unwrap();
        assert!(EnemyKind::ALL.iter().all(|k| light(*k) > 0));
        assert!(EnemyKind::ALL.iter().filter(|k| k.def().boss).all(|k| light(*k) > most_common * 5), "bosses give far more");
    }
}
