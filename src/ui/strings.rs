//! Player-facing wording, in one place so it is easy to change (and later translate).

/// Big band on the death screen.
pub const DEATH: &str = "COMPOSTED";
/// Banner the first time a Hearth (the sunflower) is rested at and stands up.
pub const HEARTH_REKINDLED: &str = "SUN VIGORATED";
/// Interaction prompt at a Hearth that is still drooping.
pub const PROMPT_REKINDLE: &str = "[E] Tend the Sunflower";
/// Interaction prompt at a Hearth that already stands.
pub const PROMPT_REST: &str = "[E] Rest by the Sunflower";
/// Online panel.
pub const ONLINE_TITLE: &str = "Online";
pub const ONLINE_EXPLAINER: &str = "Play together over the internet, peer to peer. One player hosts and shares the code; \
the other copies it and presses Join. No server or account needed.";
/// Interaction prompt next to an item on the floor.
pub const PROMPT_PICK_UP: &str = "[E] Pick up";
/// Toast after picking something up ("Picked up: Rusted Shortsword").
pub const PICKED_UP: &str = "Picked up";
/// Inventory screen title and key hints.
pub const INVENTORY_TITLE: &str = "Inventory";
pub const INVENTORY_HINTS: &str =
    "[Enter / A] Equip / Unequip    [Backspace / X] Unequip    [Q] [E] / L1 R1 Filter    [Arrows / D-pad] Move    [Tab / B] Close";
