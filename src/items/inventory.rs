//! The bag ([`Inventory`]), what is worn ([`Equipment`]) and what that adds up to ([`EquipStats`]).
//!
//! Equipping goes through [`EquipRequest`] messages (the inventory UI, debug keys and scripts
//! send them). When [`Equipment`] changes:
//! - [`EquipStats`] is recalculated (attack, absorption, block, equip load, max HP bonus) and
//!   pushed into `Health::max` and the generic `DamageAbsorption`;
//! - what the hands wield is sent as `EquipWeapon` (bare fists when the right hand is empty; a
//!   weapon in the left hand; which one is held in both hands, `Equipment::two_hand`);
//! - shield/armour models are attached to their bones ([`super::ItemModel`]); a shield is put
//!   away while something is held in both hands.
//!
//! Weapons go to the right hand by default; [`EquipRequest::slot`] can put one in the left hand
//! (dual wielding). [`TwoHandRequest`] toggles holding a hand's weapon in both hands.

use bevy::prelude::*;

use super::models::ModelBuilder;
use super::{ArmorSlot, ItemId, ItemKind};
use crate::character::action::RollWeight;
use crate::character::guard::Guard;
use crate::character::stats::{DamageAbsorption, Health};
use crate::humanoid::{HumanoidRig, HumanoidSpec};
use crate::tuning::*;
use super::ItemIcon;
use crate::ui::icons::NoticeIcon;
use crate::ui::notifications::Notify;
use crate::ui::strings;
use crate::weapon::{EquipWeapon, Hand, WeaponDef, Wielded, light_sword};

pub struct InventoryPlugin;

impl Plugin for InventoryPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<EquipRequest>()
            .add_message::<TwoHandRequest>()
            .add_systems(Update, (handle_equip_requests, handle_two_hand, (apply_equip_stats, sync_weapon, sync_models)).chain());
    }
}

/// Everything a character carries, equipped or not. Entries are never removed (no dropping yet),
/// so indices are stable and [`Equipment`] can point at them.
#[derive(Component, Debug, Clone, Default)]
pub struct Inventory {
    pub items: Vec<ItemId>,
    /// How many of each usable consumable (Poppy, Camomile) — they stack in one bag entry, which
    /// stays (greyed out) at 0 so equipment indices never shift.
    pub counts: std::collections::HashMap<ItemId, u32>,
}

impl Inventory {
    /// Starting bag from a list (stacking consumables).
    pub fn from_items(items: &[ItemId]) -> Self {
        let mut inv = Self::default();
        for &i in items {
            inv.add(i);
        }
        inv
    }

    /// Adds an item; usable consumables stack onto their existing entry. Returns its index.
    pub fn add(&mut self, item: ItemId) -> usize {
        if item.def().effect.is_some() {
            *self.counts.entry(item).or_default() += 1;
            if let Some(i) = self.items.iter().position(|i| *i == item) {
                return i;
            }
        }
        self.items.push(item);
        self.items.len() - 1
    }

    /// How many of a stacking consumable are left (1 for any other item in the bag).
    pub fn count(&self, item: ItemId) -> u32 {
        if item.def().effect.is_some() {
            self.counts.get(&item).copied().unwrap_or(0)
        } else {
            self.items.iter().filter(|i| **i == item).count() as u32
        }
    }

    /// Uses one of a stacking consumable; false if none are left.
    pub fn consume(&mut self, item: ItemId) -> bool {
        match self.counts.get_mut(&item) {
            Some(n) if *n > 0 => {
                *n -= 1;
                true
            }
            _ => false,
        }
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

/// Which inventory entry sits in each slot, and which hand's weapon is held in both hands.
#[derive(Component, Debug, Clone, Default, PartialEq)]
pub struct Equipment {
    slots: [Option<usize>; 9],
    pub two_hand: Option<Hand>,
}

impl Equipment {
    /// Raw slot → bag index table (for saving).
    pub fn slots(&self) -> [Option<usize>; 9] {
        self.slots
    }

    /// From a saved table; indices beyond the bag are dropped.
    pub fn from_slots(slots: [Option<usize>; 9], inventory: &Inventory) -> Self {
        Self { slots: slots.map(|s| s.filter(|&i| i < inventory.items.len())), two_hand: None }
    }

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
            // Only the Healing Ale lives in the quick slot; other consumables are used from the bag.
            ItemKind::Consumable if item == ItemId::HealingAle => EquipSlot::Quick,
            ItemKind::Consumable => return None,
            ItemKind::Key => return None,
        };
        self.slots[slot.index()] = Some(index);
        self.settle(inventory);
        Some(slot)
    }

    /// Puts entry `index` into a chosen hand: weapons into either hand (dual wielding), shields
    /// only into the left. Moves it if it's in the other hand already. Other slots: as [`Self::equip`].
    pub fn equip_to(&mut self, inventory: &Inventory, index: usize, slot: EquipSlot) -> Option<EquipSlot> {
        let item = *inventory.items.get(index)?;
        let fits = match (slot, item.def().kind) {
            (EquipSlot::RightHand, ItemKind::Weapon) => true,
            (EquipSlot::LeftHand, ItemKind::Weapon | ItemKind::Shield) => true,
            (EquipSlot::RightHand | EquipSlot::LeftHand, _) => false,
            _ => return self.equip(inventory, index),
        };
        if !fits {
            return None;
        }
        if let Some(old) = self.slot_of(index) {
            self.slots[old.index()] = None;
        }
        self.slots[slot.index()] = Some(index);
        self.settle(inventory);
        Some(slot)
    }

    /// The weapon in `hand`, if any (a shield isn't one; an empty right hand isn't either).
    pub fn hand_weapon(&self, inventory: &Inventory, hand: Hand) -> Option<&'static WeaponDef> {
        let slot = match hand {
            Hand::Right => EquipSlot::RightHand,
            Hand::Left => EquipSlot::LeftHand,
        };
        self.item(slot, inventory).and_then(|i| i.def().weapon)
    }

    /// Holds `hand`'s weapon in both hands, or (again) goes back to one hand. Err: nothing to hold.
    pub fn toggle_two_hand(&mut self, inventory: &Inventory, hand: Hand) -> Result<Option<Hand>, ()> {
        if self.two_hand == Some(hand) {
            self.two_hand = None;
            return Ok(None);
        }
        self.hand_weapon(inventory, hand).ok_or(())?;
        self.two_hand = Some(hand);
        Ok(self.two_hand)
    }

    /// Two-handing ends when that hand no longer holds a weapon.
    fn settle(&mut self, inventory: &Inventory) {
        if let Some(h) = self.two_hand
            && self.hand_weapon(inventory, h).is_none()
        {
            self.two_hand = None;
        }
    }

    /// Takes entry `index` off. The quick item (Healing Ale) stays. Returns whether it changed.
    pub fn unequip(&mut self, index: usize) -> bool {
        match self.slot_of(index) {
            Some(EquipSlot::Quick) | None => false,
            Some(slot) => {
                self.slots[slot.index()] = None;
                // Nothing left to hold in both hands.
                if let Some(h) = self.two_hand {
                    let held = match h {
                        Hand::Right => EquipSlot::RightHand,
                        Hand::Left => EquipSlot::LeftHand,
                    };
                    if held == slot {
                        self.two_hand = None;
                    }
                }
                true
            }
        }
    }

    pub fn can_unequip(&self, index: usize) -> bool {
        !matches!(self.slot_of(index), Some(EquipSlot::Quick) | None)
    }

    /// The main weapon: the one held in both hands, else the right-hand one (bare fists when empty).
    pub fn weapon(&self, inventory: &Inventory) -> &'static WeaponDef {
        let hand = self.two_hand.unwrap_or(Hand::Right);
        self.hand_weapon(inventory, hand).unwrap_or(&light_sword::FISTS)
    }
}

/// Hold `hand`'s weapon in both hands (again: back to one hand).
#[derive(Message, Debug, Clone, Copy)]
pub struct TwoHandRequest {
    pub character: Entity,
    pub hand: Hand,
}

/// Equip or unequip inventory entry `index` of `character`.
#[derive(Message, Debug, Clone, Copy)]
pub struct EquipRequest {
    pub character: Entity,
    pub index: usize,
    pub equip: bool,
    /// A specific hand (`RightHand` / `LeftHand`) for weapons and shields; `None` = its usual slot.
    pub slot: Option<EquipSlot>,
}

/// Totals from what is equipped.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub struct EquipStats {
    /// Base damage of the main weapon (× the two-handed bonus when held in both hands).
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
        let two_handed = if equipment.two_hand.is_some() { TWO_HAND_DAMAGE_MULT } else { 1.0 };
        let mut s = EquipStats { attack: equipment.weapon(inventory).base_damage * two_handed, max_load: PLAYER_MAX_LOAD, ..default() };
        let mut kept = 1.0;
        for slot in EquipSlot::ALL {
            let Some(def) = equipment.item(slot, inventory).map(ItemId::def) else { continue };
            s.weight += def.weight;
            kept *= 1.0 - def.absorption;
            s.block = s.block.max(def.block);
            s.max_hp_bonus += def.max_hp_bonus;
        }
        s.absorption = 1.0 - kept;
        // Two-handing: the shield is put away; you guard with the weapon instead.
        if equipment.two_hand.is_some() {
            s.block = equipment.weapon(inventory).guard_block;
        }
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
            match req.slot {
                Some(slot) => next.equip_to(inventory, req.index, slot),
                None => next.equip(inventory, req.index),
            };
        } else {
            next.unequip(req.index);
        }
        if next != *equipment {
            *equipment = next;
            info!("{} {}", if req.equip { "equipped" } else { "unequipped" }, item.def().name);
        }
    }
}

fn handle_two_hand(
    mut requests: MessageReader<TwoHandRequest>,
    mut notify: MessageWriter<Notify>,
    mut q: Query<(&Inventory, &mut Equipment, Has<crate::player::Player>)>,
) {
    for req in requests.read() {
        let Ok((inventory, mut equipment, is_player)) = q.get_mut(req.character) else { continue };
        let name = equipment.hand_weapon(inventory, req.hand).map(|w| w.name);
        let text = match equipment.toggle_two_hand(inventory, req.hand) {
            Ok(two) => strings::two_handing(name.unwrap_or("Weapon"), two.is_some()),
            Err(()) => strings::CANT_TWO_HAND.to_string(),
        };
        info!("{text}");
        if is_player {
            notify.write(Notify::new(NoticeIcon::Item(ItemIcon::Sword), text));
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
    q: Query<(Entity, &Inventory, &Equipment, Option<&Wielded>), Changed<Equipment>>,
) {
    for (entity, inventory, equipment, held) in &q {
        let right = equipment.hand_weapon(inventory, Hand::Right).unwrap_or(&light_sword::FISTS);
        let left = equipment.hand_weapon(inventory, Hand::Left);
        let same = held.is_some_and(|h| {
            std::ptr::eq(h.right, right) && h.left.map(|l| l as *const _) == left.map(|l| l as *const _) && h.two_hand == equipment.two_hand
        });
        if !same {
            equip.write(EquipWeapon { character: entity, right, left, two_hand: equipment.two_hand });
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
        // A shield is put away while something is held in both hands.
        let stowed = |slot: EquipSlot| slot == EquipSlot::LeftHand && equipment.two_hand.is_some();
        visuals.models.retain(|(slot, item, entity)| {
            let keep = equipment.item(*slot, inventory) == Some(*item) && !stowed(*slot);
            if !keep {
                commands.entity(*entity).despawn();
            }
            keep
        });
        for slot in EquipSlot::ALL {
            let Some(item) = equipment.item(slot, inventory) else { continue };
            let Some(model) = item.def().model else { continue };
            if stowed(slot) {
                continue;
            }
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

    #[test]
    fn dual_wield_and_two_hand() {
        let inv = Inventory::from_items(&[ItemId::Longsword, ItemId::RustedShortsword, ItemId::WoodenShield]);
        let mut eq = Equipment::default();
        eq.equip(&inv, 0);
        assert_eq!(eq.equip_to(&inv, 1, EquipSlot::LeftHand), Some(EquipSlot::LeftHand), "a sword in the left hand");
        assert_eq!(eq.hand_weapon(&inv, Hand::Left).map(|w| w.name), Some("Rusted Shortsword"));
        assert_eq!(eq.equip_to(&inv, 2, EquipSlot::RightHand), None, "no shield in the right hand");
        assert_eq!(eq.toggle_two_hand(&inv, Hand::Left), Ok(Some(Hand::Left)));
        assert_eq!(eq.weapon(&inv).name, "Rusted Shortsword", "the two-handed one is the main weapon");
        assert_eq!(eq.toggle_two_hand(&inv, Hand::Left), Ok(None), "again: one hand");
        eq.equip(&inv, 2); // the shield replaces the left sword
        assert_eq!(eq.toggle_two_hand(&inv, Hand::Left), Err(()), "a shield can't be two-handed");
        eq.toggle_two_hand(&inv, Hand::Right).unwrap();
        let stats = EquipStats::compute(&inv, &eq);
        assert!((stats.attack - 20.0 * TWO_HAND_DAMAGE_MULT).abs() < 1e-4);
        assert_eq!(stats.block, light_sword::LONGSWORD.guard_block, "guarding with the sword, shield put away");
        eq.unequip(0);
        assert_eq!(eq.two_hand, None, "nothing left to hold");
    }

    fn bag() -> Inventory {
        Inventory::from_items(&[ItemId::Longsword, ItemId::HealingAle, ItemId::WoodenShield, ItemId::RustedAxe, ItemId::LeatherHood])
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
    fn consumables_stack_and_run_out() {
        let mut inv = Inventory::from_items(&[ItemId::Longsword, ItemId::Poppy, ItemId::Camomile]);
        assert_eq!(inv.add(ItemId::Poppy), 1, "stacks on its entry");
        assert_eq!(inv.items.len(), 3);
        assert_eq!(inv.count(ItemId::Poppy), 2);
        assert!(inv.consume(ItemId::Poppy) && inv.consume(ItemId::Poppy));
        assert!(!inv.consume(ItemId::Poppy), "none left");
        assert_eq!(inv.items.len(), 3, "the entry stays (indices don't shift)");
        let mut eq = Equipment::default();
        assert_eq!(eq.equip(&inv, 1), None, "a Poppy isn't equipped, it's eaten");
    }

    #[test]
    fn rings_fill_both_slots() {
        let inv = Inventory { items: vec![ItemId::SunseedRing, ItemId::SunseedRing, ItemId::SunseedRing], ..default() };
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
