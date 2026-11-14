//! The Hearth (the bonfire equivalent) and the death → respawn flow.
//!
//! - **Resting:** near the Hearth the HUD shows "[E] Rest at Hearth" (or "Kindle" the first
//!   time). E starts `ActionKind::Rest` (kneel, rest, stand up). When the rest begins (its
//!   active phase) HP, stamina and Healing Ale are restored, the Hearth becomes the respawn
//!   point and the world resets ([`WorldReset`]: the training dummy stands back up).
//! - **Death:** `ActionKind::Dead` never ends by itself. [`DeathSequence`] runs the familiar
//!   flow: fall → "YOU DIED" → fade to black → respawn at the Hearth with everything restored
//!   → fade back in. The teleport happens in `Update` while the screen is black, which also
//!   resets transform interpolation (no smear across the arena, N11).

use std::f32::consts::TAU;

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::character::action::{ActionKind, ActionPhase, ActionRequest, CharacterAction};
use crate::character::controller::CharacterController;
use crate::character::stats::{Health, Stamina};
use crate::character::{ActionIntent, CharacterSystems};
use crate::healing::HealingAle;
use crate::input::PlayerInput;
use crate::player::Player;
use crate::tuning::*;
use crate::ui::Banner;

pub struct HearthPlugin;

impl Plugin for HearthPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<WorldReset>()
            .init_resource::<InteractPrompt>()
            .add_systems(Startup, spawn_hearth)
            .add_systems(FixedUpdate, rest_at_hearth.after(CharacterSystems::Actions).before(CharacterSystems::Stats))
            .add_systems(Update, (hearth_interaction, start_death_sequence, run_death_sequence, animate_hearth));
    }
}

#[derive(Component, Debug)]
pub struct Hearth {
    /// Unlit Hearths only smoulder; the first rest kindles them.
    pub kindled: bool,
}

impl Hearth {
    /// Where a character respawns: in front of the Hearth, facing the arena centre.
    pub fn respawn_spot(tf: &Transform) -> (Vec3, f32) {
        let pos = tf.translation.with_y(0.0);
        let to_centre = (-pos).normalize_or(Vec3::NEG_Z);
        let spot = pos + to_centre * HEARTH_RESPAWN_OFFSET;
        (spot, f32::atan2(-to_centre.x, -to_centre.z))
    }
}

/// Where this character comes back after dying (floor position, facing). Set by resting.
#[derive(Component, Debug, Clone, Copy)]
pub struct RespawnPoint {
    pub position: Vec3,
    pub facing: f32,
}

/// Something restored the world (a rest or a respawn): enemies and dummies reset.
#[derive(Message, Debug, Clone, Copy)]
pub struct WorldReset;

/// Interaction hint for the HUD ("[E] Rest at Hearth"), `None` when nothing is in reach.
#[derive(Resource, Default, Debug)]
pub struct InteractPrompt(pub Option<&'static str>);

/// Running on a dead player: seconds since death. Removed after the fade-in.
#[derive(Component, Debug, Default)]
pub struct DeathSequence {
    pub t: f32,
    pub respawned: bool,
}

/// (YOU DIED text alpha, black screen alpha) `t` seconds after death.
pub fn death_screen(t: f32) -> (f32, f32) {
    let respawn_at = DEATH_FADE_OUT_AT + DEATH_FADE_OUT;
    let text = ((t - DEATH_TEXT_DELAY) / DEATH_TEXT_FADE).clamp(0.0, 1.0);
    let black = if t < respawn_at {
        ((t - DEATH_FADE_OUT_AT) / DEATH_FADE_OUT).clamp(0.0, 1.0)
    } else {
        1.0 - ((t - respawn_at) / RESPAWN_FADE_IN).clamp(0.0, 1.0)
    };
    // The text leaves with the darkness: gone once the screen is black.
    let text = if t < respawn_at { text } else { 0.0 };
    (text, black)
}

/// Full HP, stamina and Ale.
fn replenish(health: &mut Health, stamina: &mut Stamina, ale: Option<Mut<HealingAle>>) {
    *health = Health::new(health.max);
    *stamina = Stamina::new(stamina.max);
    if let Some(mut ale) = ale {
        ale.refill();
    }
}

// ── The Hearth ───────────────────────────────────────────────────────────

/// Fire parts that flicker: flame cones (scaled) and the light (intensity).
#[derive(Component)]
struct HearthFlame {
    phase: f32,
    base: Vec3,
}

#[derive(Component)]
struct HearthLight;

/// Glowing coals; their emissive strength follows the fire.
#[derive(Component)]
struct HearthCoals(Handle<StandardMaterial>);

fn spawn_hearth(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let stone = materials.add(StandardMaterial {
        base_color: Color::srgb(0.36, 0.34, 0.31),
        perceptual_roughness: 0.95,
        ..default()
    });
    let ash = materials.add(StandardMaterial { base_color: Color::srgb(0.09, 0.08, 0.08), perceptual_roughness: 1.0, ..default() });
    let wood = materials.add(StandardMaterial { base_color: Color::srgb(0.2, 0.12, 0.07), perceptual_roughness: 0.9, ..default() });
    let coals = materials.add(StandardMaterial {
        base_color: Color::srgb(0.25, 0.08, 0.02),
        emissive: LinearRgba::rgb(3.0, 0.7, 0.1),
        ..default()
    });
    let flame = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.55, 0.15),
        emissive: LinearRgba::rgb(20.0, 6.5, 1.0),
        unlit: true,
        ..default()
    });
    let (x, z) = HEARTH_POSITION;
    let hearth = commands
        .spawn((
            Name::new("Hearth"),
            Hearth { kindled: false },
            Transform::from_xyz(x, 0.0, z),
            Visibility::default(),
            RigidBody::Static,
        ))
        .id();
    let mut part = |commands: &mut Commands, mesh: Mesh, mat: &Handle<StandardMaterial>, t: Transform| {
        commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(mat.clone()), t, ChildOf(hearth))).id()
    };
    // Low ring of stones around an ash bed.
    part(&mut commands, Cylinder::new(0.58, 0.04).into(), &ash, Transform::from_xyz(0.0, 0.02, 0.0));
    for i in 0..10 {
        let a = i as f32 / 10.0 * TAU;
        let s = 0.85 + 0.3 * ((i * 7 % 5) as f32 / 4.0);
        part(
            &mut commands,
            Cuboid::new(0.3 * s, 0.2 * s, 0.2).into(),
            &stone,
            Transform::from_xyz(a.cos() * 0.62, 0.09 * s, a.sin() * 0.62).with_rotation(Quat::from_rotation_y(-a + 1.57)),
        );
    }
    let coal_entity = part(&mut commands, Cylinder::new(0.36, 0.06).into(), &coals, Transform::from_xyz(0.0, 0.05, 0.0));
    commands.entity(coal_entity).insert(HearthCoals(coals.clone()));
    // Three logs leaning into a teepee.
    for i in 0..3 {
        let a = i as f32 / 3.0 * TAU + 0.4;
        let dir = Vec3::new(a.cos(), 0.0, a.sin());
        part(
            &mut commands,
            Cylinder::new(0.055, 0.75).into(),
            &wood,
            Transform::from_translation(dir * 0.2 + Vec3::Y * 0.25)
                .with_rotation(Quat::from_axis_angle(dir.cross(Vec3::Y).normalize(), -0.75)),
        );
    }
    // Flames: a tall one in the middle and two smaller licks.
    for (i, (offset, size)) in [(Vec3::new(0.0, 0.0, 0.0), 1.0), (Vec3::new(0.14, 0.0, 0.06), 0.65), (Vec3::new(-0.12, 0.0, -0.08), 0.7)]
        .into_iter()
        .enumerate()
    {
        let base = Vec3::new(size, size, size);
        let e = part(
            &mut commands,
            Cone { radius: 0.17, height: 0.6 }.into(),
            &flame,
            Transform::from_translation(offset + Vec3::Y * 0.36).with_scale(base),
        );
        commands.entity(e).insert(HearthFlame { phase: i as f32 * 2.1, base });
    }
    commands.spawn((
        Name::new("Hearth Light"),
        HearthLight,
        PointLight {
            color: Color::srgb(1.0, 0.52, 0.18),
            intensity: 0.0,
            range: 11.0,
            radius: 0.2,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(0.0, 0.75, 0.0),
        ChildOf(hearth),
    ));
    // Solid, so nobody walks through the fire.
    commands.spawn((Collider::cylinder(0.75, 1.2), Transform::from_xyz(0.0, 0.6, 0.0), ChildOf(hearth)));
}

/// Fire flicker. Smouldering (unlit) Hearths have tiny flames and a dim glow; kindling
/// grows them over a second.
fn animate_hearth(
    time: Res<Time>,
    mut size: Local<f32>,
    hearths: Query<&Hearth>,
    mut flames: Query<(&HearthFlame, &mut Transform)>,
    mut lights: Query<&mut PointLight, With<HearthLight>>,
    coals: Query<&HearthCoals>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let t = time.elapsed_secs();
    let kindled = hearths.iter().any(|h| h.kindled);
    *size += ((if kindled { 1.0 } else { 0.0 }) - *size) * (1.0 - (-2.5 * time.delta_secs()).exp());
    let fire = 0.25 + 0.75 * *size;
    let flicker = |p: f32| 1.0 + 0.12 * (t * 8.1 + p).sin() + 0.07 * (t * 14.3 + p * 1.9).sin() + 0.05 * (t * 21.7 + p * 3.1).sin();
    for (flame, mut tf) in &mut flames {
        let f = flicker(flame.phase);
        tf.scale = flame.base * Vec3::new(0.9 + 0.1 * f, f, 0.9 + 0.1 * f) * fire.powf(1.5);
    }
    for mut light in &mut lights {
        light.intensity = (30_000.0 + 420_000.0 * *size) * flicker(0.7);
    }
    for c in &coals {
        if let Some(mut mat) = materials.get_mut(&c.0) {
            let g = (1.0 + 2.5 * *size) * flicker(4.0);
            mat.emissive = LinearRgba::rgb(3.0 * g, 0.7 * g, 0.1 * g);
        }
    }
}

// ── Resting ──────────────────────────────────────────────────────────────

/// Shows the prompt near a Hearth and turns E into a rest request.
fn hearth_interaction(
    mut prompt: ResMut<InteractPrompt>,
    hearths: Query<(&Hearth, &Transform)>,
    mut player: Query<
        (&Transform, &CharacterAction, &CharacterController, &mut PlayerInput, &mut ActionIntent, Has<DeathSequence>),
        With<Player>,
    >,
) {
    prompt.0 = None;
    let Ok((tf, action, ctrl, mut input, mut intent, dying)) = player.single_mut() else { return };
    let interact = std::mem::take(&mut input.interact);
    let near = hearths
        .iter()
        .find(|(_, h)| (h.translation - tf.translation).with_y(0.0).length() < HEARTH_RANGE);
    let Some((hearth, _)) = near else { return };
    if dying || action.current.is_some() || !ctrl.grounded {
        return;
    }
    prompt.0 = Some(if hearth.kindled { "[E] Rest at Hearth" } else { "[E] Kindle the Hearth" });
    if interact {
        intent.buffer(ActionRequest::Rest);
    }
}

/// Which rest (action serial) was already applied.
#[derive(Component, Default)]
struct RestApplied(Option<u32>);

/// While kneeling: turn toward the Hearth; when the rest begins, restore everything.
#[allow(clippy::type_complexity)]
fn rest_at_hearth(
    time: Res<Time>,
    mut commands: Commands,
    mut banner: MessageWriter<Banner>,
    mut reset: MessageWriter<WorldReset>,
    mut hearths: Query<(&mut Hearth, &Transform), Without<CharacterController>>,
    mut resters: Query<(
        Entity,
        &CharacterAction,
        &Transform,
        &mut CharacterController,
        &mut Health,
        &mut Stamina,
        Option<&mut HealingAle>,
        Option<&mut RestApplied>,
    )>,
) {
    for (entity, action, tf, mut ctrl, mut health, mut stamina, ale, applied) in &mut resters {
        let Some(cur) = action.current.filter(|a| a.kind == ActionKind::Rest) else { continue };
        let Some((mut hearth, htf)) = hearths
            .iter_mut()
            .min_by(|a, b| a.1.translation.distance(tf.translation).total_cmp(&b.1.translation.distance(tf.translation)))
        else {
            continue;
        };
        // Face the fire (smoothly; movement is locked, so nothing else turns us).
        let to = (htf.translation - tf.translation).with_y(0.0);
        if to.length_squared() > 1e-4 {
            let target = f32::atan2(-to.x, -to.z);
            let d = (target - ctrl.facing + std::f32::consts::PI).rem_euclid(TAU) - std::f32::consts::PI;
            let step = TURN_SPEED * 0.6 * time.delta_secs();
            ctrl.facing += d.clamp(-step, step);
        }
        let Some(mut applied) = applied else {
            commands.entity(entity).insert(RestApplied::default());
            continue;
        };
        if cur.phase() != ActionPhase::Active || applied.0 == Some(action.serial) {
            continue;
        }
        applied.0 = Some(action.serial);
        replenish(&mut health, &mut stamina, ale);
        let (position, facing) = Hearth::respawn_spot(htf);
        commands.entity(entity).insert(RespawnPoint { position, facing });
        reset.write(WorldReset);
        if !hearth.kindled {
            hearth.kindled = true;
            banner.write(Banner("HEARTH KINDLED"));
            info!("hearth kindled");
        }
        info!("rested at the hearth: HP, stamina and Healing Ale restored");
    }
}

// ── Death and respawn ────────────────────────────────────────────────────

fn start_death_sequence(mut commands: Commands, dead: Query<(Entity, &CharacterAction), (With<Player>, Without<DeathSequence>)>) {
    for (entity, action) in &dead {
        if matches!(action.current, Some(a) if a.kind == ActionKind::Dead) {
            commands.entity(entity).insert(DeathSequence::default());
            info!("YOU DIED");
        }
    }
}

#[allow(clippy::type_complexity)]
fn run_death_sequence(
    time: Res<Time>,
    mut commands: Commands,
    mut reset: MessageWriter<WorldReset>,
    hearths: Query<&Transform, (With<Hearth>, Without<Player>)>,
    mut dying: Query<
        (
            Entity,
            &mut DeathSequence,
            &mut Transform,
            &mut CharacterController,
            &mut CharacterAction,
            &mut ActionIntent,
            &mut Health,
            &mut Stamina,
            Option<&mut HealingAle>,
            Option<&RespawnPoint>,
        ),
        With<Player>,
    >,
) {
    for (entity, mut seq, mut tf, mut ctrl, mut action, mut intent, mut health, mut stamina, ale, point) in &mut dying {
        seq.t += time.delta_secs();
        if !seq.respawned && seq.t >= DEATH_FADE_OUT_AT + DEATH_FADE_OUT {
            // Fully black: come back at the last Hearth rested at (or the first Hearth).
            let (position, facing) = match point {
                Some(p) => (p.position, p.facing),
                None => hearths.iter().next().map_or((tf.translation.with_y(0.0), ctrl.facing), Hearth::respawn_spot),
            };
            tf.translation = position + Vec3::Y * (PLAYER_HALF_HEIGHT + GROUND_SKIN);
            ctrl.velocity = Vec3::ZERO;
            ctrl.facing = facing;
            tf.rotation = Quat::from_rotation_y(facing);
            action.revive();
            intent.buffered = None;
            replenish(&mut health, &mut stamina, ale);
            reset.write(WorldReset);
            seq.respawned = true;
            info!("respawned at {:.2}", tf.translation);
        }
        if seq.t >= DEATH_FADE_OUT_AT + DEATH_FADE_OUT + RESPAWN_FADE_IN {
            commands.entity(entity).remove::<DeathSequence>();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn death_screen_flow() {
        let respawn = DEATH_FADE_OUT_AT + DEATH_FADE_OUT;
        assert_eq!(death_screen(0.2), (0.0, 0.0), "watch the fall first");
        assert_eq!(death_screen(DEATH_TEXT_DELAY + DEATH_TEXT_FADE + 0.1), (1.0, 0.0), "YOU DIED");
        assert_eq!(death_screen(respawn - 1e-3).1.round(), 1.0, "black before the respawn");
        assert_eq!(death_screen(respawn + 1e-3).0, 0.0, "text gone after the respawn");
        assert_eq!(death_screen(respawn + RESPAWN_FADE_IN + 0.1), (0.0, 0.0), "faded back in");
    }

    #[test]
    fn respawn_spot_faces_the_centre() {
        let (pos, facing) = Hearth::respawn_spot(&Transform::from_xyz(0.0, 0.0, 12.0));
        assert!((pos - Vec3::new(0.0, 0.0, 12.0 - HEARTH_RESPAWN_OFFSET)).length() < 1e-4);
        // Facing 0 looks along -Z, toward the centre from +Z.
        assert!(facing.abs() < 1e-4, "{facing}");
    }
}
