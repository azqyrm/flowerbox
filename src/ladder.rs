//! Ladders (`LevelFile::ladders`): at a ladder's foot or at its top, E gets you on (stepping on
//! from below, or turning round and lowering yourself from above). On it, forward / back climb up
//! and down (hand over hand); hold sprint going down to slide. At either end you get off by
//! yourself: pulled up over the top and onto the floor above, or stepping off at the bottom. A hit
//! that staggers you knocks you off (you fall). Enemies don't climb.
//!
//! The body is moved here, kinematically, after the controller (`CharacterSystems::Movement`);
//! the pose is the `ActionKind::Climb` action (`humanoid::anim`), so puppets climb too.

use bevy::prelude::*;
use leafwing_input_manager::prelude::*;

use crate::character::action::{ActionKind, ActiveAction, CharacterAction, ClimbMove};
use crate::character::controller::CharacterController;
use crate::character::{CharacterSystems, MoveIntent};
use crate::hearth::DeathSequence;
use crate::input::{Action, GameplayBlocked, PlayerInput};
use crate::interact::{InteractPrompt, InteractSystems};
use crate::level::Levels;
use crate::player::Player;
use crate::tuning::*;
use crate::ui::strings;

pub struct LadderPlugin;

impl Plugin for LadderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_ladders)
            .add_systems(Update, ladder_interaction.in_set(InteractSystems::Doors))
            .add_systems(FixedUpdate, climb.after(CharacterSystems::Movement));
    }
}

/// A ladder in the world.
#[derive(Component, Debug, Clone, Copy)]
pub struct Ladder {
    /// Where the climber's body stands at the foot (x, bottom floor, z).
    pub foot: Vec3,
    /// Toward the wall (horizontal unit); the climber faces this way.
    pub toward: Vec3,
    pub bottom: f32,
    pub top: f32,
    /// Where you stand after getting off at the top (on the upper floor, beyond the wall).
    pub exit: Vec3,
}

impl Ladder {
    pub fn length(&self) -> f32 {
        self.top - self.bottom
    }

    fn yaw(&self) -> f32 {
        f32::atan2(-self.toward.x, -self.toward.z)
    }

    /// The body's centre `h` metres up the ladder.
    fn body_at(&self, h: f32) -> Vec3 {
        self.foot + Vec3::Y * (h + PLAYER_HALF_HEIGHT + GROUND_SKIN)
    }
}

/// On the player while it uses a ladder: which one, and how high its feet are above the foot.
#[derive(Component, Debug, Clone, Copy)]
pub struct Climbing {
    pub ladder: Entity,
    pub h: f32,
}

fn spawn_ladders(mut commands: Commands, levels: Option<Res<Levels>>, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let Some(levels) = levels else { return };
    let wood = materials.add(StandardMaterial {
        base_color: Color::srgb(0.36, 0.24, 0.14),
        base_color_texture: crate::textures::get(crate::textures::Tex::Planks),
        perceptual_roughness: 0.85,
        ..default()
    });
    for layout in &levels.0 {
        for def in &layout.file.ladders {
            let at = layout.world(def.at);
            let toward2 = def.facing.vec();
            let toward = Vec3::new(toward2.x, 0.0, toward2.y);
            // The wall's face: the ladder leans flat on it; the top exit is beyond the wall.
            let face = layout.wall_distance(at, def.facing).unwrap_or(0.6);
            let foot = Vec3::new(at.x, def.bottom, at.y);
            let exit = foot + toward * (face + LEVEL_WALL_THICKNESS + 0.7) + Vec3::Y * (def.top - def.bottom);
            let ladder = Ladder { foot, toward, bottom: def.bottom, top: def.top, exit };
            let base = foot + toward * (face - 0.08);
            let height = def.top - def.bottom + 1.1; // the rails reach above the top as handholds
            let root = commands
                .spawn((
                    Name::new("Ladder"),
                    ladder,
                    Transform::from_translation(base).with_rotation(Quat::from_rotation_y(ladder.yaw())),
                    Visibility::default(),
                ))
                .id();
            let rail = meshes.add(Cuboid::new(0.07, height, 0.07));
            for side in [-0.25, 0.25] {
                commands.spawn((Mesh3d(rail.clone()), MeshMaterial3d(wood.clone()), Transform::from_xyz(side, height * 0.5, 0.0), ChildOf(root)));
            }
            let rung = meshes.add(Cylinder::new(0.025, 0.5));
            let mut y = 0.3;
            while y < height - 0.05 {
                commands.spawn((
                    Mesh3d(rung.clone()),
                    MeshMaterial3d(wood.clone()),
                    Transform::from_xyz(0.0, y, 0.0).with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
                    ChildOf(root),
                ));
                y += 0.3;
            }
            info!("ladder at {:.1} from {} up to {}", foot, def.bottom, def.top);
        }
    }
}

/// E at a ladder's foot or top gets you on.
#[allow(clippy::type_complexity)]
fn ladder_interaction(
    mut commands: Commands,
    mut prompt: ResMut<InteractPrompt>,
    blocked: Res<GameplayBlocked>,
    ladders: Query<(Entity, &Ladder)>,
    mut player: Query<(Entity, &Transform, &mut CharacterAction, &mut PlayerInput, &CharacterController, Has<Climbing>, Has<DeathSequence>), With<Player>>,
) {
    let Ok((me, tf, mut action, mut input, ctrl, climbing, dying)) = player.single_mut() else { return };
    if prompt.0.is_some() || blocked.0 || climbing || dying || action.current.is_some() || !ctrl.grounded {
        return;
    }
    let feet = tf.translation - Vec3::Y * (PLAYER_HALF_HEIGHT + GROUND_SKIN);
    for (e, l) in &ladders {
        let near = |p: Vec3| (p - feet).with_y(0.0).length() < LADDER_REACH && (p.y - feet.y).abs() < 0.6;
        let (text, step, h) = if near(l.foot) {
            (strings::PROMPT_CLIMB_UP, ClimbMove::OnBottom, 0.0)
        } else if near(l.exit) {
            (strings::PROMPT_CLIMB_DOWN, ClimbMove::OnTop, l.length())
        } else {
            continue;
        };
        prompt.0 = Some(text);
        if std::mem::take(&mut input.interact) {
            action.start(ActiveAction::new(ActionKind::Climb(step), l.toward, Default::default()));
            commands.entity(me).insert(Climbing { ladder: e, h });
            info!("onto the ladder ({step:?})");
        }
        return;
    }
}

fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Moves the climber (after the controller, so gravity and walls don't fight it).
#[allow(clippy::type_complexity)]
fn climb(
    time: Res<Time>,
    mut commands: Commands,
    ladders: Query<&Ladder>,
    mut player: Query<(Entity, &mut Climbing, &mut Transform, &mut CharacterController, &mut CharacterAction, &MoveIntent, Option<&ActionState<Action>>)>,
) {
    let dt = time.delta_secs();
    for (e, mut c, mut tf, mut ctrl, mut action, intent, actions) in &mut player {
        let Ok(l) = ladders.get(c.ladder).copied() else {
            commands.entity(e).remove::<Climbing>();
            continue;
        };
        let Some(cur) = action.current.filter(|a| matches!(a.kind, ActionKind::Climb(_))) else {
            // Knocked off (a stagger, death): fall from here.
            commands.entity(e).remove::<Climbing>();
            continue;
        };
        let ActionKind::Climb(step) = cur.kind else { continue };
        let duration = step.duration();
        let done = cur.elapsed >= duration;
        let k = ease(cur.elapsed / duration);
        let len = l.length();
        let pos = match step {
            ClimbMove::OnBottom => {
                c.h = 0.3 * k;
                if done {
                    action.start(ActiveAction::new(ActionKind::Climb(ClimbMove::Climbing), l.toward, cur.weapon));
                }
                tf.translation.lerp(l.body_at(c.h), (12.0 * dt).min(1.0) + k * 0.5)
            }
            ClimbMove::OnTop => {
                // From the floor above, over the edge and down onto the rungs.
                let from = l.exit + Vec3::Y * (PLAYER_HALF_HEIGHT + GROUND_SKIN);
                c.h = len - 0.6 * k;
                if done {
                    action.start(ActiveAction::new(ActionKind::Climb(ClimbMove::Climbing), l.toward, cur.weapon));
                }
                let over = Vec3::Y * 0.25 * (std::f32::consts::PI * k).sin();
                from.lerp(l.body_at(c.h), k) + over
            }
            ClimbMove::Climbing => {
                // Up / down: the stick (or keys), or moving toward / away from the wall.
                let stick = actions.map_or(0.0, |a| a.clamped_axis_pair(&Action::Move).y);
                let along = intent.dir.dot(l.toward);
                let v = if stick.abs() > along.abs() { stick } else { along };
                let v = if v.abs() < 0.2 { 0.0 } else { v.signum() };
                let speed = if v < 0.0 && intent.sprint { LADDER_SLIDE_SPEED } else { LADDER_SPEED };
                c.h = (c.h + v * speed * dt).clamp(0.0, len);
                if let Some(a) = action.current.as_mut() {
                    a.charge = (c.h / LADDER_CYCLE).rem_euclid(1.0);
                }
                if c.h >= len - 0.6 && v > 0.0 {
                    action.start(ActiveAction::new(ActionKind::Climb(ClimbMove::OffTop), l.toward, cur.weapon));
                } else if c.h <= 0.0 && v < 0.0 {
                    action.start(ActiveAction::new(ActionKind::Climb(ClimbMove::OffBottom), l.toward, cur.weapon));
                }
                l.body_at(c.h)
            }
            ClimbMove::OffTop => {
                let to = l.exit + Vec3::Y * (PLAYER_HALF_HEIGHT + GROUND_SKIN);
                let from = l.body_at(len - 0.6);
                if done {
                    finish(&mut commands, e, &mut action);
                }
                // Up first (pulled over the edge), then forward onto the floor.
                let up = ease(cur.elapsed / (duration * 0.55));
                let fwd = ease((cur.elapsed - duration * 0.35) / (duration * 0.65));
                Vec3::new(from.x + (to.x - from.x) * fwd, from.y + (to.y - from.y) * up + 0.15 * (std::f32::consts::PI * fwd).sin(), from.z + (to.z - from.z) * fwd)
            }
            ClimbMove::OffBottom => {
                let to = l.body_at(0.0) - l.toward * 0.35;
                if done {
                    finish(&mut commands, e, &mut action);
                }
                l.body_at(c.h * (1.0 - k)).lerp(to, k)
            }
        };
        tf.translation = pos;
        ctrl.velocity = Vec3::ZERO;
        ctrl.facing = l.yaw();
        tf.rotation = Quat::from_rotation_y(ctrl.facing);
        // Grounded only once off it (no falling animation on the rungs).
        ctrl.grounded = matches!(step, ClimbMove::OffTop | ClimbMove::OffBottom) && done;
    }
}

fn finish(commands: &mut Commands, e: Entity, action: &mut CharacterAction) {
    action.revive(); // free again (not a death: just ends the action)
    commands.entity(e).remove::<Climbing>();
    info!("off the ladder");
}

