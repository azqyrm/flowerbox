//! The level file format and the pure geometry built from it (no ECS): rooms rasterised onto a
//! grid of `LEVEL_CELL` cells, wall runs wherever walls belong, stairs, doors and object positions.
//!
//! Coordinates in the file are **pixels of the design drawing** (`assets/levels/lvl1.png`):
//! x grows east, y grows south. `origin_px` is the pixel that lies at world `origin_world` (x, z);
//! `px_per_m` scales. North in the drawing = world -Z.

use bevy::prelude::*;
use serde::Deserialize;

use crate::items::ItemId;
use crate::tuning::*;

/// (x0, y0, x1, y1) in drawing pixels.
pub type PxRect = (f32, f32, f32, f32);
pub type Px = (f32, f32);

/// A compass direction in the drawing (North = up = world -Z).
#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    North,
    South,
    East,
    West,
}

impl Dir {
    /// Unit vector on the floor (x, z).
    pub fn vec(self) -> Vec2 {
        match self {
            Dir::North => Vec2::new(0.0, -1.0),
            Dir::South => Vec2::new(0.0, 1.0),
            Dir::East => Vec2::new(1.0, 0.0),
            Dir::West => Vec2::new(-1.0, 0.0),
        }
    }

    /// Yaw that looks this way (0 = -Z, like characters).
    pub fn yaw(self) -> f32 {
        let v = self.vec();
        f32::atan2(-v.x, -v.y)
    }

    /// True for North / South.
    pub fn is_ns(self) -> bool {
        matches!(self, Dir::North | Dir::South)
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct LevelFile {
    pub name: String,
    pub px_per_m: f32,
    pub origin_px: Px,
    pub origin_world: Px,
    pub rooms: Vec<RoomDef>,
    /// Solid blocks inside rooms (walls are generated around them).
    #[serde(default)]
    pub blocks: Vec<PxRect>,
    #[serde(default)]
    pub stairs: Vec<StairDef>,
    #[serde(default)]
    pub doors: Vec<DoorDef>,
    #[serde(default)]
    pub torches: Vec<TorchDef>,
    #[serde(default)]
    pub enemies: Vec<EnemyDef>,
    #[serde(default)]
    pub treasures: Vec<TreasureDef>,
    #[serde(default)]
    pub sunflowers: Vec<SunflowerDef>,
}

/// A room: one or more rectangles sharing a floor height. No walls between its own rectangles.
#[derive(Deserialize, Debug, Clone)]
pub struct RoomDef {
    pub name: String,
    pub floor: f32,
    pub rects: Vec<PxRect>,
}

/// Stairs inside `rect`, going down toward `down`, from floor height `top` to `bottom`. Open at both
/// ends (toward the rooms there), walled at the sides.
#[derive(Deserialize, Debug, Clone)]
pub struct StairDef {
    pub rect: PxRect,
    pub down: Dir,
    pub top: f32,
    pub bottom: f32,
}

/// A door in a wall. `at` lies on the wall line (its middle), `width` is the clear opening (m).
/// It turns on its `hinge` side and swings open toward `opens` (which also says the wall's
/// orientation: a door opening north sits in an east-west wall).
#[derive(Deserialize, Debug, Clone)]
pub struct DoorDef {
    pub id: u32,
    pub at: Px,
    pub width: f32,
    pub hinge: Dir,
    pub opens: Dir,
}

/// A torch on the wall that lies `wall`-wards of `at`.
#[derive(Deserialize, Debug, Clone)]
pub struct TorchDef {
    pub at: Px,
    pub wall: Dir,
}

/// A Hollow's home; with `patrol` it walks back and forth between the two points while idle.
#[derive(Deserialize, Debug, Clone)]
pub struct EnemyDef {
    /// Hollow (default) or Warg.
    #[serde(default)]
    pub kind: crate::enemy::EnemyKind,
    pub at: Px,
    #[serde(default)]
    pub patrol: Option<Px>,
}

/// An item lying on the floor. `id` must be unique among all pickups (the arena's are 0..).
#[derive(Deserialize, Debug, Clone)]
pub struct TreasureDef {
    pub id: u32,
    pub item: ItemId,
    pub at: Px,
}

#[derive(Deserialize, Debug, Clone)]
pub struct SunflowerDef {
    pub id: u32,
    pub at: Px,
    pub facing: Dir,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cell {
    Empty,
    Solid,
    Room(u16),
    Stair(u16),
}

impl Cell {
    fn walkable(self) -> bool {
        matches!(self, Cell::Room(_) | Cell::Stair(_))
    }
}

/// What stands on a cell boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeKind {
    Wall,
    /// The wall above door `n`'s opening.
    Lintel(usize),
}

/// A straight wall piece along a grid line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WallRun {
    /// True: runs along X at z = `line`; false: runs along Z at x = `line`.
    pub along_x: bool,
    pub line: f32,
    pub from: f32,
    pub to: f32,
    pub kind: EdgeKind,
}

/// A door placed on the grid.
#[derive(Clone, Debug)]
pub struct DoorPlan {
    pub id: u32,
    /// Middle of the opening on the wall line (x, z), and its floor height.
    pub centre: Vec2,
    pub floor: f32,
    /// Clear opening (m), between the wall ends.
    pub opening: f32,
    pub hinge: Dir,
    pub opens: Dir,
    along_x: bool,
    /// Grid index of the wall line, and the cell span of the hole.
    line: i32,
    span: (i32, i32),
}

impl DoorPlan {
    /// The hinge point (x, z) on the wall line.
    pub fn hinge_point(&self) -> Vec2 {
        self.centre + self.hinge.vec() * self.opening * 0.5
    }
}

/// Stairs placed in world space.
#[derive(Clone, Debug)]
pub struct StairPlan {
    pub min: Vec2,
    pub max: Vec2,
    pub down: Dir,
    pub top: f32,
    pub bottom: f32,
}

impl StairPlan {
    /// Floor height at (x, z) on the slope.
    pub fn height_at(&self, p: Vec2) -> f32 {
        let t = match self.down {
            Dir::East => (p.x - self.min.x) / (self.max.x - self.min.x),
            Dir::West => (self.max.x - p.x) / (self.max.x - self.min.x),
            Dir::South => (p.y - self.min.y) / (self.max.y - self.min.y),
            Dir::North => (self.max.y - p.y) / (self.max.y - self.min.y),
        };
        self.top + (self.bottom - self.top) * t.clamp(0.0, 1.0)
    }
}

/// A room rectangle in world space (x, z).
#[derive(Clone, Copy, Debug)]
pub struct FloorRect {
    /// Index into the file's rooms.
    pub room: usize,
    pub min: Vec2,
    pub max: Vec2,
    pub floor: f32,
}

/// Everything the builder needs, in world space.
#[derive(Resource, Clone, Debug)]
pub struct Layout {
    pub file: LevelFile,
    origin: Vec2,
    nx: i32,
    nz: i32,
    cells: Vec<Cell>,
    pub floors: Vec<FloorRect>,
    pub stairs: Vec<StairPlan>,
    pub doors: Vec<DoorPlan>,
    pub walls: Vec<WallRun>,
}

fn snap(v: f32) -> f32 {
    (v / LEVEL_CELL).round() * LEVEL_CELL
}

impl Layout {
    /// Drawing pixel → world (x, z), unsnapped.
    pub fn world(&self, p: Px) -> Vec2 {
        px_to_world(&self.file, p)
    }

    fn rect(&self, r: PxRect) -> (Vec2, Vec2) {
        px_rect(&self.file, r)
    }

    fn index(&self, p: Vec2) -> Option<(i32, i32)> {
        let i = ((p.x - self.origin.x) / LEVEL_CELL).floor() as i32;
        let j = ((p.y - self.origin.y) / LEVEL_CELL).floor() as i32;
        (i >= 0 && j >= 0 && i < self.nx && j < self.nz).then_some((i, j))
    }

    fn cell(&self, i: i32, j: i32) -> Cell {
        if i < 0 || j < 0 || i >= self.nx || j >= self.nz {
            return Cell::Empty;
        }
        self.cells[(j * self.nx + i) as usize]
    }

    /// Walkable floor height at (x, z), if any.
    pub fn floor_at(&self, p: Vec2) -> Option<f32> {
        let (i, j) = self.index(p)?;
        match self.cell(i, j) {
            Cell::Room(r) => Some(self.file.rooms[r as usize].floor),
            Cell::Stair(s) => Some(self.stairs[s as usize].height_at(p)),
            _ => None,
        }
    }

    /// Floor point (x, floor, z) under drawing pixel `p` (0 if outside every room).
    pub fn ground(&self, p: Px) -> Vec3 {
        let w = self.world(p);
        Vec3::new(w.x, self.floor_at(w).unwrap_or(0.0), w.y)
    }

    /// Distance from `p` to the wall in direction `dir` (walking cell by cell while the cells
    /// stay walkable and in the same room).
    pub fn wall_distance(&self, p: Vec2, dir: Dir) -> Option<f32> {
        let (i0, j0) = self.index(p)?;
        let start = self.cell(i0, j0);
        if !start.walkable() {
            return None;
        }
        let (di, dj) = match dir {
            Dir::North => (0, -1),
            Dir::South => (0, 1),
            Dir::East => (1, 0),
            Dir::West => (-1, 0),
        };
        let (mut i, mut j) = (i0, j0);
        while self.cell(i + di, j + dj) == start {
            i += di;
            j += dj;
        }
        // The boundary after cell (i, j) in `dir`.
        let edge = match dir {
            Dir::North => self.origin.y + j as f32 * LEVEL_CELL,
            Dir::South => self.origin.y + (j + 1) as f32 * LEVEL_CELL,
            Dir::West => self.origin.x + i as f32 * LEVEL_CELL,
            Dir::East => self.origin.x + (i + 1) as f32 * LEVEL_CELL,
        };
        Some(match dir {
            Dir::North => p.y - edge,
            Dir::South => edge - p.y,
            Dir::West => p.x - edge,
            Dir::East => edge - p.x,
        })
    }

    /// Builds the grid, doors and walls from the file.
    pub fn build(file: LevelFile) -> Self {
        // Bounds of everything walkable, plus a margin.
        let mut lo = Vec2::splat(f32::MAX);
        let mut hi = Vec2::splat(f32::MIN);
        let all_rects = file.rooms.iter().flat_map(|r| r.rects.iter().copied()).chain(file.stairs.iter().map(|s| s.rect));
        for r in all_rects {
            let (a, b) = px_rect(&file, r);
            lo = lo.min(a);
            hi = hi.max(b);
        }
        let margin = 4.0 * LEVEL_CELL;
        let origin = lo - Vec2::splat(margin);
        let nx = ((hi.x - lo.x + 2.0 * margin) / LEVEL_CELL).round() as i32;
        let nz = ((hi.y - lo.y + 2.0 * margin) / LEVEL_CELL).round() as i32;
        let mut layout = Layout {
            file,
            origin,
            nx,
            nz,
            cells: vec![Cell::Empty; (nx * nz) as usize],
            floors: vec![],
            stairs: vec![],
            doors: vec![],
            walls: vec![],
        };
        let span = |l: &Layout, a: Vec2, b: Vec2| {
            let i0 = ((a.x - l.origin.x) / LEVEL_CELL).round() as i32;
            let i1 = ((b.x - l.origin.x) / LEVEL_CELL).round() as i32;
            let j0 = ((a.y - l.origin.y) / LEVEL_CELL).round() as i32;
            let j1 = ((b.y - l.origin.y) / LEVEL_CELL).round() as i32;
            (i0, i1, j0, j1)
        };
        let fill = |l: &mut Layout, a: Vec2, b: Vec2, c: Cell| {
            let (i0, i1, j0, j1) = span(l, a, b);
            for j in j0..j1 {
                for i in i0..i1 {
                    l.cells[(j * l.nx + i) as usize] = c;
                }
            }
        };
        for (n, room) in layout.file.rooms.clone().iter().enumerate() {
            for &r in &room.rects {
                let (a, b) = layout.rect(r);
                fill(&mut layout, a, b, Cell::Room(n as u16));
                layout.floors.push(FloorRect { room: n, min: a, max: b, floor: room.floor });
            }
        }
        for &r in &layout.file.blocks.clone() {
            let (a, b) = layout.rect(r);
            fill(&mut layout, a, b, Cell::Solid);
        }
        for (n, s) in layout.file.stairs.clone().iter().enumerate() {
            let (a, b) = layout.rect(s.rect);
            fill(&mut layout, a, b, Cell::Stair(n as u16));
            layout.stairs.push(StairPlan { min: a, max: b, down: s.down, top: s.top, bottom: s.bottom });
        }
        // Doors: the hole is the opening plus a wall thickness (the wall ends overlap it by half
        // a thickness on each side).
        for d in layout.file.doors.clone() {
            let along_x = d.opens.is_ns();
            let c = layout.world(d.at);
            let c = Vec2::new(snap(c.x), snap(c.y));
            let half = (d.width + LEVEL_WALL_THICKNESS) * 0.5;
            let (line, span) = if along_x {
                let line = ((c.y - origin.y) / LEVEL_CELL).round() as i32;
                (line, (((c.x - half - origin.x) / LEVEL_CELL).round() as i32, ((c.x + half - origin.x) / LEVEL_CELL).round() as i32))
            } else {
                let line = ((c.x - origin.x) / LEVEL_CELL).round() as i32;
                (line, (((c.y - half - origin.y) / LEVEL_CELL).round() as i32, ((c.y + half - origin.y) / LEVEL_CELL).round() as i32))
            };
            let hole = (span.1 - span.0) as f32 * LEVEL_CELL;
            let mid = (span.0 + span.1) as f32 * 0.5 * LEVEL_CELL;
            let centre = if along_x { Vec2::new(origin.x + mid, c.y) } else { Vec2::new(c.x, origin.y + mid) };
            // Floor: the higher of the two sides.
            let probe = d.opens.vec() * LEVEL_CELL * 0.5;
            let floor = [centre + probe, centre - probe].iter().filter_map(|p| layout.floor_at(*p)).fold(f32::MIN, f32::max);
            layout.doors.push(DoorPlan {
                id: d.id,
                centre,
                floor: if floor == f32::MIN { 0.0 } else { floor },
                opening: hole - LEVEL_WALL_THICKNESS,
                hinge: d.hinge,
                opens: d.opens,
                along_x,
                line,
                span,
            });
        }
        layout.walls = layout.wall_runs();
        layout
    }

    /// Does a wall stand between cells `a` and `b` (`b` lies `dir` of `a`)?
    fn needs_wall(&self, a: Cell, b: Cell, dir: Dir) -> bool {
        match (a, b) {
            (Cell::Room(x), Cell::Room(y)) => x != y,
            (Cell::Stair(x), Cell::Stair(y)) => x != y,
            (Cell::Stair(s), Cell::Room(_)) => !self.is_stair_end(s, dir),
            (Cell::Room(_), Cell::Stair(s)) => !self.is_stair_end(s, opposite(dir)),
            (x, y) => x.walkable() != y.walkable(),
        }
    }

    /// Stepping off stair `s` toward `dir` leaves it through its top or bottom end.
    fn is_stair_end(&self, s: u16, dir: Dir) -> bool {
        let down = self.stairs[s as usize].down;
        dir == down || dir == opposite(down)
    }

    fn door_on(&self, along_x: bool, line: i32, at: i32) -> Option<usize> {
        self.doors.iter().position(|d| d.along_x == along_x && d.line == line && (d.span.0..d.span.1).contains(&at))
    }

    /// Every boundary that needs a wall, merged into straight runs.
    fn wall_runs(&self) -> Vec<WallRun> {
        let mut runs = vec![];
        // Walls along Z: boundary x-line `bx` between cells (bx-1, j) and (bx, j).
        for bx in 0..=self.nx {
            let mut current: Option<(EdgeKind, i32)> = None;
            for j in 0..=self.nz {
                let kind = (j < self.nz)
                    .then(|| {
                        let wall = self.needs_wall(self.cell(bx - 1, j), self.cell(bx, j), Dir::East);
                        wall.then(|| self.door_on(false, bx, j).map_or(EdgeKind::Wall, EdgeKind::Lintel))
                    })
                    .flatten();
                if current.map(|c| c.0) != kind {
                    if let Some((k, start)) = current {
                        let x = self.origin.x + bx as f32 * LEVEL_CELL;
                        let (from, to) = (self.origin.y + start as f32 * LEVEL_CELL, self.origin.y + j as f32 * LEVEL_CELL);
                        runs.push(WallRun { along_x: false, line: x, from, to, kind: k });
                    }
                    current = kind.map(|k| (k, j));
                }
            }
        }
        // Walls along X: boundary z-line `bz` between cells (i, bz-1) and (i, bz).
        for bz in 0..=self.nz {
            let mut current: Option<(EdgeKind, i32)> = None;
            for i in 0..=self.nx {
                let kind = (i < self.nx)
                    .then(|| {
                        let wall = self.needs_wall(self.cell(i, bz - 1), self.cell(i, bz), Dir::South);
                        wall.then(|| self.door_on(true, bz, i).map_or(EdgeKind::Wall, EdgeKind::Lintel))
                    })
                    .flatten();
                if current.map(|c| c.0) != kind {
                    if let Some((k, start)) = current {
                        let z = self.origin.y + bz as f32 * LEVEL_CELL;
                        let (from, to) = (self.origin.x + start as f32 * LEVEL_CELL, self.origin.x + i as f32 * LEVEL_CELL);
                        runs.push(WallRun { along_x: true, line: z, from, to, kind: k });
                    }
                    current = kind.map(|k| (k, i));
                }
            }
        }
        runs
    }
}

fn opposite(d: Dir) -> Dir {
    match d {
        Dir::North => Dir::South,
        Dir::South => Dir::North,
        Dir::East => Dir::West,
        Dir::West => Dir::East,
    }
}

fn px_to_world(file: &LevelFile, p: Px) -> Vec2 {
    Vec2::new(
        file.origin_world.0 + (p.0 - file.origin_px.0) / file.px_per_m,
        file.origin_world.1 + (p.1 - file.origin_px.1) / file.px_per_m,
    )
}

/// A pixel rectangle → world (min, max), snapped to the grid.
fn px_rect(file: &LevelFile, r: PxRect) -> (Vec2, Vec2) {
    let a = px_to_world(file, (r.0, r.1));
    let b = px_to_world(file, (r.2, r.3));
    let s = |v: Vec2| Vec2::new(snap(v.x), snap(v.y));
    (s(a.min(b)), s(a.max(b)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file() -> LevelFile {
        // Two 4 × 4 m rooms side by side (west at 0, east at -1), stairs between them in the
        // east room's west strip, a door in the west room's south wall.
        ron::from_str(
            r#"(
                name: "test", px_per_m: 1.0, origin_px: (0, 0), origin_world: (0, 0),
                rooms: [
                    (name: "west", floor: 0.0, rects: [(0, 0, 4, 4)]),
                    (name: "east", floor: -1.0, rects: [(4, 0, 8, 4)]),
                ],
                blocks: [(7, 0, 8, 1)],
                stairs: [(rect: (4, 1, 6, 3), down: East, top: 0.0, bottom: -1.0)],
                doors: [(id: 1, at: (2, 4), width: 1.5, hinge: East, opens: North)],
            )"#,
        )
        .unwrap()
    }

    #[test]
    fn floors_and_stairs() {
        let l = Layout::build(file());
        assert_eq!(l.floor_at(Vec2::new(1.0, 1.0)), Some(0.0));
        assert_eq!(l.floor_at(Vec2::new(7.0, 3.0)), Some(-1.0));
        assert!((l.floor_at(Vec2::new(5.0, 2.0)).unwrap() + 0.5).abs() < 1e-4, "half way down the stairs");
        assert_eq!(l.floor_at(Vec2::new(7.5, 0.5)), None, "inside the block");
        assert_eq!(l.floor_at(Vec2::new(-3.0, 1.0)), None, "outside");
    }

    #[test]
    fn walls_split_rooms_but_not_stair_ends() {
        let l = Layout::build(file());
        let wall_at = |along_x: bool, line: f32, at: f32| {
            l.walls.iter().any(|w| w.along_x == along_x && (w.line - line).abs() < 1e-4 && w.from <= at && at <= w.to && w.kind == EdgeKind::Wall)
        };
        // Between west and east rooms at x = 4: wall beside the stairs, open at the stair's top.
        assert!(wall_at(false, 4.0, 0.5));
        assert!(wall_at(false, 4.0, 3.5));
        assert!(!wall_at(false, 4.0, 2.0), "the stairs' top end is open");
        assert!(!wall_at(false, 6.0, 2.0), "the stairs' bottom end is open");
        assert!(wall_at(true, 1.0, 5.0), "stair side toward the east room");
        // The block is walled in.
        assert!(wall_at(false, 7.0, 0.5));
        // Outer walls.
        assert!(wall_at(true, 0.0, 2.0) && wall_at(false, 0.0, 2.0) && wall_at(false, 8.0, 2.0));
    }

    #[test]
    fn doors_cut_a_lintel() {
        let l = Layout::build(file());
        let d = &l.doors[0];
        assert!((d.centre - Vec2::new(2.0, 4.0)).length() < 1e-4);
        assert!((d.opening - 1.5).abs() < 0.26, "{}", d.opening);
        assert_eq!(d.floor, 0.0);
        assert!(l.walls.iter().any(|w| w.kind == EdgeKind::Lintel(0) && w.along_x && (w.line - 4.0).abs() < 1e-4));
        assert!(!l.walls.iter().any(|w| w.kind == EdgeKind::Wall && w.along_x && (w.line - 4.0).abs() < 1e-4 && w.from < 2.0 && w.to > 2.0));
        assert!((d.hinge_point() - Vec2::new(2.0 + d.opening * 0.5, 4.0)).length() < 1e-4);
    }

    #[test]
    fn wall_distance_finds_the_wall() {
        let l = Layout::build(file());
        assert!((l.wall_distance(Vec2::new(1.1, 2.1), Dir::West).unwrap() - 1.1).abs() < 1e-4);
        assert!((l.wall_distance(Vec2::new(1.1, 2.1), Dir::North).unwrap() - 2.1).abs() < 1e-4);
    }
}
