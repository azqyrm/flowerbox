//! Player-facing wording, in one place so it is easy to change (and later translate).

/// Big band on the death screen.
pub const DEATH: &str = "COMPOSTED";
/// Banner the first time a Hearth (the sunflower) is rested at and stands up.
pub const HEARTH_REKINDLED: &str = "SUN VIGORATED";
/// Interaction prompt at a Hearth that is still drooping.
pub const PROMPT_REKINDLE: &str = "[E] Tend the Sunflower";
/// Interaction prompt at a Hearth that already stands.
pub const PROMPT_REST: &str = "[E] Rest by the Sunflower";
/// Interaction prompt next to an item on the floor.
pub const PROMPT_PICK_UP: &str = "[E] Pick up";
/// Toast after picking something up ("Picked up: Rusted Shortsword").
pub const PICKED_UP: &str = "Picked up";
/// Inventory screen title and key hints.
pub const INVENTORY_TITLE: &str = "Inventory";
pub const INVENTORY_HINTS: &str =
    "[Enter] Equip / Unequip    [Backspace] Unequip    [Q] [E] Filter    [Arrows / WASD] Move    [Tab] Close";
