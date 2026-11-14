//! Procedural animation of the primitive player model (until Phase 9 brings a rigged model).
//! Only touches the `PlayerModel` child's transform; gameplay never reads it.

use std::f32::consts::{PI, TAU};

use bevy::prelude::*;

use super::action::{ActionKind, PlayerAction};
use super::pose::arm_pose;
use super::{PlayerModel, RightHandSocket, RightShoulder};
use crate::tuning::*;

pub struct VisualsPlugin;

impl Plugin for VisualsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (animate_model, animate_arm));
    }
}

/// Height (from the feet) of the point the body somersaults around.
const ROLL_PIVOT: f32 = 0.9;

/// Smoothstep easing 0..1.
fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Model pose relative to its rest pose: (rotation about the model's X axis, scale, vertical offset).
/// Positive rotation tips the head backwards (+Z), negative tips it forwards.
pub fn model_pose(action: Option<(ActionKind, f32)>) -> (f32, f32, f32) {
    match action {
        Some((ActionKind::Roll, elapsed)) => {
            // One full forward somersault, finished a bit before the recovery ends.
            let p = elapsed / (ROLL_STARTUP + ROLL_ACTIVE + ROLL_RECOVERY * 0.5);
            let bump = (PI * p.clamp(0.0, 1.0)).sin();
            (-TAU * ease(p), 1.0 - 0.3 * bump, -0.35 * bump)
        }
        Some((ActionKind::Backstep, elapsed)) => {
            let p = elapsed / (BACKSTEP_STARTUP + BACKSTEP_ACTIVE + BACKSTEP_RECOVERY);
            let bump = (PI * p.clamp(0.0, 1.0)).sin();
            (0.3 * bump, 1.0, 0.12 * bump)
        }
        _ => (0.0, 1.0, 0.0),
    }
}

fn animate_model(
    players: Query<(&PlayerAction, &Children)>,
    mut models: Query<&mut Transform, With<PlayerModel>>,
) {
    for (action, children) in &players {
        let (angle, scale, lift) = model_pose(action.current.map(|a| (a.kind, a.elapsed)));
        for child in children.iter() {
            let Ok(mut tf) = models.get_mut(child) else { continue };
            // Rotate + scale around the pivot instead of the feet:
            // translation = rest + pivot - R * (s * pivot).
            let rot = Quat::from_rotation_x(angle);
            let pivot = Vec3::Y * ROLL_PIVOT;
            tf.rotation = rot;
            tf.scale = Vec3::splat(scale);
            tf.translation = Vec3::Y * (-PLAYER_HALF_HEIGHT + lift) + pivot - rot * (pivot * scale);
        }
    }
}

/// Poses the sword arm from `pose::arm_pose` (the same pose the hitbox uses).
fn animate_arm(
    players: Query<&PlayerAction>,
    mut shoulders: Query<&mut Transform, (With<RightShoulder>, Without<RightHandSocket>)>,
    mut sockets: Query<&mut Transform, (With<RightHandSocket>, Without<RightShoulder>)>,
) {
    // One player for now; with more, shoulders/sockets would need matching per player.
    let Some(action) = players.iter().next() else { return };
    let pose = arm_pose(action.current.as_ref());
    for mut tf in &mut shoulders {
        tf.rotation = pose.shoulder;
    }
    for mut tf in &mut sockets {
        tf.rotation = pose.grip;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roll_does_one_full_turn_and_returns_to_rest() {
        let (start, s0, _) = model_pose(Some((ActionKind::Roll, 0.0)));
        assert_eq!((start, s0), (0.0, 1.0));
        let (end, s1, y1) = model_pose(Some((ActionKind::Roll, 0.6)));
        assert!((end + TAU).abs() < 1e-4, "full somersault, got {end}");
        assert!((s1 - 1.0).abs() < 1e-4 && y1.abs() < 1e-4, "back to normal size");
        assert_eq!(model_pose(None), (0.0, 1.0, 0.0));
    }
}
