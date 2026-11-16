//! Player skins: what your character looks like, chosen in the settings (Esc → Settings → Skin).
//!
//! **Plug and play:** a skin only changes the *look* — meshes, colours, textures, clothes, ears,
//! tails. The skeleton (every joint, every bone length) stays the human one, so every animation,
//! the weapon hitboxes (forward kinematics along the same bones), the feet on the floor and the
//! 1.8 m capsule are identical for all skins. Bulk is visual: the bear is broad, the skeleton thin,
//! but they reach as far and are hit the same (`skins_change_only_the_look`).
//!
//! Clothes are light — tunics, shirts, trousers, shorts, a loincloth. Armour is equipment and goes
//! on top of any skin.
//!
//! Online the skin travels with your body (`BodyKind::Player(skin)`): others see it on your puppet,
//! wearing your slot's tunic colour where the skin has a top.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::{Bone, Covered, Eyes, Form, HumanoidSpec, Paint, Part, Shape, symmetric};
use crate::textures::Tex;

#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Skin {
    /// The wanderer as it always was: tunic, cape, pauldron, one long brow.
    #[default]
    Monobrow,
    /// A cube-headed builder in a teal shirt and blue trousers.
    Blockhead,
    /// A cat person in a linen shirt and trousers, with ears, whiskers and a tail.
    Tabby,
    /// A broad bear person in shorts and braces.
    Bruin,
    /// A skeleton in a loincloth and a rope belt.
    Rattlebones,
    /// A mushroom person under a red, white-spotted cap.
    Toadstool,
}

impl Skin {
    pub const ALL: [Skin; 6] = [Skin::Monobrow, Skin::Blockhead, Skin::Tabby, Skin::Bruin, Skin::Rattlebones, Skin::Toadstool];

    pub fn name(self) -> &'static str {
        match self {
            Skin::Monobrow => "Monobrow",
            Skin::Blockhead => "Blockhead",
            Skin::Tabby => "Tabby",
            Skin::Bruin => "Bruin",
            Skin::Rattlebones => "Rattlebones",
            Skin::Toadstool => "Toadstool",
        }
    }

    /// The next (`dir` > 0) or previous skin.
    pub fn cycle(self, dir: i32) -> Skin {
        let i = Self::ALL.iter().position(|s| *s == self).unwrap_or(0) as i32;
        Self::ALL[(i + dir).rem_euclid(Self::ALL.len() as i32) as usize]
    }

    /// The body: the human skeleton with this skin's look.
    pub fn spec(self) -> HumanoidSpec {
        let mut s = HumanoidSpec::human();
        match self {
            Skin::Monobrow => {}
            Skin::Blockhead => blockhead(&mut s),
            Skin::Tabby => tabby(&mut s),
            Skin::Bruin => bruin(&mut s),
            Skin::Rattlebones => rattlebones(&mut s),
            Skin::Toadstool => toadstool(&mut s),
        }
        s
    }
}

const fn v(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(x, y, z)
}
const NO_ROT: Vec3 = Vec3::ZERO;

fn part(bone: Bone, shape: Shape, at: Vec3, rot: Vec3, paint: Paint) -> Part {
    Part::new(bone, shape, at, rot, paint)
}

fn cloth(c: Color) -> Paint {
    Paint::Textured(c, Tex::Cloth)
}

fn blockhead(s: &mut HumanoidSpec) {
    s.torso = v(0.48, 0.46, 0.24);
    s.head_radius = 0.13;
    s.arm_radius = 0.06;
    s.leg_radius = 0.085;
    let l = &mut s.look;
    l.form = Form::Blocky;
    l.eyes = Eyes::Squares;
    l.skin = Color::srgb(0.74, 0.54, 0.4);
    l.top = Color::srgb(0.0, 0.58, 0.6);
    l.bottom = Color::srgb(0.24, 0.24, 0.56);
    l.boots = Color::srgb(0.34, 0.34, 0.36);
    l.hair = Color::srgb(0.22, 0.14, 0.08);
    l.skin_tex = None;
    l.bald = false;
    // Short sleeves: bare forearms.
    l.covered = Covered { arms: false, ..Covered::ALL };
    let hr = s.head_radius;
    let stubble = Color::srgb(0.32, 0.2, 0.12);
    l.extras = vec![
        // A short beard around the mouth and a blocky nose.
        part(Bone::Head, Shape::Box(v(hr * 0.9, hr * 0.24, 0.02)), v(0.0, hr * 0.52, -hr * 1.01), NO_ROT, Paint::Color(stubble)),
        part(Bone::Head, Shape::Box(v(hr * 0.32, hr * 0.22, 0.04)), v(0.0, hr * 0.82, -hr * 1.02), NO_ROT, Paint::Color(Color::srgb(0.6, 0.42, 0.3))),
    ];
    l.extras.extend(symmetric(&[part(Bone::UpperArmR, Shape::Box(v(0.145, 0.15, 0.145)), v(0.0, -0.03, 0.0), NO_ROT, Paint::Top)]));
}

fn tabby(s: &mut HumanoidSpec) {
    s.torso = v(0.38, 0.42, 0.22);
    s.head_radius = 0.125;
    s.arm_radius = 0.05;
    s.leg_radius = 0.07;
    let l = &mut s.look;
    l.skin = Color::srgb(0.86, 0.5, 0.2);
    l.skin_tex = Some(Tex::Fur);
    l.top = Color::srgb(0.84, 0.78, 0.64);
    l.bottom = Color::srgb(0.32, 0.24, 0.4);
    // The "belt" is a red sash.
    l.boots = Color::srgb(0.55, 0.1, 0.1);
    l.bald = true;
    l.eyes = Eyes::Dots;
    l.eye_glow = Some(LinearRgba::rgb(0.6, 2.2, 0.3));
    // A sleeveless shirt, trousers, bare paws.
    l.covered = Covered { arms: false, feet: false, ..Covered::ALL };
    let hr = s.head_radius;
    let pink = Paint::Color(Color::srgb(0.9, 0.55, 0.55));
    let cream = Paint::Textured(Color::srgb(0.95, 0.88, 0.75), Tex::Fur);
    let dark = Paint::Color(Color::srgb(0.12, 0.08, 0.06));
    l.extras = symmetric(&[
        // Ears (tilted out), pink inside.
        part(Bone::Head, Shape::Cone(0.05, 0.11), v(0.075, hr * 1.78, 0.01), v(0.0, 0.0, -0.35), Paint::Skin),
        part(Bone::Head, Shape::Cone(0.03, 0.07), v(0.072, hr * 1.74, -0.012), v(0.0, 0.0, -0.35), pink),
        // Whiskers.
        part(Bone::Head, Shape::Box(v(0.11, 0.004, 0.004)), v(0.075, hr * 0.8, -hr * 1.05), v(0.0, 0.25, -0.12), dark),
        part(Bone::Head, Shape::Box(v(0.11, 0.004, 0.004)), v(0.075, hr * 0.72, -hr * 1.03), v(0.0, 0.25, 0.1), dark),
        // Shirt shoulders (sleeveless: just over the shoulder joint).
        part(Bone::UpperArmR, Shape::Ball(0.07), NO_ROT, NO_ROT, Paint::Top),
    ]);
    l.extras.extend([
        // Muzzle and nose.
        part(Bone::Head, Shape::Ball(0.058), v(0.0, hr * 0.78, -hr * 0.8), NO_ROT, cream),
        part(Bone::Head, Shape::Ball(0.018), v(0.0, hr * 0.98, -hr * 1.26), NO_ROT, pink),
        // The tail: back and down, then curling up, with a darker tip.
        part(Bone::Pelvis, Shape::Capsule(0.034, 0.28), v(0.0, -0.07, 0.29), v(1.87, 0.0, 0.0), Paint::Skin),
        part(Bone::Pelvis, Shape::Ball(0.036), v(0.0, -0.12, 0.44), NO_ROT, Paint::Skin),
        part(Bone::Pelvis, Shape::Capsule(0.032, 0.26), v(0.0, 0.03, 0.49), v(0.3, 0.0, 0.0), Paint::Skin),
        part(Bone::Pelvis, Shape::Ball(0.036), v(0.0, 0.2, 0.53), NO_ROT, Paint::Textured(Color::srgb(0.45, 0.24, 0.1), Tex::Fur)),
    ]);
}

fn bruin(s: &mut HumanoidSpec) {
    s.torso = v(0.56, 0.48, 0.36);
    s.head_radius = 0.15;
    s.arm_radius = 0.085;
    s.leg_radius = 0.105;
    let t = s.torso;
    let l = &mut s.look;
    l.skin = Color::srgb(0.36, 0.24, 0.14);
    l.skin_tex = Some(Tex::Fur);
    l.bottom = Color::srgb(0.55, 0.16, 0.12);
    l.bottom_tex = Some(Tex::Cloth);
    l.bald = true;
    l.eyes = Eyes::Dots;
    l.eye_glow = None;
    // Fur everywhere; shorts and braces are extras.
    l.covered = Covered::NOTHING;
    let hr = s.head_radius;
    let tan = Paint::Textured(Color::srgb(0.66, 0.5, 0.32), Tex::Fur);
    let nose = Paint::Color(Color::srgb(0.06, 0.05, 0.05));
    l.extras = vec![
        // Snout with a wet black nose, belly patch.
        part(Bone::Head, Shape::Capsule(0.062, 0.06), v(0.0, hr * 0.78, -hr * 0.86), v(1.57, 0.0, 0.0), tan),
        part(Bone::Head, Shape::Ball(0.032), v(0.0, hr * 0.92, -hr * 1.42), NO_ROT, nose),
        part(Bone::Chest, Shape::Cylinder(0.17, 0.02), v(0.0, 0.2, -t.z * 0.5), v(1.57, 0.0, 0.0), tan),
        // Shorts.
        part(Bone::Pelvis, Shape::Box(v(t.x * 0.95, 0.22, t.z * 0.95)), v(0.0, 0.02, 0.0), NO_ROT, Paint::Bottom),
    ];
    l.extras.extend(symmetric(&[
        // Round ears.
        part(Bone::Head, Shape::Ball(0.048), v(0.105, hr * 1.78, 0.02), NO_ROT, Paint::Skin),
        part(Bone::Head, Shape::Ball(0.028), v(0.105, hr * 1.76, -0.012), NO_ROT, tan),
        // Short trouser legs.
        part(Bone::ThighR, Shape::Cylinder(0.125, 0.2), v(0.0, -0.08, 0.0), NO_ROT, Paint::Bottom),
        // Braces over the shoulders, front and back.
        part(Bone::Chest, Shape::Box(v(0.045, t.y, 0.012)), v(0.12, t.y * 0.5, -t.z * 0.5 - 0.006), NO_ROT, Paint::Bottom),
        part(Bone::Chest, Shape::Box(v(0.045, t.y, 0.012)), v(0.12, t.y * 0.5, t.z * 0.5 + 0.006), NO_ROT, Paint::Bottom),
        part(Bone::Chest, Shape::Ball(0.016), v(0.12, t.y * 0.72, -t.z * 0.5 - 0.012), NO_ROT, Paint::Metal(Color::srgb(0.8, 0.65, 0.3))),
        // Claws.
        part(Bone::HandR, Shape::Cone(0.012, 0.045), v(0.03, -0.1, -0.03), v(3.14, 0.0, 0.0), Paint::Color(Color::srgb(0.85, 0.8, 0.7))),
        part(Bone::HandR, Shape::Cone(0.012, 0.045), v(-0.01, -0.11, -0.04), v(3.14, 0.0, 0.0), Paint::Color(Color::srgb(0.85, 0.8, 0.7))),
    ]));
}

fn rattlebones(s: &mut HumanoidSpec) {
    // The torso box is just the spine; ribs, collarbones and the hip bone are extras.
    s.torso = v(0.06, 0.44, 0.06);
    s.head_radius = 0.12;
    s.arm_radius = 0.028;
    s.leg_radius = 0.035;
    let hr = s.head_radius;
    let l = &mut s.look;
    l.skin = Color::srgb(0.86, 0.82, 0.7);
    // Plain: the pitted bone texture turns thin limbs into leopard spots.
    l.skin_tex = None;
    l.bald = true;
    l.eyes = Eyes::Sockets;
    l.eye_glow = Some(LinearRgba::rgb(0.5, 1.4, 3.5));
    l.covered = Covered::NOTHING;
    let rag = cloth(Color::srgb(0.3, 0.26, 0.2));
    let rope = Paint::Textured(Color::srgb(0.45, 0.35, 0.2), Tex::Leather);
    let hole = Paint::Color(Color::srgb(0.03, 0.02, 0.02));
    l.extras = vec![];
    // Ribs: hoops in front of the spine, narrowing up and down.
    for (y, r) in [(0.16, 0.13), (0.22, 0.15), (0.28, 0.16), (0.34, 0.155), (0.4, 0.13)] {
        l.extras.push(part(Bone::Chest, Shape::Ring(0.013, r), v(0.0, y, -0.05), v(0.12, 0.0, 0.0), Paint::Skin));
    }
    l.extras.extend([
        part(Bone::Chest, Shape::Box(v(0.04, 0.24, 0.02)), v(0.0, 0.28, -0.2), NO_ROT, Paint::Skin),
        // Skull: jaw, teeth, nose hole.
        part(Bone::Head, Shape::Box(v(hr * 1.2, 0.035, hr * 0.95)), v(0.0, hr * 0.32, -hr * 0.36), NO_ROT, Paint::Skin),
        part(Bone::Head, Shape::Box(v(hr * 0.85, 0.022, 0.012)), v(0.0, hr * 0.5, -hr * 0.9), NO_ROT, Paint::Color(Color::srgb(0.95, 0.93, 0.85))),
        part(Bone::Head, Shape::Cone(0.02, 0.035), v(0.0, hr * 0.78, -hr * 0.94), v(3.14, 0.0, 0.0), hole),
        // Hip bone, a rope belt and a ragged loincloth front and back.
        part(Bone::Pelvis, Shape::Ring(0.022, 0.1), v(0.0, 0.04, 0.0), v(0.35, 0.0, 0.0), Paint::Skin),
        part(Bone::Pelvis, Shape::Ring(0.012, 0.13), v(0.0, 0.09, 0.0), NO_ROT, rope),
        part(Bone::Pelvis, Shape::Box(v(0.17, 0.26, 0.014)), v(0.0, -0.06, -0.13), v(0.08, 0.0, 0.0), rag),
        part(Bone::Pelvis, Shape::Box(v(0.15, 0.22, 0.014)), v(0.0, -0.05, 0.13), v(-0.08, 0.0, 0.0), rag),
    ]);
    // Collarbones out to the shoulders.
    l.extras.extend(symmetric(&[part(Bone::Chest, Shape::Capsule(0.014, 0.2), v(0.12, 0.42, -0.05), v(0.0, 0.0, 1.45), Paint::Skin)]));
}

fn toadstool(s: &mut HumanoidSpec) {
    s.torso = v(0.42, 0.42, 0.26);
    s.head_radius = 0.12;
    s.arm_radius = 0.055;
    s.leg_radius = 0.075;
    let hr = s.head_radius;
    let l = &mut s.look;
    l.skin = Color::srgb(0.93, 0.87, 0.74);
    l.skin_tex = None;
    l.top = Color::srgb(0.3, 0.42, 0.2);
    l.bottom = Color::srgb(0.36, 0.24, 0.14);
    l.boots = Color::srgb(0.25, 0.16, 0.08);
    l.bald = true;
    l.eyes = Eyes::Dots;
    l.eye_glow = None;
    // A mossy smock with sleeves, short trousers, bare feet.
    l.covered = Covered { feet: false, ..Covered::ALL };
    let red = Paint::Color(Color::srgb(0.78, 0.1, 0.07));
    let white = Paint::Color(Color::srgb(0.96, 0.94, 0.9));
    let base = hr * 1.85;
    l.extras = vec![
        // The cap: a red rim and dome over cream gills.
        part(Bone::Head, Shape::Cylinder(0.26, 0.01), v(0.0, base - 0.022, 0.0), NO_ROT, Paint::Color(Color::srgb(0.82, 0.74, 0.6))),
        part(Bone::Head, Shape::Cylinder(0.28, 0.04), v(0.0, base, 0.0), NO_ROT, red),
        part(Bone::Head, Shape::Cone(0.28, 0.15), v(0.0, base + 0.095, 0.0), NO_ROT, red),
        part(Bone::Head, Shape::Ball(0.035), v(0.0, base + 0.15, 0.0), NO_ROT, white),
    ];
    // White spots around the dome.
    for i in 0..6 {
        let a = i as f32 / 6.0 * std::f32::consts::TAU + 0.4;
        let (r, y) = if i % 2 == 0 { (0.2, base + 0.06) } else { (0.13, base + 0.1) };
        l.extras.push(part(Bone::Head, Shape::Ball(0.032), v(a.cos() * r, y, a.sin() * r), NO_ROT, white));
    }
    // Rosy cheeks.
    l.extras.extend(symmetric(&[part(Bone::Head, Shape::Ball(0.022), v(0.072, hr * 0.85, -hr * 0.8), NO_ROT, Paint::Color(Color::srgb(0.95, 0.55, 0.5)))]));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::humanoid::pose::{HumanoidPose, bone_model_transform};

    /// The promise of skins: the same skeleton, so the same animation, reach and feet.
    #[test]
    fn skins_change_only_the_look() {
        let human = HumanoidSpec::human();
        let pose = HumanoidPose::default();
        for skin in Skin::ALL {
            let s = skin.spec();
            for bone in Bone::ALL {
                assert_eq!(s.rest_offset(bone), human.rest_offset(bone), "{skin:?} moved {bone:?}");
                let (a, b) = (bone_model_transform(&s, &pose, bone), bone_model_transform(&human, &pose, bone));
                assert!(a.translation.distance(b.translation) < 1e-5, "{skin:?} {bone:?}");
            }
            assert_eq!(s.sole_points(), human.sole_points(), "{skin:?} stands differently");
            assert_eq!(s.scale, 1.0, "{skin:?} is scaled (weapons would grow)");
            // Fits the 1.8 m capsule (the head's top under 1.85 m; caps and ears may poke out a bit).
            let head = bone_model_transform(&s, &pose, Bone::Head).translation;
            assert!(head.y + 2.0 * s.head_radius < 1.85, "{skin:?}: {head}");
        }
    }

    #[test]
    fn skins_cycle_and_round_trip() {
        assert_eq!(Skin::default(), Skin::Monobrow);
        assert_eq!(Skin::Monobrow.cycle(-1), Skin::Toadstool);
        let mut s = Skin::Monobrow;
        for _ in 0..Skin::ALL.len() {
            s = s.cycle(1);
        }
        assert_eq!(s, Skin::Monobrow);
        let text = ron::to_string(&Skin::Rattlebones).unwrap();
        assert_eq!(ron::from_str::<Skin>(&text).unwrap(), Skin::Rattlebones);
    }
}
