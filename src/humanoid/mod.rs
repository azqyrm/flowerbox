//! Reusable humanoid models and animation.
//!
//! Any character can get a humanoid body: describe it with a [`HumanoidSpec`] (proportions and
//! colours), build it with [`spawn_humanoid`], and put the returned [`HumanoidRig`] plus a
//! [`anim::HumanoidAnimator`] on the character entity. The shared animation system
//! ([`anim`]) then poses it from the character's movement and actions — walk/run cycles,
//! jumping, landing, rolls, attacks, hurt, death — for the player and future enemies alike.
//!
//! **Swapping the model:** a different humanoid only needs a different `HumanoidSpec` (or, later,
//! a glTF whose bones are mapped onto [`Bone`]). Gameplay reads poses through
//! [`pose::bone_model_transform`] (forward kinematics), never the mesh, so hitboxes follow.
//!
//! Conventions: model space has its origin at the feet and faces -Z; right side is +X.
//! Limb bones hang along their local -Y. Rotating a limb bone positively about X swings it
//! forward; a negative X rotation on a shin/forearm bends the knee back, a positive one on a
//! forearm bends the elbow forward.

pub mod anim;
pub mod pose;
pub mod skins;

use bevy::camera::visibility::NoFrustumCulling;
use bevy::prelude::*;

use crate::textures::{self, Tex};

pub struct HumanoidPlugin;

impl Plugin for HumanoidPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (anim::animate_humanoids, sway_tails));
    }
}

/// The bones of the shared humanoid skeleton.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Bone {
    Pelvis,
    Chest,
    Head,
    UpperArmL,
    ForearmL,
    HandL,
    UpperArmR,
    ForearmR,
    HandR,
    ThighL,
    ShinL,
    FootL,
    ThighR,
    ShinR,
    FootR,
}

impl Bone {
    pub const ALL: [Bone; 15] = [
        Bone::Pelvis,
        Bone::Chest,
        Bone::Head,
        Bone::UpperArmL,
        Bone::ForearmL,
        Bone::HandL,
        Bone::UpperArmR,
        Bone::ForearmR,
        Bone::HandR,
        Bone::ThighL,
        Bone::ShinL,
        Bone::FootL,
        Bone::ThighR,
        Bone::ShinR,
        Bone::FootR,
    ];
    pub const COUNT: usize = Self::ALL.len();

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn parent(self) -> Option<Bone> {
        use Bone::*;
        Some(match self {
            Pelvis => return None,
            Chest => Pelvis,
            Head => Chest,
            UpperArmL | UpperArmR => Chest,
            ForearmL => UpperArmL,
            ForearmR => UpperArmR,
            HandL => ForearmL,
            HandR => ForearmR,
            ThighL | ThighR => Pelvis,
            ShinL => ThighL,
            ShinR => ThighR,
            FootL => ShinL,
            FootR => ShinR,
        })
    }

    /// The same bone on the other side (itself for the middle).
    pub fn mirror(self) -> Bone {
        use Bone::*;
        match self {
            UpperArmL => UpperArmR,
            ForearmL => ForearmR,
            HandL => HandR,
            UpperArmR => UpperArmL,
            ForearmR => ForearmL,
            HandR => HandL,
            ThighL => ThighR,
            ShinL => ShinR,
            FootL => FootR,
            ThighR => ThighL,
            ShinR => ShinL,
            FootR => FootL,
            other => other,
        }
    }

    /// Chest, head and arms: the part of the body an upper-body action (drinking, charging) poses.
    pub fn is_upper_body(self) -> bool {
        use Bone::*;
        matches!(self, Chest | Head | UpperArmL | ForearmL | HandL | UpperArmR | ForearmR | HandR)
    }
}

/// Colours, textures and extra parts of a humanoid.
#[derive(Clone, Debug)]
pub struct HumanoidLook {
    pub skin: Color,
    pub top: Color,
    pub bottom: Color,
    pub boots: Color,
    pub hair: Color,
    /// Glowing eyes (emissive), e.g. for Hollows. `None` = plain dark eyes.
    pub eye_glow: Option<LinearRgba>,
    /// Procedural detail textures (tinted by the colours above), `None` = plain.
    pub skin_tex: Option<Tex>,
    pub top_tex: Option<Tex>,
    pub bottom_tex: Option<Tex>,
    /// No hair cap (bald heads, masks, hoods made of extras).
    pub bald: bool,
    /// Decorative shapes on bones: pauldrons, spikes, horns, robes, crowns… They follow the
    /// bones like the body, but gameplay ignores them (hits use the capsule).
    pub extras: Vec<Part>,
    /// Round (spheres and capsules) or blocky (boxes) body parts.
    pub form: Form,
    pub eyes: Eyes,
    /// Which body parts the clothes (top, bottom, boots) cover; the rest shows skin (or fur, or
    /// bone). Lighter clothes (shorts, sleeves, loincloths) are extras on top.
    pub covered: Covered,
    /// An optional tail that sways on its own.
    pub tail: Option<Tail>,
}

/// A tail: a chain of segments growing from the pelvis, each joint swinging a little side to side
/// a moment after the one before (a lazy idle sway). Decoration: gameplay ignores it.
#[derive(Clone, Copy, Debug)]
pub struct Tail {
    /// Where it starts, in the pelvis' frame (+Z = behind).
    pub root: Vec3,
    pub segments: usize,
    /// Each segment's length; the radius tapers from `radius` to `radius * taper` at the tip.
    pub length: f32,
    pub radius: f32,
    pub taper: f32,
    /// Pitch of the first segment, from straight up toward the back (radians; π/2 = straight back).
    pub lift: f32,
    /// Extra pitch per following segment (negative curls it up).
    pub curl: f32,
    /// How far each joint swings sideways (radians; 0 = rigid).
    pub sway: f32,
    /// Swings per second.
    pub speed: f32,
    pub paint: Paint,
    /// The last segment's paint (a darker tip), else `paint`.
    pub tip: Option<Paint>,
}

/// One joint of a [`Tail`], posed by [`sway_tails`].
#[derive(Component, Clone, Copy, Debug)]
pub struct TailJoint {
    pub index: usize,
    pub rest: Quat,
    pub sway: f32,
    pub speed: f32,
    /// Per character, so tails don't swing in lockstep.
    pub phase: f32,
}

/// Swings every tail joint: a sine wave travelling from the root to the tip.
pub fn sway_tails(time: Res<Time>, mut joints: Query<(&TailJoint, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (j, mut tf) in &mut joints {
        let wave = (t * j.speed * std::f32::consts::TAU + j.phase - j.index as f32 * 0.8).sin();
        tf.rotation = j.rest * Quat::from_rotation_z(wave * j.sway);
    }
}

/// How the body's own parts are shaped.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Form {
    #[default]
    Round,
    /// Cube head, box limbs and hands.
    Blocky,
}

/// The face's eyes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Eyes {
    /// One dark band across the face (the monobrow).
    #[default]
    Bar,
    /// Two small round eyes.
    Dots,
    /// Two square eyes: white with a dark pupil.
    Squares,
    /// Two big dark hollows (skulls), with the glow as pupils.
    Sockets,
}

/// Which parts the clothing colours cover.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Covered {
    /// The chest box wears `top`.
    pub chest: bool,
    /// The arms wear `top` (long sleeves).
    pub arms: bool,
    /// Hips and legs wear `bottom` (and a belt).
    pub legs: bool,
    /// The feet wear `boots`.
    pub feet: bool,
}

impl Covered {
    pub const ALL: Covered = Covered { chest: true, arms: true, legs: true, feet: true };
    pub const NOTHING: Covered = Covered { chest: false, arms: false, legs: false, feet: false };
}

impl Default for Covered {
    fn default() -> Self {
        Self::ALL
    }
}

impl Default for HumanoidLook {
    fn default() -> Self {
        HumanoidSpec::human().look
    }
}

/// One decorative shape on a bone (`B`: a humanoid [`Bone`] or a quadruped's `QBone`), in the
/// bone's frame.
#[derive(Clone, Copy, Debug)]
pub struct Part<B = Bone> {
    pub bone: B,
    pub shape: Shape,
    pub at: Vec3,
    /// XYZ euler angles (radians).
    pub rot: Vec3,
    pub paint: Paint,
}

impl<B: Copy> Part<B> {
    pub const fn new(bone: B, shape: Shape, at: Vec3, rot: Vec3, paint: Paint) -> Self {
        Self { bone, shape, at, rot, paint }
    }

    pub fn scaled(self, f: f32) -> Self {
        Self { at: self.at * f, shape: self.shape.scaled(f), ..self }
    }

    pub fn transform(&self) -> Transform {
        Transform::from_translation(self.at).with_rotation(Quat::from_euler(EulerRot::XYZ, self.rot.x, self.rot.y, self.rot.z))
    }
}

impl Part<Bone> {
    /// The same part on the other side of the body.
    pub fn mirrored(self) -> Self {
        Self { bone: self.bone.mirror(), at: Vec3::new(-self.at.x, self.at.y, self.at.z), rot: Vec3::new(self.rot.x, -self.rot.y, -self.rot.z), ..self }
    }
}

/// `parts` plus their mirror images (for symmetric decorations).
pub fn symmetric(parts: &[Part]) -> Vec<Part> {
    parts.iter().flat_map(|p| [*p, p.mirrored()]).collect()
}

#[derive(Clone, Copy, Debug)]
pub enum Shape {
    Box(Vec3),
    Ball(f32),
    /// Radius, length of the straight part (along Y).
    Capsule(f32, f32),
    /// Base radius, height (tip along +Y).
    Cone(f32, f32),
    /// Radius, height (along Y).
    Cylinder(f32, f32),
    /// Tube radius, ring radius (lying in XZ).
    Ring(f32, f32),
}

impl Shape {
    pub fn scaled(self, f: f32) -> Self {
        match self {
            Shape::Box(v) => Shape::Box(v * f),
            Shape::Ball(r) => Shape::Ball(r * f),
            Shape::Capsule(r, l) => Shape::Capsule(r * f, l * f),
            Shape::Cone(r, h) => Shape::Cone(r * f, h * f),
            Shape::Cylinder(r, h) => Shape::Cylinder(r * f, h * f),
            Shape::Ring(a, b) => Shape::Ring(a * f, b * f),
        }
    }

    pub fn mesh(self) -> Mesh {
        match self {
            Shape::Box(v) => Cuboid::from_size(v).into(),
            Shape::Ball(r) => Sphere::new(r).into(),
            Shape::Capsule(r, l) => Capsule3d::new(r, l).into(),
            Shape::Cone(r, h) => Cone { radius: r, height: h }.into(),
            Shape::Cylinder(r, h) => Cylinder::new(r, h).into(),
            Shape::Ring(a, b) => Torus::new(b - a, b + a).into(),
        }
    }
}

impl Paint {
    /// The material for this paint; the named body colours come from `body` (skin, top, bottom,
    /// boots, hair).
    pub fn material(self, materials: &mut Assets<StandardMaterial>, body: &[Handle<StandardMaterial>; 5]) -> Handle<StandardMaterial> {
        let plain = |materials: &mut Assets<StandardMaterial>, c: Color, tex: Option<Tex>| {
            materials.add(StandardMaterial { base_color: c, base_color_texture: tex.and_then(textures::get), perceptual_roughness: 0.85, ..default() })
        };
        match self {
            Paint::Skin => body[0].clone(),
            Paint::Top => body[1].clone(),
            Paint::Bottom => body[2].clone(),
            Paint::Boots => body[3].clone(),
            Paint::Hair => body[4].clone(),
            Paint::Color(c) => plain(materials, c, None),
            Paint::Textured(c, t) => plain(materials, c, Some(t)),
            Paint::Metal(c) => materials.add(StandardMaterial { base_color: c, metallic: 0.9, perceptual_roughness: 0.35, ..default() }),
            Paint::TexturedMetal(c, t) => materials.add(StandardMaterial {
                base_color: c,
                base_color_texture: textures::get(t),
                metallic: 0.85,
                perceptual_roughness: 0.4,
                ..default()
            }),
            Paint::Glow(g) => materials.add(StandardMaterial { base_color: Color::BLACK, emissive: g, ..default() }),
            Paint::GlowTex(g, t) => materials.add(StandardMaterial {
                base_color: Color::srgb(0.05, 0.05, 0.06),
                emissive: g,
                emissive_texture: textures::get(t),
                metallic: 0.8,
                perceptual_roughness: 0.4,
                ..default()
            }),
        }
    }
}

/// What an extra part is made of.
#[derive(Clone, Copy, Debug)]
pub enum Paint {
    Skin,
    Top,
    Bottom,
    Boots,
    Hair,
    Color(Color),
    /// A colour with a procedural texture.
    Textured(Color, Tex),
    /// Polished metal.
    Metal(Color),
    /// Metal with a procedural texture (rust, damascus…).
    TexturedMetal(Color, Tex),
    /// Glowing (emissive), e.g. eyes, embers, runes.
    Glow(LinearRgba),
    /// Dark metal glowing where the texture is bright (rune-etched blades).
    GlowTex(LinearRgba, Tex),
}

/// Proportions (metres) and look of a humanoid. Swap this to get a different body.
#[derive(Component, Clone, Debug)]
pub struct HumanoidSpec {
    /// Hip joint height above the feet.
    pub pelvis_height: f32,
    /// Lower-spine pivot above the pelvis.
    pub spine: f32,
    /// Shoulder joints above the chest pivot.
    pub shoulder_height: f32,
    pub shoulder_x: f32,
    /// Head pivot (base of the skull) above the chest pivot.
    pub neck_height: f32,
    pub upper_arm: f32,
    pub forearm: f32,
    pub hip_x: f32,
    pub thigh: f32,
    pub shin: f32,
    /// Width, height, depth of the torso box.
    pub torso: Vec3,
    pub head_radius: f32,
    pub arm_radius: f32,
    pub leg_radius: f32,
    /// Overall size relative to the human (feet, neck; set by [`HumanoidSpec::scaled`]).
    pub scale: f32,
    pub look: HumanoidLook,
}

impl HumanoidSpec {
    /// The player's proportions: ~1.8 m tall, matches the 1.8 m capsule.
    pub fn human() -> Self {
        Self {
            pelvis_height: 0.94,
            spine: 0.1,
            shoulder_height: 0.36,
            shoulder_x: 0.26,
            neck_height: 0.44,
            upper_arm: 0.28,
            forearm: 0.26,
            hip_x: 0.11,
            thigh: 0.43,
            shin: 0.43,
            torso: Vec3::new(0.44, 0.44, 0.25),
            head_radius: 0.13,
            arm_radius: 0.06,
            leg_radius: 0.08,
            scale: 1.0,
            look: HumanoidLook {
                skin: Color::srgb(0.78, 0.6, 0.48),
                top: Color::srgb(0.32, 0.22, 0.14),
                bottom: Color::srgb(0.15, 0.14, 0.13),
                boots: Color::srgb(0.12, 0.08, 0.05),
                hair: Color::srgb(0.12, 0.08, 0.05),
                eye_glow: None,
                skin_tex: None,
                top_tex: Some(Tex::Cloth),
                bottom_tex: Some(Tex::Cloth),
                bald: false,
                extras: Self::wanderer_gear(),
                form: Form::Round,
                eyes: Eyes::Bar,
                covered: Covered::ALL,
                tail: None,
            },
        }
    }

    /// The player's gear on top of the tunic: a tattered half-cape, a pauldron with a spike on the
    /// left shoulder, leather bracers, a belt with pouches and a brass buckle, and a glowing
    /// sunflower brooch (the sunflower's bearer).
    fn wanderer_gear() -> Vec<Part> {
        let leather = Color::srgb(0.24, 0.15, 0.08);
        let iron = Color::srgb(0.34, 0.33, 0.35);
        let mut gear = vec![
            Part::new(Bone::Chest, Shape::Box(Vec3::new(0.36, 0.62, 0.025)), Vec3::new(0.06, 0.12, 0.15), Vec3::new(-0.12, 0.0, 0.08), Paint::Textured(Color::srgb(0.18, 0.15, 0.12), Tex::Cloth)),
            Part::new(Bone::Chest, Shape::Box(Vec3::new(0.2, 0.3, 0.02)), Vec3::new(-0.12, -0.08, 0.16), Vec3::new(-0.2, 0.0, -0.15), Paint::Textured(Color::srgb(0.16, 0.13, 0.1), Tex::Cloth)),
            Part::new(Bone::UpperArmL, Shape::Ball(0.095), Vec3::new(-0.02, 0.02, 0.0), Vec3::ZERO, Paint::TexturedMetal(iron, Tex::Rust)),
            Part::new(Bone::UpperArmL, Shape::Cone(0.025, 0.12), Vec3::new(-0.08, 0.08, 0.0), Vec3::new(0.0, 0.0, 0.7), Paint::Metal(iron)),
            Part::new(Bone::Chest, Shape::Box(Vec3::new(0.05, 0.5, 0.02)), Vec3::new(-0.05, 0.25, -0.13), Vec3::new(0.0, 0.0, 0.6), Paint::Textured(leather, Tex::Leather)),
            Part::new(Bone::Pelvis, Shape::Ring(0.022, 0.2), Vec3::new(0.0, 0.11, 0.0), Vec3::ZERO, Paint::Textured(leather, Tex::Leather)),
            Part::new(Bone::Pelvis, Shape::Box(Vec3::new(0.07, 0.06, 0.02)), Vec3::new(0.0, 0.11, -0.13), Vec3::ZERO, Paint::Metal(Color::srgb(0.8, 0.6, 0.25))),
            Part::new(Bone::Pelvis, Shape::Box(Vec3::new(0.09, 0.1, 0.06)), Vec3::new(0.19, 0.05, -0.02), Vec3::new(0.0, 0.0, 0.1), Paint::Textured(leather, Tex::Leather)),
            Part::new(Bone::Pelvis, Shape::Box(Vec3::new(0.07, 0.09, 0.05)), Vec3::new(-0.18, 0.05, 0.05), Vec3::new(0.0, 0.0, -0.1), Paint::Textured(leather, Tex::Leather)),
            // The sunflower brooch: golden petals around a dark heart.
            Part::new(Bone::Chest, Shape::Cylinder(0.04, 0.012), Vec3::new(-0.11, 0.33, -0.135), Vec3::new(1.57, 0.0, 0.0), Paint::Glow(LinearRgba::rgb(4.0, 2.6, 0.3))),
            Part::new(Bone::Chest, Shape::Cylinder(0.018, 0.016), Vec3::new(-0.11, 0.33, -0.142), Vec3::new(1.57, 0.0, 0.0), Paint::Color(Color::srgb(0.2, 0.1, 0.03))),
        ];
        gear.extend(symmetric(&[Part::new(Bone::ForearmR, Shape::Cylinder(0.068, 0.15), Vec3::new(0.0, -0.17, 0.0), Vec3::ZERO, Paint::Textured(leather, Tex::Leather))]));
        gear
    }

    /// A Hollow: same skeleton (so every animation fits), gaunt and grey, in rags, with
    /// smouldering eyes.
    pub fn hollow() -> Self {
        Self {
            torso: Vec3::new(0.38, 0.42, 0.21),
            head_radius: 0.12,
            arm_radius: 0.045,
            leg_radius: 0.065,
            look: HumanoidLook {
                skin: Color::srgb(0.42, 0.43, 0.38),
                top: Color::srgb(0.2, 0.18, 0.15),
                bottom: Color::srgb(0.13, 0.12, 0.1),
                boots: Color::srgb(0.08, 0.07, 0.06),
                hair: Color::srgb(0.3, 0.3, 0.27),
                eye_glow: Some(LinearRgba::rgb(6.0, 2.2, 0.4)),
                skin_tex: Some(Tex::Leather),
                top_tex: Some(Tex::Cloth),
                bottom_tex: Some(Tex::Cloth),
                bald: false,
                // Rags, no gear.
                extras: vec![],
                form: Form::Round,
                eyes: Eyes::Bar,
                covered: Covered::ALL,
                tail: None,
            },
            ..Self::human()
        }
    }

    /// The same body `f` times as big (every length, radius and extra part): poses are angles, so
    /// every animation fits, and the FK hitboxes grow with it.
    pub fn scaled(mut self, f: f32) -> Self {
        for v in [
            &mut self.pelvis_height,
            &mut self.spine,
            &mut self.shoulder_height,
            &mut self.shoulder_x,
            &mut self.neck_height,
            &mut self.upper_arm,
            &mut self.forearm,
            &mut self.hip_x,
            &mut self.thigh,
            &mut self.shin,
            &mut self.head_radius,
            &mut self.arm_radius,
            &mut self.leg_radius,
        ] {
            *v *= f;
        }
        self.torso *= f;
        self.scale *= f;
        self.look.extras = self.look.extras.iter().map(|p| p.scaled(f)).collect();
        if let Some(t) = &mut self.look.tail {
            t.root *= f;
            t.length *= f;
            t.radius *= f;
        }
        self
    }

    /// The body armour is fitted to: at least a human build (so a skeleton's mail hangs like on
    /// a person instead of shrinking onto the spine), broader where the body is (the bear).
    pub fn armour_fit(&self) -> HumanoidSpec {
        let human = HumanoidSpec::human();
        let f = self.scale;
        let mut fit = self.clone();
        fit.torso = self.torso.max(human.torso * f);
        fit.head_radius = self.head_radius.max(human.head_radius * f);
        fit.arm_radius = self.arm_radius.max(human.arm_radius * f);
        fit.leg_radius = self.leg_radius.max(human.leg_radius * f);
        fit
    }

    /// Height of the ankle (the foot bone's origin) above the floor in the rest pose.
    pub fn ankle_height(&self) -> f32 {
        self.pelvis_height - 0.02 * self.scale - self.thigh - self.shin
    }

    /// Heel and toe of the sole in the foot bone's frame (matches the boot mesh). Locomotion
    /// keeps the lowest of these on the floor.
    pub fn sole_points(&self) -> [Vec3; 2] {
        let y = -self.ankle_height();
        let (off, len) = (FOOT_OFFSET * self.scale, FOOT_LENGTH * self.scale);
        [Vec3::new(0.0, y, off + len * 0.5), Vec3::new(0.0, y, off - len * 0.5)]
    }

    /// Rest translation of `bone` relative to its parent (Pelvis: relative to the feet).
    pub fn rest_offset(&self, bone: Bone) -> Vec3 {
        use Bone::*;
        match bone {
            Pelvis => Vec3::Y * self.pelvis_height,
            Chest => Vec3::Y * self.spine,
            Head => Vec3::Y * self.neck_height,
            UpperArmL => Vec3::new(-self.shoulder_x, self.shoulder_height, 0.0),
            UpperArmR => Vec3::new(self.shoulder_x, self.shoulder_height, 0.0),
            ForearmL | ForearmR => Vec3::NEG_Y * self.upper_arm,
            HandL | HandR => Vec3::NEG_Y * self.forearm,
            ThighL => Vec3::new(-self.hip_x, -0.02 * self.scale, 0.0),
            ThighR => Vec3::new(self.hip_x, -0.02 * self.scale, 0.0),
            ShinL | ShinR => Vec3::NEG_Y * self.thigh,
            FootL | FootR => Vec3::NEG_Y * self.shin,
        }
    }
}

/// Boot length and how far its centre sits in front of the ankle (-Z).
const FOOT_LENGTH: f32 = 0.27;
const FOOT_OFFSET: f32 = -0.05;

/// Entities of a spawned humanoid: the model root (child of the character) and every bone.
#[derive(Component, Clone, Debug)]
pub struct HumanoidRig {
    pub model: Entity,
    pub bones: [Entity; Bone::COUNT],
    /// Height of the character's origin above the feet (the capsule center).
    pub root_height: f32,
}

impl HumanoidRig {
    pub fn bone(&self, bone: Bone) -> Entity {
        self.bones[bone.index()]
    }
}

/// Builds the bone hierarchy and primitive meshes under `character`.
/// `root_height` = how far the character's origin sits above its feet.
pub fn spawn_humanoid(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    character: Entity,
    spec: &HumanoidSpec,
    root_height: f32,
) -> HumanoidRig {
    fn mat(materials: &mut Assets<StandardMaterial>, c: Color, tex: Option<Tex>) -> Handle<StandardMaterial> {
        materials.add(StandardMaterial { base_color: c, base_color_texture: tex.and_then(textures::get), perceptual_roughness: 0.85, ..default() })
    }
    let look = &spec.look;
    let skin = mat(materials, look.skin, look.skin_tex);
    let top = mat(materials, look.top, look.top_tex);
    let bottom = mat(materials, look.bottom, look.bottom_tex);
    let boots = mat(materials, look.boots, Some(Tex::Leather));
    let hair = mat(materials, look.hair, None);
    let body = [skin.clone(), top.clone(), bottom.clone(), boots.clone(), hair.clone()];
    let extra_mats: Vec<Handle<StandardMaterial>> = look.extras.iter().map(|p| p.paint.material(materials, &body)).collect();
    let dark = match spec.look.eye_glow {
        Some(glow) => materials.add(StandardMaterial { base_color: Color::BLACK, emissive: glow, ..default() }),
        None => mat(materials, Color::srgb(0.03, 0.03, 0.03), None),
    };

    let model = commands
        .spawn((Name::new("Humanoid Model"), Transform::from_xyz(0.0, -root_height, 0.0), Visibility::default()))
        .id();
    commands.entity(character).add_child(model);

    let mut bones = [Entity::PLACEHOLDER; Bone::COUNT];
    for bone in Bone::ALL {
        let e = commands
            .spawn((
                Name::new(format!("{bone:?}")),
                Transform::from_translation(spec.rest_offset(bone)),
                Visibility::default(),
            ))
            .id();
        bones[bone.index()] = e;
        let parent = bone.parent().map_or(model, |p| bones[p.index()]);
        commands.entity(parent).add_child(e);
    }

    // NoFrustumCulling: Bevy 0.20's culling hid the posed body parts mid-roll even though their
    // transforms were correct (ROADMAP N13). ~30 tiny meshes per humanoid, so skipping it is cheap.
    let mut part = |bone: Bone, mesh: Mesh, material: &Handle<StandardMaterial>, t: Transform| {
        let e = commands
            .spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(material.clone()), t, NoFrustumCulling))
            .id();
        commands.entity(bones[bone.index()]).add_child(e);
    };
    let t = Transform::from_xyz;
    let s = spec;
    let cover = look.covered;
    let blocky = look.form == Form::Blocky;
    let (chest_m, arm_m, leg_m, foot_m) = (
        if cover.chest { &top } else { &skin },
        if cover.arms { &top } else { &skin },
        if cover.legs { &bottom } else { &skin },
        if cover.feet { &boots } else { &skin },
    );
    // Torso
    part(Bone::Pelvis, Cuboid::new(s.torso.x * 0.85, 0.2 * s.scale, s.torso.z * 0.9).into(), leg_m, t(0.0, 0.02 * s.scale, 0.0));
    if cover.legs {
        part(Bone::Pelvis, Cuboid::new(s.torso.x * 0.9, 0.07 * s.scale, s.torso.z * 0.95).into(), &boots, t(0.0, 0.1 * s.scale, 0.0));
    }
    part(Bone::Chest, Cuboid::from_size(s.torso).into(), chest_m, t(0.0, s.torso.y * 0.5 + 0.02 * s.scale, 0.0));
    if !blocky {
        part(Bone::Chest, Cylinder::new(0.055 * s.scale, 0.1 * s.scale).into(), &skin, t(0.0, s.neck_height - 0.03 * s.scale, 0.0));
    }
    // Head
    let hr = s.head_radius;
    if blocky {
        part(Bone::Head, Cuboid::from_length(hr * 2.0).into(), &skin, t(0.0, hr, 0.0));
        if !look.bald {
            // A cap of hair on top and down the back.
            part(Bone::Head, Cuboid::new(hr * 2.08, hr * 0.5, hr * 2.08).into(), &hair, t(0.0, hr * 1.8, 0.0));
            part(Bone::Head, Cuboid::new(hr * 2.08, hr * 1.2, hr * 0.4).into(), &hair, t(0.0, hr * 1.4, hr * 0.86));
        }
    } else {
        part(Bone::Head, Sphere::new(hr).into(), &skin, t(0.0, hr, 0.0));
        if !look.bald {
            part(Bone::Head, Sphere::new(hr * 1.04).into(), &hair, t(0.0, hr * 1.2, hr * 0.2));
        }
    }
    let front = if blocky { -hr * 1.01 } else { -hr * 0.95 };
    match look.eyes {
        Eyes::Bar => part(Bone::Head, Cuboid::new(hr, 0.025 * s.scale, 0.02 * s.scale).into(), &dark, t(0.0, hr * 1.1, front)),
        Eyes::Dots => {
            for x in [-0.36, 0.36] {
                part(Bone::Head, Sphere::new(hr * 0.13).into(), &dark, t(x * hr, hr * 1.1, front + hr * 0.04));
            }
        }
        Eyes::Squares => {
            let white = mat(materials, Color::srgb(0.95, 0.95, 0.93), None);
            for x in [-0.45, 0.45] {
                part(Bone::Head, Cuboid::new(hr * 0.4, hr * 0.2, 0.02 * s.scale).into(), &white, t(x * hr, hr * 1.05, front));
                // The pupil on the inner half (a friendly, slightly cross-eyed look).
                part(Bone::Head, Cuboid::new(hr * 0.2, hr * 0.2, 0.024 * s.scale).into(), &dark, t(x * hr * 0.78, hr * 1.05, front - 0.002));
            }
        }
        Eyes::Sockets => {
            let hollow = mat(materials, Color::srgb(0.02, 0.015, 0.01), None);
            for x in [-0.38, 0.38] {
                part(Bone::Head, Sphere::new(hr * 0.26).into(), &hollow, t(x * hr, hr * 1.08, front + hr * 0.12));
                part(Bone::Head, Sphere::new(hr * 0.08).into(), &dark, t(x * hr, hr * 1.08, front - hr * 0.08));
            }
        }
    }
    // Arms
    for (upper, fore, hand) in [(Bone::UpperArmL, Bone::ForearmL, Bone::HandL), (Bone::UpperArmR, Bone::ForearmR, Bone::HandR)] {
        let r = s.arm_radius;
        if blocky {
            part(upper, Cuboid::new(r * 2.2, s.upper_arm + r, r * 2.2).into(), arm_m, t(0.0, -s.upper_arm * 0.5 + r * 0.5, 0.0));
            part(fore, Cuboid::new(r * 2.1, s.forearm, r * 2.1).into(), arm_m, t(0.0, -s.forearm * 0.5, 0.0));
            part(hand, Cuboid::from_length(r * 2.2).into(), &skin, t(0.0, -0.03 * s.scale, 0.0));
        } else {
            part(upper, Sphere::new(r * 1.3).into(), arm_m, Transform::IDENTITY);
            part(upper, Capsule3d::new(r * 1.1, s.upper_arm - r).into(), arm_m, t(0.0, -s.upper_arm * 0.5, 0.0));
            part(fore, Capsule3d::new(r, s.forearm - r).into(), arm_m, t(0.0, -s.forearm * 0.5, 0.0));
            part(hand, Sphere::new(r * 1.15).into(), &skin, t(0.0, -0.03 * s.scale, 0.0));
        }
    }
    // Legs
    for (thigh, shin, foot) in [(Bone::ThighL, Bone::ShinL, Bone::FootL), (Bone::ThighR, Bone::ShinR, Bone::FootR)] {
        let r = s.leg_radius;
        if blocky {
            part(thigh, Cuboid::new(r * 2.2, s.thigh + r, r * 2.2).into(), leg_m, t(0.0, -s.thigh * 0.5 + r * 0.5, 0.0));
            part(shin, Cuboid::new(r * 2.1, s.shin, r * 2.1).into(), leg_m, t(0.0, -s.shin * 0.5, 0.0));
        } else {
            part(thigh, Capsule3d::new(r * 1.1, s.thigh - r).into(), leg_m, t(0.0, -s.thigh * 0.5, 0.0));
            part(shin, Capsule3d::new(r, s.shin - r).into(), leg_m, t(0.0, -s.shin * 0.5, 0.0));
        }
        part(
            foot,
            Cuboid::new(r * 2.0, 0.1 * s.scale, FOOT_LENGTH * s.scale).into(),
            foot_m,
            t(0.0, -s.ankle_height() + 0.05 * s.scale, FOOT_OFFSET * s.scale),
        );
    }
    for (p, m) in s.look.extras.iter().zip(&extra_mats) {
        part(p.bone, p.shape.mesh(), m, p.transform());
    }
    if let Some(tail) = look.tail {
        let fur = tail.paint.material(materials, &body);
        let tip = tail.tip.map_or(fur.clone(), |t| t.material(materials, &body));
        let phase = (character.index_u32() % 29) as f32 * 0.9;
        let (mut parent, mut at) = (bones[Bone::Pelvis.index()], tail.root);
        for index in 0..tail.segments {
            let rest = Quat::from_rotation_x(if index == 0 { tail.lift } else { tail.curl });
            let joint = commands
                .spawn((
                    TailJoint { index, rest, sway: tail.sway, speed: tail.speed, phase },
                    Transform::from_translation(at).with_rotation(rest),
                    Visibility::default(),
                    ChildOf(parent),
                ))
                .id();
            let f = index as f32 / (tail.segments.max(2) - 1) as f32;
            let r = tail.radius * (1.0 - f * (1.0 - tail.taper));
            let material = if index + 1 == tail.segments { tip.clone() } else { fur.clone() };
            commands.spawn((
                Mesh3d(meshes.add(Capsule3d::new(r, tail.length))),
                MeshMaterial3d(material),
                Transform::from_xyz(0.0, tail.length * 0.5, 0.0),
                NoFrustumCulling,
                ChildOf(joint),
            ));
            (parent, at) = (joint, Vec3::Y * tail.length);
        }
    }

    HumanoidRig { model, bones, root_height }
}
