//! Save and load (autosave). One save file in the OS data folder (`~/.local/share/flowerbox/
//! save.ron` on Linux), written at most once a second after something worth keeping changed, and
//! when quitting. Loaded at start: you continue at your rest site with your bag.
//!
//! Saved: the bag (items, stack counts, what's equipped), Healing Ale charges, whether the
//! sunflower is revived, the respawn point and which arena items were taken. While visiting
//! someone else's world (online guest) only the character is saved — the world fields keep what
//! they were in your own world.
//!
//! `--new-game` (command line) or "Delete save" in the menu start over (the latter takes effect
//! on the next start; autosaving stops for this session). Autopilot runs never touch the save
//! unless `FLOWERBOX_DATA_DIR` is set.

use std::path::PathBuf;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::healing::HealingAle;
use crate::hearth::{Hearth, RespawnPoint};
use crate::items::ItemId;
use crate::items::inventory::{Equipment, Inventory};
use crate::items::pickup::{PICKUPS, Pickup};
use crate::net::NetSession;
use crate::player::Player;
use crate::tuning::*;
use crate::ui::icons::NoticeIcon;
use crate::ui::notifications::Notify;
use crate::ui::strings;

pub struct SavePlugin;

impl Plugin for SavePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<DeleteSave>()
            .init_resource::<SaveState>()
            .add_systems(PostStartup, load_save)
            .add_systems(Update, (track_world, mark_dirty, autosave, delete_save).chain())
            .add_systems(Last, save_on_exit);
    }
}

/// Bump when the format changes incompatibly (older saves are then ignored).
const SAVE_VERSION: u32 = 1;

/// Delete the save file (the menu's "Delete save").
#[derive(Message, Debug, Clone, Copy)]
pub struct DeleteSave;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(default)]
pub struct SaveData {
    pub version: u32,
    pub items: Vec<ItemId>,
    pub counts: Vec<(ItemId, u32)>,
    pub equipped: [Option<usize>; 9],
    pub ale_charges: u32,
    pub ale_max: u32,
    pub sun_revived: bool,
    pub respawn: Option<([f32; 3], f32)>,
    pub taken: Vec<u32>,
}

#[derive(Resource, Default)]
struct SaveState {
    dirty: bool,
    /// Seconds until the next write is allowed.
    cooldown: f32,
    /// After "Delete save": stop writing until the next start.
    disabled: bool,
    /// The own world's state (not updated while visiting someone else's world).
    sun_revived: bool,
    respawn: Option<([f32; 3], f32)>,
    taken: Vec<u32>,
}

fn save_path() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("FLOWERBOX_DATA_DIR") {
        return Some(PathBuf::from(dir).join("save.ron"));
    }
    if std::env::var("FLOWERBOX_AUTOPILOT").is_ok() {
        return None;
    }
    dirs::data_dir().map(|d| d.join("flowerbox").join("save.ron"))
}

fn read_save() -> Option<SaveData> {
    let path = save_path()?;
    if std::env::args().any(|a| a == "--new-game") {
        let _ = std::fs::remove_file(&path);
        info!("new game: save deleted");
        return None;
    }
    let text = std::fs::read_to_string(&path).ok()?;
    match ron::from_str::<SaveData>(&text) {
        Ok(data) if data.version == SAVE_VERSION => Some(data),
        Ok(_) => {
            warn!("save file {} is from another version; starting fresh", path.display());
            None
        }
        Err(e) => {
            warn!("save file {} unreadable ({e}); starting fresh", path.display());
            None
        }
    }
}

#[allow(clippy::type_complexity)]
fn load_save(
    mut commands: Commands,
    mut state: ResMut<SaveState>,
    mut player: Query<(Entity, &mut Inventory, &mut Equipment, &mut HealingAle, &mut Transform), With<Player>>,
    mut hearths: Query<&mut Hearth>,
    pickups: Query<(Entity, &Pickup)>,
) {
    let Some(data) = read_save() else { return };
    let Ok((entity, mut inventory, mut equipment, mut ale, mut tf)) = player.single_mut() else { return };
    *inventory = Inventory { items: data.items.clone(), counts: data.counts.iter().copied().collect() };
    *equipment = Equipment::from_slots(data.equipped, &inventory);
    ale.max = data.ale_max.max(1);
    ale.charges = data.ale_charges.min(ale.max);
    for mut h in &mut hearths {
        h.kindled = data.sun_revived;
    }
    if let Some((pos, facing)) = data.respawn {
        let position = Vec3::from_array(pos);
        commands.entity(entity).insert(RespawnPoint { position, facing });
        tf.translation = position + Vec3::Y * (PLAYER_HALF_HEIGHT + GROUND_SKIN);
        tf.rotation = Quat::from_rotation_y(facing);
    }
    for (e, p) in &pickups {
        if data.taken.contains(&p.id) {
            commands.entity(e).despawn();
        }
    }
    state.sun_revived = data.sun_revived;
    state.respawn = data.respawn;
    state.taken = data.taken.clone();
    info!("save loaded ({} items, {} Ale)", data.items.len(), data.ale_charges);
}

/// Keeps the own world's fields current (not while visiting another world).
fn track_world(
    session: Res<NetSession>,
    mut state: ResMut<SaveState>,
    hearths: Query<&Hearth>,
    player: Query<Option<&RespawnPoint>, With<Player>>,
    pickups: Query<&Pickup>,
) {
    if session.is_client() {
        return;
    }
    let sun = hearths.iter().any(|h| h.kindled);
    let respawn = player.single().ok().flatten().map(|r| (r.position.to_array(), r.facing));
    let present: Vec<u32> = pickups.iter().map(|p| p.id).collect();
    let taken: Vec<u32> = (0..PICKUPS.len() as u32).filter(|id| !present.contains(id)).collect();
    if sun != state.sun_revived || respawn != state.respawn || taken != state.taken {
        state.sun_revived = sun;
        state.respawn = respawn;
        state.taken = taken;
        state.dirty = true;
    }
}

#[allow(clippy::type_complexity)]
fn mark_dirty(
    mut state: ResMut<SaveState>,
    changed: Query<(), (With<Player>, Or<(Changed<Inventory>, Changed<Equipment>, Changed<HealingAle>)>)>,
) {
    if !changed.is_empty() {
        state.dirty = true;
    }
}

fn snapshot(state: &SaveState, inventory: &Inventory, equipment: &Equipment, ale: &HealingAle) -> SaveData {
    let mut counts: Vec<(ItemId, u32)> = inventory.counts.iter().map(|(k, v)| (*k, *v)).collect();
    counts.sort_by_key(|(k, _)| format!("{k:?}"));
    SaveData {
        version: SAVE_VERSION,
        items: inventory.items.clone(),
        counts,
        equipped: equipment.slots(),
        ale_charges: ale.charges,
        ale_max: ale.max,
        sun_revived: state.sun_revived,
        respawn: state.respawn,
        taken: state.taken.clone(),
    }
}

fn write(data: &SaveData) {
    let Some(path) = save_path() else { return };
    let text = match ron::ser::to_string_pretty(data, ron::ser::PrettyConfig::default()) {
        Ok(t) => t,
        Err(e) => return warn!("couldn't encode the save: {e}"),
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    // Write a temp file and rename it, so a crash mid-write never leaves a broken save.
    let tmp = path.with_extension("ron.tmp");
    if std::fs::write(&tmp, text).and_then(|_| std::fs::rename(&tmp, &path)).is_err() {
        warn!("couldn't write the save to {}", path.display());
    }
}

fn autosave(
    time: Res<Time<Real>>,
    mut state: ResMut<SaveState>,
    player: Query<(&Inventory, &Equipment, &HealingAle), With<Player>>,
) {
    state.cooldown = (state.cooldown - time.delta_secs()).max(0.0);
    if !state.dirty || state.cooldown > 0.0 || state.disabled {
        return;
    }
    let Ok((inventory, equipment, ale)) = player.single() else { return };
    write(&snapshot(&state, inventory, equipment, ale));
    state.dirty = false;
    state.cooldown = 1.0;
}

fn save_on_exit(mut exits: MessageReader<AppExit>, state: Res<SaveState>, player: Query<(&Inventory, &Equipment, &HealingAle), With<Player>>) {
    if exits.read().count() == 0 || state.disabled {
        return;
    }
    if let Ok((inventory, equipment, ale)) = player.single() {
        write(&snapshot(&state, inventory, equipment, ale));
    }
}

fn delete_save(mut requests: MessageReader<DeleteSave>, mut state: ResMut<SaveState>, mut notify: MessageWriter<Notify>) {
    if requests.read().count() == 0 {
        return;
    }
    if let Some(path) = save_path() {
        let _ = std::fs::remove_file(path);
    }
    state.disabled = true;
    notify.write(Notify::new(NoticeIcon::Died, strings::SAVE_DELETED));
    info!("save deleted; autosave off until the next start");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_round_trips() {
        let inventory = Inventory::from_items(&[ItemId::Longsword, ItemId::HealingAle, ItemId::Poppy, ItemId::Poppy]);
        let mut equipment = Equipment::default();
        equipment.equip(&inventory, 0);
        equipment.equip(&inventory, 1);
        let ale = HealingAle::new(3);
        let state = SaveState { sun_revived: true, respawn: Some(([0.0, 0.0, 10.4], 0.0)), taken: vec![1], ..default() };
        let data = snapshot(&state, &inventory, &equipment, &ale);
        let text = ron::ser::to_string_pretty(&data, ron::ser::PrettyConfig::default()).unwrap();
        let back: SaveData = ron::from_str(&text).unwrap();
        assert_eq!(back, data);
        assert_eq!(back.counts, vec![(ItemId::Poppy, 2)]);
        let restored = Inventory { items: back.items.clone(), counts: back.counts.iter().copied().collect() };
        assert_eq!(Equipment::from_slots(back.equipped, &restored).weapon(&restored).name, "Longsword");
    }
}
