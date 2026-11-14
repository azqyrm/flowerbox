//! Weapons: **classes** (how a weapon type fights and animates) and **definitions** (a concrete
//! weapon: name, class, damage, model).
//!
//! # Adding a weapon class (two-hander, dagger, …)
//! 1. Add a variant to [`WeaponClass`].
//! 2. Create `weapon/<class>.rs` with a `pub const MOVESET: Moveset` (copy `light_sword.rs` and
//!    tune timings, lunges, damage multipliers, swing keyframes, guard stance, combo length).
//! 3. Return it from [`WeaponClass::moveset`].
//!
//! Everything else — action timings, stamina costs, forward lunges, the animation poses and the
//! FK sword hitbox — reads the moveset of the weapon held when the action starts.
//!
//! # Adding / replacing a weapon model
//! Add a [`WeaponDef`] (e.g. in `light_sword.rs`) with a [`ModelSource`]: primitive parts now,
//! or a glTF scene later. Equip it with an [`EquipWeapon`] message; the old model is despawned and
//! the new one attached to the right hand. Blade span (`blade`) is part of the definition, so the
//! hitbox matches the model.

pub mod light_sword;

use bevy::camera::visibility::NoFrustumCulling;
use bevy::prelude::*;
use bevy::world_serialization::WorldAssetRoot;

use crate::humanoid::{Bone, HumanoidRig};

pub struct WeaponPlugin;

impl Plugin for WeaponPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<EquipWeapon>().add_systems(Update, equip_weapons);
    }
}

// ── Classes and movesets ─────────────────────────────────────────────────

/// How a weapon fights. Each class has its own [`Moveset`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum WeaponClass {
    /// One-handed sword (the starting longsword).
    #[default]
    LightSword,
}

impl WeaponClass {
    /// Every class (tests iterate over this, so new classes get checked automatically).
    #[allow(dead_code)] // only tests use it so far
    pub const ALL: [WeaponClass; 1] = [WeaponClass::LightSword];

    pub fn moveset(self) -> &'static Moveset {
        match self {
            WeaponClass::LightSword => &light_sword::MOVESET,
        }
    }
}

/// A pose keyframe of a swing: chest twist/lean and the sword arm's direction (radians).
/// Arm pitch: 0 = hanging down, π/2 = straight forward, π = straight up. Yaw: + = toward the left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SwingKey {
    pub chest_yaw: f32,
    pub chest_pitch: f32,
    pub arm_yaw: f32,
    pub arm_pitch: f32,
}

impl SwingKey {
    pub const fn new(chest_yaw: f32, chest_pitch: f32, arm_yaw: f32, arm_pitch: f32) -> Self {
        Self { chest_yaw, chest_pitch, arm_yaw, arm_pitch }
    }
}

/// Lower-body stance during an attack.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stance {
    /// Left foot forward, hips dropped.
    Lunge,
    /// Legs split mid-air (leaping attacks).
    Leap,
    /// Knees tucked (attacks from the air).
    Tucked,
}

/// One attack of a moveset: timing, cost, motion, damage and animation.
#[derive(Clone, Copy, Debug)]
pub struct AttackSpec {
    pub startup: f32,
    pub active: f32,
    pub recovery: f32,
    /// Seconds after start from which another action may cancel it.
    pub cancel: f32,
    pub stamina_cost: f32,
    /// Forward motion during the attack (m).
    pub lunge: f32,
    /// Damage = weapon base damage × this.
    pub damage_mult: f32,
    pub windup: SwingKey,
    pub strike: SwingKey,
    pub stance: Stance,
}

/// How the arms hold the weapon when not attacking (radians, local bone rotations).
#[derive(Clone, Copy, Debug)]
pub struct GuardSpec {
    /// Right upper arm forward swing.
    pub arm_pitch: f32,
    /// Right elbow bend.
    pub elbow: f32,
    /// Weapon orientation in the hand, as XYZ euler angles relative to the *upper arm* frame.
    pub grip: Vec3,
    /// Left arm forward swing and elbow bend (a two-hander would bring it to the grip).
    pub off_arm_pitch: f32,
    pub off_elbow: f32,
}

/// Everything a weapon class does differently.
#[derive(Clone, Copy, Debug)]
pub struct Moveset {
    /// Light attack combo chain; wraps around after the last one.
    pub light: &'static [AttackSpec],
    /// The swing after releasing a charge.
    pub heavy: AttackSpec,
    /// Max charge time after the hold threshold; damage scales to `heavy_full_charge_mult`.
    pub heavy_charge_max: f32,
    pub heavy_charge_cost: f32,
    pub heavy_full_charge_mult: f32,
    pub running: AttackSpec,
    /// Upward velocity of the running attack's leap (m/s).
    pub running_hop: f32,
    /// Jump attack: `active` is the max plunge time; the downward swing takes `jump_strike_time`.
    pub jump: AttackSpec,
    pub jump_strike_time: f32,
    pub guard: GuardSpec,
}

impl Moveset {
    /// Light attack `index` of the combo (wraps).
    pub fn light(&self, index: u8) -> &AttackSpec {
        &self.light[index as usize % self.light.len()]
    }
}

// ── Concrete weapons and their models ────────────────────────────────────

/// Primitive shape of a model part.
#[derive(Clone, Copy, Debug)]
pub enum PartShape {
    Box(Vec3),
    Cylinder { radius: f32, height: f32 },
    Sphere(f32),
}

/// A primitive part of a weapon model, placed along the hand's +Y (the blade direction).
#[derive(Clone, Copy, Debug)]
pub struct WeaponPart {
    pub shape: PartShape,
    pub color: Color,
    pub metallic: f32,
    pub roughness: f32,
    pub offset: Vec3,
}

/// Where a weapon's visuals come from.
#[derive(Clone, Copy, Debug)]
pub enum ModelSource {
    /// Built from primitive parts.
    Parts(&'static [WeaponPart]),
    /// A glTF scene (path under `assets/`), placed with `transform` relative to the hand.
    /// The model's blade should point along +Y from the grip at the origin. (Untested until a
    /// real asset exists.)
    #[allow(dead_code)] // ready for real models (Phase 9)
    Gltf { path: &'static str, transform: Transform },
}

/// A concrete weapon.
#[derive(Clone, Copy, Debug)]
pub struct WeaponDef {
    pub name: &'static str,
    pub class: WeaponClass,
    pub base_damage: f32,
    /// Blade span along the hand's +Y (crossguard → tip), metres. Used for the hitbox.
    pub blade: (f32, f32),
    pub model: ModelSource,
}

/// The weapon a character holds in its right hand.
#[derive(Component, Clone, Debug)]
pub struct EquippedWeapon {
    pub def: &'static WeaponDef,
    /// Spawned model entity (child of the hand bone).
    model: Option<Entity>,
}

impl EquippedWeapon {
    pub fn class(&self) -> WeaponClass {
        self.def.class
    }
}

/// Request: put `def` into `character`'s right hand (replacing the current weapon).
#[derive(Message, Clone, Debug)]
pub struct EquipWeapon {
    pub character: Entity,
    pub def: &'static WeaponDef,
}

/// Marks a weapon model entity.
#[derive(Component)]
pub struct WeaponModel;

fn equip_weapons(
    mut requests: MessageReader<EquipWeapon>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
    mut characters: Query<(&HumanoidRig, Option<&mut EquippedWeapon>)>,
) {
    for req in requests.read() {
        let Ok((rig, equipped)) = characters.get_mut(req.character) else { continue };
        if let Some(old) = equipped.as_ref().and_then(|e| e.model) {
            commands.entity(old).despawn();
        }
        let model = spawn_weapon_model(&mut commands, &mut meshes, &mut materials, &asset_server, req.def);
        commands.entity(rig.bone(Bone::HandR)).add_child(model);
        let new = EquippedWeapon { def: req.def, model: Some(model) };
        match equipped {
            Some(mut e) => *e = new,
            None => {
                commands.entity(req.character).insert(new);
            }
        }
        info!("equipped {} ({:?})", req.def.name, req.def.class);
    }
}

/// Spawns the visuals of `def` (not yet attached to anything).
pub fn spawn_weapon_model(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    asset_server: &AssetServer,
    def: &WeaponDef,
) -> Entity {
    let root = commands.spawn((Name::new(def.name), WeaponModel, Transform::default(), Visibility::default())).id();
    match def.model {
        ModelSource::Parts(parts) => {
            for part in parts {
                let mesh: Mesh = match part.shape {
                    PartShape::Box(size) => Cuboid::from_size(size).into(),
                    PartShape::Cylinder { radius, height } => Cylinder::new(radius, height).into(),
                    PartShape::Sphere(r) => Sphere::new(r).into(),
                };
                let material = materials.add(StandardMaterial {
                    base_color: part.color,
                    metallic: part.metallic,
                    perceptual_roughness: part.roughness,
                    ..default()
                });
                let e = commands
                    .spawn((
                        Mesh3d(meshes.add(mesh)),
                        MeshMaterial3d(material),
                        Transform::from_translation(part.offset),
                        NoFrustumCulling, // see ROADMAP N13
                    ))
                    .id();
                commands.entity(root).add_child(e);
            }
        }
        ModelSource::Gltf { path, transform } => {
            let scene = commands
                .spawn((WorldAssetRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset(path))), transform))
                .id();
            commands.entity(root).add_child(scene);
        }
    }
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::action::{ActionKind, ActionMovement, ActiveAction, action_def};

    fn attacks(class: WeaponClass) -> Vec<ActionKind> {
        let mut kinds: Vec<_> = (0..class.moveset().light.len() as u8).map(ActionKind::LightAttack).collect();
        kinds.extend([ActionKind::HeavyAttack, ActionKind::RunningAttack, ActionKind::JumpAttack]);
        kinds
    }

    #[test]
    fn movesets_are_sane() {
        for class in WeaponClass::ALL {
            let m = class.moveset();
            assert!(!m.light.is_empty(), "{class:?} needs a light combo");
            assert!(m.heavy_full_charge_mult >= m.heavy.damage_mult, "{class:?}");
            assert!(m.jump_strike_time <= m.jump.active, "{class:?}");
            for kind in attacks(class) {
                let a = ActiveAction::new(kind, Vec3::NEG_Z, class);
                let spec = a.attack().unwrap();
                assert!(spec.startup > 0.0 && spec.active > 0.0 && spec.recovery > 0.0, "{class:?} {kind:?}");
                assert!(a.damage_multiplier().unwrap() > 0.0, "{class:?} {kind:?}");
            }
        }
    }

    #[test]
    fn lunges_cover_their_distance() {
        let dt = 1.0 / 64.0;
        for class in WeaponClass::ALL {
            for kind in attacks(class) {
                let def = action_def(kind, class);
                let spec = *ActiveAction::new(kind, Vec3::NEG_Z, class).attack().unwrap();
                let moved: f32 = match def.movement {
                    ActionMovement::Scripted(curve) => (0..(def.total() / dt) as usize).map(|i| curve.speed(i as f32 * dt) * dt).sum(),
                    _ => 0.0,
                };
                if kind != ActionKind::JumpAttack {
                    assert!((moved - spec.lunge).abs() < 0.15, "{class:?} {kind:?}: moved {moved}, want {}", spec.lunge);
                }
            }
        }
    }

    #[test]
    fn heavy_damage_scales_with_charge() {
        for class in WeaponClass::ALL {
            let m = class.moveset();
            let mut a = ActiveAction::new(ActionKind::HeavyAttack, Vec3::NEG_Z, class);
            assert_eq!(a.damage_multiplier(), Some(m.heavy.damage_mult));
            a.charge = 1.0;
            assert_eq!(a.damage_multiplier(), Some(m.heavy_full_charge_mult));
            assert_eq!(ActiveAction::new(ActionKind::HeavyCharge, Vec3::NEG_Z, class).damage_multiplier(), None);
            assert_eq!(ActiveAction::new(ActionKind::Roll, Vec3::NEG_Z, class).damage_multiplier(), None);
        }
    }
}
