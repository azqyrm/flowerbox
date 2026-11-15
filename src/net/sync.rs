//! Turning bodies into network state and back.
//!
//! - [`send_states`]: ~30×/s, the local player's [`BodyState`] (client → host), or a snapshot of
//!   every body (host → clients: the host's player, the guests' players as last reported, and
//!   the host's real enemies).
//! - [`receive`]: upserts **puppets** for bodies owned elsewhere and applies guests' damage and
//!   parries to the host's real enemies.
//! - [`drive_puppets`]: eases puppets toward their state and feeds the same components the local
//!   characters have (`CharacterController`, `CharacterAction`, `Guard`, `Health`, equipment), so
//!   the shared animation, weapon models, hit detection against *you*, lock-on and health bars
//!   just work. Puppets carry `Remote`: no physics/AI drives them and damage to them isn't applied
//!   locally — it's forwarded ([`forward_hits`]).

use std::collections::HashSet;

use avian3d::prelude::*;
use bevy::prelude::*;

use super::protocol::*;
use super::transport::{HOST_PEER, NetCmd, NetLink};
use super::{NetInbox, NetSession};
use crate::character::Remote;
use crate::character::action::{ActionKind, ActiveAction, CharacterAction, action_def};
use crate::character::controller::CharacterController;
use crate::character::guard::{Guard, Parried};
use crate::character::stats::{DamageMessage, Health};
use crate::combat::{Dummy, Hitstop, Hurtbox, Team};
use crate::enemy::{Enemy, EnemiesSuppressed, EnemyKind, warg};
use crate::humanoid::anim::HumanoidAnimator;
use crate::humanoid::{HumanoidSpec, spawn_humanoid};
use crate::items::inventory::{EquipSlot, EquipStats, EquipVisuals, Equipment, Inventory};
use crate::lockon::Targetable;
use crate::player::Player;
use crate::pvp::Pvp;
use crate::tuning::*;
use crate::weapon::{EquipWeapon, EquippedWeapon, WeaponSystems, hollow_blade, warg_fangs};

pub struct SyncPlugin;

impl Plugin for SyncPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (role_changed, receive, drive_puppets, send_states, forward_hits).chain().before(WeaponSystems::Equip));
    }
}

/// States per second each machine sends.
const SEND_RATE: f32 = 30.0;
/// How fast puppets ease toward their reported position / facing (1/s).
const PUPPET_FOLLOW: f32 = 14.0;

/// A character owned by another machine.
#[derive(Component, Debug)]
pub struct Puppet {
    pub id: NetId,
    pub state: BodyState,
    /// Seconds since `state` arrived (for extrapolation).
    age: f32,
    /// Equipment shown, to rebuild the puppet's bag when it changes.
    shown_gear: [Option<crate::items::ItemId>; 3],
    /// Action elapsed as last reported (to tell a new action from the same one).
    last_elapsed: f32,
}

/// Net id of one of the host's real enemies (stable while the entity lives).
fn enemy_id(e: Entity) -> NetId {
    ENEMY_ID_BASE + e.index_u32()
}

#[allow(clippy::too_many_arguments)]
fn body_state(
    id: NetId,
    kind: BodyKind,
    tf: &Transform,
    ctrl: &CharacterController,
    action: &CharacterAction,
    guard: Option<&Guard>,
    health: &Health,
    gear: [Option<crate::items::ItemId>; 3],
    pvp: bool,
) -> BodyState {
    BodyState {
        id,
        kind,
        pos: v3(tf.translation),
        facing: ctrl.facing,
        moved: v3(ctrl.moved),
        vertical: ctrl.velocity.y,
        grounded: ctrl.grounded,
        action: action.current.map(|a| NetAction { kind: a.kind, elapsed: a.elapsed, dir: v3(a.dir), charge: a.charge, weapon: a.weapon }),
        guard: guard.and_then(|g| g.raised_for),
        hp: health.current,
        hp_max: health.max,
        right_hand: gear[0],
        left_hand: gear[1],
        head: gear[2],
        pvp,
    }
}

/// Session became client / went offline: swap the host's enemies for puppets or back.
fn role_changed(
    mut commands: Commands,
    mut inbox: MessageReader<NetInbox>,
    session: Res<NetSession>,
    suppressed: Option<ResMut<EnemiesSuppressed>>,
    real_enemies: Query<Entity, (With<Enemy>, Without<Remote>)>,
    puppets: Query<Entity, With<Puppet>>,
    mut player: Query<(&mut Transform, &mut CharacterController), With<Player>>,
) {
    if !inbox.read().any(|m| matches!(m, NetInbox::RoleChanged)) {
        return;
    }
    // A guest arrives next to the host's start (not inside the host's character).
    if session.role == super::Role::Client
        && let Ok((mut tf, mut ctrl)) = player.single_mut()
    {
        let (x, y, z) = PLAYER_SPAWN;
        let slot = session.my_id.saturating_sub(PLAYER_ID_BASE) as f32;
        tf.translation = Vec3::new(x + 1.6 * slot, y, z);
        ctrl.velocity = Vec3::ZERO;
    }
    let client = session.is_client();
    if let Some(mut s) = suppressed {
        s.0 = client;
    }
    if client {
        // The host's world is the real one: our own enemies go, theirs arrive as puppets.
        for e in &real_enemies {
            commands.entity(e).despawn();
        }
    } else {
        for e in &puppets {
            commands.entity(e).despawn();
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn receive(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut equip_weapon: MessageWriter<EquipWeapon>,
    mut damage: MessageWriter<DamageMessage>,
    mut inbox: MessageReader<NetInbox>,
    session: Res<NetSession>,
    mut puppets: Query<(Entity, &mut Puppet)>,
    mut real_enemies: Query<(Entity, &mut CharacterAction), (With<Enemy>, Without<Remote>)>,
    mut me: Query<&mut CharacterAction, (With<Player>, Without<Enemy>)>,
    dummy: Query<Entity, With<Dummy>>,
    link: Res<NetLink>,
) {
    if !session.online() {
        return;
    }
    let mut seen_snapshot: Option<HashSet<NetId>> = None;
    let mut stun_me = false;
    let mut fresh: Vec<BodyState> = vec![];
    for msg in inbox.read() {
        match msg {
            NetInbox::Datagram(peer, Datagram::State(body)) if session.is_host() => {
                // A guest's own body (only accept the id we gave them).
                if session.peers.get(peer) == Some(&body.id) && body.kind == BodyKind::Player {
                    fresh.push(body.clone());
                }
            }
            NetInbox::Datagram(HOST_PEER, Datagram::Snapshot(bodies)) if session.is_client() => {
                let ids = seen_snapshot.get_or_insert_default();
                for b in bodies.iter().filter(|b| b.id != session.my_id) {
                    ids.insert(b.id);
                    fresh.push(b.clone());
                }
            }
            NetInbox::Reliable(peer, Reliable::Damage { target, amount, from }) if session.is_host() => {
                let attacker = session.peers.get(peer).and_then(|id| puppets.iter().find(|(_, p)| p.id == *id).map(|(e, _)| e));
                if *target == DUMMY_ID {
                    if let Some(e) = dummy.iter().next() {
                        info!("a guest hit the dummy for {amount}");
                        damage.write(DamageMessage { target: e, amount: *amount, from: Some(Vec3::from_array(*from)), attacker });
                    }
                } else if let Some((e, _)) = real_enemies.iter().find(|(e, _)| enemy_id(*e) == *target) {
                    info!("a guest hit enemy {target} for {amount}");
                    damage.write(DamageMessage { target: e, amount: *amount, from: Some(Vec3::from_array(*from)), attacker });
                }
            }
            NetInbox::Reliable(_, Reliable::Parried { attacker }) if session.is_host() => {
                if let Some((_, mut action)) = real_enemies.iter_mut().find(|(e, _)| enemy_id(*e) == *attacker) {
                    action.force(ActionKind::Stunned, Vec3::ZERO);
                    info!("a guest parried enemy {attacker}");
                } else if *attacker == session.my_id {
                    stun_me = true;
                } else if let Some((&peer, _)) = session.peers.iter().find(|(_, id)| **id == *attacker) {
                    // One guest parried another: tell the parried one.
                    link.send(NetCmd::Reliable { to: Some(peer), bytes: encode(&Reliable::Stun) });
                }
            }
            NetInbox::Reliable(HOST_PEER, Reliable::Stun) if session.is_client() => {
                stun_me = true;
            }
            _ => {}
        }
    }
    if stun_me && let Ok(mut action) = me.single_mut() {
        info!("parried by another player: stunned");
        action.force(ActionKind::Stunned, Vec3::ZERO);
    }
    // Upsert puppets (the latest state per body wins; several can arrive in one frame).
    let mut latest: Vec<BodyState> = vec![];
    for body in fresh {
        match latest.iter_mut().find(|b| b.id == body.id) {
            Some(b) => *b = body,
            None => latest.push(body),
        }
    }
    for body in latest {
        match puppets.iter_mut().find(|(_, p)| p.id == body.id) {
            Some((_, mut p)) => {
                p.state = body;
                p.age = 0.0;
            }
            None => spawn_puppet(&mut commands, &mut meshes, &mut materials, &mut equip_weapon, body),
        }
    }
    // Despawn puppets that are gone: missing from the host's snapshot, or (host) a guest who left.
    let mut kept: Vec<NetId> = vec![];
    for (e, p) in &puppets {
        if kept.contains(&p.id) {
            commands.entity(e).despawn(); // a duplicate (shouldn't happen, but never keep two)
            continue;
        }
        kept.push(p.id);
        let gone = match &seen_snapshot {
            Some(ids) => !ids.contains(&p.id),
            None => session.is_host() && !session.peers.values().any(|id| *id == p.id),
        };
        if gone {
            commands.entity(e).despawn();
        }
    }
}

/// Other players wear coloured tunics so you can tell them apart (by player slot).
fn guest_look(id: NetId) -> HumanoidSpec {
    const TUNICS: [Color; 4] =
        [Color::srgb(0.45, 0.12, 0.1), Color::srgb(0.12, 0.25, 0.45), Color::srgb(0.2, 0.38, 0.16), Color::srgb(0.42, 0.34, 0.1)];
    let mut spec = HumanoidSpec::human();
    spec.look.top = TUNICS[(id.saturating_sub(PLAYER_ID_BASE) as usize) % TUNICS.len()];
    spec
}

fn spawn_puppet(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    equip_weapon: &mut MessageWriter<EquipWeapon>,
    body: BodyState,
) {
    let pos = Vec3::from_array(body.pos);
    let name = match body.kind {
        BodyKind::Player => format!("Player {}", body.id),
        BodyKind::Hollow => "Hollow (remote)".to_string(),
        BodyKind::Warg => "Warg (remote)".to_string(),
    };
    let e = commands
        .spawn((
            Name::new(name),
            Remote,
            Puppet { id: body.id, state: body.clone(), age: 0.0, shown_gear: [None; 3], last_elapsed: 0.0 },
            Transform::from_translation(pos).with_rotation(Quat::from_rotation_y(body.facing)),
            Visibility::default(),
            CharacterController { facing: body.facing, ..default() },
            CharacterAction::default(),
            Guard::default(),
            Health { current: body.hp, max: body.hp_max },
            // Solid, and something for weapons to hit.
            RigidBody::Kinematic,
            Collider::capsule(PLAYER_RADIUS, PLAYER_CAPSULE_LENGTH),
        ))
        .id();
    let humanoid = |commands: &mut Commands, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>, spec: HumanoidSpec| {
        let rig = spawn_humanoid(commands, meshes, materials, e, &spec, PLAYER_HALF_HEIGHT);
        commands.entity(e).insert((spec, rig, HumanoidAnimator::default()));
    };
    match body.kind {
        BodyKind::Player => {
            humanoid(commands, meshes, materials, guest_look(body.id));
            commands.entity(e).insert((
                Team::Player,
                Pvp(body.pvp),
                Inventory::default(),
                Equipment::default(),
                EquipStats::default(),
                EquipVisuals::default(),
            ));
        }
        BodyKind::Hollow => {
            humanoid(commands, meshes, materials, HumanoidSpec::hollow());
            commands.entity(e).insert((
                Team::Hostile,
                Hurtbox,
                Targetable { height: 0.55 },
                Enemy { home: pos, patrol: None, kind: EnemyKind::Hollow },
            ));
            equip_weapon.write(EquipWeapon { character: e, def: &hollow_blade::HOLLOW_SWORD });
        }
        BodyKind::Warg => {
            // Its bites and pounce run from the replicated actions; they only catch *our* player.
            warg::attach_warg_body(commands, meshes, materials, e);
            commands.entity(e).insert((
                Team::Hostile,
                Hurtbox,
                Targetable { height: warg::WARG_TARGET_HEIGHT },
                Enemy { home: pos, patrol: None, kind: EnemyKind::Warg },
                EquippedWeapon::natural(&warg_fangs::WARG_FANGS),
            ));
        }
    }
    info!("puppet {} appeared", body.id);
}

#[allow(clippy::type_complexity)]
fn drive_puppets(
    time: Res<Time>,
    mut puppets: Query<(
        &mut Puppet,
        &mut Transform,
        &mut CharacterController,
        &mut CharacterAction,
        &mut Guard,
        &mut Health,
        Option<(&mut Inventory, &mut Equipment)>,
        Option<&mut Pvp>,
    )>,
) {
    let dt = time.delta_secs();
    let follow = 1.0 - (-PUPPET_FOLLOW * dt).exp();
    for (mut p, mut tf, mut ctrl, mut action, mut guard, mut health, gear, pvp) in &mut puppets {
        p.age += dt;
        let s = p.state.clone();
        // Position: where it was reported, extrapolated by its movement (briefly), eased.
        let moved = Vec3::from_array(s.moved);
        let target = Vec3::from_array(s.pos) + moved * p.age.min(0.2);
        tf.translation = tf.translation.lerp(target, follow);
        if tf.translation.distance(target) > 4.0 {
            tf.translation = target; // teleported (respawn): snap
        }
        let d = (s.facing - ctrl.facing + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
        ctrl.facing += d * follow;
        tf.rotation = Quat::from_rotation_y(ctrl.facing);
        ctrl.moved = moved;
        ctrl.wish_speed = moved.with_y(0.0).length();
        ctrl.velocity.y = s.vertical;
        ctrl.grounded = s.grounded;

        // Action: keep ticking locally (smooth), resync when it drifts or a new one starts.
        match s.action {
            Some(a) => {
                let new = action.current.is_none_or(|c| c.kind != a.kind) || a.elapsed + 0.05 < p.last_elapsed;
                if new {
                    let mut started = ActiveAction::new(a.kind, Vec3::from_array(a.dir), a.weapon);
                    started.def = action_def(a.kind, a.weapon);
                    started.elapsed = a.elapsed;
                    started.charge = a.charge;
                    action.start(started);
                } else if let Some(cur) = action.current.as_mut()
                    && (cur.elapsed - a.elapsed).abs() > 0.12
                {
                    cur.elapsed = a.elapsed;
                }
                p.last_elapsed = a.elapsed;
            }
            None => {
                // Dead never ends by itself; a revived body reports no action.
                if action.current.is_some_and(|c| c.kind == ActionKind::Dead) {
                    action.revive();
                }
                p.last_elapsed = 0.0;
            }
        }
        if let Some(mut pvp) = pvp {
            pvp.set_if_neq(Pvp(s.pvp));
        }
        if guard.raised_for != s.guard {
            guard.raised_for = s.guard;
        }
        if health.current != s.hp || health.max != s.hp_max {
            *health = Health { current: s.hp, max: s.hp_max };
        }
        // Equipment: rebuild the puppet's tiny bag when it changes (weapon/shield/hood models follow).
        let shown = [s.right_hand, s.left_hand, s.head];
        if let Some((mut inv, mut eq)) = gear
            && shown != p.shown_gear
        {
            p.shown_gear = shown;
            *inv = Inventory { items: shown.iter().flatten().copied().collect(), ..default() };
            let mut next = Equipment::default();
            for i in 0..inv.items.len() {
                next.equip(&inv, i);
            }
            *eq = next;
        }
    }
}

#[allow(clippy::type_complexity)]
fn send_states(
    real: Res<Time<Real>>,
    mut timer: Local<f32>,
    link: Res<NetLink>,
    session: Res<NetSession>,
    player: Query<(&Transform, &CharacterController, &CharacterAction, &Guard, &Health, &Inventory, &Equipment, &Pvp), With<Player>>,
    puppets: Query<&Puppet>,
    enemies: Query<(Entity, &Enemy, &Transform, &CharacterController, &CharacterAction, &Health), Without<Remote>>,
) {
    if !session.online() {
        return;
    }
    *timer += real.delta_secs();
    if *timer < 1.0 / SEND_RATE {
        return;
    }
    *timer = 0.0;
    let Ok((tf, ctrl, action, guard, health, inv, eq, pvp)) = player.single() else { return };
    let gear = [EquipSlot::RightHand, EquipSlot::LeftHand, EquipSlot::Head].map(|s| eq.item(s, inv));
    let me = body_state(session.my_id, BodyKind::Player, tf, ctrl, action, Some(guard), health, gear, pvp.0);
    if session.is_host() {
        let mut bodies = vec![me];
        bodies.extend(puppets.iter().filter(|p| p.state.kind == BodyKind::Player).map(|p| p.state.clone()));
        for (e, enemy, tf, ctrl, action, health) in &enemies {
            let kind = match enemy.kind {
                EnemyKind::Hollow => BodyKind::Hollow,
                EnemyKind::Warg => BodyKind::Warg,
            };
            bodies.push(body_state(enemy_id(e), kind, tf, ctrl, action, None, health, [None; 3], false));
        }
        link.send(NetCmd::Datagram { to: None, bytes: encode(&Datagram::Snapshot(bodies)) });
    } else if session.role == super::Role::Client {
        link.send(NetCmd::Datagram { to: Some(HOST_PEER), bytes: encode(&Datagram::State(me)) });
    }
}

/// Guest side: my hits on remote enemies and my parries of them go to the host.
fn forward_hits(
    link: Res<NetLink>,
    session: Res<NetSession>,
    mut hitstop: ResMut<Hitstop>,
    mut damage: MessageReader<DamageMessage>,
    mut parries: MessageReader<Parried>,
    puppets: Query<&Puppet>,
    dummies: Query<(), With<Dummy>>,
) {
    if session.is_host() {
        // The host parried a guest: stun them on their machine.
        damage.clear();
        for parry in parries.read() {
            if let Some(p) = parry.attacker.and_then(|a| puppets.get(a).ok())
                && let Some((&peer, _)) = session.peers.iter().find(|(_, id)| **id == p.id)
            {
                link.send(NetCmd::Reliable { to: Some(peer), bytes: encode(&Reliable::Stun) });
            }
        }
        return;
    }
    if session.role != super::Role::Client {
        damage.clear();
        parries.clear();
        return;
    }
    for d in damage.read() {
        let target = match puppets.get(d.target) {
            Ok(p) => p.id,
            Err(_) if dummies.contains(d.target) => DUMMY_ID,
            Err(_) => continue,
        };
        let from = d.from.unwrap_or_default();
        link.send(NetCmd::Reliable { to: Some(HOST_PEER), bytes: encode(&Reliable::Damage { target, amount: d.amount, from: v3(from) }) });
        hitstop.remaining = hitstop.remaining.max(HITSTOP_LIGHT);
    }
    for parry in parries.read() {
        if let Some(p) = parry.attacker.and_then(|a| puppets.get(a).ok()) {
            link.send(NetCmd::Reliable { to: Some(HOST_PEER), bytes: encode(&Reliable::Parried { attacker: p.id }) });
        }
    }
}
