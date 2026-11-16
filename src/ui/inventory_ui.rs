//! The inventory screen (Tab / I). The world keeps running (no pause in a Soulslike), but the
//! player gets no gameplay input while it is open ([`GameplayBlocked`]) and the cursor is free.
//!
//! Layout: equipment slots (left) · filter tabs + item grid (middle) · details of the selected
//! item and the status summary with a preview of what the change would do (right).
//!
//! Two ways to equip, as in Souls games:
//! - **Slot first** ([`Focus::Slots`]): walk the equipment column (W/S, arrows, D-pad), Enter on
//!   a slot → the grid shows only what fits it ([`InventoryMenu::picking`]); Enter equips the
//!   item into exactly that slot and returns to the column. Nothing fits → a short notification.
//!   Backspace on a slot empties it.
//! - **Item first** ([`Focus::Grid`]): Enter on an item opens its **options** popup
//!   ([`ItemOption`]): Drink / Eat, Equip (right / left hand for weapons), Unequip, Throw out.
//!   Throwing out puts the item on the floor in front of you, where anyone can take it
//!   (`items::pickup::ThrowOutRequest`).
//!
//! Left at the grid's first column moves to the slots, Right from a slot to the grid. Q/E filter,
//! Esc / Backspace go back one step (popup → picking → slots), Tab/I close. Gamepad: D-pad,
//! L1/R1 filter, A select, B back, X unequip. Mouse: moving over something selects it (movement,
//! not mere hovering: a rebuild spawns fresh nodes under a resting mouse, and those mustn't snap
//! the keyboard selection back to it), click a slot to pick for it,
//! double-click or right-click an item for its options.
//!
//! The content is rebuilt whenever something it shows changes (it's small, so this is cheap and
//! keeps the code simple).

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use leafwing_input_manager::prelude::ActionState;

use super::hud::{FRAME, GOLD_TEXT};
use super::icons::{NoticeIcon, spawn_icon};
use super::notifications::Notify;
use super::{DISPLAY_FONT, strings};
use crate::character::stats::Health;
use crate::healing::HealingAle;
use crate::hearth::DeathSequence;
use crate::input::{Action, InputSystems, MenuInput};
use crate::items::inventory::{EquipRequest, EquipSlot, EquipStats, Equipment, Inventory};
use crate::items::pickup::{ThrowOutRequest, throwable};
use crate::items::{ItemIcon, ItemId, ItemKind};
use crate::player::Player;
use crate::pvp::UseItemRequest;

pub struct InventoryUiPlugin;

impl Plugin for InventoryUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InventoryMenu>()
            .add_systems(Startup, spawn_menu_root)
            .add_systems(Update, (toggle_menu.in_set(InputSystems::Menus), menu_keys, rebuild_menu).chain());
    }
}

const COLS: usize = 5;
const CELL: f32 = 74.0;
const BACKDROP: Color = Color::srgba(0.0, 0.0, 0.0, 0.55);
const PANEL_BG: Color = Color::srgb(0.045, 0.04, 0.035);
const CELL_BG: Color = Color::srgba(0.0, 0.0, 0.0, 0.45);
const SELECTED_BG: Color = Color::srgba(0.35, 0.28, 0.14, 0.55);
const TEXT: Color = Color::srgb(0.9, 0.88, 0.82);
const DIM: Color = Color::srgb(0.55, 0.52, 0.47);
const BETTER: Color = Color::srgb(0.45, 0.8, 0.45);
const WORSE: Color = Color::srgb(0.85, 0.4, 0.35);

/// Where the keyboard / pad selection is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Focus {
    /// The equipment column (`InventoryMenu::slot`).
    Slots,
    /// The item grid (`InventoryMenu::cursor`).
    #[default]
    Grid,
}

/// Inventory screen state.
#[derive(Resource, Default, Debug)]
pub struct InventoryMenu {
    pub open: bool,
    pub filter: Filter,
    /// Position of the selection in the shown list.
    pub cursor: usize,
    pub focus: Focus,
    /// Selected row of the equipment column (index into `EquipSlot::ALL`).
    pub slot: usize,
    /// Choosing an item for this slot: the grid shows only what fits it.
    pub picking: Option<EquipSlot>,
    /// The options popup of a bag entry, and its highlighted option.
    pub popup: Option<(usize, usize)>,
    /// Rebuild the content next frame.
    pub dirty: bool,
}

impl InventoryMenu {
    /// One step back: popup → picking → (slots stay). True if there was something to go back from.
    fn back(&mut self) -> bool {
        self.dirty = true;
        if self.popup.take().is_some() {
            return true;
        }
        if self.picking.take().is_some() {
            self.focus = Focus::Slots;
            return true;
        }
        self.dirty = false;
        false
    }

    /// Something open inside the screen that Esc should close first.
    pub fn nested(&self) -> bool {
        self.popup.is_some() || self.picking.is_some()
    }

    fn select_slot(&mut self, i: usize) {
        if (self.focus, self.slot) != (Focus::Slots, i) {
            (self.focus, self.slot, self.dirty) = (Focus::Slots, i, true);
        }
    }

    fn select_cell(&mut self, pos: usize) {
        if (self.focus, self.cursor) != (Focus::Grid, pos) {
            (self.focus, self.cursor, self.dirty) = (Focus::Grid, pos, true);
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Filter {
    #[default]
    All,
    Weapons,
    Shields,
    Armor,
    Rings,
    Consumables,
}

impl Filter {
    const ALL: [Filter; 6] = [Filter::All, Filter::Weapons, Filter::Shields, Filter::Armor, Filter::Rings, Filter::Consumables];

    fn label(self) -> &'static str {
        match self {
            Filter::All => "All",
            Filter::Weapons => "Weapons",
            Filter::Shields => "Shields",
            Filter::Armor => "Armour",
            Filter::Rings => "Rings",
            Filter::Consumables => "Items",
        }
    }

    fn matches(self, kind: ItemKind) -> bool {
        match self {
            Filter::All => true,
            Filter::Weapons => kind == ItemKind::Weapon,
            Filter::Shields => kind == ItemKind::Shield,
            Filter::Armor => matches!(kind, ItemKind::Armor(_)),
            Filter::Rings => kind == ItemKind::Ring,
            Filter::Consumables => matches!(kind, ItemKind::Consumable | ItemKind::Key),
        }
    }

    fn step(self, by: i32) -> Filter {
        let i = Filter::ALL.iter().position(|f| *f == self).unwrap_or(0) as i32;
        Filter::ALL[(i + by).rem_euclid(Filter::ALL.len() as i32) as usize]
    }
}

/// Inventory indices shown under `filter`, in bag order.
pub fn visible(inventory: &Inventory, filter: Filter) -> Vec<usize> {
    (0..inventory.items.len()).filter(|&i| filter.matches(inventory.items[i].def().kind)).collect()
}

/// What the grid shows: the candidates for the slot being picked for, else the filtered bag.
pub fn shown(menu: &InventoryMenu, inventory: &Inventory) -> Vec<usize> {
    match menu.picking {
        Some(slot) => slot.candidates(inventory),
        None => visible(inventory, menu.filter),
    }
}

/// Consumables are used, not equipped: the Healing Ale (drink) and the stacking ones (eat).
fn usable(item: ItemId) -> bool {
    item == ItemId::HealingAle || item.def().effect.is_some()
}

/// One entry of an item's options popup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemOption {
    /// Drink / eat it.
    Use,
    /// Into this slot (a weapon: either hand; anything else: its own slot).
    EquipTo(EquipSlot),
    /// Into its usual slot (rings: a free ring slot).
    Equip,
    Unequip,
    ThrowOut,
}

impl ItemOption {
    fn label(self, item: ItemId) -> &'static str {
        match self {
            ItemOption::Use if item == ItemId::HealingAle => "Drink",
            ItemOption::Use => "Eat",
            ItemOption::EquipTo(EquipSlot::RightHand) => "Equip in right hand",
            ItemOption::EquipTo(EquipSlot::LeftHand) => "Equip in left hand",
            ItemOption::EquipTo(_) | ItemOption::Equip => "Equip",
            ItemOption::Unequip => "Unequip",
            ItemOption::ThrowOut => "Throw out",
        }
    }
}

/// Everything you can do with bag entry `index`, in menu order.
pub fn item_options(inventory: &Inventory, equipment: &Equipment, index: usize) -> Vec<ItemOption> {
    let Some(&item) = inventory.items.get(index) else { return vec![] };
    let mut out = vec![];
    if usable(item) {
        out.push(ItemOption::Use);
    }
    let slot = equipment.slot_of(index);
    match item.def().kind {
        ItemKind::Weapon => {
            for hand in [EquipSlot::RightHand, EquipSlot::LeftHand] {
                if slot != Some(hand) {
                    out.push(ItemOption::EquipTo(hand));
                }
            }
        }
        ItemKind::Shield | ItemKind::Armor(_) | ItemKind::Ring if slot.is_none() => out.push(ItemOption::Equip),
        _ => {}
    }
    if equipment.can_unequip(index) {
        out.push(ItemOption::Unequip);
    }
    if throwable(item) && inventory.count(item) > 0 {
        out.push(ItemOption::ThrowOut);
    }
    out
}

/// Everything an inventory action may send.
#[derive(SystemParam)]
struct ItemActions<'w> {
    equip: MessageWriter<'w, EquipRequest>,
    use_item: MessageWriter<'w, UseItemRequest>,
    throw: MessageWriter<'w, ThrowOutRequest>,
    notify: MessageWriter<'w, Notify>,
}

impl ItemActions<'_> {
    fn apply(&mut self, character: Entity, inventory: &Inventory, index: usize, option: ItemOption) {
        let Some(&item) = inventory.items.get(index) else { return };
        match option {
            ItemOption::Use => {
                self.use_item.write(UseItemRequest { item });
            }
            ItemOption::EquipTo(slot) => {
                self.equip.write(EquipRequest { character, index, equip: true, slot: Some(slot) });
            }
            ItemOption::Equip => {
                self.equip.write(EquipRequest { character, index, equip: true, slot: None });
            }
            ItemOption::Unequip => {
                self.equip.write(EquipRequest { character, index, equip: false, slot: None });
            }
            ItemOption::ThrowOut => {
                self.throw.write(ThrowOutRequest { index });
            }
        }
    }

    /// Enter on a slot: pick an item for it, or say that nothing fits.
    fn start_picking(&mut self, menu: &mut InventoryMenu, inventory: &Inventory, equipment: &Equipment) {
        let slot = EquipSlot::ALL[menu.slot.min(EquipSlot::ALL.len() - 1)];
        let candidates = slot.candidates(inventory);
        if candidates.is_empty() {
            self.notify.write(Notify::new(NoticeIcon::Item(ItemIcon::Sword), strings::NOTHING_TO_EQUIP));
            return;
        }
        // Start on what's in the slot now, if anything.
        let current = equipment.get(slot).and_then(|i| candidates.iter().position(|c| *c == i));
        (menu.picking, menu.focus, menu.cursor, menu.dirty) = (Some(slot), Focus::Grid, current.unwrap_or(0), true);
    }

    /// Enter on a candidate while picking: into the chosen slot, back to the column.
    fn pick(&mut self, menu: &mut InventoryMenu, character: Entity, equipment: &Equipment, index: usize) {
        let Some(slot) = menu.picking.take() else { return };
        if equipment.get(slot) != Some(index) {
            self.equip.write(EquipRequest { character, index, equip: true, slot: Some(slot) });
        }
        (menu.focus, menu.dirty) = (Focus::Slots, true);
    }
}

#[derive(Component)]
struct MenuRoot;

fn spawn_menu_root(mut commands: Commands) {
    commands.spawn((
        Name::new("Inventory Screen"),
        MenuRoot,
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(BACKDROP),
        GlobalZIndex(5),
        Visibility::Hidden,
    ));
}

#[allow(clippy::too_many_arguments)]
fn toggle_menu(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    mut menu: ResMut<InventoryMenu>,
    mut menus: ResMut<MenuInput>,
    real: Res<Time<Real>>,
    mut last_toggle: Local<f32>,
    player: Query<(&ActionState<Action>, Has<DeathSequence>), With<Player>>,
    mut root: Query<&mut Visibility, With<MenuRoot>>,
) {
    let Ok((actions, dying)) = player.single() else { return };
    // Esc / B closes the screen only when nothing inside it is open (`menu_keys` steps back).
    let back = (keys.just_pressed(KeyCode::Escape) || pads.iter().any(|g| g.just_pressed(GamepadButton::East))) && !menu.nested();
    // Not over another menu (the pause menu or the online panel).
    let others = menus.pause || menus.online;
    // A second toggle within 0.25 s is ignored: a key delivered twice (or a press bouncing through
    // the desktop's shortcut handling) mustn't open and close the screen in one go.
    let now = real.elapsed_secs();
    let settled = now - *last_toggle > 0.25;
    let toggle = settled && ((actions.just_pressed(&Action::Inventory) && (menu.open || !others)) || (menu.open && back));
    if toggle {
        *last_toggle = now;
    }
    let want = !dying && (menu.open != toggle);
    menus.inventory = want;
    if want != menu.open {
        menu.open = want;
        menu.dirty = true;
        menu.popup = None;
        menu.picking = None;
        for mut v in &mut root {
            *v = if want { Visibility::Inherited } else { Visibility::Hidden };
        }
    }
}

fn menu_keys(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    mut menu: ResMut<InventoryMenu>,
    mut actions: ItemActions,
    player: Query<(Entity, &Inventory, &Equipment), With<Player>>,
) {
    if !menu.open {
        return;
    }
    let Ok((entity, inventory, equipment)) = player.single() else { return };
    let pressed = |ks: &[KeyCode]| ks.iter().any(|k| keys.just_pressed(*k));
    let pad = |b: GamepadButton| pads.iter().any(|g| g.just_pressed(b));
    use GamepadButton as G;
    use KeyCode::*;
    let up = pressed(&[ArrowUp, KeyW]) || pad(G::DPadUp);
    let down = pressed(&[ArrowDown, KeyS]) || pad(G::DPadDown);
    let left = pressed(&[ArrowLeft, KeyA]) || pad(G::DPadLeft);
    let right = pressed(&[ArrowRight, KeyD]) || pad(G::DPadRight);
    let confirm = pressed(&[Enter, NumpadEnter, Space, KeyF]) || pad(G::South);
    let back = pressed(&[Escape, Backspace]) || pad(G::East);

    // The options popup takes every key while open.
    if let Some((index, choice)) = menu.popup {
        let options = item_options(inventory, equipment, index);
        if options.is_empty() || back {
            menu.back();
            return;
        }
        let n = options.len() as i32;
        let c = (choice as i32 + i32::from(down) - i32::from(up)).rem_euclid(n) as usize;
        if c != choice {
            (menu.popup, menu.dirty) = (Some((index, c)), true);
        }
        if confirm {
            actions.apply(entity, inventory, index, options[c]);
            (menu.popup, menu.dirty) = (None, true);
        }
        return;
    }
    if back && menu.back() {
        return;
    }

    if menu.focus == Focus::Slots {
        let n = EquipSlot::ALL.len() as i32;
        let s = (menu.slot as i32 + i32::from(down) - i32::from(up)).clamp(0, n - 1) as usize;
        menu.select_slot(s);
        if right {
            menu.select_cell(0);
        } else if confirm {
            actions.start_picking(&mut menu, inventory, equipment);
        } else if (pressed(&[Backspace, KeyX]) || pad(G::West))
            && let Some(index) = equipment.get(EquipSlot::ALL[menu.slot])
            && equipment.can_unequip(index)
        {
            actions.apply(entity, inventory, index, ItemOption::Unequip);
        }
        return;
    }

    // The grid.
    if menu.picking.is_none() {
        let (prev, next) = (pressed(&[KeyQ]) || pad(G::LeftTrigger), pressed(&[KeyE]) || pad(G::RightTrigger));
        if prev || next {
            menu.filter = menu.filter.step(if prev { -1 } else { 1 });
            menu.cursor = 0;
            menu.dirty = true;
        }
    }
    let list = shown(&menu, inventory);
    if left && menu.cursor.is_multiple_of(COLS) {
        // Off the grid's left edge: back to the equipment column.
        if menu.picking.take().is_none() {
            menu.slot = menu.slot.min(EquipSlot::ALL.len() - 1);
        }
        (menu.focus, menu.dirty) = (Focus::Slots, true);
        return;
    }
    let mut c = menu.cursor as i32;
    c += i32::from(right) - i32::from(left);
    c += (i32::from(down) - i32::from(up)) * COLS as i32;
    let c = c.clamp(0, list.len().saturating_sub(1) as i32) as usize;
    menu.select_cell(c);
    let Some(&index) = list.get(menu.cursor) else { return };
    if confirm {
        if menu.picking.is_some() {
            actions.pick(&mut menu, entity, equipment, index);
        } else if !item_options(inventory, equipment, index).is_empty() {
            (menu.popup, menu.dirty) = (Some((index, 0)), true);
        }
    } else if (pressed(&[KeyX]) || pad(G::West)) && equipment.can_unequip(index) {
        actions.apply(entity, inventory, index, ItemOption::Unequip);
    } else if (pressed(&[KeyL]) || pad(G::North)) && inventory.items[index].def().kind == ItemKind::Weapon {
        actions.apply(entity, inventory, index, ItemOption::EquipTo(EquipSlot::LeftHand));
    }
}

#[allow(clippy::type_complexity)]
fn rebuild_menu(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut menu: ResMut<InventoryMenu>,
    root: Single<Entity, With<MenuRoot>>,
    player: Query<(Entity, &Inventory, &Equipment, &EquipStats, &Health, Option<&HealingAle>), With<Player>>,
    changed: Query<(), (With<Player>, Or<(Changed<Inventory>, Changed<Equipment>, Changed<EquipStats>, Changed<HealingAle>)>)>,
) {
    if !menu.open || !(menu.dirty || !changed.is_empty()) {
        return;
    }
    menu.dirty = false;
    debug!("inventory: focus {:?} slot {} cursor {} picking {:?} popup {:?}", menu.focus, menu.slot, menu.cursor, menu.picking, menu.popup);
    let Ok((entity, inventory, equipment, stats, health, ale)) = player.single() else { return };
    let list = shown(&menu, inventory);
    menu.cursor = menu.cursor.min(list.len().saturating_sub(1));
    if menu.popup.is_some_and(|(i, _)| i >= inventory.items.len()) {
        menu.popup = None;
    }
    let selected = match menu.focus {
        Focus::Grid => list.get(menu.cursor).copied(),
        Focus::Slots => equipment.get(EquipSlot::ALL[menu.slot]),
    };
    let display: Handle<Font> = assets.load(DISPLAY_FONT);
    let ctx = Ctx { entity, inventory, equipment, ale, display, menu: &menu };

    let root = *root;
    commands.entity(root).despawn_children();
    let panel = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(22.0)),
                row_gap: px(14.0),
                border: UiRect::all(px(2.0)),
                border_radius: BorderRadius::all(px(6.0)),
                ..default()
            },
            BackgroundColor(PANEL_BG),
            BorderColor::all(FRAME),
            ChildOf(root),
        ))
        .id();
    commands.spawn((
        Text::new(strings::INVENTORY_TITLE.to_uppercase()),
        TextFont { font: ctx.display.clone().into(), font_size: FontSize::Px(34.0), ..default() },
        TextColor(GOLD_TEXT),
        ChildOf(panel),
    ));
    let columns = commands.spawn((Node { column_gap: px(22.0), ..default() }, ChildOf(panel))).id();
    equipment_column(&mut commands, columns, &ctx);
    grid_column(&mut commands, columns, &ctx, &list);
    details_column(&mut commands, columns, &ctx, selected, stats, health);
    commands.spawn((Text::new(strings::INVENTORY_HINTS), TextFont::from_font_size(14.0), TextColor(DIM), ChildOf(panel)));
    if let Some((index, choice)) = menu.popup {
        options_popup(&mut commands, root, &ctx, index, choice);
    }
}

/// What the builders need to know about the player.
struct Ctx<'a> {
    entity: Entity,
    inventory: &'a Inventory,
    equipment: &'a Equipment,
    ale: Option<&'a HealingAle>,
    display: Handle<Font>,
    menu: &'a InventoryMenu,
}

fn px(v: f32) -> Val {
    Val::Px(v)
}

fn text(commands: &mut Commands, parent: Entity, s: impl Into<String>, size: f32, color: Color) -> Entity {
    commands.spawn((Text::new(s), TextFont::from_font_size(size), TextColor(color), ChildOf(parent))).id()
}

fn header(commands: &mut Commands, parent: Entity, s: &str) {
    let h = commands
        .spawn((Node { border: UiRect::bottom(px(1.0)), padding: UiRect::bottom(px(3.0)), margin: UiRect::bottom(px(4.0)), ..default() }, BorderColor::all(FRAME), ChildOf(parent)))
        .id();
    text(commands, h, s.to_uppercase(), 14.0, GOLD_TEXT);
}

fn column(commands: &mut Commands, parent: Entity, width: f32) -> Entity {
    commands.spawn((Node { width: px(width), flex_direction: FlexDirection::Column, row_gap: px(5.0), ..default() }, ChildOf(parent))).id()
}

fn equipment_column(commands: &mut Commands, parent: Entity, ctx: &Ctx) {
    let col = column(commands, parent, 250.0);
    header(commands, col, "Equipment");
    let menu = ctx.menu;
    for (i, slot) in EquipSlot::ALL.into_iter().enumerate() {
        let item = ctx.equipment.item(slot, ctx.inventory);
        let focused = menu.focus == Focus::Slots && menu.slot == i;
        let picking = menu.picking == Some(slot);
        let row = commands
            .spawn((
                Node {
                    align_items: AlignItems::Center,
                    column_gap: px(8.0),
                    padding: UiRect::all(px(2.0)),
                    border: UiRect::all(px(1.0)),
                    border_radius: BorderRadius::all(px(4.0)),
                    ..default()
                },
                BackgroundColor(if focused || picking { SELECTED_BG } else { Color::NONE }),
                BorderColor::all(if picking { GOLD_TEXT } else if focused { FRAME } else { Color::NONE }),
                ChildOf(col),
            ))
            .observe(move |_: On<PointerMove>, mut menu: ResMut<InventoryMenu>| {
                if menu.popup.is_none() && menu.picking.is_none() {
                    menu.select_slot(i);
                }
            })
            .observe(
                move |_: On<PointerClick>,
                      mut menu: ResMut<InventoryMenu>,
                      mut actions: ItemActions,
                      player: Query<(&Inventory, &Equipment), With<Player>>| {
                    let Ok((inventory, equipment)) = player.single() else { return };
                    (menu.popup, menu.picking) = (None, None);
                    menu.select_slot(i);
                    actions.start_picking(&mut menu, inventory, equipment);
                },
            )
            .id();
        let icon_box = commands
            .spawn((
                Node { width: px(40.0), height: px(40.0), border: UiRect::all(px(1.0)), border_radius: BorderRadius::all(px(4.0)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, overflow: Overflow::clip(), ..default() },
                BackgroundColor(CELL_BG),
                BorderColor::all(FRAME),
                ChildOf(row),
            ))
            .id();
        if let Some(item) = item {
            spawn_icon(commands, icon_box, item.def().icon, 0.68);
        }
        let label = commands.spawn((Node { flex_direction: FlexDirection::Column, ..default() }, ChildOf(row))).id();
        text(commands, label, slot.label(), 11.0, if picking { GOLD_TEXT } else { DIM });
        text(commands, label, item.map_or("Empty", |i| i.def().name), 15.0, if item.is_some() { TEXT } else { DIM });
    }
}

fn grid_column(commands: &mut Commands, parent: Entity, ctx: &Ctx, list: &[usize]) {
    let menu = ctx.menu;
    let col = column(commands, parent, COLS as f32 * (CELL + 6.0));
    let tabs = commands.spawn((Node { column_gap: px(4.0), margin: UiRect::bottom(px(6.0)), ..default() }, ChildOf(col))).id();
    if let Some(slot) = menu.picking {
        // Picking for a slot: a title instead of the filter tabs.
        let t = commands.spawn((Node { padding: UiRect::axes(px(8.0), px(4.0)), border: UiRect::bottom(px(2.0)), ..default() }, BorderColor::all(GOLD_TEXT), ChildOf(tabs))).id();
        text(commands, t, format!("Equip: {}   [Esc] back", slot.label()), 14.0, GOLD_TEXT);
    } else {
        for f in Filter::ALL {
            let active = f == menu.filter;
            let tab = commands
                .spawn((
                    Node { padding: UiRect::axes(px(8.0), px(4.0)), border: UiRect::bottom(px(2.0)), ..default() },
                    BackgroundColor(if active { SELECTED_BG } else { Color::NONE }),
                    BorderColor::all(if active { GOLD_TEXT } else { Color::NONE }),
                    ChildOf(tabs),
                ))
                .observe(move |_: On<PointerClick>, mut menu: ResMut<InventoryMenu>| {
                    if menu.filter != f {
                        (menu.filter, menu.cursor, menu.focus, menu.dirty) = (f, 0, Focus::Grid, true);
                    }
                })
                .id();
            text(commands, tab, f.label(), 14.0, if active { GOLD_TEXT } else { DIM });
        }
    }
    let grid = commands
        .spawn((Node { flex_wrap: FlexWrap::Wrap, column_gap: px(6.0), row_gap: px(6.0), min_height: px(CELL * 3.0 + 12.0), ..default() }, ChildOf(col)))
        .id();
    if list.is_empty() {
        text(commands, grid, "Nothing here.", 15.0, DIM);
    }
    for (pos, &index) in list.iter().enumerate() {
        let item = ctx.inventory.items[index];
        let selected = menu.focus == Focus::Grid && pos == menu.cursor;
        let character = ctx.entity;
        let cell = commands
            .spawn((
                Node {
                    width: px(CELL),
                    height: px(CELL),
                    border: UiRect::all(px(2.0)),
                    border_radius: BorderRadius::all(px(5.0)),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    overflow: Overflow::clip(),
                    ..default()
                },
                BackgroundColor(if selected { SELECTED_BG } else { CELL_BG }),
                BorderColor::all(if selected { GOLD_TEXT } else { FRAME }),
                ChildOf(grid),
            ))
            .observe(move |_: On<PointerMove>, mut menu: ResMut<InventoryMenu>| {
                if menu.popup.is_none() {
                    menu.select_cell(pos);
                }
            })
            .observe(
                move |click: On<PointerClick>,
                      mut menu: ResMut<InventoryMenu>,
                      mut actions: ItemActions,
                      player: Query<(&Equipment, &Inventory), With<Player>>| {
                    let Ok((equipment, inventory)) = player.single() else { return };
                    menu.popup = None;
                    menu.select_cell(pos);
                    if menu.picking.is_some() {
                        actions.pick(&mut menu, character, equipment, index);
                    } else if (click.button == PointerButton::Secondary || click.count >= 2)
                        && !item_options(inventory, equipment, index).is_empty()
                    {
                        (menu.popup, menu.dirty) = (Some((index, 0)), true);
                    }
                },
            )
            .id();
        spawn_icon(commands, cell, item.def().icon, 1.0);
        if ctx.equipment.slot_of(index).is_some() {
            let badge = commands
                .spawn((
                    Node { position_type: PositionType::Absolute, left: px(3.0), top: px(1.0), ..default() },
                    ChildOf(cell),
                ))
                .id();
            text(commands, badge, "E", 13.0, GOLD_TEXT);
        }
        let count = if item == ItemId::HealingAle {
            ctx.ale.map(|a| a.charges)
        } else if item.def().effect.is_some() {
            Some(ctx.inventory.count(item))
        } else {
            None
        };
        if let Some(n) = count {
            let badge = commands
                .spawn((Node { position_type: PositionType::Absolute, right: px(4.0), bottom: px(1.0), ..default() }, ChildOf(cell)))
                .id();
            text(commands, badge, n.to_string(), 14.0, if n == 0 { DIM } else { TEXT });
        }
    }
}

/// The small options window over the screen: one row per [`ItemOption`]. Clicking outside closes it.
fn options_popup(commands: &mut Commands, root: Entity, ctx: &Ctx, index: usize, choice: usize) {
    let item = ctx.inventory.items[index];
    let options = item_options(ctx.inventory, ctx.equipment, index);
    let overlay = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.35)),
            ChildOf(root),
        ))
        .observe(|_: On<PointerClick>, mut menu: ResMut<InventoryMenu>| {
            menu.back();
        })
        .id();
    let window = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                min_width: px(240.0),
                padding: UiRect::all(px(12.0)),
                row_gap: px(3.0),
                border: UiRect::all(px(2.0)),
                border_radius: BorderRadius::all(px(6.0)),
                ..default()
            },
            BackgroundColor(PANEL_BG),
            BorderColor::all(GOLD_TEXT),
            ChildOf(overlay),
        ))
        .observe(|mut click: On<PointerClick>| click.propagate(false))
        .id();
    let top = commands.spawn((Node { column_gap: px(8.0), align_items: AlignItems::Center, margin: UiRect::bottom(px(6.0)), ..default() }, ChildOf(window))).id();
    let icon = commands.spawn((Node { width: px(36.0), height: px(36.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }, ChildOf(top))).id();
    spawn_icon(commands, icon, item.def().icon, 0.6);
    commands.spawn((
        Text::new(item.def().name),
        TextFont { font: ctx.display.clone().into(), font_size: FontSize::Px(20.0), ..default() },
        TextColor(TEXT),
        ChildOf(top),
    ));
    let character = ctx.entity;
    for (n, option) in options.into_iter().enumerate() {
        let selected = n == choice;
        let row = commands
            .spawn((
                Node { padding: UiRect::axes(px(10.0), px(5.0)), border_radius: BorderRadius::all(px(4.0)), ..default() },
                BackgroundColor(if selected { SELECTED_BG } else { Color::NONE }),
                ChildOf(window),
            ))
            .observe(move |_: On<PointerMove>, mut menu: ResMut<InventoryMenu>| {
                if menu.popup != Some((index, n)) {
                    (menu.popup, menu.dirty) = (Some((index, n)), true);
                }
            })
            .observe(
                move |mut click: On<PointerClick>, mut menu: ResMut<InventoryMenu>, mut actions: ItemActions, player: Query<&Inventory, With<Player>>| {
                    click.propagate(false);
                    if let Ok(inventory) = player.single() {
                        actions.apply(character, inventory, index, option);
                    }
                    (menu.popup, menu.dirty) = (None, true);
                },
            )
            .id();
        let color = if selected { GOLD_TEXT } else if option == ItemOption::ThrowOut { Color::srgb(0.8, 0.55, 0.45) } else { TEXT };
        text(commands, row, option.label(item), 16.0, color);
    }
}

fn details_column(commands: &mut Commands, parent: Entity, ctx: &Ctx, selected: Option<usize>, stats: &EquipStats, health: &Health) {
    let col = column(commands, parent, 300.0);
    header(commands, col, "Details");
    let mut preview = None;
    if let Some(index) = selected {
        let item = ctx.inventory.items[index];
        let def = item.def();
        let top = commands.spawn((Node { column_gap: px(10.0), align_items: AlignItems::Center, ..default() }, ChildOf(col))).id();
        let big = commands
            .spawn((
                Node { width: px(70.0), height: px(70.0), border: UiRect::all(px(1.0)), border_radius: BorderRadius::all(px(5.0)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
                BackgroundColor(CELL_BG),
                BorderColor::all(FRAME),
                ChildOf(top),
            ))
            .id();
        spawn_icon(commands, big, def.icon, 1.1);
        let names = commands.spawn((Node { flex_direction: FlexDirection::Column, flex_shrink: 1.0, ..default() }, ChildOf(top))).id();
        commands.spawn((
            Text::new(def.name),
            TextFont { font: ctx.display.clone().into(), font_size: FontSize::Px(23.0), ..default() },
            TextColor(TEXT),
            ChildOf(names),
        ));
        let slot = ctx.equipment.slot_of(index);
        let kind = match slot {
            Some(s) => format!("{}, equipped ({})", def.kind.label(), s.label().to_lowercase()),
            None => def.kind.label().to_string(),
        };
        text(commands, names, kind, 13.0, DIM);
        text(commands, col, def.description, 14.0, Color::srgb(0.75, 0.72, 0.66));

        // The item's own numbers.
        let lines = commands.spawn((Node { flex_direction: FlexDirection::Column, margin: UiRect::vertical(px(4.0)), ..default() }, ChildOf(col))).id();
        let line = |commands: &mut Commands, label: &str, value: String| row(commands, lines, label, value, None);
        if let Some(w) = def.weapon {
            line(commands, "Attack", format!("{:.0}", w.base_damage));
            line(commands, "Reach", format!("{:.2} m", w.blade.1));
        }
        if def.block > 0.0 {
            line(commands, "Block (Phase 8)", format!("{:.0}%", def.block * 100.0));
        }
        if def.absorption > 0.0 {
            line(commands, "Absorption", format!("{:.0}%", def.absorption * 100.0));
        }
        if def.max_hp_bonus > 0.0 {
            line(commands, "Max HP", format!("+{:.0}", def.max_hp_bonus));
        }
        if item == ItemId::HealingAle
            && let Some(ale) = ctx.ale
        {
            line(commands, "Charges", format!("{} / {}", ale.charges, ale.max));
            line(commands, "Heals", format!("{:.0}% of max HP", crate::tuning::ALE_HEAL_FRACTION * 100.0));
        }
        if def.effect.is_some() {
            line(commands, "Owned", ctx.inventory.count(item).to_string());
        }
        if def.weight > 0.0 {
            line(commands, "Weight", format!("{:.1}", def.weight));
        }

        // The preview: what equipping it into the slot being picked for would change, else
        // what its first equip / unequip option would.
        let mut next = ctx.equipment.clone();
        let changes = match ctx.menu.picking {
            Some(slot) => next.equip_to(ctx.inventory, index, slot).is_some(),
            None => match item_options(ctx.inventory, ctx.equipment, index)
                .into_iter()
                .filter(|o| !matches!(o, ItemOption::Use | ItemOption::ThrowOut))
                // Worn: show what taking it off does (not moving it to the other hand).
                .min_by_key(|o| if ctx.equipment.slot_of(index).is_some() { u8::from(*o != ItemOption::Unequip) } else { 0 })
            {
                Some(ItemOption::EquipTo(slot)) => next.equip_to(ctx.inventory, index, slot).is_some(),
                Some(ItemOption::Equip) => next.equip(ctx.inventory, index).is_some(),
                Some(ItemOption::Unequip) => next.unequip(index),
                _ => false,
            },
        };
        if changes {
            preview = Some(EquipStats::compute(ctx.inventory, &next));
        }
        if ctx.menu.picking.is_none() && !item_options(ctx.inventory, ctx.equipment, index).is_empty() {
            let button = commands
                .spawn((
                    Node { padding: UiRect::axes(px(14.0), px(6.0)), border: UiRect::all(px(1.0)), border_radius: BorderRadius::all(px(4.0)), align_self: AlignSelf::FlexStart, ..default() },
                    BackgroundColor(SELECTED_BG),
                    BorderColor::all(GOLD_TEXT),
                    ChildOf(col),
                ))
                .observe(move |_: On<PointerClick>, mut menu: ResMut<InventoryMenu>| {
                    (menu.popup, menu.dirty) = (Some((index, 0)), true);
                })
                .id();
            text(commands, button, "Options  [Enter]", 15.0, GOLD_TEXT);
        }
    } else {
        text(commands, col, "Nothing selected.", 14.0, DIM);
    }

    // Status summary (with the preview of equipping / unequipping the selection).
    let status = commands.spawn((Node { flex_direction: FlexDirection::Column, margin: UiRect::top(px(10.0)), ..default() }, ChildOf(col))).id();
    header(commands, status, "Status");
    let p = preview.filter(|p| p != stats);
    let cmp = |cur: f32, new: Option<f32>, higher_better: bool, fmt: &dyn Fn(f32) -> String| {
        new.filter(|n| (n - cur).abs() > 1e-4).map(|n| (fmt(n), if (n > cur) == higher_better { BETTER } else { WORSE }))
    };
    let int = |v: f32| format!("{v:.0}");
    let pct = |v: f32| format!("{:.0}%", v * 100.0);
    let hp_base = health.max - stats.max_hp_bonus;
    row(commands, status, "Attack", int(stats.attack), cmp(stats.attack, p.map(|p| p.attack), true, &int));
    row(commands, status, "Max HP", int(health.max), cmp(health.max, p.map(|p| hp_base + p.max_hp_bonus), true, &int));
    row(commands, status, "Absorption", pct(stats.absorption), cmp(stats.absorption, p.map(|p| p.absorption), true, &pct));
    row(commands, status, "Block", pct(stats.block), cmp(stats.block, p.map(|p| p.block), true, &pct));
    let load = |w: f32| format!("{w:.1} / {:.0}", stats.max_load);
    row(commands, status, "Equip load", load(stats.weight), cmp(stats.weight, p.map(|p| p.weight), false, &load));
    let class = p.map_or(stats.load_class(), |p| p.load_class());
    row(commands, status, "Load class", stats.load_class().to_string(), (class != stats.load_class()).then(|| (class.to_string(), DIM)));
}

/// "Label ........ value [> new]".
fn row(commands: &mut Commands, parent: Entity, label: &str, value: String, change: Option<(String, Color)>) {
    let r = commands.spawn((Node { justify_content: JustifyContent::SpaceBetween, padding: UiRect::vertical(px(1.0)), ..default() }, ChildOf(parent))).id();
    text(commands, r, label, 15.0, DIM);
    let right = commands.spawn((Node { column_gap: px(6.0), ..default() }, ChildOf(r))).id();
    text(commands, right, value, 15.0, TEXT);
    if let Some((new, color)) = change {
        text(commands, right, ">", 15.0, DIM);
        text(commands, right, new, 15.0, color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_pick_their_kinds() {
        let inv = Inventory {
            items: vec![ItemId::Longsword, ItemId::HealingAle, ItemId::WoodenShield, ItemId::RustedAxe, ItemId::LeatherHood, ItemId::SunseedRing],
            ..default()
        };
        assert_eq!(visible(&inv, Filter::All).len(), 6);
        assert_eq!(visible(&inv, Filter::Weapons), vec![0, 3]);
        assert_eq!(visible(&inv, Filter::Shields), vec![2]);
        assert_eq!(visible(&inv, Filter::Armor), vec![4]);
        assert_eq!(visible(&inv, Filter::Rings), vec![5]);
        assert_eq!(visible(&inv, Filter::Consumables), vec![1]);
        let menu = InventoryMenu { picking: Some(EquipSlot::LeftHand), ..default() };
        assert_eq!(shown(&menu, &inv), vec![0, 2, 3], "picking for the left hand: weapons and shields");
        assert_eq!(Filter::All.step(-1), Filter::Consumables);
    }

    #[test]
    fn options_fit_the_item() {
        let inv = Inventory::from_items(&[ItemId::Longsword, ItemId::HealingAle, ItemId::WoodenShield, ItemId::Poppy, ItemId::ForgeKey]);
        let mut eq = Equipment::default();
        eq.equip(&inv, 0);
        eq.equip(&inv, 1);
        use ItemOption::*;
        assert_eq!(item_options(&inv, &eq, 0), vec![EquipTo(EquipSlot::LeftHand), Unequip, ThrowOut], "sword in the right hand");
        assert_eq!(item_options(&inv, &eq, 1), vec![Use], "the flask: drink only");
        assert_eq!(item_options(&inv, &eq, 2), vec![Equip, ThrowOut]);
        assert_eq!(item_options(&inv, &eq, 3), vec![Use, ThrowOut]);
        assert_eq!(item_options(&inv, &eq, 4), vec![ThrowOut], "a key can only be thrown out");
    }
}
