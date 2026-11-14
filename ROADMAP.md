# Roadmap — Soulslike Dungeon Prototype (Rust)

> **For future sessions:** Read this file first, then `PROMPT.md` (the original request).
> Find the first phase that isn't checked off and continue from there. When you finish a task, tick its box,
> and add any decision that changes the plan to the **Decision Log** at the bottom.
> Title: **FlowerBox**.

---

## 1. Tech Stack Decision

| Concern | Choice | Version (verified 2026-11-14) | Why |
|---|---|---|---|
| Language | Rust | rustc 1.99 (installed) | Required |
| Engine | **Bevy** | `0.20` | The most mature Rust game engine: ECS, PBR 3D renderer, lights and shadows, UI, glTF, animation, audio. Writing our own engine would cost months before we got to gameplay. |
| Physics / collision | **avian3d** | `0.8` | ECS-native Bevy physics. Gives us colliders, sensors (hitboxes) and shape casts (grounding). Targets Bevy 0.20. |
| Input | **leafwing-input-manager** | `0.22` | Action-based input with rebinding and gamepad support. Hold durations make tap vs. hold easy to detect. Targets Bevy 0.20. |
| Debug (dev only) | bevy-inspector-egui | `0.38` | Live tweaking of components and values. Optional, behind a `dev` cargo feature. |

**Rejected options:**
- Fyrox: capable, but has a smaller ecosystem.
- Macroquad: too limited for 3D.
- A custom wgpu engine: too much work before any gameplay.
- bevy_rapier3d: viable, but avian is more idiomatic in Bevy.
- bevy-tnua (character controller): we need frame-exact control over rolls and attacks, so we write our own kinematic controller.

**Hardware:** AMD RX 6800-class GPU on Linux, so Vulkan works out of the box. Bevy also needs these Linux packages: `libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev`.

**Build tips:** set `[profile.dev] opt-level = 1` and `[profile.dev.package."*"] opt-level = 3`. You can use Bevy's `dynamic_linking` feature to speed up rebuilds during development.

---

## 2. Controls (default bindings, keyboard + mouse; gamepad added later)

| Action | Key | Notes |
|---|---|---|
| Move | **W A S D** | Relative to the camera |
| Camera | **Mouse** | Third-person orbit camera; the cursor is locked during gameplay |
| Jump | **Space** | |
| Dodge roll / sprint | **Left Shift** | Tap to roll (i-frames). Hold to sprint (familiar element). Rolling while standing still does a backstep. |
| Attack | **Left Mouse** | **Tap** (released within 0.25 s) = light attack. **Hold** (0.25 s or longer) = heavy attack, released when charged or automatically at full charge. **In the air** = jump attack. |
| Block (later) | Right Mouse | Planned familiar element, not in the core scope |
| Drink Healing Ale | **R** | |
| Interact / rest at Hearth | **E** | |
| Lock-on target | **Q** or Middle Mouse | Familiar element (Phase 8) |
| Inventory / Equipment | **Tab** or **I** | Pauses gameplay input |
| Pause menu | **Esc** | |

All tuning values (timings, costs, damage) live in one `tuning.rs` constants module, or in a RON file later, so it's easy to adjust game feel.

---

## 3. Game Design Spec (scope of this prototype)

### Dungeon arena
- A circular stone floor with a **radius of about 18 m** and a ring wall about 6 m high. Both have colliders.
- 6–8 pillars around the edge, plus some rubble, to break line of sight and add depth.
- **Lighting:** dark ambient light, flickering **torches** on the walls (point lights with shadows and an orange tint), and a soft cool moonlight shaft from above (a spotlight). Add distance fog for mood, plus bloom on the flames.
- A **Hearth** (rest point, in the spirit of a bonfire) at the edge of the arena.
- A **training dummy** in the center that takes damage, shows hit feedback and resets.
- 2–3 **item pickups** on the floor to test the inventory (for example a shield, a second weapon and a ring).

### Player
- A placeholder human made of primitives (a capsule body, a sphere head, arms) with a **sword** mesh attached to the right hand. In Phase 9 this becomes a rigged glTF model, such as a CC0 Quaternius or Kenney character.
- **Stats:** HP 100, Stamina 100 (regenerates about 35/s after a 0.6 s delay; regen pauses while you act), Poise later.
- **State machine** (the heart of the game feel; one enum component):
  `Idle | Walk | Sprint | Jump | Fall | Roll | Backstep | LightAttack(n) | HeavyCharge | HeavyAttack | JumpAttack | Drinking | Hurt | Dead | Resting`
  - Every action has **startup, active and recovery** windows. You can only do **input buffering** (an input queued about 0.2 s ahead) and **cancels** at defined points. This is what makes it feel "souls".

### Movement and jump
- Walk at about 4.5 m/s and sprint at about 7 m/s (sprint drains stamina). The character turns toward the movement direction.
- Gravity is about 20 m/s² and a jump reaches about 1.2 m. Air control is limited. There is a coyote time of 0.1 s.

### Dodge roll
- Lasts 0.6 s, covers about 3.5 m and costs 20 stamina.
- **I-frames from 0.05 s to 0.40 s.** During that window the player has an `Invulnerable` marker component, and damage is ignored.
- Recovery takes 0.15 s, and you can cancel it into an attack or another roll.
- Weight classes (fast, medium, fat roll, based on equip load) come later, once the inventory carries weight.

### Attacks
- **Light:** a combo of up to 2–3 swings, about 0.5 s each, costing 15 stamina and dealing 1.0× damage.
- **Heavy:** a 0.25–0.9 s charge (moving slowly) followed by an overhead swing. Costs 30 stamina, deals 1.6× damage, or 2.0× when fully charged.
- **Jump attack:** if you attack while airborne, you plunge down with a slam on landing. Costs 25 stamina and deals 1.8× damage.
- **Hitboxes:** an avian **sensor collider** on the sword blade, enabled only during the active frames. Each swing hits a target at most once (track a `HitList` per swing). Add a **hitstop** of about 60 ms and a small camera shake when a hit lands.

### Healing Ale (estus equivalent)
- **3 charges** at the start. Each drink heals **40% of max HP** over a short tick, takes about 1.1 s, and lets you move slowly while drinking. You can't cancel a drink once it starts, which is the risk.
- **You replenish it by resting at the Hearth** (press E). Resting restores HP and Ale charges and respawns or resets the dummy. Later, upgrades raise the max charges.
- If you die, you respawn at the last Hearth with full Ale.

### Inventory and equipment
- **Items** are data-driven. An `ItemId` points to an `ItemDef { name, kind, icon, stats }` in a registry, which a RON file can feed later.
- **Item kinds:** Weapon, Shield, Armor (Head/Chest/Hands/Legs), Ring, Consumable, Key.
- **Equipment slots:** Right Hand, Left Hand, Head, Chest, Hands, Legs, Ring ×2. The Healing Ale gets a quick-item slot.
- **UI** (bevy_ui, opened with Tab):
  - an item grid with a filter by kind
  - a details panel with stats
  - Equip/Unequip actions (click, or Enter/Backspace)
  - a stat summary showing the damage you'd deal and your equip load.
- When you equip a weapon, the mesh in the hand swaps and the damage stats change. When you equip armor, defense changes (and visuals later).
- **Starting inventory:**
  - Equipped: Longsword (RH), Healing Ale ×3.
  - In the bag: Wooden Shield, Rusted Axe, Leather Hood.
  - Pickups in the arena add more.

### HUD
- HP (red), Stamina (green) and the Ale count with an icon, in the top-left corner.
- The equipped-items quadrant in the bottom-left corner.
- An interact prompt ("[E] Rest at Hearth").
- "YOU DIED" and death/respawn flow (familiar element).

---

## 4. Code Architecture

```
claude_ds/
├─ Cargo.toml
├─ PROMPT.md            # original request
├─ ROADMAP.md           # this file
├─ CLAUDE.md            # short pointer for future AI sessions
├─ assets/              # meshes, textures, sounds, items.ron (later)
└─ src/
   ├─ main.rs           # App setup, plugin registration
   ├─ tuning.rs         # all gameplay constants
   ├─ states.rs         # AppState (Loading, InGame, Inventory, Paused, Dead)
   ├─ input.rs          # leafwing actions, tap/hold detection, input buffer
   ├─ camera.rs         # third-person orbit camera, collision pull-in, lock-on (later)
   ├─ arena.rs          # circular dungeon, pillars, torches, lighting, fog
   ├─ player/
   │  ├─ mod.rs         # PlayerPlugin, spawn
   │  ├─ controller.rs  # kinematic movement, gravity, grounding, jump
   │  ├─ action.rs      # state machine: roll, attacks, drinking, timings
   │  └─ stats.rs       # Health, Stamina, regen, Invulnerable
   ├─ combat.rs         # hitboxes, damage events, hitstop, dummy
   ├─ items/
   │  ├─ mod.rs         # ItemDef registry, ItemId
   │  ├─ inventory.rs   # Inventory, Equipment components, equip/unequip events
   │  └─ pickup.rs      # world pickups
   ├─ healing.rs        # Healing Ale charges, drinking
   ├─ hearth.rs         # rest point, replenish, respawn
   └─ ui/
      ├─ hud.rs
      └─ inventory_ui.rs
```

**Principles**
- Each feature gets its own Bevy `Plugin`.
- Systems talk to each other through **Events/Messages** (`DamageEvent`, `EquipEvent`, `RestEvent`) instead of reaching into each other's components.
- Gameplay runs in `FixedUpdate` for deterministic timing. Visuals and the camera run in `Update`.
- Keep the code compiling and runnable at the end of every phase (`cargo run`).

---

## 5. Phases

### Phase 0 — Project setup
- [x] `git init`, a `.gitignore` (`/target`), and `cargo init --name flowerbox`
- [x] Add dependencies: bevy 0.20, avian3d 0.8, leafwing-input-manager 0.22 (and inspector behind the `dev` feature)
- [x] Set up dev profile optimizations and check the Linux system packages (packages missing, see Notes N1)
- [x] Open a window titled "FlowerBox" with a clear color, and confirm `cargo run` works (verified 2026-11-14: Vulkan on RX 6800 XT / RADV)

### Phase 1 — Circular dungeon arena and lighting
- [x] Circular floor (cylinder mesh) plus a ring wall built from segmented boxes or an inverted cylinder, with static colliders
- [x] Pillars and rubble with colliders
- [x] Torches: an emissive flame mesh plus a point light with flicker and shadows. Low ambient light, distance fog, bloom and tonemapping.
- [x] A temporary free or orbit camera to look around

### Phase 2 — Player controller and camera
- [ ] Spawn the primitive human and the sword attached to the hand
- [ ] Kinematic controller: WASD relative to the camera, rotation toward movement, gravity, ground check (shape cast), slide along walls
- [ ] Jump with coyote time, plus sprint on held Shift
- [ ] Third-person orbit camera (mouse) with a collision pull-in so it never clips through walls. Cursor grab and release.

### Phase 3 — Stats and action state machine
- [ ] `Health`, `Stamina` (with regen delay), `Invulnerable` components
- [ ] `PlayerAction` state enum with timers (startup, active, recovery), an input buffer and cancel windows
- [ ] Debug overlay that shows the current state and timers (dev feature)

### Phase 4 — Dodge roll with i-frames
- [ ] Roll in the input direction (backstep with no input), using a speed curve, stamina cost, and `Invulnerable` during the i-frame window
- [ ] Debug visual: tint the player during i-frames so the window can be checked
- [ ] Test that damage during i-frames is ignored. Add a unit test for the timing windows.

### Phase 5 — Combat: light, heavy and jump attacks
- [ ] Tap/hold detection on the attack action (threshold in `tuning.rs`)
- [ ] Light combo chain, heavy charge and release, and a jump attack (plunge) when airborne
- [ ] Procedural swing animation (rotating the arm/sword pivot) until real animations arrive
- [ ] Sword sensor hitbox that's only on during active frames, a per-swing hit list, `DamageEvent`
- [ ] Training dummy: HP, a hit flash, a knockback wobble, and it resets when it "dies"
- [ ] Hitstop and a small camera shake on hit

### Phase 6 — Healing Ale, Hearth, death and HUD
- [ ] Healing Ale: charges, drinking state, slowed movement, heal tick
- [ ] Hearth: interact prompt, rest (restore HP, stamina and Ale), set the respawn point
- [ ] Death: "YOU DIED" overlay, then respawn at the Hearth (debug key `K` kills the player for testing)
- [ ] HUD: HP and stamina bars, Ale counter, equipped-items quadrant

### Phase 7 — Inventory and equipment
- [ ] Item registry (in code first, `assets/items.ron` later), `Inventory`, `Equipment`
- [ ] Equip/Unequip events, plus stat recalculation (damage, defense, equip load)
- [ ] Inventory UI on Tab: grid, filters, details panel, equip/unequip, pauses gameplay input
- [ ] Swap the visual weapon or shield in hand when equipment changes
- [ ] World pickups (E to pick up) with a "Picked up: X" toast

### Phase 8 — Familiar extras (after the core works)
- [ ] Lock-on (Q): the camera focuses the target and movement strafes around it
- [ ] Block with a shield (RMB), which uses stamina
- [ ] Roll weight classes based on equip load
- [ ] Gamepad bindings
- [ ] Simple enemy (a hollow-type melee AI) to test i-frames against real attacks

### Phase 9 — Polish
- [ ] Replace the primitives with a rigged glTF character (CC0) and blend its animations into the state machine
- [ ] Sound: footsteps, swings, hits, the drink gulp, Hearth ambience and torches crackling
- [ ] Settings: mouse sensitivity, key rebinding, invert Y
- [ ] Save and load (inventory, Hearth, Ale upgrades)

---

## 6. Definition of Done (prototype)
When you start the game you're in the lit circular dungeon. You can walk with WASD, jump, sprint, and roll through damage. You can do light, heavy and jump attacks on the dummy. You can drink Healing Ale and refill it at the Hearth. You can open the inventory to equip or unequip a weapon, shield or armor and see the change. The game runs at 60+ FPS on the target machine.

---

## 7. Decision Log
| Date | Decision |
|---|---|
| 2026-11-14 | Engine: Bevy 0.20, plus avian3d 0.8 for physics and leafwing-input-manager 0.22 for input. Custom kinematic controller. |
| 2026-11-14 | The estus equivalent is named **Healing Ale**, with 3 charges, refilled at the **Hearth** (bonfire equivalent). |
| 2026-11-14 | Project title: **FlowerBox** (crate `flowerbox`). |
| 2026-11-14 | Attack: tap = light, hold of 0.25 s or longer = heavy, airborne = jump attack. Shift: tap = roll, hold = sprint. Space = jump. |

---

## 8. Notes (issues and findings, newest at the bottom)
- **N1 (2026-11-14) — RESOLVED (the user installed the packages):** The Linux dev packages Bevy needs aren't installed, so `cargo check` fails at `wayland-sys` (pkg-config can't find `wayland-client`). This needs sudo, so the user has to run:
  `sudo apt install libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev`
  The `!` prefix in Claude Code can't prompt for a sudo password, so the user has to run sudo commands in a separate terminal.
  (`pkg-config` is already installed.) The session runs on Wayland. Bevy's default features include both `wayland` and `x11`.
- **N2 (2026-11-14):** `cargo run --features dev` turns on Bevy `dynamic_linking` (faster rebuilds) and bevy-inspector-egui. Don't use it for release builds.
- **N3 (2026-11-14):** Bevy 0.20 renamed some APIs compared with older docs and examples. For example, `PointLight.shadows_enabled` is now **`shadow_maps_enabled`** (and there's also `contact_shadows_enabled`). When an API doesn't compile, check the real source under `~/.cargo/registry/src/*/bevy_*-0.20*/` instead of guessing.
- **N4 (2026-11-14):** Bevy logs nothing when its output is piped into `head` and the process gets killed. To check a run, use `timeout -s INT 10 ./target/debug/flowerbox > log 2>&1`. The log shows the GPU adapter line and "Creating new window FlowerBox".
- **N5 (2026-11-14) — visual checks without a human:** `FLOWERBOX_SCREENSHOT=/path/x.png ./target/debug/flowerbox` saves a screenshot after 120 frames and exits after 180 (`src/debug.rs`). `FLOWERBOX_CAM="yaw,pitch,dist"` sets the camera pose of the temporary orbit camera. Read the PNG to check the look. Phase 2 has to keep this camera override working for the player camera (for example as an offset).
- **N6 (2026-11-14) — Phase 1 result:**
  - Lighting values were tuned by screenshot: torches at 600k lm with range 20, moon spotlight at 30M lm, ambient brightness 30, TonyMcMapface tonemapping, `Bloom::NATURAL` and linear fog from 12 to 55 m. They all live in `tuning.rs`.
  - All 8 torches and the moon cast shadows; reduce this if FPS drops later.
  - The ring wall is made of 40 box segments, each with its own collider.
  - Some import paths differ from older docs: `bevy::camera::Hdr`, `bevy::post_process::bloom::Bloom`, `bevy::core_pipeline::tonemapping::Tonemapping`, and `GlobalAmbientLight` is a resource.
  - The temporary orbit camera uses right mouse to rotate and the wheel to zoom.
