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
//! - [`AbilityEffect::Spin`]: a strike during which the body turns `turns` full circles — the blade
//!   sweeps all around (an area attack); a target can be hit once per turn.
//! - [`AbilityEffect::Slam`]: a strike whose impact (start of the active phase) sends a
//!   shockwave ring out to `radius`: blockable, rolled through with i-frames.
//! - [`AbilityEffect::Shriek`]: a roar (the buff stance) whose wave can't be blocked — only
//!   rolled through or outrun.
//!
//! Enemies use abilities as their special moves (`ActionRequest::Special(id)`, any character may
//! perform any ability; the player only gets their weapon class's one via F). Waves are decided on
//! the victim's machine, like every enemy hit: each machine checks only its own player against
//! every body (puppets included) that starts a wave ([`shockwaves`]).
//!
//! Adding one: a variant in [`AbilityId`], its `AbilityDef` in [`AbilityId::def`], and name it in a
//! moveset or an enemy profile. A new kind of effect: a variant in `AbilityEffect` and its handling
//! in [`apply_abilities`] / [`shockwaves`].

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::character::action::{ActionKind, ActionPhase, CharacterAction};
use crate::character::controller::CharacterController;
use crate::character::stats::DamageMessage;
use crate::character::{CharacterSystems, Remote};
use crate::tuning::PLAYER_HALF_HEIGHT;
use crate::weapon::Wielded;
use crate::items::ItemIcon;
use crate::player::Player;
use crate::ui::icons::NoticeIcon;
use crate::ui::notifications::Notify;
use crate::ui::strings;
use crate::weapon::{AttackSpec, Stance, SwingKey};

pub struct AbilityPlugin;

impl Plugin for AbilityPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            (apply_abilities, spin_bodies, shockwaves, tick_buffs).chain().after(CharacterSystems::Actions).before(CharacterSystems::Combat),
        )
        .add_systems(Update, animate_waves);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AbilityId {
    /// Light swords: a long, hard forward thrust.
    LungingThrust,
    /// Axes: a roar that strengthens your attacks for a while.
    Warcry,
    // ── Enemy specials ──
    /// A whirling double turn with the blade held out (an area attack).
    Whirlwind,
    /// A long, fast lunge that closes the distance with a thrust.
    DashStrike,
    /// A leap-less overhead slam whose impact sends out a shockwave.
    GroundSlam,
    /// A blood-curdling shriek: an unblockable wave that staggers.
    Shriek,
    /// A frenzied claw flurry turning half around (the Mourner).
    RendingTurn,
    /// Greatswords: one and a half turns with the blade held out (the player's Whirlwind).
    PenitentWhirl,
    // ── The Archivist (the library's boss) ──
    /// A tome the size of a door raised high and slammed down: a wide shockwave.
    TomeSlam,
    /// Ink flung from the quill: a fan of dark bolts flying at you.
    InkVolley,
    /// A whirl of the quill in a storm of torn pages (two turns, wide).
    PageStorm,
    /// "Hush": a hiss that can't be blocked, wide.
    Hush,
    // ── The Matron (the Womb's boss) ──
    /// Bile sprayed in a wide fan.
    BileSpray,
    /// Both long arms brought down together: a crushing slam with a shockwave.
    CrushingEmbrace,
    /// A mother's wail: unblockable, very wide.
    Wail,
}

/// What an ability does.
#[derive(Clone, Copy, Debug)]
pub enum AbilityEffect {
    /// An attack with its own swing (damage = weapon base × `damage_mult`).
    Strike(AttackSpec),
    /// A stance with these timings; at the start of its active phase, attacks deal `attack_mult`
    /// × damage for `duration` seconds.
    Buff { startup: f32, active: f32, recovery: f32, attack_mult: f32, duration: f32 },
    /// A strike while turning `turns` full circles during the active phase.
    Spin { swing: AttackSpec, turns: f32 },
    /// A strike whose impact sends a shockwave to `radius` (weapon base × `wave_mult` damage).
    Slam { swing: AttackSpec, radius: f32, wave_mult: f32 },
    /// A roar (buff stance timings) whose wave to `radius` deals `damage` and can't be blocked.
    Shriek { startup: f32, active: f32, recovery: f32, radius: f32, damage: f32 },
    /// A stance that, at the start of its active phase, flings `count` projectiles in a fan
    /// `spread` radians wide where the body faces (`crate::projectile`).
    Volley { startup: f32, active: f32, recovery: f32, count: u32, spread: f32, speed: f32, damage: f32, kind: crate::projectile::ProjectileKind },
}

/// A shockwave: where it can reach and what it deals.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaveSpec {
    pub radius: f32,
    /// Damage (already in points).
    pub damage: f32,
    pub blockable: bool,
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

const WHIRLWIND: AbilityDef = AbilityDef {
    name: "Whirlwind",
    icon: ItemIcon::Sword,
    stamina_cost: 0.0,
    effect: AbilityEffect::Spin {
        // Blade swept far back, then held out flat at shoulder height while the body spins.
        swing: AttackSpec {
            startup: 0.85,
            active: 1.0,
            recovery: 0.9,
            cancel: 2.75,
            stamina_cost: 0.0,
            lunge: 3.5,
            damage_mult: 1.1,
            windup: SwingKey::new(-1.0, 0.25, -1.2, 1.2),
            strike: SwingKey::new(0.2, 0.25, 0.9, 1.1),
            stance: Stance::Lunge,
        },
        turns: 2.0,
    },
};

const DASH_STRIKE: AbilityDef = AbilityDef {
    name: "Dash Strike",
    icon: ItemIcon::Sword,
    stamina_cost: 0.0,
    effect: AbilityEffect::Strike(AttackSpec {
        startup: 0.6,
        active: 0.3,
        recovery: 0.8,
        cancel: 1.7,
        stamina_cost: 0.0,
        lunge: 8.0,
        damage_mult: 1.5,
        windup: SwingKey::new(-0.5, 0.25, -0.3, 0.7),
        strike: SwingKey::new(0.3, 0.15, 0.05, 1.2),
        stance: Stance::Lunge,
    }),
};

const GROUND_SLAM: AbilityDef = AbilityDef {
    name: "Ground Slam",
    icon: ItemIcon::Axe,
    stamina_cost: 0.0,
    effect: AbilityEffect::Slam {
        swing: AttackSpec {
            startup: 1.05,
            active: 0.22,
            recovery: 1.1,
            cancel: 2.37,
            stamina_cost: 0.0,
            lunge: 1.2,
            damage_mult: 1.8,
            // Raised high behind the head, then brought straight down into the floor.
            windup: SwingKey::new(0.0, -0.35, 0.0, 3.0),
            strike: SwingKey::new(0.0, 0.5, 0.0, 0.55),
            stance: Stance::Lunge,
        },
        radius: 5.5,
        wave_mult: 1.0,
    },
};

const SHRIEK: AbilityDef = AbilityDef {
    name: "Shriek",
    icon: ItemIcon::Sword,
    stamina_cost: 0.0,
    effect: AbilityEffect::Shriek { startup: 0.7, active: 0.5, recovery: 0.6, radius: 6.0, damage: 14.0 },
};

const RENDING_TURN: AbilityDef = AbilityDef {
    name: "Rending Turn",
    icon: ItemIcon::Sword,
    stamina_cost: 0.0,
    effect: AbilityEffect::Spin {
        swing: AttackSpec {
            startup: 0.5,
            active: 0.45,
            recovery: 0.55,
            cancel: 1.5,
            stamina_cost: 0.0,
            lunge: 1.8,
            damage_mult: 1.3,
            windup: SwingKey::new(-0.9, 0.3, -1.4, 1.2),
            strike: SwingKey::new(0.6, 0.1, 1.1, 1.3),
            stance: Stance::Lunge,
        },
        turns: 1.0,
    },
};

const PENITENT_WHIRL: AbilityDef = AbilityDef {
    name: "Penitent Whirl",
    icon: ItemIcon::Sword,
    stamina_cost: 42.0,
    effect: AbilityEffect::Spin {
        swing: AttackSpec {
            startup: 0.5,
            active: 0.7,
            recovery: 0.55,
            cancel: 1.6,
            stamina_cost: 42.0,
            lunge: 2.0,
            damage_mult: 1.0,
            windup: SwingKey::new(-1.0, 0.2, -1.2, 1.4),
            strike: SwingKey::new(0.2, 0.0, 0.9, 1.5),
            stance: Stance::Lunge,
        },
        turns: 1.5,
    },
};

const TOME_SLAM: AbilityDef = AbilityDef {
    name: "Tome Slam",
    icon: ItemIcon::Axe,
    stamina_cost: 0.0,
    effect: AbilityEffect::Slam {
        swing: AttackSpec {
            startup: 1.2,
            active: 0.25,
            recovery: 1.0,
            cancel: 2.45,
            stamina_cost: 0.0,
            lunge: 1.6,
            damage_mult: 1.6,
            windup: SwingKey::new(0.1, -0.4, 0.0, 3.05),
            strike: SwingKey::new(0.0, 0.55, 0.0, 0.5),
            stance: Stance::Lunge,
        },
        radius: 7.0,
        wave_mult: 0.9,
    },
};

const INK_VOLLEY: AbilityDef = AbilityDef {
    name: "Ink Volley",
    icon: ItemIcon::Sword,
    stamina_cost: 0.0,
    effect: AbilityEffect::Volley { startup: 0.75, active: 0.35, recovery: 0.7, count: 5, spread: 0.9, speed: 11.0, damage: 22.0, kind: crate::projectile::ProjectileKind::Ink },
};

const PAGE_STORM: AbilityDef = AbilityDef {
    name: "Page Storm",
    icon: ItemIcon::Sword,
    stamina_cost: 0.0,
    effect: AbilityEffect::Spin {
        swing: AttackSpec {
            startup: 0.7,
            active: 1.1,
            recovery: 0.8,
            cancel: 2.6,
            stamina_cost: 0.0,
            lunge: 4.0,
            damage_mult: 1.0,
            windup: SwingKey::new(-1.1, 0.2, -1.3, 1.3),
            strike: SwingKey::new(0.2, 0.2, 0.9, 1.2),
            stance: Stance::Lunge,
        },
        turns: 2.0,
    },
};

const HUSH: AbilityDef = AbilityDef {
    name: "Hush",
    icon: ItemIcon::Sword,
    stamina_cost: 0.0,
    effect: AbilityEffect::Shriek { startup: 0.9, active: 0.5, recovery: 0.7, radius: 9.0, damage: 18.0 },
};

const BILE_SPRAY: AbilityDef = AbilityDef {
    name: "Bile Spray",
    icon: ItemIcon::Sword,
    stamina_cost: 0.0,
    effect: AbilityEffect::Volley { startup: 0.85, active: 0.4, recovery: 0.8, count: 7, spread: 1.3, speed: 9.0, damage: 20.0, kind: crate::projectile::ProjectileKind::Bile },
};

const CRUSHING_EMBRACE: AbilityDef = AbilityDef {
    name: "Crushing Embrace",
    icon: ItemIcon::Axe,
    stamina_cost: 0.0,
    effect: AbilityEffect::Slam {
        swing: AttackSpec {
            startup: 1.1,
            active: 0.25,
            recovery: 1.1,
            cancel: 2.45,
            stamina_cost: 0.0,
            lunge: 2.2,
            damage_mult: 1.5,
            windup: SwingKey::new(0.0, -0.45, 0.0, 2.9),
            strike: SwingKey::new(0.0, 0.6, 0.0, 0.6),
            stance: Stance::Lunge,
        },
        radius: 6.5,
        wave_mult: 0.85,
    },
};

const WAIL: AbilityDef = AbilityDef {
    name: "Wail",
    icon: ItemIcon::Sword,
    stamina_cost: 0.0,
    effect: AbilityEffect::Shriek { startup: 1.0, active: 0.6, recovery: 0.8, radius: 11.0, damage: 20.0 },
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
            AbilityId::Whirlwind => &WHIRLWIND,
            AbilityId::DashStrike => &DASH_STRIKE,
            AbilityId::GroundSlam => &GROUND_SLAM,
            AbilityId::Shriek => &SHRIEK,
            AbilityId::RendingTurn => &RENDING_TURN,
            AbilityId::PenitentWhirl => &PENITENT_WHIRL,
            AbilityId::TomeSlam => &TOME_SLAM,
            AbilityId::InkVolley => &INK_VOLLEY,
            AbilityId::PageStorm => &PAGE_STORM,
            AbilityId::Hush => &HUSH,
            AbilityId::BileSpray => &BILE_SPRAY,
            AbilityId::CrushingEmbrace => &CRUSHING_EMBRACE,
            AbilityId::Wail => &WAIL,
        }
    }

    /// The swing, if this ability is a strike (plain, spinning or slamming).
    pub fn strike(self) -> Option<&'static AttackSpec> {
        match &self.def().effect {
            AbilityEffect::Strike(spec) | AbilityEffect::Spin { swing: spec, .. } | AbilityEffect::Slam { swing: spec, .. } => Some(spec),
            AbilityEffect::Buff { .. } | AbilityEffect::Shriek { .. } | AbilityEffect::Volley { .. } => None,
        }
    }

    /// Full turns of the body during the active phase (spin attacks).
    pub fn turns(self) -> f32 {
        match self.def().effect {
            AbilityEffect::Spin { turns, .. } => turns,
            _ => 0.0,
        }
    }

    /// The shockwave sent at the start of the active phase, for a wielder dealing `base_damage`.
    pub fn wave(self, base_damage: f32) -> Option<WaveSpec> {
        match self.def().effect {
            AbilityEffect::Slam { radius, wave_mult, .. } => Some(WaveSpec { radius, damage: (base_damage * wave_mult).round(), blockable: true }),
            AbilityEffect::Shriek { radius, damage, .. } => Some(WaveSpec { radius, damage, blockable: false }),
            _ => None,
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
    mut shots: MessageWriter<crate::projectile::SpawnProjectile>,
    q: Query<(Entity, &CharacterAction, Option<&AbilityApplied>, Has<Player>, &Transform, &CharacterController, Option<&crate::combat::Team>)>,
) {
    for (entity, action, applied, is_player, tf, ctrl, team) in &q {
        let Some(cur) = action.current else { continue };
        let ActionKind::Ability(id) = cur.kind else { continue };
        if cur.phase() != ActionPhase::Active || applied.is_some_and(|a| a.0 == Some(action.serial)) {
            continue;
        }
        commands.entity(entity).insert(AbilityApplied(Some(action.serial)));
        // A volley: every machine flings it from its copy of the body (puppets too); each
        // projectile only hurts that machine's own bodies (`crate::projectile`).
        if let AbilityEffect::Volley { count, spread, speed, damage, kind, .. } = id.def().effect {
            let from = tf.translation + Vec3::Y * 0.6;
            for i in 0..count {
                let a = ctrl.facing + if count > 1 { spread * (i as f32 / (count - 1) as f32 - 0.5) } else { 0.0 };
                let dir = Vec3::new(-a.sin(), -0.04, -a.cos()).normalize();
                shots.write(crate::projectile::SpawnProjectile {
                    pos: from + dir * 0.6,
                    vel: dir * speed,
                    damage,
                    stance: damage,
                    team: team.copied(),
                    kind,
                    by: Some(entity),
                });
            }
        }
        if let AbilityEffect::Buff { attack_mult, duration, .. } = id.def().effect {
            commands.entity(entity).insert(AttackBuff { mult: attack_mult, left: duration });
            info!("{}: ×{attack_mult} damage for {duration} s", id.def().name);
            if is_player {
                notify.write(Notify::new(NoticeIcon::Item(id.def().icon), strings::buff(id.def().name, attack_mult, duration)));
            }
        }
    }
}

/// Spinning attacks turn the body during their active phase (the blade sweeps around with it).
/// Remote bodies get their facing from their owner.
fn spin_bodies(time: Res<Time>, mut q: Query<(&CharacterAction, &mut CharacterController), Without<Remote>>) {
    for (action, mut ctrl) in &mut q {
        let Some(cur) = action.current else { continue };
        let ActionKind::Ability(id) = cur.kind else { continue };
        let turns = id.turns();
        if turns == 0.0 || cur.phase() != ActionPhase::Active || cur.def.active <= 0.0 {
            continue;
        }
        ctrl.facing += std::f32::consts::TAU * turns / cur.def.active * time.delta_secs();
    }
}

/// Which action (serial) already sent its wave.
#[derive(Component, Default)]
struct WaveSent(u32);

/// An expanding shockwave ring (visual only).
#[derive(Component)]
struct WaveRing {
    age: f32,
    radius: f32,
    material: Handle<StandardMaterial>,
}

const WAVE_TIME: f32 = 0.45;

/// At the start of a slam's or shriek's active phase: a ring expands from the body, and *our* player
/// within reach (|Δy| < 1.6) takes the hit (blockable from the front for slams; rolling through
/// with i-frames avoids it, `apply_damage`). Runs for every body, puppets included, so each
/// machine decides for its own player.
#[allow(clippy::type_complexity)]
fn shockwaves(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut damage: MessageWriter<DamageMessage>,
    mut shake: ResMut<crate::combat::CameraShake>,
    mut sounds: MessageWriter<crate::sound::PlaySound>,
    bodies: Query<(Entity, &Transform, &CharacterAction, Option<&Wielded>, Option<&WaveSent>)>,
    victims: Query<(Entity, &Transform), (With<Player>, Without<Remote>)>,
) {
    for (entity, tf, action, wielded, sent) in &bodies {
        let Some(cur) = action.current else { continue };
        let ActionKind::Ability(id) = cur.kind else { continue };
        if cur.phase() != ActionPhase::Active || sent.is_some_and(|s| s.0 == action.serial) {
            continue;
        }
        let Some(wave) = id.wave(wielded.map_or(10.0, |w| w.right.base_damage)) else { continue };
        commands.entity(entity).insert(WaveSent(action.serial));
        let sound = if wave.blockable { crate::sound::Sound::Shockwave } else { crate::sound::Sound::Shriek };
        sounds.write(crate::sound::PlaySound::at(sound, tf.translation));
        let floor = tf.translation - Vec3::Y * (PLAYER_HALF_HEIGHT - 0.05);
        let material = materials.add(StandardMaterial {
            base_color: Color::srgba(1.0, 0.45, 0.25, 0.8),
            emissive: if wave.blockable { LinearRgba::rgb(9.0, 3.0, 0.8) } else { LinearRgba::rgb(5.0, 0.6, 2.2) },
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        });
        commands.spawn((
            Name::new("Shockwave"),
            WaveRing { age: 0.0, radius: wave.radius, material: material.clone() },
            Mesh3d(meshes.add(Torus::new(0.85, 1.0))),
            MeshMaterial3d(material),
            Transform::from_translation(floor).with_scale(Vec3::new(0.1, 0.4, 0.1)),
        ));
        for (victim, vtf) in &victims {
            let d = vtf.translation - tf.translation;
            if victim == entity || d.with_y(0.0).length() > wave.radius || d.y.abs() > 1.6 {
                continue;
            }
            shake.trauma = (shake.trauma + 0.5).min(1.0);
            info!("{} hits us for {} ({})", id.def().name, wave.damage, if wave.blockable { "blockable" } else { "unblockable" });
            let from = wave.blockable.then_some(tf.translation);
            damage.write(DamageMessage::plain(victim, wave.damage, from, Some(entity)));
        }
    }
}

/// Rings grow to their radius and fade.
fn animate_waves(time: Res<Time>, mut commands: Commands, mut materials: ResMut<Assets<StandardMaterial>>, mut q: Query<(Entity, &mut WaveRing, &mut Transform)>) {
    for (entity, mut ring, mut tf) in &mut q {
        ring.age += time.delta_secs();
        let t = (ring.age / WAVE_TIME).min(1.0);
        let r = ring.radius * (1.0 - (1.0 - t) * (1.0 - t));
        tf.scale = Vec3::new(r.max(0.1), 0.4 * (1.0 - t) + 0.05, r.max(0.1));
        if let Some(mut m) = materials.get_mut(&ring.material) {
            m.base_color.set_alpha(0.8 * (1.0 - t));
        }
        if t >= 1.0 {
            commands.entity(entity).despawn();
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
