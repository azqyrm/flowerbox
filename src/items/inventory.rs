//! The bag ([`Inventory`]), what is worn ([`Equipment`]) and what that adds up to ([`EquipStats`]).
//!
//! Equipping goes through [`EquipRequest`] messages (the inventory UI, debug keys and scripts
//! send them). When [`Equipment`] changes:
//! - [`EquipStats`] is recalculated (attack, absorption, block, equip load, max HP bonus) and
//!   pushed into `Health::max` and the generic `DamageAbsorption`;
//! - the right-hand weapon is swapped via `EquipWeapon` (bare fists when the hand is empty);
//! - shield/armour models are attached to their bones ([`super::ItemModel`]).

use bevy::prelude::*;

use super::models::ModelBuilder;
use super::{ArmorSlot, ItemId, ItemKind};
use crate::character::action::RollWeight;
use crate::character::guard::Guard;
use crate::character::stats::{DamageAbsorption, Health};
use crate::humanoid::{HumanoidRig, HumanoidSpec};
use crate::tuning::*;
use crate::weapon::{EquipWeapon, EquippedWeapon, WeaponDef, light_sword};

pub struct InventoryPlugin;

impl Plugin for InventoryPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<EquipRequest>()
            .add_systems(Update, (handle_equip_requests, (apply_equip_stats, sync_weapon, sync_models)).chain());
    }
}

/// Everything a character carries, equipped or not. Entries are never removed (no dropping yet),
/// so indices are stable and [`Equipment`] can point at them.
#[derive(Component, Debug, Clone, Default)]
pub struct Inventory {
    pub items: Vec<ItemId>,
}

impl Inventory {
    pub fn add(&mut self, item: ItemId) -> usize {
        self.items.push(item);
        self.items.len() - 1
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EquipSlot {
    RightHand,
    LeftHand,
    Head,
    Chest,
    Hands,
    Legs,
    Ring1,
    Ring2,
    /// Quick item (the Healing Ale).
    Quick,
}

impl EquipSlot {
    pub const ALL: [EquipSlot; 9] = [
        EquipSlot::RightHand,
        EquipSlot::LeftHand,
        EquipSlot::Head,
        EquipSlot::Chest,
        EquipSlot::Hands,
        EquipSlot::Legs,
        EquipSlot::Ring1,
        EquipSlot::Ring2,
        EquipSlot::Quick,
    ];

    pub fn label(self) -> &'static str {
        match self {
            EquipSlot::RightHand => "Right hand",
            EquipSlot::LeftHand => "Left hand",
            EquipSlot::Head => "Head",
            EquipSlot::Chest => "Chest",
            EquipSlot::Hands => "Hands",
            EquipSlot::Legs => "Legs",
            EquipSlot::Ring1 | EquipSlot::Ring2 => "Ring",
            EquipSlot::Quick => "Quick item",
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// Which inventory entry sits in each slot.
#[derive(Component, Debug, Clone, Default, PartialEq)]
pub struct Equipment {
    slots: [Option<usize>; 9],
}

impl Equipment {
    pub fn get(&self, slot: EquipSlot) -> Option<usize> {
        self.slots[slot.index()]
    }

    pub fn item(&self, slot: EquipSlot, inventory: &Inventory) -> Option<ItemId> {
        self.get(slot).and_then(|i| inventory.items.get(i).copied())
    }

    pub fn slot_of(&self, index: usize) -> Option<EquipSlot> {
        EquipSlot::ALL.into_iter().find(|s| self.get(*s) == Some(index))
    }

    /// Puts inventory entry `index` into its slot (replacing what was there). Rings take a free
    /// ring slot, or the first one. Returns the slot, or `None` if the item can't be equipped.
    pub fn equip(&mut self, inventory: &Inventory, index: usize) -> Option<EquipSlot> {
        let item = *inventory.items.get(index)?;
        if let Some(slot) = self.slot_of(index) {
            return Some(slot);
        }
        let slot = match item.def().kind {
            ItemKind::Weapon => EquipSlot::RightHand,
            ItemKind::Shield => EquipSlot::LeftHand,
            ItemKind::Armor(ArmorSlot::Head) => EquipSlot::Head,
            ItemKind::Armor(ArmorSlot::Chest) => EquipSlot::Chest,
            ItemKind::Armor(ArmorSlot::Hands) => EquipSlot::Hands,
            ItemKind::Armor(ArmorSlot::Legs) => EquipSlot::Legs,
            ItemKind::Ring => [EquipSlot::Ring1, EquipSlot::Ring2].into_iter().find(|s| self.get(*s).is_none()).unwrap_or(EquipSlot::Ring1),
            ItemKind::Consumable => EquipSlot::Quick,
            ItemKind::Key => return None,
        };
        self.slots[slot.index()] = Some(index);
        Some(slot)
    }

    /// Takes entry `index` off. The quick item (Healing Ale) stays. Returns whether it changed.
    pub fn unequip(&mut self, index: usize) -> bool {
        match self.slot_of(index) {
            Some(EquipSlot::Quick) | None => false,
            Some(slot) => {
                self.slots[slot.index()] = None;
                true
            }
        }
    }

    pub fn can_unequip(&self, index: usize) -> bool {
        !matches!(self.slot_of(index), Some(EquipSlot::Quick) | None)
    }

    /// The right-hand weapon (bare fists when empty).
    pub fn weapon(&self, inventory: &Inventory) -> &'static WeaponDef {
        self.item(EquipSlot::RightHand, inventory)
            .and_then(|i| i.def().weapon)
            .unwrap_or(&light_sword::FISTS)
    }
}

/// Equip or unequip inventory entry `index` of `character`.
#[derive(Message, Debug, Clone, Copy)]
pub struct EquipRequest {
    pub character: Entity,
    pub index: usize,
    pub equip: bool,
}

/// Totals from what is equipped.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub struct EquipStats {
    /// Base damage of the right-hand weapon.
    pub attack: f32,
    /// Fraction of damage absorbed by armour.
    pub absorption: f32,
    /// Shield block fraction (stopped while guarding).
    pub block: f32,
    pub weight: f32,
    pub max_load: f32,
    pub max_hp_bonus: f32,
}

impl EquipStats {
    pub fn compute(inventory: &Inventory, equipment: &Equipment) -> Self {
        let mut s = EquipStats { attack: equipment.weapon(inventory).base_damage, max_load: PLAYER_MAX_LOAD, ..default() };
        let mut kept = 1.0;
        for slot in EquipSlot::ALL {
            let Some(def) = equipment.item(slot, inventory).map(ItemId::def) else { continue };
            s.weight += def.weight;
            kept *= 1.0 - def.absorption;
            s.block = s.block.max(def.block);
            s.max_hp_bonus += def.max_hp_bonus;
        }
        s.absorption = 1.0 - kept;
        s
    }

    pub fn load_ratio(&self) -> f32 {
        self.weight / self.max_load
    }

    /// The roll this load gives.
    pub fn roll_weight(&self) -> RollWeight {
        RollWeight::from_load(self.load_ratio())
    }

    /// Light / Medium / Heavy / Overloaded, with the roll it gives.
    pub fn load_class(&self) -> &'static str {
        match self.roll_weight() {
            RollWeight::Light => "Light (fast roll)",
            RollWeight::Medium => "Medium (normal roll)",
            RollWeight::Heavy => "Heavy (slow roll)",
            RollWeight::Overloaded => "Overloaded (fat roll)",
        }
    }
}

fn handle_equip_requests(mut requests: MessageReader<EquipRequest>, mut q: Query<(&Inventory, &mut Equipment)>) {
    for req in requests.read() {
        let Ok((inventory, mut equipment)) = q.get_mut(req.character) else { continue };
        let Some(item) = inventory.items.get(req.index) else { continue };
        let mut next = equipment.clone();
        if req.equip {
            next.equip(inventory, req.index);
        } else {
            next.unequip(req.index);
        }
        if next != *equipment {
            *equipment = next;
            info!("{} {}", if req.equip { "equipped" } else { "unequipped" }, item.def().name);
        }
    }
}

#[allow(clippy::type_complexity)]
fn apply_equip_stats(
    mut q: Query<
        (
            &Inventory,
            &Equipment,
            &mut EquipStats,
            Option<&mut Health>,
            Option<&mut DamageAbsorption>,
            Option<&mut Guard>,
            Option<&mut RollWeight>,
        ),
        Or<(Changed<Equipment>, Changed<Inventory>)>,
    >,
) {
    for (inventory, equipment, mut stats, health, absorption, guard, roll) in &mut q {
        let new = EquipStats::compute(inventory, equipment);
        if let Some(mut health) = health {
            let delta = new.max_hp_bonus - stats.max_hp_bonus;
            if delta != 0.0 {
                health.max += delta;
                if !health.is_dead() {
                    health.current = (health.current + delta.max(0.0)).min(health.max);
                }
            }
        }
        if let Some(mut a) = absorption {
            a.0 = new.absorption;
        }
        if let Some(mut r) = roll {
            r.set_if_neq(new.roll_weight());
        }
        if let Some(mut g) = guard {
            g.strength = new.block; // 0 without a shield: can't guard
        }
        stats.set_if_neq(new);
    }
}

fn sync_weapon(
    mut equip: MessageWriter<EquipWeapon>,
    q: Query<(Entity, &Inventory, &Equipment, Option<&EquippedWeapon>), Changed<Equipment>>,
) {
    for (entity, inventory, equipment, held) in &q {
        let want = equipment.weapon(inventory);
        if held.is_none_or(|h| !std::ptr::eq(h.def, want)) {
            equip.write(EquipWeapon { character: entity, def: want });
        }
    }
}

/// Spawned shield/armour models per slot.
#[derive(Component, Default)]
pub struct EquipVisuals {
    models: Vec<(EquipSlot, ItemId, Entity)>,
}

fn sync_models(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut q: Query<(&Inventory, &Equipment, &HumanoidRig, &HumanoidSpec, &mut EquipVisuals), Changed<Equipment>>,
) {
    for (inventory, equipment, rig, spec, mut visuals) in &mut q {
        // Drop models whose slot no longer holds the same item.
        visuals.models.retain(|(slot, item, entity)| {
            let keep = equipment.item(*slot, inventory) == Some(*item);
            if !keep {
                commands.entity(*entity).despawn();
            }
            keep
        });
        for slot in EquipSlot::ALL {
            let Some(item) = equipment.item(slot, inventory) else { continue };
            let Some(model) = item.def().model else { continue };
            if visuals.models.iter().any(|(s, _, _)| *s == slot) {
                continue;
            }
            let root = commands
                .spawn((Name::new(item.def().name), Transform::default(), Visibility::default(), ChildOf(rig.bone(model.bone))))
                .id();
            (model.build)(&mut ModelBuilder { commands: &mut commands, meshes: &mut meshes, materials: &mut materials, root }, spec);
            visuals.models.push((slot, item, root));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bag() -> Inventory {
        Inventory {
            items: vec![ItemId::Longsword, ItemId::HealingAle, ItemId::WoodenShield, ItemId::RustedAxe, ItemId::LeatherHood],
        }
    }

    #[test]
    fn items_go_to_their_slots_and_replace() {
        let inv = bag();
        let mut eq = Equipment::default();
        assert_eq!(eq.equip(&inv, 0), Some(EquipSlot::RightHand));
        assert_eq!(eq.equip(&inv, 1), Some(EquipSlot::Quick));
        assert_eq!(eq.equip(&inv, 2), Some(EquipSlot::LeftHand));
        assert_eq!(eq.equip(&inv, 4), Some(EquipSlot::Head));
        assert_eq!(eq.weapon(&inv).name, "Longsword");
        assert_eq!(eq.equip(&inv, 3), Some(EquipSlot::RightHand), "the axe replaces the sword");
        assert_eq!(eq.slot_of(0), None);
        assert_eq!(eq.weapon(&inv).name, "Rusted Axe");
    }

    #[test]
    fn unequipping_leaves_fists_and_keeps_the_ale() {
        let inv = bag();
        let mut eq = Equipment::default();
        eq.equip(&inv, 0);
        eq.equip(&inv, 1);
        assert!(eq.unequip(0));
        assert_eq!(eq.weapon(&inv).name, "Fists");
        assert!(!eq.unequip(1), "the quick item stays");
        assert!(!eq.can_unequip(1));
    }

    #[test]
    fn rings_fill_both_slots() {
        let inv = Inventory { items: vec![ItemId::SunseedRing, ItemId::SunseedRing, ItemId::SunseedRing] };
        let mut eq = Equipment::default();
        assert_eq!(eq.equip(&inv, 0), Some(EquipSlot::Ring1));
        assert_eq!(eq.equip(&inv, 1), Some(EquipSlot::Ring2));
        assert_eq!(eq.equip(&inv, 2), Some(EquipSlot::Ring1), "both full: replace the first");
        assert_eq!(EquipStats::compute(&inv, &eq).max_hp_bonus, 40.0);
    }

    #[test]
    fn stats_add_up() {
        let inv = bag();
        let mut eq = Equipment::default();
        for i in 0..5 {
            eq.equip(&inv, i);
        }
        eq.equip(&inv, 0);
        let s = EquipStats::compute(&inv, &eq);
        assert_eq!(s.attack, 20.0);
        assert_eq!(s.weight, 3.0 + 3.0 + 1.0);
        assert!((s.absorption - 0.06).abs() < 1e-6);
        assert_eq!(s.block, 0.6);
        assert_eq!(s.roll_weight(), RollWeight::Light);
    }
}
