//! Damage numbers popping out of the training dummy: they pop in a bit large, rise and drift
//! sideways, then fade. Driven by `combat::DummyStruck`, so online every player sees them.

use bevy::prelude::*;
use bevy::transform::TransformSystems;

use crate::combat::{Dummy, DummyStruck};

pub struct DamageNumbersPlugin;

impl Plugin for DamageNumbersPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, spawn_numbers)
            .add_systems(PostUpdate, move_numbers.after(TransformSystems::Propagate));
    }
}

/// Seconds a number lives, how high it rises (m) and where it starts above the dummy's origin.
const LIFETIME: f32 = 1.1;
const RISE: f32 = 0.9;
const START_HEIGHT: f32 = 0.75;

#[derive(Component)]
struct DamageNumber {
    world: Vec3,
    drift: Vec3,
    age: f32,
    size: f32,
    color: Color,
}

fn spawn_numbers(mut commands: Commands, mut struck: MessageReader<DummyStruck>, dummies: Query<&Transform, With<Dummy>>, mut n: Local<u32>) {
    let Ok(tf) = dummies.single() else { return };
    for hit in struck.read() {
        *n = n.wrapping_add(1);
        // Alternate sides so quick hits don't stack on top of each other.
        let side = if n.is_multiple_of(2) { 1.0 } else { -1.0 };
        let (size, color) = if hit.killed {
            (40.0, Color::srgb(1.0, 0.35, 0.2))
        } else if hit.amount >= 30.0 {
            (34.0, Color::srgb(1.0, 0.75, 0.3))
        } else {
            (27.0, Color::srgb(1.0, 0.97, 0.9))
        };
        commands.spawn((
            Name::new("Damage number"),
            DamageNumber {
                world: tf.translation + Vec3::new(0.15 * side, START_HEIGHT, 0.0),
                drift: Vec3::new(0.35 * side, 0.0, 0.0),
                age: 0.0,
                size,
                color,
            },
            Text::new(format!("{:.0}", hit.amount)),
            TextFont::from_font_size(size),
            TextColor(color),
            TextShadow { offset: Vec2::splat(2.0), color: Color::BLACK },
            Node { position_type: PositionType::Absolute, ..default() },
            GlobalZIndex(4),
            Visibility::Hidden,
        ));
    }
}

#[allow(clippy::type_complexity)]
fn move_numbers(
    mut commands: Commands,
    time: Res<Time<Real>>,
    cams: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut numbers: Query<(Entity, &mut DamageNumber, &mut Node, &mut TextFont, &mut TextColor, &mut TextShadow, &mut Visibility)>,
) {
    let Ok((camera, cam_tf)) = cams.single() else { return };
    for (e, mut num, mut node, mut font, mut color, mut shadow, mut vis) in &mut numbers {
        num.age += time.delta_secs();
        let t = num.age / LIFETIME;
        if t >= 1.0 {
            commands.entity(e).despawn();
            continue;
        }
        let rise = 1.0 - (1.0 - t) * (1.0 - t); // fast, then slowing
        let pos = num.world + Vec3::Y * RISE * rise + num.drift * t;
        let Ok(screen) = camera.world_to_viewport(cam_tf, pos) else {
            vis.set_if_neq(Visibility::Hidden);
            continue;
        };
        // Pop: start 40% larger, settle within 0.15 s.
        let pop = 1.0 + 0.4 * (1.0 - (num.age / 0.15).min(1.0));
        let px = num.size * pop;
        font.font_size = FontSize::Px(px);
        node.left = Val::Px(screen.x - px * 0.5);
        node.top = Val::Px(screen.y - px * 0.5);
        let alpha = 1.0 - ((t - 0.6) / 0.4).clamp(0.0, 1.0);
        color.0 = num.color.with_alpha(alpha);
        shadow.color = Color::BLACK.with_alpha(alpha);
        vis.set_if_neq(Visibility::Inherited);
    }
}
