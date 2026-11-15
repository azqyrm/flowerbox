//! The inventory screen (Tab / I). The world keeps running (no pause in a Soulslike), but the
//! player gets no gameplay input while it is open ([`GameplayBlocked`]) and the cursor is free.
//!
//! Layout: equipment slots (left) · filter tabs + item grid (middle) · details of the selected
//! item, Equip/Unequip button and the status summary with a preview of what equipping the
//! selection would change (right). Keys: arrows/WASD move, Q/E filter, Enter equip/unequip,
//! Backspace unequip, Tab/I/Esc close. Gamepad: D-pad move, L1/R1 filter, A equip/unequip,
//! X unequip, Start/B close. Mouse: hover selects, click selects, double-click or the
//! button equips.
//!
//! The content is rebuilt whenever something it shows changes (it's small, so this is cheap and
//! keeps the code simple).

use bevy::prelude::*;
use bevy::window::{CursorOptions, PrimaryWindow};
use leafwing_input_manager::prelude::ActionState;

use super::hud::{FRAME, GOLD_TEXT};
use super::icons::spawn_icon;
use super::{DISPLAY_FONT, strings};
use crate::character::stats::Health;
use crate::healing::HealingAle;
use crate::hearth::DeathSequence;
use crate::input::{Action, GameplayBlocked, set_grab, toggle_cursor};
use crate::items::inventory::{EquipRequest, EquipSlot, EquipStats, Equipment, Inventory};
use crate::items::{ItemId, ItemKind};
use crate::player::Player;
use crate::pvp::UseItemRequest;

pub struct InventoryUiPlugin;

impl Plugin for InventoryUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InventoryMenu>()
            .add_systems(Startup, spawn_menu_root)
            .add_systems(Update, (toggle_menu.after(toggle_cursor), menu_keys, rebuild_menu).chain());
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

/// Inventory screen state.
#[derive(Resource, Default, Debug)]
pub struct InventoryMenu {
    pub open: bool,
    pub filter: Filter,
    /// Position of the selection in the filtered list.
    pub cursor: usize,
    /// Rebuild the content next frame.
    pub dirty: bool,
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

/// Equip if not worn, unequip if worn (and removable).
/// Consumables are used, not equipped: the Healing Ale (drink) and the stacking ones (eat).
fn usable(item: ItemId) -> bool {
    item == ItemId::HealingAle || item.def().effect.is_some()
}

fn toggle_request(character: Entity, equipment: &Equipment, index: usize) -> Option<EquipRequest> {
    match equipment.slot_of(index) {
        None => Some(EquipRequest { character, index, equip: true }),
        Some(_) if equipment.can_unequip(index) => Some(EquipRequest { character, index, equip: false }),
        Some(_) => None,
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

fn toggle_menu(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    mut menu: ResMut<InventoryMenu>,
    mut blocked: ResMut<GameplayBlocked>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
    player: Query<(&ActionState<Action>, Has<DeathSequence>), With<Player>>,
    mut root: Query<&mut Visibility, With<MenuRoot>>,
) {
    let Ok((actions, dying)) = player.single() else { return };
    let back = keys.just_pressed(KeyCode::Escape) || pads.iter().any(|g| g.just_pressed(GamepadButton::East));
    let toggle = actions.just_pressed(&Action::Inventory) || (menu.open && back);
    let want = !dying && (menu.open != toggle);
    if want != menu.open || blocked.0 != want {
        menu.open = want;
        menu.dirty = true;
        blocked.0 = want;
        set_grab(&mut cursor, !want);
        for mut v in &mut root {
            *v = if want { Visibility::Inherited } else { Visibility::Hidden };
        }
    }
}

fn menu_keys(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    mut menu: ResMut<InventoryMenu>,
    mut equip: MessageWriter<EquipRequest>,
    mut use_item: MessageWriter<UseItemRequest>,
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
    let (prev, next) = (pressed(&[KeyQ]) || pad(G::LeftTrigger), pressed(&[KeyE]) || pad(G::RightTrigger));
    if prev || next {
        menu.filter = menu.filter.step(if prev { -1 } else { 1 });
        menu.cursor = 0;
        menu.dirty = true;
    }
    let list = visible(inventory, menu.filter);
    let mut c = menu.cursor as i32;
    if pressed(&[ArrowLeft, KeyA]) || pad(G::DPadLeft) {
        c -= 1;
    }
    if pressed(&[ArrowRight, KeyD]) || pad(G::DPadRight) {
        c += 1;
    }
    if pressed(&[ArrowUp, KeyW]) || pad(G::DPadUp) {
        c -= COLS as i32;
    }
    if pressed(&[ArrowDown, KeyS]) || pad(G::DPadDown) {
        c += COLS as i32;
    }
    let c = c.clamp(0, list.len().saturating_sub(1) as i32) as usize;
    if c != menu.cursor {
        menu.cursor = c;
        menu.dirty = true;
    }
    let Some(&index) = list.get(menu.cursor) else { return };
    if pressed(&[Enter, NumpadEnter, Space, KeyF]) || pad(G::South) {
        let item = inventory.items[index];
        if usable(item) {
            use_item.write(UseItemRequest { item });
        } else if let Some(req) = toggle_request(entity, equipment, index) {
            equip.write(req);
        }
    } else if (pressed(&[Backspace, KeyX]) || pad(G::West)) && equipment.can_unequip(index) {
        equip.write(EquipRequest { character: entity, index, equip: false });
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
    let Ok((entity, inventory, equipment, stats, health, ale)) = player.single() else { return };
    let list = visible(inventory, menu.filter);
    menu.cursor = menu.cursor.min(list.len().saturating_sub(1));
    let selected = list.get(menu.cursor).copied();
    let display: Handle<Font> = assets.load(DISPLAY_FONT);
    let ctx = Ctx { entity, inventory, equipment, ale, display };

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
    grid_column(&mut commands, columns, &ctx, menu.filter, &list, menu.cursor);
    details_column(&mut commands, columns, &ctx, selected, stats, health);
    commands.spawn((Text::new(strings::INVENTORY_HINTS), TextFont::from_font_size(14.0), TextColor(DIM), ChildOf(panel)));
}

/// What the builders need to know about the player.
struct Ctx<'a> {
    entity: Entity,
    inventory: &'a Inventory,
    equipment: &'a Equipment,
    ale: Option<&'a HealingAle>,
    display: Handle<Font>,
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

/// Selects bag entry `index` (switching to the "All" filter, where position = index).
fn select_in_all(menu: &mut InventoryMenu, index: usize) {
    menu.filter = Filter::All;
    menu.cursor = index;
    menu.dirty = true;
}

fn equipment_column(commands: &mut Commands, parent: Entity, ctx: &Ctx) {
    let col = column(commands, parent, 250.0);
    header(commands, col, "Equipment");
    for slot in EquipSlot::ALL {
        let index = ctx.equipment.get(slot);
        let item = ctx.equipment.item(slot, ctx.inventory);
        let row = commands
            .spawn((
                Node { align_items: AlignItems::Center, column_gap: px(8.0), padding: UiRect::all(px(2.0)), border_radius: BorderRadius::all(px(4.0)), ..default() },
                BackgroundColor(Color::NONE),
                ChildOf(col),
            ))
            .id();
        if let Some(index) = index {
            commands.entity(row).observe(move |_: On<PointerClick>, mut menu: ResMut<InventoryMenu>| select_in_all(&mut menu, index));
        }
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
        text(commands, label, slot.label(), 11.0, DIM);
        text(commands, label, item.map_or("Empty", |i| i.def().name), 15.0, if item.is_some() { TEXT } else { DIM });
    }
}

fn grid_column(commands: &mut Commands, parent: Entity, ctx: &Ctx, filter: Filter, list: &[usize], cursor: usize) {
    let col = column(commands, parent, COLS as f32 * (CELL + 6.0));
    // Filter tabs.
    let tabs = commands.spawn((Node { column_gap: px(4.0), margin: UiRect::bottom(px(6.0)), ..default() }, ChildOf(col))).id();
    for f in Filter::ALL {
        let active = f == filter;
        let tab = commands
            .spawn((
                Node { padding: UiRect::axes(px(8.0), px(4.0)), border: UiRect::bottom(px(2.0)), ..default() },
                BackgroundColor(if active { SELECTED_BG } else { Color::NONE }),
                BorderColor::all(if active { GOLD_TEXT } else { Color::NONE }),
                ChildOf(tabs),
            ))
            .observe(move |_: On<PointerClick>, mut menu: ResMut<InventoryMenu>| {
                if menu.filter != f {
                    menu.filter = f;
                    menu.cursor = 0;
                    menu.dirty = true;
                }
            })
            .id();
        text(commands, tab, f.label(), 14.0, if active { GOLD_TEXT } else { DIM });
    }
    let grid = commands
        .spawn((Node { flex_wrap: FlexWrap::Wrap, column_gap: px(6.0), row_gap: px(6.0), min_height: px(CELL * 3.0 + 12.0), ..default() }, ChildOf(col)))
        .id();
    if list.is_empty() {
        text(commands, grid, "Nothing here.", 15.0, DIM);
    }
    for (pos, &index) in list.iter().enumerate() {
        let item = ctx.inventory.items[index];
        let selected = pos == cursor;
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
            .observe(move |_: On<PointerOver>, mut menu: ResMut<InventoryMenu>| {
                if menu.cursor != pos {
                    menu.cursor = pos;
                    menu.dirty = true;
                }
            })
            .observe(
                move |click: On<PointerClick>,
                      mut menu: ResMut<InventoryMenu>,
                      mut equip: MessageWriter<EquipRequest>,
                      mut use_item: MessageWriter<UseItemRequest>,
                      player: Query<&Equipment, With<Player>>| {
                    menu.cursor = pos;
                    menu.dirty = true;
                    if click.count >= 2 {
                        if usable(item) {
                            use_item.write(UseItemRequest { item });
                        } else if let Ok(equipment) = player.single()
                            && let Some(req) = toggle_request(character, equipment, index)
                        {
                            equip.write(req);
                        }
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

        // Use button (consumables), or Equip / Unequip and the preview of that change.
        if usable(item) {
            let button = commands
                .spawn((
                    Node { padding: UiRect::axes(px(14.0), px(6.0)), border: UiRect::all(px(1.0)), border_radius: BorderRadius::all(px(4.0)), align_self: AlignSelf::FlexStart, ..default() },
                    BackgroundColor(SELECTED_BG),
                    BorderColor::all(GOLD_TEXT),
                    ChildOf(col),
                ))
                .observe(move |_: On<PointerClick>, mut use_item: MessageWriter<UseItemRequest>| {
                    use_item.write(UseItemRequest { item });
                })
                .id();
            text(commands, button, if item == ItemId::HealingAle { "Drink  [Enter]" } else { "Eat  [Enter]" }, 15.0, GOLD_TEXT);
        }
        let request = if usable(item) { None } else { toggle_request(ctx.entity, ctx.equipment, index) };
        if let Some(req) = request {
            let mut next = ctx.equipment.clone();
            if req.equip {
                next.equip(ctx.inventory, index);
            } else {
                next.unequip(index);
            }
            preview = Some(EquipStats::compute(ctx.inventory, &next));
            let button = commands
                .spawn((
                    Node { padding: UiRect::axes(px(14.0), px(6.0)), border: UiRect::all(px(1.0)), border_radius: BorderRadius::all(px(4.0)), align_self: AlignSelf::FlexStart, ..default() },
                    BackgroundColor(SELECTED_BG),
                    BorderColor::all(GOLD_TEXT),
                    ChildOf(col),
                ))
                .observe(move |_: On<PointerClick>, mut equip: MessageWriter<EquipRequest>| {
                    equip.write(req);
                })
                .id();
            text(commands, button, if req.equip { "Equip  [Enter]" } else { "Unequip  [Enter]" }, 15.0, GOLD_TEXT);
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
        assert_eq!(Filter::All.step(-1), Filter::Consumables);
    }
}
