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
//! # Hands
//! A character wields up to two weapons ([`Wielded`]): the right hand (bare fists when empty) and
//! optionally a weapon in the left hand (a shield in the left hand isn't a weapon). Either can be
//! held in **both hands** (`two_hand`). Attacks name the hand they swing ([`AttackHand`]): the
//! right, the left (the same moveset, posed as the mirror image), or both at once — the **dual**
//! attack, only when both weapons share a class. Nothing here is class-specific, so a new class
//! gets left-hand, dual and two-handed play for free.
//!
//! # Adding / replacing a weapon model
//! Add a [`WeaponDef`] (e.g. in `light_sword.rs`) with a [`ModelSource`]: primitive parts now,
//! or a glTF scene later. Equip it with an [`EquipWeapon`] message; the old model is despawned and
//! the new one attached to the right hand. Blade span (`blade`) is part of the definition, so the
//! hitbox matches the model.

pub mod axe;
pub mod claws;
pub mod greatsword;
pub mod hollow_blade;
pub mod light_sword;
pub mod warg_fangs;

use bevy::camera::visibility::NoFrustumCulling;
use bevy::prelude::*;
use bevy::world_serialization::WorldAssetRoot;

use crate::humanoid::{Bone, HumanoidRig};

pub struct WeaponPlugin;

impl Plugin for WeaponPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<EquipWeapon>().add_systems(Update, equip_weapons.in_set(WeaponSystems::Equip));
    }
}

/// Systems that spawn characters and send `EquipWeapon` for them in `Update` must run
/// `.before(WeaponSystems::Equip)`, so the new entity exists when the request is handled.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum WeaponSystems {
    Equip,
}

// ── Classes and movesets ─────────────────────────────────────────────────

/// How a weapon fights. Each class has its own [`Moveset`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum WeaponClass {
    /// One-handed sword (the starting longsword).
    #[default]
    LightSword,
    /// Enemy swings with long wind-ups (the Hollow).
    HollowBlade,
    /// A beast's jaws: quick bites and the pounce grab (the warg). A natural weapon: no model.
    WargFangs,
    /// Axes: slower, vertical chops.
    Axe,
    /// A horror's long hooked fingers: quick raking flurries. A natural weapon: no model.
    Claws,
    /// Huge two-handed blades: slow, wide, crushing sweeps (the Thorned Saint's).
    Greatsword,
}

impl WeaponClass {
    /// Every class (tests iterate over this, so new classes get checked automatically).
    #[allow(dead_code)] // only tests use it so far
    pub const ALL: [WeaponClass; 6] =
        [WeaponClass::LightSword, WeaponClass::HollowBlade, WeaponClass::WargFangs, WeaponClass::Axe, WeaponClass::Claws, WeaponClass::Greatsword];

    /// A creature's body part rather than something held (no humanoid swing animations).
    #[allow(dead_code)] // only tests use it so far
    pub fn is_natural(self) -> bool {
        matches!(self, WeaponClass::WargFangs)
    }

    /// No model in the hand (jaws, claws): the body is the weapon.
    #[allow(dead_code)]
    pub fn is_bodily(self) -> bool {
        matches!(self, WeaponClass::WargFangs | WeaponClass::Claws)
    }

    pub fn moveset(self) -> &'static Moveset {
        match self {
            WeaponClass::LightSword => &light_sword::MOVESET,
            WeaponClass::HollowBlade => &hollow_blade::MOVESET,
            WeaponClass::WargFangs => &warg_fangs::MOVESET,
            WeaponClass::Axe => &axe::MOVESET,
            WeaponClass::Claws => &claws::MOVESET,
            WeaponClass::Greatsword => &greatsword::MOVESET,
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
    /// While sprinting: right arm swing, elbow and weapon orientation (same frame as `grip`),
    /// e.g. a sword trailing low behind.
    pub run_arm_pitch: f32,
    pub run_elbow: f32,
    pub run_grip: Vec3,
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
    /// A grab leap (`ActionKind::Pounce`), for creatures that have one.
    pub pounce: Option<PounceSpec>,
    /// The class's activated ability (F / L2), if any (`crate::ability`).
    pub ability: Option<crate::ability::AbilityId>,
}

/// A creature's grab leap: lean back (`windup`), leap `distance` metres in `leap` seconds catching
/// whoever is in the way or at the landing spot, then bite and shake (`recovery`). The bite's damage
/// is `damage` (it can't be blocked; only rolling through the leap avoids it).
#[derive(Clone, Copy, Debug)]
pub struct PounceSpec {
    pub windup: f32,
    pub leap: f32,
    pub recovery: f32,
    pub distance: f32,
    pub stamina_cost: f32,
    pub damage: f32,
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
    /// Built from shaped, textured or glowing parts (any body [`crate::humanoid::Shape`] with a
    /// [`crate::humanoid::Paint`], rotated), in the hand's frame (blade along +Y).
    Styled(&'static [Styled]),
    /// A glTF scene (path under `assets/`), placed with `transform` relative to the hand.
    /// The model's blade should point along +Y from the grip at the origin. (Untested until a
    /// real asset exists.)
    #[allow(dead_code)] // ready for real models (Phase 9)
    Gltf { path: &'static str, transform: Transform },
}

/// One part of a [`ModelSource::Styled`] weapon.
#[derive(Clone, Copy, Debug)]
pub struct Styled {
    pub shape: crate::humanoid::Shape,
    pub at: Vec3,
    /// XYZ euler angles (radians).
    pub rot: Vec3,
    pub paint: crate::humanoid::Paint,
}

impl Styled {
    pub const fn new(shape: crate::humanoid::Shape, y: f32, paint: crate::humanoid::Paint) -> Self {
        Self { shape, at: Vec3::new(0.0, y, 0.0), rot: Vec3::ZERO, paint }
    }

    pub const fn at(shape: crate::humanoid::Shape, at: Vec3, rot: Vec3, paint: crate::humanoid::Paint) -> Self {
        Self { shape, at, rot, paint }
    }
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
    /// Share of damage stopped when guarding with it held in both hands (weaker than a shield).
    pub guard_block: f32,
    /// Stance damage of a hit (× the attack's damage multiplier): fills the target's stance bar
    /// (`character::stats::Stance`); a full bar staggers.
    pub stance_damage: f32,
}

/// A hand.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Hand {
    #[default]
    Right,
    Left,
}

/// Which hand(s) an attack swings: one of them, or both at once (the dual attack).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum AttackHand {
    #[default]
    Right,
    Left,
    Both,
}

/// What a character has in its hands. The right hand always holds something (bare fists when
/// empty); the left holds a weapon or nothing (a shield isn't a weapon). `two_hand` holds one of
/// them in both hands: the other is put away.
#[derive(Component, Clone, Debug)]
pub struct Wielded {
    pub right: &'static WeaponDef,
    pub left: Option<&'static WeaponDef>,
    pub two_hand: Option<Hand>,
    /// Spawned models (right hand bone, left hand bone).
    models: [Option<Entity>; 2],
}

impl Wielded {
    /// Its models are gone (the body they hung on was replaced): don't despawn them again.
    pub fn forget_models(&mut self) {
        self.models = [None; 2];
    }

    /// A weapon that is part of the body (a beast's jaws): no model to attach.
    pub fn natural(def: &'static WeaponDef) -> Self {
        Self { right: def, left: None, two_hand: None, models: [None; 2] }
    }

    /// The weapon in `hand` (the left may be empty).
    pub fn weapon(&self, hand: Hand) -> Option<&'static WeaponDef> {
        match hand {
            Hand::Right => Some(self.right),
            Hand::Left => self.left,
        }
    }

    /// The weapon the main (right-hand) attacks swing: the two-handed one, if any.
    pub fn main(&self) -> &'static WeaponDef {
        match self.two_hand {
            Some(Hand::Left) => self.left.unwrap_or(self.right),
            _ => self.right,
        }
    }

    pub fn class(&self) -> WeaponClass {
        self.main().class
    }

    /// The left hand attacks (a left weapon, not two-handing).
    pub fn left_attacks(&self) -> bool {
        self.left.is_some() && self.two_hand.is_none()
    }

    /// Both hands hold weapons of the same class: the left button does the dual attack.
    pub fn dual_class(&self) -> Option<WeaponClass> {
        self.left.filter(|l| self.two_hand.is_none() && l.class == self.right.class).map(|l| l.class)
    }

    /// The weapons an attack swings, with the hand bone each is in.
    pub fn blades(&self, hand: AttackHand) -> Vec<(Bone, &'static WeaponDef)> {
        match hand {
            AttackHand::Right => vec![(Bone::HandR, self.main())],
            AttackHand::Left => self.left.map(|l| vec![(Bone::HandL, l)]).unwrap_or_default(),
            AttackHand::Both => {
                let mut v = vec![(Bone::HandR, self.right)];
                v.extend(self.left.map(|l| (Bone::HandL, l)));
                v
            }
        }
    }

    /// The weapon class whose moveset an attack of `hand` uses.
    pub fn class_for(&self, hand: AttackHand) -> WeaponClass {
        match hand {
            AttackHand::Left => self.left.map_or(self.right.class, |l| l.class),
            _ => self.main().class,
        }
    }
}

/// Request: set what `character` wields. Old models are removed, new ones attached: the
/// two-handed weapon (or the right one) in the right hand, a left weapon in the left hand.
#[derive(Message, Clone, Debug)]
pub struct EquipWeapon {
    pub character: Entity,
    pub right: &'static WeaponDef,
    pub left: Option<&'static WeaponDef>,
    pub two_hand: Option<Hand>,
}

impl EquipWeapon {
    /// Just a right-hand weapon (enemies).
    pub fn right(character: Entity, def: &'static WeaponDef) -> Self {
        Self { character, right: def, left: None, two_hand: None }
    }
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
    mut characters: Query<(&HumanoidRig, Option<&crate::humanoid::HumanoidSpec>, Option<&mut Wielded>)>,
) {
    for req in requests.read() {
        let Ok((rig, spec, wielded)) = characters.get_mut(req.character) else { continue };
        // A big body holds a big weapon: the model grows with it (the blade span is per weapon).
        let scale = spec.map_or(1.0, |s| s.scale);
        for old in wielded.as_ref().map_or([None; 2], |w| w.models).into_iter().flatten() {
            commands.entity(old).despawn();
        }
        let mut new = Wielded { right: req.right, left: req.left, two_hand: req.two_hand, models: [None; 2] };
        // Two-handed: only that weapon shows, in the right hand; the other is put away.
        let shown: [(Bone, Option<&'static WeaponDef>); 2] = match req.two_hand {
            Some(h) => [(Bone::HandR, new.weapon(h)), (Bone::HandL, None)],
            None => [(Bone::HandR, Some(req.right)), (Bone::HandL, req.left)],
        };
        for (i, (bone, def)) in shown.into_iter().enumerate() {
            let Some(def) = def else { continue };
            let model = spawn_weapon_model(&mut commands, &mut meshes, &mut materials, &asset_server, def);
            if scale != 1.0 {
                commands.entity(model).insert(Transform::from_scale(Vec3::splat(scale)));
            }
            commands.entity(rig.bone(bone)).add_child(model);
            new.models[i] = Some(model);
        }
        info!("wielding {} / {} (two-handed: {:?})", req.right.name, req.left.map_or("-", |l| l.name), req.two_hand);
        match wielded {
            Some(mut w) => *w = new,
            None => {
                commands.entity(req.character).insert(new);
            }
        }
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
                // Texture by what it looks like: rough metal rusts, wood and leather show their grain;
                // polished steel stays clean.
                let tex = if part.metallic >= 0.5 && part.roughness >= 0.5 {
                    Some(crate::textures::Tex::Rust)
                } else if part.metallic < 0.2 {
                    Some(crate::textures::Tex::Leather)
                } else {
                    None
                };
                let material = materials.add(StandardMaterial {
                    base_color: part.color,
                    base_color_texture: tex.and_then(crate::textures::get),
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
        ModelSource::Styled(parts) => {
            let none = materials.add(StandardMaterial::default());
            let body = [none.clone(), none.clone(), none.clone(), none.clone(), none];
            for part in parts {
                let material = part.paint.material(materials, &body);
                let t = crate::humanoid::Part::new(Bone::HandR, part.shape, part.at, part.rot, part.paint).transform();
                let e = commands.spawn((Mesh3d(meshes.add(part.shape.mesh())), MeshMaterial3d(material), t, NoFrustumCulling)).id();
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
