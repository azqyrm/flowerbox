//! The Hearth (the bonfire equivalent, shown as a sunflower) and the death → respawn flow.
//! Player-facing wording lives in `ui::strings`.
//!
//! - **Resting:** near the Hearth the HUD shows a rest prompt (a "tend" prompt the first
//!   time, while the sunflower still droops; the first rest makes it stand up). E starts `ActionKind::Rest` (kneel, rest, stand up). When the rest begins (its
//!   active phase) HP, stamina and Healing Ale are restored, the Hearth becomes the respawn
//!   point and the world resets ([`WorldReset`]: the training dummy stands back up).
//! - **Death:** `ActionKind::Dead` never ends by itself. [`DeathSequence`] runs the familiar
//!   flow: fall → death band (`strings::DEATH`) → fade to black → respawn at the Hearth with everything restored
//!   → fade back in. The teleport happens in `Update` while the screen is black, which also
//!   resets transform interpolation (no smear across the arena, N11).

use std::f32::consts::TAU;

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::character::action::{ActionKind, ActionPhase, ActionRequest, ActiveAction, CharacterAction};
use crate::character::controller::CharacterController;
use crate::character::stats::{Health, Stamina};
use crate::character::{ActionIntent, CharacterSystems, MoveIntent};
use crate::healing::HealingAle;
use crate::input::{GameplayBlocked, PlayerInput};
use crate::interact::{InteractPrompt, InteractSystems};
use crate::player::Player;
use crate::tuning::*;
use crate::ui::{Banner, strings};

pub struct HearthPlugin;

impl Plugin for HearthPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<WorldReset>()
            .add_message::<SunRevived>()
            .add_message::<RestComplete>()
            .init_resource::<SharedRest>()
            .add_systems(Startup, spawn_hearth)
            .add_systems(FixedUpdate, (rest_at_hearth, complete_rest).chain().after(CharacterSystems::Actions).before(CharacterSystems::Stats))
            .add_systems(Update, (hearth_interaction.in_set(InteractSystems::Hearth), start_death_sequence, run_death_sequence, animate_hearth));
    }
}

/// A rest site (drawn as a sunflower). Several can exist (the arena's is id 0; levels add more).
#[derive(Component, Debug)]
pub struct Hearth {
    /// Stable id (saves, online sync).
    pub id: u32,
    /// Unrested Hearths droop; the first rest revives ("kindles") them.
    pub kindled: bool,
    /// Shown revival 0..1 (eases toward `kindled`).
    revive: f32,
}

impl Hearth {
    /// Where a character respawns: in front of the flower (the way it faces), on its floor.
    pub fn respawn_spot(tf: &Transform) -> (Vec3, f32) {
        let forward = (tf.rotation * Vec3::NEG_Z).with_y(0.0).normalize_or(Vec3::NEG_Z);
        let spot = tf.translation + forward * HEARTH_RESPAWN_OFFSET;
        (spot, f32::atan2(-forward.x, -forward.z))
    }
}

/// The animated parts of one sunflower.
#[derive(Component)]
struct SunflowerParts {
    segments: Vec<Entity>,
    head: Entity,
    petals: Vec<Entity>,
    leaves: Vec<Entity>,
    light: Entity,
}

/// Where this character comes back after dying (floor position, facing). Set by resting.
#[derive(Component, Debug, Clone, Copy)]
pub struct RespawnPoint {
    pub position: Vec3,
    pub facing: f32,
}

/// The rest at the sunflower is complete: enemies respawn ([`WorldReset`]) and the resting
/// player is restored. Offline it follows the start of the rest at once; online the host sends it
/// when every player rests (`net::world`).
#[derive(Message, Debug, Clone, Copy)]
pub struct RestComplete;

/// Online: the rest only completes when every player rests, decided by the host. Offline: false.
#[derive(Resource, Default, Debug, PartialEq)]
pub struct SharedRest(pub bool);

/// The local player just revived the sunflower (online, the host tells everyone: `net::world`).
#[derive(Message, Debug, Clone, Copy)]
pub struct SunRevived {
    pub id: u32,
}

/// Something restored the world (a rest or a respawn): enemies and dummies reset.
#[derive(Message, Debug, Clone, Copy)]
pub struct WorldReset;

/// Running on a dead player: seconds since death. Removed after the fade-in.
#[derive(Component, Debug, Default)]
pub struct DeathSequence {
    pub t: f32,
    pub respawned: bool,
}

/// (death text alpha, black screen alpha) `t` seconds after death.
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

// ── The Hearth: a sunflower ──────────────────────────────────────────────
//
// An unrested Hearth is a depressed sunflower: the stem bowed over, the head hanging toward
// the ground, petals wilted back and dull. Resting by it for the first time revives it: over
// ~2 s it straightens up (with a little overshoot), lifts its face toward the arena, and its
// petals open and turn gold. Its warm light grows with it.

/// Stem segments from the soil up.
const STEM_SEGMENTS: usize = 7;
const STEM_SEGMENT_LENGTH: f32 = 0.32;
/// Per-segment bend (rad about local X; negative bows forward, toward the arena), drooping…
const STEM_DROOP: [f32; STEM_SEGMENTS] = [-0.04, -0.1, -0.18, -0.32, -0.45, -0.55, -0.6];
/// …and standing (a slight natural curve).
const STEM_UPRIGHT: [f32; STEM_SEGMENTS] = [0.0, 0.01, 0.02, 0.02, 0.03, 0.04, 0.05];
/// Head pitch relative to the stem tip (positive = face looks up).
const HEAD_DROOP: f32 = -0.5;
const HEAD_UPRIGHT: f32 = 0.3;
/// Petal fold (rad about the petal's base on the rim of the seed disc; negative = toward the face).
/// Wilted petals hang forward past the face, which looks at the ground while the head droops (a
/// fold toward the back made them stick up over the bowed head, detached from it). Open, they cup
/// forward a little.
const PETAL_DROOP: f32 = -1.25;
const PETAL_UPRIGHT: f32 = -0.12;
/// Petals are hinged on this circle (just inside the seed disc's edge), and half as long as this.
const PETAL_RIM: f32 = 0.23;
const PETAL_HALF_LENGTH: f32 = 0.15;
/// Leaf tilt (rad; negative hangs down).
const LEAF_DROOP: f32 = -1.0;
const LEAF_UPRIGHT: f32 = 0.35;
const PETALS: usize = 20;
/// How long the sunflower takes to stand up once revived.
const STAND_UP_TIME: f32 = 2.2;

#[derive(Component)]
struct StemSegment(usize);

#[derive(Component)]
struct FlowerHead;

#[derive(Component)]
struct Petal {
    /// Small per-petal variation so the wilt looks organic.
    jitter: f32,
    around: f32,
}

#[derive(Component)]
struct Leaf {
    yaw: f32,
}

#[derive(Component)]
struct HearthLight;

/// Materials that change colour as the sunflower revives.
#[derive(Component)]
struct SunflowerMaterials {
    petal: Handle<StandardMaterial>,
    stem: Handle<StandardMaterial>,
}

/// Wilted and vigorous colours (sRGB).
const PETAL_WILTED: [f32; 3] = [0.5, 0.38, 0.15];
const PETAL_VIGOROUS: [f32; 3] = [1.0, 0.76, 0.08];
const STEM_WILTED: [f32; 3] = [0.3, 0.3, 0.14];
const STEM_VIGOROUS: [f32; 3] = [0.22, 0.48, 0.13];

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> Color {
    Color::srgb(a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t)
}

/// Ease with a small overshoot, so the flower springs up rather than slides.
fn ease_out_back(x: f32) -> f32 {
    let c1 = 1.3;
    let c3 = c1 + 1.0;
    let x = x.clamp(0.0, 1.0) - 1.0;
    1.0 + c3 * x * x * x + c1 * x * x
}

/// The arena's sunflower (id 0), facing the arena centre.
fn spawn_hearth(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let (x, z) = HEARTH_POSITION;
    let to_centre = Vec3::new(-x, 0.0, -z).normalize_or(Vec3::NEG_Z);
    let facing = f32::atan2(-to_centre.x, -to_centre.z);
    spawn_sunflower(&mut commands, &mut meshes, &mut materials, 0, Vec3::new(x, 0.0, z), facing);
}

/// Spawns a rest site at `pos` (on the floor), its face toward `facing` (yaw, 0 = -Z). Reusable
/// for levels (`crate::level`).
pub fn spawn_sunflower(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    id: u32,
    pos: Vec3,
    facing: f32,
) -> Entity {
    let stone = materials.add(StandardMaterial {
        base_color: Color::srgb(0.36, 0.34, 0.31),
        perceptual_roughness: 0.95,
        ..default()
    });
    let soil = materials.add(StandardMaterial { base_color: Color::srgb(0.13, 0.09, 0.06), perceptual_roughness: 1.0, ..default() });
    let stem = materials.add(StandardMaterial { base_color: mix(STEM_WILTED, STEM_WILTED, 0.0), perceptual_roughness: 0.8, ..default() });
    let petal = materials.add(StandardMaterial {
        base_color: mix(PETAL_WILTED, PETAL_WILTED, 0.0),
        perceptual_roughness: 0.7,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    let seeds = materials.add(StandardMaterial { base_color: Color::srgb(0.16, 0.09, 0.04), perceptual_roughness: 0.95, ..default() });
    let calyx = materials.add(StandardMaterial { base_color: Color::srgb(0.2, 0.32, 0.1), perceptual_roughness: 0.85, ..default() });

    let hearth = commands
        .spawn((
            Name::new(format!("Hearth {id} (Sunflower)")),
            Hearth { id, kindled: false, revive: 0.0 },
            SunflowerMaterials { petal: petal.clone(), stem: stem.clone() },
            // Local -Z faces `facing`, so the flower bows (and later looks) that way.
            Transform::from_translation(pos).with_rotation(Quat::from_rotation_y(facing)),
            Visibility::default(),
            RigidBody::Static,
        ))
        .id();
    let mut mesh = |commands: &mut Commands, mesh: Mesh, mat: &Handle<StandardMaterial>, t: Transform, parent: Entity| {
        commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(mat.clone()), t, ChildOf(parent))).id()
    };
    let pivot = |commands: &mut Commands, t: Transform, parent: Entity| commands.spawn((t, Visibility::default(), ChildOf(parent))).id();

    // A ring of stones around a mound of soil.
    mesh(commands, Sphere::new(1.0).into(), &soil, Transform::from_xyz(0.0, 0.0, 0.0).with_scale(Vec3::new(0.58, 0.12, 0.58)), hearth);
    for i in 0..10 {
        let a = i as f32 / 10.0 * TAU;
        let s = 0.85 + 0.3 * ((i * 7 % 5) as f32 / 4.0);
        mesh(
            commands,
            Cuboid::new(0.3 * s, 0.2 * s, 0.2).into(),
            &stone,
            Transform::from_xyz(a.cos() * 0.62, 0.09 * s, a.sin() * 0.62).with_rotation(Quat::from_rotation_y(-a + 1.57)),
            hearth,
        );
    }

    // The stem: a chain of pivots, each bending a little.
    let mut parent = hearth;
    let mut at = Vec3::Y * 0.08;
    let (mut segments, mut petals, mut leaves) = (vec![], vec![], vec![]);
    for i in 0..STEM_SEGMENTS {
        let seg = commands
            .spawn((StemSegment(i), Transform::from_translation(at), Visibility::default(), ChildOf(parent)))
            .id();
        segments.push(seg);
        let r = 0.05 - 0.003 * i as f32;
        mesh(
            commands,
            Cylinder::new(r, STEM_SEGMENT_LENGTH + 0.02).into(),
            &stem,
            Transform::from_xyz(0.0, STEM_SEGMENT_LENGTH * 0.5, 0.0),
            seg,
        );
        if i == 1 || i == 3 {
            // A pair of leaves, alternating sides.
            for side in [0.0, std::f32::consts::PI] {
                let yaw = side + if i == 1 { 0.3 } else { 1.7 };
                let leaf = commands
                    .spawn((Leaf { yaw }, Transform::from_xyz(0.0, STEM_SEGMENT_LENGTH * 0.5, 0.0), Visibility::default(), ChildOf(seg)))
                    .id();
                leaves.push(leaf);
                mesh(
                    commands,
                    Sphere::new(1.0).into(),
                    &stem,
                    Transform::from_xyz(0.2, 0.0, 0.0).with_scale(Vec3::new(0.21, 0.014, 0.1)),
                    leaf,
                );
            }
        }
        parent = seg;
        at = Vec3::Y * STEM_SEGMENT_LENGTH;
    }

    // The head: a green calyx behind, the dark seed disc in front (facing local -Z), petals around.
    let head = commands.spawn((FlowerHead, Transform::from_translation(at), Visibility::default(), ChildOf(parent))).id();
    let disc = |z: f32| Transform::from_xyz(0.0, 0.08, z).with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2));
    mesh(commands, Cylinder::new(0.2, 0.08).into(), &calyx, disc(-0.04), head);
    mesh(commands, Cylinder::new(0.25, 0.06).into(), &seeds, disc(-0.1), head);
    mesh(commands, Sphere::new(1.0).into(), &seeds, Transform::from_xyz(0.0, 0.08, -0.13).with_scale(Vec3::new(0.22, 0.22, 0.04)), head);
    let ring = pivot(commands, Transform::from_xyz(0.0, 0.08, -0.08), head);
    for i in 0..PETALS {
        let around = i as f32 / PETALS as f32 * TAU;
        let jitter = ((i * 37 % 11) as f32 / 10.0 - 0.5) * 2.0;
        let p = commands.spawn((Petal { jitter, around }, Transform::default(), Visibility::default(), ChildOf(ring))).id();
        petals.push(p);
        mesh(
            commands,
            Sphere::new(1.0).into(),
            &petal,
            // From the hinge outward, so folding keeps the base on the rim.
            Transform::from_xyz(0.0, PETAL_HALF_LENGTH, 0.0).with_scale(Vec3::new(0.07, PETAL_HALF_LENGTH, 0.012)),
            p,
        );
    }
    let light = commands.spawn((
        Name::new("Hearth Light"),
        HearthLight,
        PointLight {
            color: Color::srgb(1.0, 0.78, 0.42),
            intensity: 0.0,
            range: 11.0,
            radius: 0.2,
            shadow_maps_enabled: true,
            ..default()
        },
        // In front of the face, so the light falls where the flower looks.
        Transform::from_xyz(0.0, 0.3, -1.3),
        ChildOf(head),
    )).id();
    // Solid, so nobody walks through the flower bed.
    commands.spawn((Collider::cylinder(0.72, 1.2), Transform::from_xyz(0.0, 0.6, 0.0), ChildOf(hearth)));
    commands.entity(hearth).insert(SunflowerParts { segments, head, petals, leaves, light });
    hearth
}

/// Bends the sunflower between drooping and standing, with a gentle sway, and blends its
/// colours and light. Reviving (the first rest) takes `STAND_UP_TIME`.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn animate_hearth(
    time: Res<Time>,
    mut hearths: Query<(&mut Hearth, &SunflowerMaterials, &SunflowerParts)>,
    segments: Query<&StemSegment>,
    petals: Query<&Petal>,
    leaves: Query<&Leaf>,
    mut transforms: Query<&mut Transform>,
    mut lights: Query<&mut PointLight, With<HearthLight>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let t = time.elapsed_secs();
    for (mut hearth, mats, parts) in &mut hearths {
        let target = if hearth.kindled { 1.0 } else { 0.0 };
        let step = time.delta_secs() / STAND_UP_TIME;
        hearth.revive = if hearth.revive < target { (hearth.revive + step).min(target) } else { (hearth.revive - step).max(target) };
        let up = ease_out_back(hearth.revive);
        let lerp = |a: f32, b: f32| a + (b - a) * up;
        // Drooping: a slow, heavy sway. Standing: a lighter breeze. Each flower sways a bit off.
        let (sway_amp, sway_freq) = (lerp(0.025, 0.018), lerp(0.55, 1.1));
        let offset = hearth.id as f32 * 1.7;
        let sway = |phase: f32| (t * sway_freq + phase + offset).sin() * sway_amp;
        for &e in &parts.segments {
            let (Ok(seg), Ok(mut tf)) = (segments.get(e), transforms.get_mut(e)) else { continue };
            let i = seg.0;
            let k = (i + 1) as f32 / STEM_SEGMENTS as f32;
            let pitch = lerp(STEM_DROOP[i], STEM_UPRIGHT[i]) + sway(0.0) * k;
            let roll = sway(1.7) * k * 0.7;
            tf.rotation = Quat::from_rotation_x(pitch) * Quat::from_rotation_z(roll);
        }
        if let Ok(mut tf) = transforms.get_mut(parts.head) {
            tf.rotation = Quat::from_rotation_x(lerp(HEAD_DROOP, HEAD_UPRIGHT) + sway(0.9) * 1.5);
        }
        for &e in &parts.petals {
            let (Ok(petal), Ok(mut tf)) = (petals.get(e), transforms.get_mut(e)) else { continue };
            let fold = lerp(PETAL_DROOP + 0.25 * petal.jitter, PETAL_UPRIGHT + 0.05 * petal.jitter);
            let around = Quat::from_rotation_z(petal.around);
            tf.translation = around * Vec3::Y * PETAL_RIM;
            tf.rotation = around * Quat::from_rotation_x(fold);
        }
        for &e in &parts.leaves {
            let (Ok(leaf), Ok(mut tf)) = (leaves.get(e), transforms.get_mut(e)) else { continue };
            tf.rotation = Quat::from_rotation_y(leaf.yaw) * Quat::from_rotation_z(lerp(LEAF_DROOP, LEAF_UPRIGHT) + sway(2.3) * 2.0);
        }
        let shown = hearth.revive.clamp(0.0, 1.0);
        if let Ok(mut light) = lights.get_mut(parts.light) {
            light.intensity = 30_000.0 + 380_000.0 * shown * (1.0 + 0.03 * (t * 1.3).sin());
        }
        if let Some(mut m) = materials.get_mut(&mats.petal) {
            m.base_color = mix(PETAL_WILTED, PETAL_VIGOROUS, shown);
            m.emissive = LinearRgba::rgb(0.18 * shown, 0.1 * shown, 0.0);
        }
        if let Some(mut m) = materials.get_mut(&mats.stem) {
            m.base_color = mix(STEM_WILTED, STEM_VIGOROUS, shown);
        }
    }
}

// ── Resting ──────────────────────────────────────────────────────────────

/// Shows the prompt near a Hearth and turns E into a rest request.
fn hearth_interaction(
    mut prompt: ResMut<InteractPrompt>,
    blocked: Res<GameplayBlocked>,
    hearths: Query<(&Hearth, &Transform)>,
    mut player: Query<
        (&Transform, &mut CharacterAction, &CharacterController, &mut PlayerInput, &mut ActionIntent, &MoveIntent, Has<DeathSequence>),
        With<Player>,
    >,
) {
    let Ok((tf, mut action, ctrl, mut input, mut intent, movement, dying)) = player.single_mut() else { return };
    if prompt.0.is_some() || blocked.0 {
        return; // something closer claimed E, or a menu is open
    }
    // Resting: E again, or moving, stands up (once fully down).
    if let Some(cur) = action.current.filter(|a| a.kind == ActionKind::Rest) {
        let down = cur.phase() == ActionPhase::Active;
        prompt.0 = down.then_some(strings::PROMPT_STAND_UP);
        let interact = std::mem::take(&mut input.interact);
        if down && (interact || movement.dir.length() > 0.3) {
            let weapon = cur.weapon;
            action.start(ActiveAction::new(ActionKind::StandUp, cur.dir, weapon));
        }
        return;
    }
    let interact = std::mem::take(&mut input.interact);
    let near = hearths
        .iter()
        .find(|(_, h)| (h.translation - tf.translation).with_y(0.0).length() < HEARTH_RANGE);
    let Some((hearth, _)) = near else { return };
    if dying || action.current.is_some() || !ctrl.grounded {
        return;
    }
    prompt.0 = Some(if hearth.kindled { strings::PROMPT_REST } else { strings::PROMPT_REKINDLE });
    if interact {
        intent.buffer(ActionRequest::Rest);
    }
}

/// Which rest (action serial) was already applied.
#[derive(Component, Default)]
struct RestApplied(Option<u32>);

/// While kneeling: turn toward the sunflower. Once down: revive it if needed, set the respawn
/// point, and (offline) complete the rest at once.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn rest_at_hearth(
    time: Res<Time>,
    mut commands: Commands,
    shared: Res<SharedRest>,
    mut banner: MessageWriter<Banner>,
    mut revived: MessageWriter<SunRevived>,
    mut complete: MessageWriter<RestComplete>,
    mut hearths: Query<(&mut Hearth, &Transform), Without<CharacterController>>,
    mut resters: Query<(Entity, &CharacterAction, &Transform, &mut CharacterController, Option<&mut RestApplied>), With<Player>>,
) {
    for (entity, action, tf, mut ctrl, applied) in &mut resters {
        let Some(cur) = action.current.filter(|a| a.kind == ActionKind::Rest) else { continue };
        let Some((mut hearth, htf)) = hearths
            .iter_mut()
            .min_by(|a, b| a.1.translation.distance(tf.translation).total_cmp(&b.1.translation.distance(tf.translation)))
        else {
            continue;
        };
        // Face the flower (smoothly; movement is locked, so nothing else turns us).
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
        let (position, facing) = Hearth::respawn_spot(htf);
        commands.entity(entity).insert(RespawnPoint { position, facing });
        if !shared.0 {
            complete.write(RestComplete);
        }
        if !hearth.kindled {
            hearth.kindled = true;
            banner.write(Banner(strings::HEARTH_REKINDLED));
            revived.write(SunRevived { id: hearth.id });
            info!("hearth kindled");
        }
        info!("resting at the sunflower");
    }
}

/// The rest completed: enemies respawn (world reset), then the resting player is restored.
#[allow(clippy::type_complexity)]
fn complete_rest(
    mut done: MessageReader<RestComplete>,
    mut reset: MessageWriter<WorldReset>,
    mut player: Query<(&CharacterAction, &mut Health, &mut Stamina, Option<&mut HealingAle>), With<Player>>,
) {
    if done.read().count() == 0 {
        return;
    }
    reset.write(WorldReset);
    let Ok((action, mut health, mut stamina, ale)) = player.single_mut() else { return };
    if matches!(action.current, Some(a) if a.kind == ActionKind::Rest) {
        replenish(&mut health, &mut stamina, ale);
        info!("rest complete: enemies respawn; HP, stamina and Healing Ale restored");
    }
}

// ── Death and respawn ────────────────────────────────────────────────────

fn start_death_sequence(mut commands: Commands, dead: Query<(Entity, &CharacterAction), (With<Player>, Without<DeathSequence>)>) {
    for (entity, action) in &dead {
        if matches!(action.current, Some(a) if a.kind == ActionKind::Dead) {
            commands.entity(entity).insert(DeathSequence::default());
            info!("{}", strings::DEATH);
        }
    }
}

#[allow(clippy::type_complexity)]
fn run_death_sequence(
    time: Res<Time>,
    mut commands: Commands,
    mut reset: MessageWriter<WorldReset>,
    hearths: Query<(&Hearth, &Transform), Without<Player>>,
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
                None => hearths
                    .iter()
                    .min_by_key(|(h, _)| h.id)
                    .map_or((tf.translation.with_y(0.0), ctrl.facing), |(_, t)| Hearth::respawn_spot(t)),
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
        assert_eq!(death_screen(DEATH_TEXT_DELAY + DEATH_TEXT_FADE + 0.1), (1.0, 0.0), "death band shown");
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
