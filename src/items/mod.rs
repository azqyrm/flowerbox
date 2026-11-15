//! Items: a static registry ([`ItemId`] → [`ItemDef`]), the player's [`inventory`] and equipment,
//! equipment visuals ([`models`]) and world [`pickup`]s.
//!
//! # Adding an item
//! 1. Add a variant to [`ItemId`] (and to [`ItemId::ALL`]).
//! 2. Add a `static` [`ItemDef`] below and return it from [`ItemId::def`].
//!    - Weapons point at a `&'static WeaponDef` (`crate::weapon`), which carries the moveset class,
//!      damage, hitbox and model.
//!    - Shields and armour may have an [`ItemModel`]: a builder fn that attaches primitive parts to
//!      a bone (see `models.rs`).
//!    - Give it an [`ItemIcon`]; `ui/icons.rs` draws icons from primitives.
//!
//! The registry lives in code for now; a RON file (`assets/items.ron`) can feed it later.

pub mod inventory;
pub mod models;
pub mod pickup;

use bevy::prelude::*;

use crate::humanoid::{Bone, HumanoidSpec};
use crate::weapon::{WeaponDef, light_sword};

pub struct ItemsPlugin;

impl Plugin for ItemsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((inventory::InventoryPlugin, pickup::PickupPlugin));
    }
}

/// Every item in the game.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ItemId {
    Longsword,
    RustedShortsword,
    RustedAxe,
    WoodenShield,
    StuddedBuckler,
    LeatherHood,
    SunseedRing,
    HealingAle,
    Poppy,
    Camomile,
    HopCone,
    ThornbriarRing,
}

impl ItemId {
    #[allow(dead_code)] // tests
    pub const ALL: [ItemId; 12] = [
        ItemId::Longsword,
        ItemId::RustedShortsword,
        ItemId::RustedAxe,
        ItemId::WoodenShield,
        ItemId::StuddedBuckler,
        ItemId::LeatherHood,
        ItemId::SunseedRing,
        ItemId::HealingAle,
        ItemId::Poppy,
        ItemId::Camomile,
        ItemId::HopCone,
        ItemId::ThornbriarRing,
    ];

    pub fn def(self) -> &'static ItemDef {
        match self {
            ItemId::Longsword => &LONGSWORD,
            ItemId::RustedShortsword => &RUSTED_SHORTSWORD,
            ItemId::RustedAxe => &RUSTED_AXE,
            ItemId::WoodenShield => &WOODEN_SHIELD,
            ItemId::StuddedBuckler => &STUDDED_BUCKLER,
            ItemId::LeatherHood => &LEATHER_HOOD,
            ItemId::SunseedRing => &SUNSEED_RING,
            ItemId::HealingAle => &HEALING_ALE,
            ItemId::Poppy => &POPPY,
            ItemId::Camomile => &CAMOMILE,
            ItemId::HopCone => &HOP_CONE,
            ItemId::ThornbriarRing => &THORNBRIAR_RING,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ArmorSlot {
    Head,
    #[allow(dead_code)] // no chest/hand/leg armour exists yet
    Chest,
    #[allow(dead_code)]
    Hands,
    #[allow(dead_code)]
    Legs,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ItemKind {
    Weapon,
    Shield,
    Armor(ArmorSlot),
    Ring,
    Consumable,
    #[allow(dead_code)] // no keys yet
    Key,
}

impl ItemKind {
    pub fn label(self) -> &'static str {
        match self {
            ItemKind::Weapon => "Weapon",
            ItemKind::Shield => "Shield",
            ItemKind::Armor(ArmorSlot::Head) => "Head armour",
            ItemKind::Armor(ArmorSlot::Chest) => "Chest armour",
            ItemKind::Armor(ArmorSlot::Hands) => "Hand armour",
            ItemKind::Armor(ArmorSlot::Legs) => "Leg armour",
            ItemKind::Ring => "Ring",
            ItemKind::Consumable => "Consumable",
            ItemKind::Key => "Key item",
        }
    }
}

/// Which picture `ui/icons.rs` draws for an item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemIcon {
    Sword,
    ShortSword,
    Axe,
    Fist,
    RoundShield,
    Buckler,
    Hood,
    Ring,
    ThornRing,
    Tankard,
    HopCone,
    Poppy,
    Camomile,
}

/// Primitive model attached to a bone while the item is equipped (shields, armour).
#[derive(Clone, Copy)]
pub struct ItemModel {
    pub bone: Bone,
    pub build: fn(&mut models::ModelBuilder, &HumanoidSpec),
}

/// Static description of an item.
pub struct ItemDef {
    pub name: &'static str,
    pub kind: ItemKind,
    pub description: &'static str,
    pub icon: ItemIcon,
    /// Equip load.
    pub weight: f32,
    /// Fraction of incoming damage absorbed while worn (armour).
    pub absorption: f32,
    /// Fraction of damage stopped when blocking (shields; blocking comes in Phase 8).
    pub block: f32,
    /// Extra max HP while worn (rings).
    pub max_hp_bonus: f32,
    pub weapon: Option<&'static WeaponDef>,
    pub model: Option<ItemModel>,
    /// What using (eating) it does; such items stack and are used from the inventory.
    pub effect: Option<ItemEffect>,
}

/// Effect of a usable consumable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemEffect {
    /// Opt in to fighting other players.
    PvpOn,
    /// Opt out again.
    PvpOff,
    /// One more Healing Ale charge for good (and one poured at once).
    AleUp,
}

impl ItemDef {
    const fn base(name: &'static str, kind: ItemKind, icon: ItemIcon, description: &'static str) -> Self {
        Self {
            name,
            kind,
            description,
            icon,
            weight: 0.0,
            absorption: 0.0,
            block: 0.0,
            max_hp_bonus: 0.0,
            weapon: None,
            model: None,
            effect: None,
        }
    }
}

static LONGSWORD: ItemDef = ItemDef {
    weight: 3.0,
    weapon: Some(&light_sword::LONGSWORD),
    ..ItemDef::base(
        "Longsword",
        ItemKind::Weapon,
        ItemIcon::Sword,
        "A plain, well-balanced straight sword. Reliable in any hand, and long enough to keep trouble at arm's length.",
    )
};

static RUSTED_SHORTSWORD: ItemDef = ItemDef {
    weight: 2.0,
    weapon: Some(&light_sword::SHORTSWORD),
    ..ItemDef::base(
        "Rusted Shortsword",
        ItemKind::Weapon,
        ItemIcon::ShortSword,
        "A short blade gone orange with rust. Light, but it neither reaches nor bites like a proper sword.",
    )
};

static RUSTED_AXE: ItemDef = ItemDef {
    weight: 4.0,
    weapon: Some(&light_sword::RUSTED_AXE),
    ..ItemDef::base(
        "Rusted Axe",
        ItemKind::Weapon,
        ItemIcon::Axe,
        "A woodcutter's hand axe. Only the head does any cutting, but it does it heavily.",
    )
};

static WOODEN_SHIELD: ItemDef = ItemDef {
    weight: 3.0,
    block: 0.6,
    model: Some(ItemModel { bone: Bone::ForearmL, build: models::wooden_shield }),
    ..ItemDef::base(
        "Wooden Shield",
        ItemKind::Shield,
        ItemIcon::RoundShield,
        "Oak planks bound with an iron rim. Splinters long before it breaks.",
    )
};

static STUDDED_BUCKLER: ItemDef = ItemDef {
    weight: 1.5,
    block: 0.4,
    model: Some(ItemModel { bone: Bone::ForearmL, build: models::studded_buckler }),
    ..ItemDef::base(
        "Studded Buckler",
        ItemKind::Shield,
        ItemIcon::Buckler,
        "A small iron buckler studded around its boss. Light on the arm; stops less than a full shield.",
    )
};

static LEATHER_HOOD: ItemDef = ItemDef {
    weight: 1.0,
    absorption: 0.06,
    model: Some(ItemModel { bone: Bone::Head, build: models::leather_hood }),
    ..ItemDef::base(
        "Leather Hood",
        ItemKind::Armor(ArmorSlot::Head),
        ItemIcon::Hood,
        "A worn hood of oiled leather. Keeps off the drip from the ceiling, and a little of the blows.",
    )
};

static SUNSEED_RING: ItemDef = ItemDef {
    max_hp_bonus: 20.0,
    ..ItemDef::base(
        "Sunseed Ring",
        ItemKind::Ring,
        ItemIcon::Ring,
        "A brass band set with a single sunflower seed. Whoever wears it feels a little harder to compost.",
    )
};

static HEALING_ALE: ItemDef = ItemDef::base(
    "Healing Ale",
    ItemKind::Consumable,
    ItemIcon::Tankard,
    "Thick, dark ale that knits wounds. Refilled when resting by the sunflower. Always ready in the quick-item slot.",
);

static POPPY: ItemDef = ItemDef {
    effect: Some(ItemEffect::PvpOn),
    ..ItemDef::base(
        "Poppy",
        ItemKind::Consumable,
        ItemIcon::Poppy,
        "A blood-red poppy. Eating it stirs up a reckless anger: other players who ate one can hurt you, and you them. Fades when you are composted.",
    )
};

static CAMOMILE: ItemDef = ItemDef {
    effect: Some(ItemEffect::PvpOff),
    ..ItemDef::base(
        "Camomile",
        ItemKind::Consumable,
        ItemIcon::Camomile,
        "A little white camomile flower. Chewing it calms the Poppy's anger: other players can't hurt you any more.",
    )
};

static HOP_CONE: ItemDef = ItemDef {
    effect: Some(ItemEffect::AleUp),
    ..ItemDef::base(
        "Hop Cone",
        ItemKind::Consumable,
        ItemIcon::HopCone,
        "A papery green hop cone, still fragrant. Crumble it into your tankard and the ale brews stronger: one more Healing Ale for good.",
    )
};

static THORNBRIAR_RING: ItemDef = ItemDef {
    weight: 0.5,
    absorption: 0.1,
    ..ItemDef::base(
        "Thornbriar Ring",
        ItemKind::Ring,
        ItemIcon::ThornRing,
        "A ring of dried briar, its thorns turned outward. Blows seem to snag on it before they reach you.",
    )
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_is_consistent() {
        for id in ItemId::ALL {
            let def = id.def();
            assert_eq!(def.weapon.is_some(), def.kind == ItemKind::Weapon, "{id:?}");
            assert!(def.weight >= 0.0 && (0.0..1.0).contains(&def.absorption), "{id:?}");
            assert!(!def.name.is_empty() && !def.description.is_empty(), "{id:?}");
        }
    }
}
