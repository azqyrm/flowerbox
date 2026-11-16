//! Player-facing wording, in one place so it is easy to change (and later translate).

/// Big band on the death screen.
pub const DEATH: &str = "COMPOSTED";
/// Banner the first time a Hearth (the sunflower) is rested at and stands up.
pub const HEARTH_REKINDLED: &str = "SUN VIGORATED";
/// Interaction prompt at a Hearth that is still drooping.
pub const PROMPT_REKINDLE: &str = "[E] Tend the Sunflower";
/// While kneeling at the sunflower.
pub const PROMPT_STAND_UP: &str = "[E] Stand up";
/// Interaction prompt at a Hearth that already stands.
pub const PROMPT_REST: &str = "[E] Rest by the Sunflower";
/// Online panel.
pub const ONLINE_TITLE: &str = "Online";
pub const ONLINE_EXPLAINER: &str = "Play together over the internet, peer to peer. One player hosts and shares the code; \
the other copies it and presses Join. No server or account needed.";
/// Notifications about other players ("Guest 1 picked up Rusted Axe").
pub fn joined(who: &str) -> String {
    format!("{who} joined")
}
pub fn left(who: &str) -> String {
    format!("{who} left")
}
pub fn revived_sun(who: &str) -> String {
    format!("{who} vigorated the Sun")
}
pub fn picked_up_by(who: &str, item: &str) -> String {
    format!("{who} picked up {item}")
}
pub fn composted(who: &str) -> String {
    format!("{who} was composted")
}
/// PvP and consumables.
pub const PVP_ON: &str = "PvP";
pub const PVP_OFF_BY_DEATH: &str = "Composted: the Poppy's anger is gone (PvP off)";
pub fn ate(who: &str, item: &str, pvp_on: bool) -> String {
    format!("{who} ate a {item}: PvP {}", if pvp_on { "on" } else { "off" })
}
pub fn none_left(item: &str) -> String {
    format!("No {item} left")
}
/// Shared rest at the sunflower.
pub fn rest_waiting(who: &str, resting: u32, total: u32) -> String {
    format!("{who} waiting for other players at rest site {resting}/{total}")
}
pub const REST_COMPLETE: &str = "Everyone rests at the sunflower: the world stirs";
/// The menu (Esc).
pub const MENU_TITLE: &str = "Menu";
pub const MENU_HINTS: &str = "[Up/Down] Select   [Left/Right] Change   [Enter] Choose   [Esc] Back";
pub const SAVE_DELETED: &str = "Save deleted: the next start is a new game";
/// Interaction prompt next to an item on the floor.
pub const PROMPT_PICK_UP: &str = "[E] Pick up";
pub const DOOR_SEALED: &str = "Sealed. Runes crawl over the stone: an old rite holds it shut";
pub const PROMPT_RING: &str = "[E] Ring the bell";
pub const PROMPT_READ: &str = "[E] Read";
pub const RITE_WRONG: &str = "The bells clash in discord. The rite begins anew";
pub const RITE_SOLVED: &str = "THE SEAL IS BROKEN";
pub fn rang(who: &str, bell: &str) -> String {
    format!("{who} rang {bell}")
}
pub fn rite_progress(done: usize, total: usize) -> String {
    format!("The bell answers... ({done} of {total})")
}
pub const PROMPT_FOG: &str = "[E] Pass through the mist";
pub const BOSS_DEFEATED: &str = "HALLOWED FOE UNMADE";
pub const PROMPT_OPEN_DOOR: &str = "[E] Open";
pub const DOOR_OTHER_SIDE: &str = "This door only opens from the other side";
pub fn door_tried(who: &str) -> String {
    format!("{who} tried a door: it only opens from the other side")
}
pub fn buff(name: &str, mult: f32, secs: f32) -> String {
    format!("{name}: +{:.0}% damage for {secs:.0} s", (mult - 1.0) * 100.0)
}
pub const CANT_TWO_HAND: &str = "Only weapons can be held in both hands";
pub fn two_handing(item: &str, on: bool) -> String {
    if on { format!("{item}: held in both hands") } else { format!("{item}: back to one hand") }
}
pub fn opened_door(who: &str) -> String {
    format!("{who} opened a door")
}
pub fn door_locked(key: &str) -> String {
    format!("Locked: it needs the {key}")
}
pub fn door_unlocked(key: &str) -> String {
    format!("Unlocked with the {key}")
}
pub fn ale_up(max: u32) -> String {
    format!("The ale brews stronger: {max} Healing Ale")
}
/// Toast after picking something up ("Picked up: Rusted Shortsword").
pub const PICKED_UP: &str = "Picked up";
/// Inventory screen title and key hints.
pub fn threw_out(item: &str) -> String {
    format!("Threw out: {item}")
}
pub fn other_threw_out(who: &str, item: &str) -> String {
    format!("{who} threw out {item}")
}
pub const NOTHING_TO_EQUIP: &str = "Nothing in the bag fits this slot";
pub const INVENTORY_TITLE: &str = "Inventory";
pub const INVENTORY_HINTS: &str =
    "[Enter] Options / choose   [Left / Right] Slots / bag   [X] Unequip   [L] To left hand   [Q] [E] Filter   [Esc] Back   [Tab] Close";
/// Chat: the typing line, and a message in the feed ("gordelius: I said something!").
pub const CHAT_PROMPT: &str = "Say:";
pub fn said(who: &str, text: &str) -> String {
    format!("{who}: {text}")
}
/// The name field in the settings.
pub const NAME_LABEL: &str = "Name";
pub const NAME_NONE: &str = "(none: Host / GuestN)";
pub const NAME_EDITING_HINT: &str = "Type your name, [Enter] keep, [Esc] cancel";
