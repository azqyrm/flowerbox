//! Healing Ale (the estus equivalent): limited charges, refilled at the Hearth.
//!
//! Drinking is a normal character action (`ActionKind::DrinkAle`, R): slow movement, can't be
//! cancelled. The charge is used at the **swallow** (start of the active phase), so being hit
//! during the wind-up interrupts the drink without wasting a charge. With no charges left the
//! character still lifts the empty tankard, and nothing happens: a familiar punishment.
//! The heal is spread over `ALE_HEAL_TIME`. A tankard prop appears in the left hand while drinking.

use bevy::camera::visibility::NoFrustumCulling;
use bevy::prelude::*;

use crate::character::action::{ActionKind, ActionPhase, CharacterAction};
use crate::character::stats::Health;
use crate::character::CharacterSystems;
use crate::humanoid::{Bone, HumanoidRig};
use crate::tuning::*;

pub struct HealingPlugin;

impl Plugin for HealingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, drink_ale.after(CharacterSystems::Actions).before(CharacterSystems::Stats))
            .add_systems(Update, (spawn_tankards, show_tankards));
    }
}

#[derive(Component, Debug, Clone)]
pub struct HealingAle {
    pub charges: u32,
    pub max: u32,
    /// Action serial of the last drink that reached the swallow (each drink counts once).
    swallowed: Option<u32>,
    /// HP still to be restored by drinks in progress.
    heal_left: f32,
}

impl HealingAle {
    pub fn new(max: u32) -> Self {
        Self { charges: max, max, swallowed: None, heal_left: 0.0 }
    }

    /// Back to full charges (resting, respawning).
    pub fn refill(&mut self) {
        self.charges = self.max;
    }

    /// Called at the swallow: uses a charge and queues the heal. False if empty.
    fn swallow(&mut self, max_hp: f32) -> bool {
        if self.charges == 0 {
            return false;
        }
        self.charges -= 1;
        self.heal_left += max_hp * ALE_HEAL_FRACTION;
        true
    }

    /// Heal to apply this tick.
    fn heal_tick(&mut self, max_hp: f32, dt: f32) -> f32 {
        let amount = self.heal_left.min(max_hp * ALE_HEAL_FRACTION / ALE_HEAL_TIME * dt);
        self.heal_left -= amount;
        amount
    }
}

fn drink_ale(time: Res<Time>, mut q: Query<(&CharacterAction, &mut HealingAle, &mut Health)>) {
    let dt = time.delta_secs();
    for (action, mut ale, mut health) in &mut q {
        if let Some(cur) = action.current {
            let swallowing = cur.kind == ActionKind::DrinkAle && cur.phase() == ActionPhase::Active;
            if swallowing && ale.swallowed != Some(action.serial) {
                ale.swallowed = Some(action.serial);
                if ale.swallow(health.max) {
                    info!("drank Healing Ale ({} left)", ale.charges);
                } else {
                    info!("the tankard is empty");
                }
            }
        }
        if health.is_dead() {
            if ale.heal_left > 0.0 {
                ale.heal_left = 0.0;
            }
            continue;
        }
        if ale.heal_left > 0.0 {
            let heal = ale.heal_tick(health.max, dt);
            health.current = (health.current + heal).min(health.max);
        }
    }
}

// ── Tankard prop ─────────────────────────────────────────────────────────

/// The tankard entity in a character's left hand.
#[derive(Component)]
struct Tankard(Entity);

fn spawn_tankards(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    drinkers: Query<(Entity, &HumanoidRig), (With<HealingAle>, Without<Tankard>)>,
) {
    for (entity, rig) in &drinkers {
        let wood = materials.add(StandardMaterial {
            base_color: Color::srgb(0.42, 0.26, 0.12),
            perceptual_roughness: 0.8,
            ..default()
        });
        let iron = materials.add(StandardMaterial {
            base_color: Color::srgb(0.3, 0.3, 0.32),
            metallic: 0.8,
            perceptual_roughness: 0.4,
            ..default()
        });
        let foam = materials.add(StandardMaterial {
            base_color: Color::srgb(0.93, 0.86, 0.66),
            perceptual_roughness: 0.9,
            ..default()
        });
        // Held in the fist: the mug's axis along the forearm, opening toward the fingers' side.
        let tankard = commands
            .spawn((
                Name::new("Ale Tankard"),
                Transform::from_xyz(0.0, -0.08, -0.06).with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
                Visibility::Hidden,
                ChildOf(rig.bone(Bone::HandL)),
            ))
            .id();
        let mut part = |mesh: Mesh, mat: &Handle<StandardMaterial>, t: Transform| {
            commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(mat.clone()), t, NoFrustumCulling, ChildOf(tankard)));
        };
        part(Cylinder::new(0.055, 0.15).into(), &wood, Transform::IDENTITY);
        part(Cylinder::new(0.058, 0.015).into(), &iron, Transform::from_xyz(0.0, 0.045, 0.0));
        part(Cylinder::new(0.058, 0.015).into(), &iron, Transform::from_xyz(0.0, -0.045, 0.0));
        part(Cylinder::new(0.05, 0.02).into(), &foam, Transform::from_xyz(0.0, 0.07, 0.0));
        part(Cuboid::new(0.02, 0.1, 0.035).into(), &wood, Transform::from_xyz(0.075, 0.0, 0.0));
        commands.entity(entity).insert(Tankard(tankard));
    }
}

fn show_tankards(drinkers: Query<(&CharacterAction, &Tankard)>, mut vis: Query<&mut Visibility>) {
    for (action, tankard) in &drinkers {
        let drinking = matches!(action.current, Some(a) if a.kind == ActionKind::DrinkAle);
        if let Ok(mut v) = vis.get_mut(tankard.0) {
            let want = if drinking { Visibility::Inherited } else { Visibility::Hidden };
            v.set_if_neq(want);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drink_heals_its_fraction_over_time() {
        let mut ale = HealingAle::new(3);
        assert!(ale.swallow(100.0));
        assert_eq!(ale.charges, 2);
        let dt = 1.0 / 64.0;
        let total: f32 = (0..64).map(|_| ale.heal_tick(100.0, dt)).sum();
        assert!((total - 100.0 * ALE_HEAL_FRACTION).abs() < 1e-3, "{total}");
        // Spread out, not instant.
        let mut ale = HealingAle::new(1);
        ale.swallow(100.0);
        assert!(ale.heal_tick(100.0, dt) < 5.0);
    }

    #[test]
    fn empty_tankard_heals_nothing_and_refill_restores() {
        let mut ale = HealingAle::new(2);
        assert!(ale.swallow(100.0) && ale.swallow(100.0));
        ale.heal_left = 0.0;
        assert!(!ale.swallow(100.0), "empty");
        assert_eq!(ale.heal_tick(100.0, 1.0), 0.0);
        ale.refill();
        assert_eq!(ale.charges, 2);
    }
}
