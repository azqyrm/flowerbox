//! Levels built from a data file (`assets/levels/*.ron`, format in [`layout`]): rooms are
//! rectangles with a floor height, rasterised onto a fine grid; walls are generated on the grid
//! lines wherever a room borders nothing, a solid block or another room (except at doors and at
//! the ends of stairs). Every room shares one ceiling height, so deeper rooms are taller.
//!
//! The builder spawns floors, ceilings, walls (merged into long runs), stairs (an invisible ramp
//! to walk on + visible steps), torches, doors ([`door`]), sunflowers (`hearth::spawn_sunflower`),
//! enemy spawn points and the level's treasures (added to `PickupLayout`). Where a door sits on
//! the arena's ring wall the arena leaves a gap ([`ArenaOpenings`]).
//!
//! The file is read at start from the asset folder (`BEVY_ASSET_ROOT`, the crate folder or next to
//! the binary); a copy compiled into the binary is the fallback.

pub mod door;
pub mod layout;
pub mod props;

use std::path::PathBuf;

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::arena::{ArenaOpenings, TorchAssets, spawn_torch};
use crate::enemy::SpawnPoint;
use crate::hearth::spawn_sunflower;
use crate::items::pickup::{PickupLayout, UniqueDrops};
use crate::tuning::*;
use layout::{EdgeKind, Layout, LevelFile};

pub struct LevelPlugin;

/// The level loaded at start.
const LEVEL: &str = "levels/lvl1.ron";
const EMBEDDED: &str = include_str!("../../assets/levels/lvl1.ron");

impl Plugin for LevelPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((door::DoorPlugin, props::PropsPlugin));
        let Some(layout) = load_level() else { return };
        info!("level \"{}\": {} rooms, {} wall runs, {} doors", layout.file.name, layout.floors.len(), layout.walls.len(), layout.doors.len());
        let treasures: Vec<_> = layout.file.treasures.iter().map(|t| (t.id, t.item, layout.ground(t.at))).collect();
        app.world_mut().get_resource_or_init::<PickupLayout>().0.extend(treasures);
        let unique: Vec<u32> = layout.file.enemies.iter().flat_map(|e| e.drops.iter().filter_map(|d| d.unique)).collect();
        app.world_mut().get_resource_or_init::<UniqueDrops>().0.extend(unique);
        // Doors on the arena's ring wall open a gap in it.
        let openings = layout
            .doors
            .iter()
            .filter(|d| (d.centre.length() - ARENA_RADIUS).abs() < 0.75)
            .map(|d| (d.centre, (d.opening + LEVEL_WALL_THICKNESS) * 0.5))
            .collect();
        app.insert_resource(ArenaOpenings(openings)).insert_resource(layout).add_systems(Startup, spawn_level);
    }
}

fn load_level() -> Option<Layout> {
    let mut roots: Vec<PathBuf> = vec![];
    if let Ok(r) = std::env::var("BEVY_ASSET_ROOT") {
        roots.push(r.into());
    }
    if let Ok(r) = std::env::var("CARGO_MANIFEST_DIR") {
        roots.push(r.into());
    }
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(PathBuf::from)) {
        roots.push(dir);
    }
    let text = roots
        .iter()
        .find_map(|r| std::fs::read_to_string(r.join("assets").join(LEVEL)).ok())
        .unwrap_or_else(|| EMBEDDED.to_string());
    match ron::from_str::<LevelFile>(&text) {
        Ok(file) => Some(Layout::build(file)),
        Err(e) => {
            error!("level file {LEVEL} unreadable: {e}");
            None
        }
    }
}

/// True if nothing that `consider` accepts lies on the straight line between the two points
/// (walls, closed doors; pass a predicate that skips characters).
pub fn line_of_sight(spatial: &SpatialQuery, from: Vec3, to: Vec3, consider: &dyn Fn(Entity) -> bool) -> bool {
    let d = to - from;
    let Ok(dir) = Dir3::new(d) else { return true };
    spatial.cast_ray_predicate(from, dir, d.length(), true, &SpatialQueryFilter::default(), consider).is_none()
}

fn stone(materials: &mut Assets<StandardMaterial>, r: f32, g: f32, b: f32) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial { base_color: Color::srgb(r, g, b), perceptual_roughness: 0.92, reflectance: 0.2, ..default() })
}

#[allow(clippy::too_many_arguments)]
fn spawn_level(
    mut commands: Commands,
    layout: Res<Layout>,
    torches: Res<TorchAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let floor_mat = stone(&mut materials, 0.19, 0.18, 0.17);
    let wall_mats = [stone(&mut materials, 0.24, 0.23, 0.22), stone(&mut materials, 0.21, 0.205, 0.2)];
    let ceiling_mat = stone(&mut materials, 0.12, 0.115, 0.11);
    let step_mat = stone(&mut materials, 0.27, 0.26, 0.24);
    let root = commands.spawn((Name::new(format!("Level: {}", layout.file.name)), Transform::default(), Visibility::default())).id();
    let mut block = |commands: &mut Commands, name: &str, min: Vec3, max: Vec3, mat: &Handle<StandardMaterial>, solid: bool| {
        let size = max - min;
        let mut e = commands.spawn((
            Name::new(name.to_string()),
            Mesh3d(meshes.add(Cuboid::from_size(size))),
            MeshMaterial3d(mat.clone()),
            Transform::from_translation((min + max) * 0.5),
            ChildOf(root),
        ));
        if solid {
            e.insert((RigidBody::Static, Collider::cuboid(size.x, size.y, size.z)));
        }
    };

    let holes = layout.ceiling_holes();
    let ceiling = |rect: (Vec2, Vec2)| holes.iter().fold(vec![rect], |pieces, h| pieces.into_iter().flat_map(|p| layout::subtract_rect(p, *h)).collect());
    for f in &layout.floors {
        for (a, b) in ceiling((f.min, f.max)) {
            block(&mut commands, "Ceiling", Vec3::new(a.x, LEVEL_CEILING, a.y), Vec3::new(b.x, LEVEL_CEILING + 0.3, b.y), &ceiling_mat, true);
        }
        // No floor under stairs: their ramp is solid, and a floor slab reaching under a stair
        // from the upper room would stick up through the ramp as a step.
        let name = format!("Floor: {}", layout.file.rooms[f.room].name);
        let pieces = layout.stairs.iter().fold(vec![(f.min, f.max)], |pieces, st| {
            pieces.into_iter().flat_map(|p| layout::subtract_rect(p, (st.min, st.max))).collect()
        });
        for (a, b) in pieces {
            block(&mut commands, &name, Vec3::new(a.x, f.floor - 0.5, a.y), Vec3::new(b.x, f.floor, b.y), &floor_mat, true);
        }
    }
    let t = LEVEL_WALL_THICKNESS * 0.5;
    for (n, w) in layout.walls.iter().enumerate() {
        let bottom = match w.kind {
            EdgeKind::Wall => LEVEL_BOTTOM,
            EdgeKind::Lintel(d) => layout.doors[d].floor + DOOR_HEIGHT,
        };
        let (min, max) = if w.along_x {
            (Vec3::new(w.from - t, bottom, w.line - t), Vec3::new(w.to + t, LEVEL_WALL_TOP, w.line + t))
        } else {
            (Vec3::new(w.line - t, bottom, w.from - t), Vec3::new(w.line + t, LEVEL_WALL_TOP, w.to + t))
        };
        block(&mut commands, "Wall", min, max, &wall_mats[n % 2], true);
    }

    // Stairs: an invisible ramp to walk on, and steps to look at.
    for s in &layout.stairs {
        block(&mut commands, "Stair Ceiling", Vec3::new(s.min.x, LEVEL_CEILING, s.min.y), Vec3::new(s.max.x, LEVEL_CEILING + 0.3, s.max.y), &ceiling_mat, true);
        let down = s.down.vec();
        let size = s.max - s.min;
        let (length, width) = if s.down.is_ns() { (size.y, size.x) } else { (size.x, size.y) };
        let centre = (s.min + s.max) * 0.5;
        let across = Vec2::new(-down.y, down.x);
        let base = s.bottom - 0.5;
        let mut points = vec![];
        for (along, h) in [(-0.5, s.top), (0.5, s.bottom)] {
            for side in [-0.5, 0.5] {
                let p = down * along * length + across * side * width;
                points.push(Vec3::new(p.x, h, p.y));
                points.push(Vec3::new(p.x, base, p.y));
            }
        }
        if let Some(ramp) = Collider::convex_hull(&points) {
            commands.spawn((Name::new("Stair Ramp"), RigidBody::Static, ramp, Transform::from_xyz(centre.x, 0.0, centre.y), ChildOf(root)));
        }
        let steps = ((s.top - s.bottom).abs() / STAIR_RISE).ceil().max(1.0) as usize;
        let run = length / steps as f32;
        for k in 0..steps {
            let h = s.top + (s.bottom - s.top) * (k as f32 + 0.5) / steps as f32;
            let mid = centre + down * (-0.5 * length + (k as f32 + 0.5) * run);
            let half = if s.down.is_ns() { Vec2::new(width, run) * 0.5 } else { Vec2::new(run, width) * 0.5 };
            block(&mut commands, "Step", Vec3::new(mid.x - half.x, base, mid.y - half.y), Vec3::new(mid.x + half.x, h, mid.y + half.y), &step_mat, false);
        }
    }

    for (n, torch) in layout.file.torches.iter().enumerate() {
        let p = layout.world(torch.at);
        let (Some(dist), Some(floor)) = (layout.wall_distance(p, torch.wall), layout.floor_at(p)) else {
            warn!("torch {n} at {:?} isn't in a room", torch.at);
            continue;
        };
        let face = p + torch.wall.vec() * (dist - t);
        let inward = -torch.wall.vec();
        spawn_torch(
            &mut commands,
            &torches,
            Vec3::new(face.x, floor + TORCH_HEIGHT, face.y),
            Vec3::new(inward.x, 0.0, inward.y),
            100 + n as u32,
            LEVEL_TORCH_INTENSITY,
            LEVEL_TORCH_RANGE,
        );
    }
    for d in &layout.doors {
        door::spawn_door(&mut commands, &mut meshes, &mut materials, d);
    }
    props::spawn_props(&mut commands, &mut meshes, &mut materials, &torches, &layout);
    for f in &layout.file.sunflowers {
        spawn_sunflower(&mut commands, &mut meshes, &mut materials, f.id, layout.ground(f.at), f.facing.yaw());
    }
    for (n, e) in layout.file.enemies.iter().enumerate() {
        let point = SpawnPoint::new(e.kind, layout.ground(e.at), e.patrol.map(|p| layout.ground(p)), 100 + n as u32).with_drops(e.drops.clone());
        commands.spawn((Name::new(format!("Spawn point: {:?}", e.kind)), point));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_level_builds() {
        let file: LevelFile = ron::from_str(EMBEDDED).expect("lvl1.ron parses");
        let l = Layout::build(file);
        assert!(l.walls.len() > 20);
        // Every object stands on a floor.
        for t in &l.file.treasures {
            assert!(l.floor_at(l.world(t.at)).is_some(), "treasure {} off the floor", t.id);
        }
        for e in &l.file.enemies {
            assert!(l.floor_at(l.world(e.at)).is_some(), "enemy at {:?} off the floor", e.at);
        }
        for f in &l.file.sunflowers {
            assert!(l.floor_at(l.world(f.at)).is_some());
        }
        for (n, t) in l.file.torches.iter().enumerate() {
            let d = l.wall_distance(l.world(t.at), t.wall);
            assert!(d.is_some_and(|d| d < 3.0), "torch {n} at {:?} is {d:?} m from its wall", t.at);
        }
        // The first door joins the arena's north wall.
        assert!((l.doors[0].centre - Vec2::new(0.0, -ARENA_RADIUS)).length() < 0.2);
        assert_eq!(l.doors[0].floor, 0.0);
        // Stairs go down away from the start: corridor (0) → middle (-1.5).
        assert_eq!(l.floor_at(Vec2::new(-1.0, -20.0)), Some(0.0));
    }
}
