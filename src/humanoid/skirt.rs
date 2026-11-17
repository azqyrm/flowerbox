//! Robes, skirts and capes that keep clear of the legs. A fixed shape can't: a striding thigh
//! swings 30–50° and pokes through any cone narrow enough to still look like a robe. So the cloth
//! is a mesh rebuilt every frame: from the waist ring straight down to the hem, and wherever a
//! knee or foot would come through it, that side of the hem is pushed out past it (then falls
//! back a little slower, like cloth).

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use std::f32::consts::{FRAC_PI_2, PI};

use super::{Bone, HumanoidSpec, Paint};

/// Columns around the full circle (an open cape gets its share).
const SEGMENTS: usize = 36;
/// How fast a hem pushed out by a leg falls back (metres per second, at the hem).
const SETTLE_SPEED: f32 = 1.2;

/// A piece of cloth hanging from `bone` (the pelvis for robes and skirts, the chest for capes).
/// Lengths are in that bone's frame; angle 0 points to the right (+X), π/2 to the back (+Z).
#[derive(Clone, Copy, Debug)]
pub struct Skirt {
    pub bone: Bone,
    /// Height of the waist ring in the bone's frame.
    pub top: f32,
    /// Half-widths of the waist ring: x = sideways, y = front to back.
    pub waist: Vec2,
    /// Half-widths of the hem at rest (standing).
    pub hem: Vec2,
    /// From the waist ring down to the hem.
    pub length: f32,
    /// Half the angle it covers, centred on the back: π = all round, less = an open cape.
    pub span: f32,
    pub paint: Paint,
}

impl Skirt {
    /// A robe all the way round, hanging from the pelvis.
    pub const fn robe(top: f32, waist: Vec2, hem: Vec2, length: f32, paint: Paint) -> Self {
        Self { bone: Bone::Pelvis, top, waist, hem, length, span: PI, paint }
    }

    pub fn scaled(self, f: f32) -> Self {
        Self { top: self.top * f, waist: self.waist * f, hem: self.hem * f, length: self.length * f, ..self }
    }

    fn closed(&self) -> bool {
        self.span >= PI - 1e-3
    }

    /// Columns (the seam column is repeated on a closed robe so the texture wraps).
    fn columns(&self) -> usize {
        ((SEGMENTS as f32 * self.span / PI).ceil() as usize).max(2) + 1
    }

    fn angle(&self, i: usize) -> f32 {
        let n = self.columns() - 1;
        FRAC_PI_2 - self.span + 2.0 * self.span * i as f32 / n as f32
    }

    /// Waist point, rest hem point and the outward direction of column `i`.
    fn column(&self, i: usize) -> (Vec3, Vec3, Vec3) {
        let (s, c) = self.angle(i).sin_cos();
        let out = Vec3::new(c, 0.0, s);
        let waist = Vec3::new(self.waist.x * c, self.top, self.waist.y * s);
        let hem = Vec3::new(self.hem.x * c, self.top - self.length, self.hem.y * s);
        (waist, hem, out)
    }
}

/// The cloth's state, on its mesh entity (a child of the bone it hangs from).
#[derive(Component)]
pub struct SkirtCloth {
    skirt: Skirt,
    /// Thigh, shin, foot of each leg.
    legs: [[Entity; 3]; 2],
    /// The chest bone (for capes: the legs are measured in its frame).
    chest: Entity,
    /// Heel and toe in the foot's frame.
    sole: [Vec3; 2],
    /// Clearance kept from a leg's centre line (its radius and a little air).
    margin: f32,
    /// How far each column's hem is pushed out past its rest position.
    push: Vec<f32>,
}

/// Spawns the cloth under its bone (call from `spawn_humanoid`).
pub fn spawn(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    body: &[Handle<StandardMaterial>; 5],
    bones: &[Entity; Bone::COUNT],
    spec: &HumanoidSpec,
    skirt: &Skirt,
) {
    // Both sides drawn: from below or through an open cape you see the inside.
    let base = skirt.paint.material(materials, body);
    let mut material = materials.get(&base).cloned().unwrap_or_default();
    material.double_sided = true;
    material.cull_mode = None;
    let material = materials.add(material);
    let push = vec![0.0; skirt.columns()];
    let mesh = meshes.add(build_mesh(skirt, &push));
    let b = |bone: Bone| bones[bone.index()];
    let cloth = SkirtCloth {
        skirt: *skirt,
        legs: [[b(Bone::ThighL), b(Bone::ShinL), b(Bone::FootL)], [b(Bone::ThighR), b(Bone::ShinR), b(Bone::FootR)]],
        chest: b(Bone::Chest),
        sole: spec.sole_points(),
        margin: spec.leg_radius * 1.25 + 0.02 * spec.scale,
        push,
    };
    let e = commands
        .spawn((
            Name::new("Skirt"),
            cloth,
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Transform::IDENTITY,
            bevy::camera::visibility::NoFrustumCulling,
        ))
        .id();
    commands.entity(b(skirt.bone)).add_child(e);
}

/// Pushes every hem out past the legs (after the animation has posed them).
pub fn drape_skirts(
    time: Res<Time>,
    bones: Query<&Transform, Without<SkirtCloth>>,
    mut cloths: Query<(&mut SkirtCloth, &Mesh3d)>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let dt = time.delta_secs();
    for (mut cloth, mesh) in &mut cloths {
        // Knees, ankles, heels and toes in the pelvis' frame…
        let mut points = Vec::with_capacity(8);
        for [thigh, shin, foot] in cloth.legs {
            let (Ok(thigh), Ok(shin), Ok(foot)) = (bones.get(thigh), bones.get(shin), bones.get(foot)) else { continue };
            let thigh = thigh.compute_affine();
            let shin = thigh * shin.compute_affine();
            let foot = shin * foot.compute_affine();
            points.push(shin.translation.into());
            points.push(foot.translation.into());
            points.extend(cloth.sole.map(|p| foot.transform_point3(p)));
        }
        // …or the chest's, for a cape.
        if cloth.skirt.bone == Bone::Chest {
            let Ok(chest) = bones.get(cloth.chest) else { continue };
            let to_chest = chest.compute_affine().inverse();
            for p in &mut points {
                *p = to_chest.transform_point3(*p);
            }
        }
        let skirt = cloth.skirt;
        let margin = cloth.margin;
        let mut changed = false;
        for (i, push) in cloth.push.iter_mut().enumerate() {
            let (waist, hem, out) = skirt.column(i);
            let rest = (hem - waist).dot(out);
            // The hem pushed out so the straight line from the waist passes outside every point.
            let mut need = 0.0f32;
            for p in &points {
                let depth = (skirt.top - p.y).min(skirt.length);
                if depth < skirt.length * 0.15 {
                    continue;
                }
                let reach = (p.dot(out) + margin - waist.dot(out)) * skirt.length / depth;
                need = need.max(reach - rest);
            }
            let need = need.min(skirt.length * 1.2);
            let settled = if need >= *push { need } else { (*push - SETTLE_SPEED * dt).max(need) };
            changed |= (settled - *push).abs() > 1e-4;
            *push = settled;
        }
        if changed && let Some(mut m) = meshes.get_mut(&mesh.0) {
            let (positions, normals) = shape(&skirt, &cloth.push);
            m.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
            m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        }
    }
}

/// Positions (waist ring, then hem ring) and outward normals.
fn shape(skirt: &Skirt, push: &[f32]) -> (Vec<[f32; 3]>, Vec<[f32; 3]>) {
    let n = skirt.columns();
    let mut waist = Vec::with_capacity(n);
    let mut hem = Vec::with_capacity(n);
    for (i, push) in push.iter().enumerate().take(n) {
        let (w, h, out) = skirt.column(i);
        waist.push(w);
        hem.push(h + out * *push);
    }
    let mut normals = Vec::with_capacity(n);
    for i in 0..n {
        let (prev, next) = match (i, skirt.closed()) {
            (0, true) => (n - 2, 1),
            (i, true) if i == n - 1 => (n - 2, 1),
            (i, _) => (i.saturating_sub(1), (i + 1).min(n - 1)),
        };
        let around = (hem[next] + waist[next]) - (hem[prev] + waist[prev]);
        let down = hem[i] - waist[i];
        let mut normal = around.cross(down).normalize_or_zero();
        if normal.dot(skirt.column(i).2) < 0.0 {
            normal = -normal;
        }
        normals.push(normal.to_array());
    }
    let positions = waist.iter().chain(&hem).map(|p| p.to_array()).collect();
    let normals = normals.iter().chain(&normals).copied().collect();
    (positions, normals)
}

fn build_mesh(skirt: &Skirt, push: &[f32]) -> Mesh {
    let n = skirt.columns();
    let (positions, normals) = shape(skirt, push);
    // The cloth texture repeats a few times round and once down.
    let repeats = (skirt.span / PI * 3.0).max(1.0);
    let uv: Vec<[f32; 2]> = (0..2).flat_map(|row| (0..n).map(move |i| [i as f32 / (n - 1) as f32 * repeats, row as f32])).collect();
    let mut indices = Vec::with_capacity((n - 1) * 6);
    for i in 0..n as u32 - 1 {
        let (a, b, c, d) = (i, i + 1, i + n as u32, i + 1 + n as u32);
        indices.extend([a, b, c, b, d, c]); // counter-clockwise seen from outside
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
        .with_inserted_indices(Indices::U32(indices))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn robe() -> Skirt {
        Skirt::robe(0.1, Vec2::new(0.2, 0.13), Vec2::new(0.3, 0.27), 0.95, Paint::Bottom)
    }

    #[test]
    fn closed_robe_wraps_and_cape_stays_behind() {
        let r = robe();
        let n = r.columns();
        let (first, last) = (r.column(0).0, r.column(n - 1).0);
        assert!(first.distance(last) < 1e-4, "seam closes");
        let cape = Skirt { bone: Bone::Chest, span: 1.6, ..r };
        for i in 0..cape.columns() {
            assert!(cape.column(i).2.z > -0.05, "cape column {i} faces the back");
        }
    }

    #[test]
    fn normals_point_outward() {
        let r = robe();
        let (positions, normals) = shape(&r, &vec![0.0; r.columns()]);
        assert_eq!(positions.len(), normals.len());
        for (i, n) in normals.iter().enumerate().take(r.columns()) {
            assert!(Vec3::from(*n).dot(r.column(i).2) > 0.5, "column {i}");
        }
    }
}
