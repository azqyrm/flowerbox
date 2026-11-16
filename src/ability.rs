//! Activated abilities (F / gamepad L2), in the spirit of weapon skills: each weapon class may name
//! one in its moveset (`Moveset::ability`); the ability of the main weapon (the right one, or the
//! one held in both hands) is used.
//!
//! An ability is **data** ([`AbilityDef`]): a name, its stamina cost, and an [`AbilityEffect`]:
//! - [`AbilityEffect::Strike`]: an attack with its own swing (an [`AttackSpec`]) — it runs through the
//!   normal attack pipeline: timings, lunge, the swing pose and the blade hitbox, all as for any
//!   other attack (`ActiveAction::attack`).
//! - [`AbilityEffect::Buff`]: a stance (its own timings and pose) that, at the start of its active
//!   phase, grants an [`AttackBuff`] for a while.
//!
//! Adding one: a variant in [`AbilityId`], its `AbilityDef` in [`AbilityId::def`], and name it in a
//! moveset. A new kind of effect: a variant in `AbilityEffect` and its handling in [`apply_abilities`].

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::character::CharacterSystems;
use crate::character::action::{ActionKind, ActionPhase, CharacterAction};
use crate::items::ItemIcon;
use crate::player::Player;
use crate::ui::icons::NoticeIcon;
use crate::ui::notifications::Notify;
use crate::ui::strings;
use crate::weapon::{AttackSpec, Stance, SwingKey};

pub struct AbilityPlugin;

impl Plugin for AbilityPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, (apply_abilities, tick_buffs).chain().after(CharacterSystems::Actions).before(CharacterSystems::Combat));
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AbilityId {
    /// Light swords: a long, hard forward thrust.
    LungingThrust,
    /// Axes: a roar that strengthens your attacks for a while.
    Warcry,
}

/// What an ability does.
#[derive(Clone, Copy, Debug)]
pub enum AbilityEffect {
    /// An attack with its own swing (damage = weapon base × `damage_mult`).
    Strike(AttackSpec),
    /// A stance with these timings; at the start of its active phase, attacks deal `attack_mult`
    /// × damage for `duration` seconds.
    Buff { startup: f32, active: f32, recovery: f32, attack_mult: f32, duration: f32 },
}

#[derive(Clone, Copy, Debug)]
pub struct AbilityDef {
    pub name: &'static str,
    /// Shown with its notification.
    pub icon: ItemIcon,
    pub stamina_cost: f32,
    pub effect: AbilityEffect,
}

const LUNGING_THRUST: AbilityDef = AbilityDef {
    name: "Lunging Thrust",
    icon: ItemIcon::Sword,
    stamina_cost: 32.0,
    effect: AbilityEffect::Strike(AttackSpec {
        startup: 0.4,
        active: 0.16,
        recovery: 0.45,
        cancel: 0.85,
        stamina_cost: 32.0,
        lunge: 2.6,
        damage_mult: 1.9,
        // Blade drawn back low at the hip, then driven straight ahead.
        windup: SwingKey::new(-0.4, 0.1, -0.25, 0.75),
        strike: SwingKey::new(0.25, -0.15, 0.05, 1.6),
        stance: Stance::Lunge,
    }),
};

const WARCRY: AbilityDef = AbilityDef {
    name: "Warcry",
    icon: ItemIcon::Axe,
    stamina_cost: 25.0,
    effect: AbilityEffect::Buff { startup: 0.45, active: 0.6, recovery: 0.45, attack_mult: 1.25, duration: 10.0 },
};

impl AbilityId {
    pub fn def(self) -> &'static AbilityDef {
        match self {
            AbilityId::LungingThrust => &LUNGING_THRUST,
            AbilityId::Warcry => &WARCRY,
        }
    }

    /// The swing, if this ability is a strike.
    pub fn strike(self) -> Option<&'static AttackSpec> {
        match &self.def().effect {
            AbilityEffect::Strike(spec) => Some(spec),
            AbilityEffect::Buff { .. } => None,
        }
    }
}

/// Attacks deal `mult` × damage for `left` more seconds.
#[derive(Component, Debug, Clone, Copy)]
pub struct AttackBuff {
    pub mult: f32,
    pub left: f32,
}

/// Which ability use (action serial) was already applied.
#[derive(Component, Default)]
struct AbilityApplied(Option<u32>);

/// Buffs land at the start of the ability's active phase (once per use).
fn apply_abilities(
    mut commands: Commands,
    mut notify: MessageWriter<Notify>,
    q: Query<(Entity, &CharacterAction, Option<&AbilityApplied>, Has<Player>)>,
) {
    for (entity, action, applied, is_player) in &q {
        let Some(cur) = action.current else { continue };
        let ActionKind::Ability(id) = cur.kind else { continue };
        if cur.phase() != ActionPhase::Active || applied.is_some_and(|a| a.0 == Some(action.serial)) {
            continue;
        }
        commands.entity(entity).insert(AbilityApplied(Some(action.serial)));
        if let AbilityEffect::Buff { attack_mult, duration, .. } = id.def().effect {
            commands.entity(entity).insert(AttackBuff { mult: attack_mult, left: duration });
            info!("{}: ×{attack_mult} damage for {duration} s", id.def().name);
            if is_player {
                notify.write(Notify::new(NoticeIcon::Item(id.def().icon), strings::buff(id.def().name, attack_mult, duration)));
            }
        }
    }
}

fn tick_buffs(time: Res<Time>, mut commands: Commands, mut q: Query<(Entity, &mut AttackBuff)>) {
    for (entity, mut buff) in &mut q {
        buff.left -= time.delta_secs();
        if buff.left <= 0.0 {
            commands.entity(entity).remove::<AttackBuff>();
        }
    }
}
