//! Levels built from a data file (`assets/levels/*.ron`, format in [`layout`]): rooms are
//! rectangles with a floor height, rasterised onto a fine grid; walls are generated on the grid
//! lines wherever a room borders nothing, a solid block or another room (except at doors and at
//! the ends of stairs). Rooms share the level's ceiling height (deeper rooms are taller) unless a
//! room sets its own `height` above its floor.
//!
//! The builder spawns floors, ceilings, walls (merged into long runs), stairs (an invisible ramp
//! to walk on + visible steps), torches, doors ([`door`]), sunflowers (`hearth::spawn_sunflower`),
//! enemy spawn points and the level's treasures (added to `PickupLayout`). Where a door sits on
//! the arena's ring wall the arena leaves a gap ([`ArenaOpenings`]).
//!
//! Several levels can hang off the arena ([`LEVELS`]): each is its own file and grid, with its own
//! look ([`layout::Theme`]), ceiling height and ids. The files are read at start from the asset
//! folder (`BEVY_ASSET_ROOT`, the crate folder or next to the binary); copies compiled into the
//! binary are the fallback.

pub mod door;
pub mod gothic;
pub mod layout;
pub mod library;
pub mod womb;
pub mod props;
pub mod riddle;

use std::path::PathBuf;

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::arena::{ArenaOpenings, TorchAssets, spawn_torch};
use crate::enemy::SpawnPoint;
use crate::hearth::spawn_sunflower;
use crate::items::pickup::{PickupLayout, UniqueDrops};
use crate::tuning::*;
use crate::textures::{self, Tex};
use layout::{EdgeKind, Layout, LevelFile, Theme};

pub struct LevelPlugin;

/// The levels loaded at start (asset path, embedded copy): the Sunken Halls north of the arena,
/// the Cathedral of Ashes south of it.
pub const LEVELS: [(&str, &str); 4] = [
    ("levels/lvl1.ron", include_str!("../../assets/levels/lvl1.ron")),
    ("levels/lvl2.ron", include_str!("../../assets/levels/lvl2.ron")),
    ("levels/lvl3.ron", include_str!("../../assets/levels/lvl3.ron")),
    ("levels/lvl4.ron", include_str!("../../assets/levels/lvl4.ron")),
];

/// Every loaded level.
#[derive(Resource, Clone, Debug, Default)]
pub struct Levels(pub Vec<Layout>);

impl Levels {
    /// Walkable floor height at (x, z) in any level.
    pub fn floor_at(&self, p: Vec2) -> Option<f32> {
        self.0.iter().find_map(|l| l.floor_at(p))
    }
}

impl Plugin for LevelPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((door::DoorPlugin, props::PropsPlugin, riddle::RiddlePlugin, library::LibraryPlugin, womb::WombPlugin));
        let mut levels = vec![];
        let mut openings = vec![];
        let mut riddles = vec![];
        for (path, embedded) in LEVELS {
            let Some(layout) = load_level(path, embedded) else { continue };
            info!("level \"{}\": {} rooms, {} wall runs, {} doors", layout.file.name, layout.floors.len(), layout.walls.len(), layout.doors.len());
            let treasures: Vec<_> = layout.file.treasures.iter().map(|t| (t.id, t.item, layout.ground(t.at))).collect();
            app.world_mut().get_resource_or_init::<PickupLayout>().0.extend(treasures);
            let unique: Vec<u32> = layout.file.enemies.iter().flat_map(|e| e.drops.iter().filter_map(|d| d.unique)).collect();
            app.world_mut().get_resource_or_init::<UniqueDrops>().0.extend(unique);
            // Doors on the arena's ring wall open a gap in it.
            openings.extend(
                layout
                    .doors
                    .iter()
                    .filter(|d| (d.centre.length() - ARENA_RADIUS).abs() < 0.75)
                    .map(|d| (d.centre, (d.opening + LEVEL_WALL_THICKNESS) * 0.5)),
            );
            riddles.extend(layout.file.riddles.iter().cloned().map(|r| (r, 0)));
            levels.push(layout);
        }
        app.insert_resource(ArenaOpenings(openings))
            .insert_resource(riddle::Riddles(riddles))
            .insert_resource(Levels(levels))
            .add_systems(Startup, spawn_levels);
    }
}

fn load_level(path: &str, embedded: &str) -> Option<Layout> {
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
        .find_map(|r| std::fs::read_to_string(r.join("assets").join(path)).ok())
        .unwrap_or_else(|| embedded.to_string());
    match ron::from_str::<LevelFile>(&text) {
        Ok(file) => Some(Layout::build(file)),
        Err(e) => {
            error!("level file {path} unreadable: {e}");
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

fn stone(materials: &mut Assets<StandardMaterial>, r: f32, g: f32, b: f32, tex: Option<Tex>) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color: Color::srgb(r, g, b),
        base_color_texture: tex.and_then(textures::get),
        perceptual_roughness: 0.92,
        reflectance: 0.2,
        ..default()
    })
}

fn spawn_levels(
    mut commands: Commands,
    levels: Res<Levels>,
    torches: Res<TorchAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for layout in &levels.0 {
        spawn_level(&mut commands, layout, &torches, &mut meshes, &mut materials);
    }
}

fn spawn_level(commands: &mut Commands, layout: &Layout, torches: &TorchAssets, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) {
    let mut commands = commands.reborrow();
    let (floor_mat, wall_mats, ceiling_mat, step_mat, tile) = match layout.file.theme {
        Theme::Sunken => (
            stone(materials, 0.19, 0.18, 0.17, None),
            [stone(materials, 0.24, 0.23, 0.22, None), stone(materials, 0.21, 0.205, 0.2, None)],
            stone(materials, 0.12, 0.115, 0.11, None),
            stone(materials, 0.27, 0.26, 0.24, None),
            None,
        ),
        // Cold, dark ashlar; worn flagstones; a soot-black vault.
        Theme::Gothic => (
            stone(materials, 0.36, 0.34, 0.35, Some(Tex::Flagstones)),
            [stone(materials, 0.34, 0.33, 0.37, Some(Tex::Bricks)), stone(materials, 0.31, 0.3, 0.34, Some(Tex::Bricks))],
            stone(materials, 0.12, 0.11, 0.13, Some(Tex::Bricks)),
            stone(materials, 0.4, 0.38, 0.4, Some(Tex::Marble)),
            Some(2.4),
        ),
        // Dark oak: panelled walls, plank floors, a soot-dark coffered ceiling.
        Theme::Library => (
            stone(materials, 0.42, 0.3, 0.2, Some(Tex::Planks)),
            [stone(materials, 0.34, 0.23, 0.15, Some(Tex::Panels)), stone(materials, 0.31, 0.21, 0.14, Some(Tex::Panels))],
            stone(materials, 0.14, 0.1, 0.08, Some(Tex::Panels)),
            stone(materials, 0.4, 0.29, 0.2, Some(Tex::Planks)),
            Some(3.0),
        ),
        // Living flesh: everything glows a little and throbs with the heartbeat (`womb::pulse_flesh`).
        Theme::Flesh => {
            let mut flesh = |c: Color, glow: LinearRgba| {
                materials.add(StandardMaterial {
                    base_color: c,
                    base_color_texture: textures::get(Tex::Flesh),
                    emissive: glow,
                    perceptual_roughness: 0.3,
                    reflectance: 0.5,
                    ..default()
                })
            };
            let mats = (
                flesh(Color::srgb(0.2, 0.065, 0.065), LinearRgba::rgb(0.006, 0.0, 0.0)),
                [flesh(Color::srgb(0.26, 0.08, 0.085), LinearRgba::rgb(0.01, 0.0005, 0.0005)), flesh(Color::srgb(0.23, 0.07, 0.075), LinearRgba::rgb(0.01, 0.0005, 0.0005))],
                flesh(Color::srgb(0.16, 0.04, 0.045), LinearRgba::rgb(0.008, 0.0, 0.0)),
                flesh(Color::srgb(0.22, 0.07, 0.07), LinearRgba::rgb(0.006, 0.0, 0.0)),
                Some(2.0),
            );
            let throb = [&mats.0, &mats.1[0], &mats.1[1], &mats.2, &mats.3].map(|h| (h.clone(), materials.get(h).map_or(LinearRgba::BLACK, |m| m.emissive)));
            commands.insert_resource(womb::FleshMaterials(throb.to_vec()));
            mats
        }
    };
    let ceiling_y = layout.ceiling();
    let wall_top = layout.wall_top();
    let root = commands.spawn((Name::new(format!("Level: {}", layout.file.name)), Transform::default(), Visibility::default())).id();
    let mut block = |commands: &mut Commands, name: &str, min: Vec3, max: Vec3, mat: &Handle<StandardMaterial>, solid: bool| {
        let size = max - min;
        let mesh = match tile {
            Some(t) => textures::tiled_box(size, (min + max) * 0.5, t),
            None => Cuboid::from_size(size).into(),
        };
        let mut e = commands.spawn((
            Name::new(name.to_string()),
            Mesh3d(meshes.add(mesh)),
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
        let room_ceiling = layout.room_ceiling(f.room);
        for (a, b) in ceiling((f.min, f.max)) {
            block(&mut commands, "Ceiling", Vec3::new(a.x, room_ceiling, a.y), Vec3::new(b.x, room_ceiling + 0.3, b.y), &ceiling_mat, true);
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
            (Vec3::new(w.from - t, bottom, w.line - t), Vec3::new(w.to + t, wall_top, w.line + t))
        } else {
            (Vec3::new(w.line - t, bottom, w.from - t), Vec3::new(w.line + t, wall_top, w.to + t))
        };
        block(&mut commands, "Wall", min, max, &wall_mats[n % 2], true);
        // A doorway between floors of different heights: wall below it, from the lower floor up
        // (a ladder climbs it).
        if let EdgeKind::Lintel(d) = w.kind
            && layout.doors[d].floor - layout.doors[d].low > 0.3
        {
            let riser_top = layout.doors[d].floor - 0.01;
            block(&mut commands, "Riser", Vec3::new(min.x, LEVEL_BOTTOM, min.z), Vec3::new(max.x, riser_top, max.z), &wall_mats[n % 2], true);
        }
    }

    // Stairs: an invisible ramp to walk on, and steps to look at.
    for s in &layout.stairs {
        block(&mut commands, "Stair Ceiling", Vec3::new(s.min.x, ceiling_y, s.min.y), Vec3::new(s.max.x, ceiling_y + 0.3, s.max.y), &ceiling_mat, true);
        let down = s.down.vec();
        let size = s.max - s.min;
        let (length, width) = if s.down.is_ns() { (size.y, size.x) } else { (size.x, size.y) };
        let centre = (s.min + s.max) * 0.5;
        let across = Vec2::new(-down.y, down.x);
        // A room at either end with a lower ceiling: close the gap between its ceiling and the
        // stair's with a header over the opening (else you'd look into the space above the room).
        for end in [-0.5, 0.5] {
            let edge = centre + down * end * length;
            let room_ceiling = layout.ceiling_at(edge + down * end.signum() * 0.5);
            if room_ceiling < ceiling_y - 0.05 {
                let half = across.abs() * width * 0.5 + down.abs() * LEVEL_WALL_THICKNESS * 0.5;
                block(&mut commands, "Stair Header", Vec3::new(edge.x - half.x, room_ceiling, edge.y - half.y), Vec3::new(edge.x + half.x, ceiling_y + 0.3, edge.y + half.y), &wall_mats[0], true);
            }
        }
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
            torches,
            Vec3::new(face.x, floor + TORCH_HEIGHT, face.y),
            Vec3::new(inward.x, 0.0, inward.y),
            layout.file.seed_base + n as u32,
            LEVEL_TORCH_INTENSITY,
            LEVEL_TORCH_RANGE,
        );
    }
    for d in &layout.doors {
        door::spawn_door(&mut commands, meshes, materials, d);
    }
    props::spawn_props(&mut commands, meshes, materials, torches, layout);
    for f in &layout.file.sunflowers {
        spawn_sunflower(&mut commands, meshes, materials, f.id, layout.ground(f.at), f.facing.yaw());
    }
    for (n, e) in layout.file.enemies.iter().enumerate() {
        let mut point = SpawnPoint::new(e.kind, layout.ground(e.at), e.patrol.map(|p| layout.ground(p)), layout.file.seed_base + n as u32).with_drops(e.drops.clone());
        point.facing = e.facing.map(|d| d.yaw());
        commands.spawn((Name::new(format!("Spawn point: {:?}", e.kind)), point));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_library_builds_and_hangs_together() {
        hangs_together(2);
    }

    #[test]
    fn the_womb_builds_and_hangs_together() {
        hangs_together(3);
    }

    /// Things on floors, torches on walls, ladders up walls to raised doorways, every room
    /// reachable from the first, ceilings under the walls' top.
    fn hangs_together(level: usize) {
        let file: LevelFile = ron::from_str(LEVELS[level].1).expect("level file parses");
        let l = Layout::build(file);
        let on_floor = |p: layout::Px| l.floor_at(l.world(p)).is_some();
        for t in &l.file.treasures {
            assert!(on_floor(t.at), "treasure {} off the floor", t.id);
        }
        for e in &l.file.enemies {
            assert!(on_floor(e.at), "{:?} at {:?} off the floor", e.kind, e.at);
            if let Some(p) = e.patrol {
                assert!(on_floor(p), "patrol {p:?} off the floor");
            }
        }
        for f in &l.file.sunflowers {
            assert!(on_floor(f.at), "sunflower {}", f.id);
        }
        for p in &l.file.props {
            // An illusory wall stands in its doorway (on the wall line).
            if !matches!(p.kind, layout::PropKind::IllusoryWall | layout::PropKind::FogGate) {
                assert!(on_floor(p.at), "{:?} at {:?} off the floor", p.kind, p.at);
            }
        }
        for (n, t) in l.file.torches.iter().enumerate() {
            let d = l.wall_distance(l.world(t.at), t.wall);
            assert!(d.is_some_and(|d| d < 3.0), "torch {n} at {:?} is {d:?} m from its wall", t.at);
        }
        // Each ladder: its foot on the lower floor, close to the wall it climbs, and a raised
        // doorway right above it onto the upper floor.
        for ld in &l.file.ladders {
            let at = l.world(ld.at);
            assert_eq!(l.floor_at(at), Some(ld.bottom), "ladder foot at {:?}", ld.at);
            let d = l.wall_distance(at, ld.facing).expect("a wall to climb");
            assert!(d < 1.0, "ladder {:?} is {d} m from its wall", ld.at);
            let beyond = at + ld.facing.vec() * (d + LEVEL_WALL_THICKNESS + 0.7);
            assert_eq!(l.floor_at(beyond), Some(ld.top), "the top of ladder {:?} leads onto its upper floor", ld.at);
            assert!(l.doors.iter().any(|door| door.centre.distance(at + ld.facing.vec() * (d + 0.25)) < 1.2 && door.floor == ld.top), "a doorway over ladder {:?}", ld.at);
        }
        // Every room can be reached from the entrance (through doors, archways, stairs, ladders).
        let room = |p: Vec2| l.room_at(p);
        let mut links: Vec<(usize, usize)> = vec![];
        for d in &l.doors {
            let v = d.opens.vec() * 0.6;
            if let (Some(a), Some(b)) = (room(d.centre + v), room(d.centre - v)) {
                links.push((a, b));
            }
        }
        for s in &l.stairs {
            let (c, v) = ((s.min + s.max) * 0.5, s.down.vec());
            let half = if s.down.is_ns() { (s.max.y - s.min.y) * 0.5 } else { (s.max.x - s.min.x) * 0.5 };
            if let (Some(a), Some(b)) = (room(c + v * (half + 0.4)), room(c - v * (half + 0.4))) {
                links.push((a, b));
            }
        }
        for ld in &l.file.ladders {
            let at = l.world(ld.at);
            let d = l.wall_distance(at, ld.facing).unwrap_or(0.0);
            if let (Some(a), Some(b)) = (room(at), room(at + ld.facing.vec() * (d + LEVEL_WALL_THICKNESS + 0.7))) {
                links.push((a, b));
            }
        }
        let mut seen = vec![0usize];
        while let Some(&(a, b)) = links.iter().find(|(a, b)| seen.contains(a) != seen.contains(b)) {
            seen.push(if seen.contains(&a) { b } else { a });
        }
        for (n, r) in l.file.rooms.iter().enumerate() {
            assert!(seen.contains(&n), "{} can't be reached", r.name);
        }
        // Every room's ceiling is under the walls' top (else you'd see out over them).
        for r in 0..l.file.rooms.len() {
            assert!(l.room_ceiling(r) <= l.ceiling(), "{} pokes out of the level", l.file.rooms[r].name);
        }
    }

    #[test]
    fn the_cathedral_builds_and_hangs_together() {
        let file: LevelFile = ron::from_str(LEVELS[1].1).expect("lvl2.ron parses");
        let l = Layout::build(file);
        assert!(l.walls.len() > 30);
        let on_floor = |p: layout::Px| l.floor_at(l.world(p)).is_some();
        for t in &l.file.treasures {
            assert!(on_floor(t.at), "treasure {} off the floor", t.id);
        }
        for e in &l.file.enemies {
            assert!(on_floor(e.at), "{:?} at {:?} off the floor", e.kind, e.at);
            if let Some(p) = e.patrol {
                assert!(on_floor(p), "patrol {p:?} off the floor");
            }
        }
        for f in &l.file.sunflowers {
            assert!(on_floor(f.at), "sunflower {}", f.id);
        }
        for p in &l.file.props {
            assert!(on_floor(p.at), "{:?} at {:?} off the floor", p.kind, p.at);
        }
        for (n, t) in l.file.torches.iter().enumerate() {
            let d = l.wall_distance(l.world(t.at), t.wall);
            assert!(d.is_some_and(|d| d < 3.0), "torch {n} at {:?} is {d:?} m from its wall", t.at);
        }
        // Windows stand against a wall.
        for p in l.file.props.iter().filter(|p| p.kind == layout::PropKind::Window) {
            let d = l.wall_distance(l.world(p.at), p.facing.opposite());
            assert!(d.is_some_and(|d| d < 1.0), "window at {:?} is {d:?} m from its wall", p.at);
        }
        // Door 11 joins the arena's south wall.
        let first = l.doors.iter().find(|d| d.id == 11).unwrap();
        assert!((first.centre - Vec2::new(0.0, ARENA_RADIUS)).length() < 0.2);
        // The riddle: three bells that exist, opening a sealed door.
        let riddle = &l.file.riddles[0];
        for b in &riddle.bells {
            assert!(l.file.props.iter().any(|p| p.kind == layout::PropKind::Bell && p.id == *b), "bell {b} missing");
        }
        assert!(l.doors.iter().any(|d| d.id == riddle.door && d.sealed), "the riddle's door must be sealed");
        // The fog gate's boss lives behind it.
        let gate = l.file.props.iter().find(|p| p.kind == layout::PropKind::FogGate).unwrap();
        let boss = gate.boss.unwrap();
        assert!(l.file.enemies.iter().any(|e| e.kind == boss));
        // Ids don't collide with level 1's.
        let lvl1 = Layout::build(ron::from_str(LEVELS[0].1).unwrap());
        for d in &l.doors {
            assert!(lvl1.doors.iter().all(|o| o.id != d.id), "door id {} used twice", d.id);
        }
        for t in &l.file.treasures {
            assert!(lvl1.file.treasures.iter().all(|o| o.id != t.id), "treasure id {} used twice", t.id);
        }
        for f in &l.file.sunflowers {
            assert!(lvl1.file.sunflowers.iter().all(|o| o.id != f.id));
        }
    }

    #[test]
    fn the_shipped_level_builds() {
        let file: LevelFile = ron::from_str(LEVELS[0].1).expect("lvl1.ron parses");
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
