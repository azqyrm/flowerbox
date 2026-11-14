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
| Dodge roll / sprint | **Left Shift** | Tap to roll (i-frames). Hold to sprint (familiar element). Rolling while standing still (no WASD input) does a backstep. There's no separate backstep input. |
| Attack | **Left Mouse** | **Tap** (released within 0.25 s) = light attack. **Hold** (0.25 s or longer) = heavy attack, released when charged or automatically at full charge. **Tap while airborne** = jump attack (the jump attack is the airborne light attack). A heavy attack can't be done in the air. |
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
- **Jump attack:** a light attack (tap) while airborne becomes the jump attack: you plunge down with a slam on landing. Costs 25 stamina and deals 1.8× damage.
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
   ├─ character/       # GENERIC: anything that moves/acts/takes damage (player, future enemies)
   │  ├─ mod.rs         # CharacterPlugin, CharacterSystems, MoveIntent, ActionIntent, character_bundle()
   │  ├─ controller.rs  # kinematic movement from MoveIntent, gravity, grounding, jump, air momentum
   │  ├─ action.rs      # state machine: roll, attacks, drinking, timings, lunges, running attack
   │  └─ stats.rs       # Health, Stamina, regen, Invulnerable, DamageMessage/DamageTaken
   ├─ humanoid/        # GENERIC: reusable humanoid body + shared animations
   │  ├─ mod.rs         # Bone enum, HumanoidSpec (proportions/look), spawn_humanoid(), HumanoidRig
   │  ├─ pose.rs        # HumanoidPose (+ blending/masks), forward kinematics
   │  └─ anim.rs        # locomotion (idle/walk/run/jump/land) + action poses, HumanoidAnimator
   ├─ player/
   │  └─ mod.rs         # Player = character_bundle + HumanoidSpec::human() + longsword + input
   ├─ weapon/          # weapon CLASSES (movesets + swing animations) and weapon DEFINITIONS (models)
   │  ├─ mod.rs         # WeaponClass, Moveset/AttackSpec/SwingKey/GuardSpec, WeaponDef, ModelSource, EquipWeapon
   │  └─ light_sword.rs # LightSword moveset + Longsword / Rusted Shortsword definitions
   ├─ combat.rs         # blade hitbox via FK (uses the equipped WeaponDef), hitstop, camera shake, dummy
   ├─ debug.rs          # F3 overlay, debug keys, screenshots, autopilot scripts
   ├─ items/
   │  ├─ mod.rs         # ItemDef registry, ItemId
   │  ├─ inventory.rs   # Inventory, Equipment components, equip/unequip events
   │  └─ pickup.rs      # world pickups
   ├─ healing.rs        # HealingAle charges, swallow + heal over time, tankard prop
   ├─ hearth.rs         # Hearth (kindle/rest), RespawnPoint, WorldReset, DeathSequence → respawn
   └─ ui/
      ├─ mod.rs         # UiPlugin, Banner message, display font path
      ├─ hud.rs         # bars, Ale, equipment quadrant, prompt, banner, YOU DIED + fades
      └─ inventory_ui.rs  (Phase 7)
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
- [x] Spawn the primitive human and the sword attached to the hand
- [x] Kinematic controller: WASD relative to the camera, rotation toward movement, gravity, ground check (shape cast), slide along walls
- [x] Jump with coyote time, plus sprint on held Shift
- [x] Third-person orbit camera (mouse) with a collision pull-in so it never clips through walls. Cursor grab and release.

### Phase 3 — Stats and action state machine
- [x] `Health`, `Stamina` (with regen delay), `Invulnerable` components
- [x] `PlayerAction` state enum with timers (startup, active, recovery), an input buffer and cancel windows
- [x] Debug overlay that shows the current state and timers (F3, always compiled in; see N9)

### Phase 4 — Dodge roll with i-frames
- [x] Roll in the input direction (backstep with no input), using a speed curve, stamina cost, and `Invulnerable` during the i-frame window
- [x] Debug visual: tint the player during i-frames so the window can be checked (done as a cyan ring at the feet while F3 is on, which is easier to read than a tint)
- [x] Test that damage during i-frames is ignored. Add a unit test for the timing windows.

### Phase 5 — Combat: light, heavy and jump attacks
- [x] Tap/hold detection on the attack action (threshold in `tuning.rs`)
- [x] Light combo chain, heavy charge and release, and a jump attack (plunge) when airborne
- [x] Procedural swing animation (rotating the arm/sword pivot) until real animations arrive
- [x] Sword hitbox that's only on during active frames, a per-swing hit list, `DamageMessage`. This is done with blade shape queries instead of a physics sensor; see N12.
- [x] Training dummy: HP, a hit flash, a knockback wobble, and it resets when it "dies"
- [x] Hitstop and a small camera shake on hit

### Phase 5.5 — Humanoid animation and movement interface (user request, 2026-11-14)
- [x] Generic `character` module: any entity with `character_bundle()` moves and acts from `MoveIntent` and `ActionIntent` (player input today, AI later)
- [x] Reusable `humanoid` module: bone skeleton from a `HumanoidSpec`, a shared animator, and FK so the hitboxes follow the bones
- [x] Movement animations: idle breathing, walk/run cycle with arm counter-swing, a rising/falling jump pose, and a landing crouch
- [x] Full-body action poses: tucked roll, backstep, light/heavy/jump attacks with stance, hurt, death fall, and drinking (upper body)
- [x] Attacks step forward (light 0.45 m, heavy 0.7 m). The running attack (light while sprinting) leaps 3.2 m with a hop. Jumps and jump attacks keep their horizontal momentum.

### Phase 5.6 — Weapon classes (user request, 2026-11-14)
- [x] `WeaponClass` + per-class `Moveset` (attack timings, costs, lunges, damage multipliers, swing keyframes, stance, guard pose, combo length). The current sword is class `LightSword`.
- [x] `WeaponDef` (name, class, base damage, blade span, model source) with replaceable models. `EquipWeapon` swaps the model in the hand at runtime.
- [x] Second model `Rusted Shortsword` (same class, shorter blade, 16 damage), with the debug key F6 to swap
- [ ] Future classes (two-handers, daggers, …): add a variant plus `weapon/<class>.rs` (see N15)

### Phase 5.7 — Locomotion polish (user request, 2026-11-14)
- [x] Walking into a wall no longer stutters: the ground probe was flickering (N16)
- [x] Slow, braced walk while pushing against an obstacle (free hand on the wall)
- [x] New gait: slow walk / jog / sprint blended by speed, with heel strike, toe-off, hip sway and counter-twist; feet planted on the floor; the sprint trails the weapon (per-class `GuardSpec::run_*`)

### Phase 6 — Healing Ale, Hearth, death and HUD
- [x] Healing Ale: charges, drinking state, slowed movement, heal tick (R; the charge is used at the swallow, an empty tankard does nothing)
- [x] Hearth: interact prompt, rest (restore HP, stamina and Ale), set the respawn point (E; the first rest kindles it; resting also resets the dummy)
- [x] Death: "YOU DIED" overlay, then respawn at the Hearth (debug key `K` kills the player for testing)
- [x] HUD: HP and stamina bars, Ale counter, equipped-items quadrant (plus the HP "damage chunk", the interact prompt and banners)

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
- [ ] Weapon models from glTF via `ModelSource::Gltf` (already wired, untested).
- [ ] Replace the primitives with a rigged glTF character (CC0). Map its skeleton onto `humanoid::Bone` so the shared poses and the FK hitbox keep working, or blend its clips in `humanoid::anim` (see N13).
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
| 2026-11-14 | Debug overlay on F3 is always compiled in (it's cheap). Debug keys (H, K, 1-4, F6) only exist in debug builds. Not tied to the `dev` feature. |
| 2026-11-14 | Backstep = rolling with no movement input; there's no separate input or request for it (user request). |
| 2026-11-14 | Sword hits are detected with blade shape queries that sample the same pose function the animation uses (`player::pose`), not avian sensors. This is deterministic, needs no kinematic/static sensor pairs, and keeps the visuals and hitbox identical. |
| 2026-11-14 | Characters are generic (`character` module, driven by `MoveIntent`/`ActionIntent`) and bodies are generic (`humanoid` module: `HumanoidSpec` + shared animator). To add another humanoid (an enemy), spawn `character_bundle()` + `spawn_humanoid(spec)` + `HumanoidAnimator`, and write its intents from AI (user request). |
| 2026-11-14 | A light attack while sprinting = running attack (a leaping slash). Light and heavy attacks lunge forward, and jump attacks keep their momentum (user request). |
| 2026-11-14 | Weapons are split into a class (how it fights and animates: `Moveset` in `weapon/<class>.rs`) and a definition (a concrete weapon: damage, blade span, model). Attack numbers moved out of `tuning.rs` into the class files. The starting sword is class `LightSword` (user request). |
| 2026-11-14 | Locomotion animation reads the movement actually achieved (`CharacterController::moved`), not the controller's velocity. Wanting to move but barely moving = "blocked" → braced slow walk. The normal move speed (4.5 m/s) animates as a jog and sprint as a run (user request). |
| 2026-11-14 | Healing Ale is used at the **swallow** (the start of the drink's active phase): a hit during the wind-up keeps the charge. Drinking with 0 charges still plays the drink and does nothing. |
| 2026-11-14 | Hearths start **unlit** (smouldering); the first rest kindles them ("HEARTH KINDLED"). Resting = the `Rest` action (kneel 0.7 s, rest, stand 0.8 s); its effect lands at the start of the active phase. Death respawns at the last Hearth rested at, or the first Hearth if none. |
| 2026-11-14 | `Dead` never ends by itself (infinite recovery). Whoever owns revival (the Hearth's `DeathSequence` for the player) calls `CharacterAction::revive()`. Future enemies stay dead until a `WorldReset`. |
| 2026-11-14 | UI display font: PT Serif (ParaType Free Font License, `assets/fonts/PTSerif-LICENSE.txt`), for banners and YOU DIED. Body text uses Bevy's default font. |
| 2026-11-14 | Attack: tap = light, hold of 0.25 s or longer = heavy, light while airborne = jump attack (user request). Shift: tap = roll, hold = sprint. Space = jump. |

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
- **N7 (2026-11-14) — automated movement test:** `FLOWERBOX_AUTOPILOT=1 timeout -s INT 12 ./target/debug/flowerbox > log 2>&1` makes the player walk camera-forward (-Z from spawn) and jump every 2 s, and logs `player pos/vel/grounded/r` every 0.5 s (`src/debug.rs`, `input.rs`).
  - Expected result: speed 4.5 m/s, a jump of about 1.1 m, the player slides around the pillar at (0, -14), and it stops at r ≈ 17.65 (wall minus the capsule radius).
  - Extend the autopilot when later phases need scripted input (roll, attacks).
- **N8 (2026-11-14) — Phase 2 architecture:**
  - Raw input (leafwing `ActionState<Action>`) is turned into the `PlayerInput` component in `Update`: a camera-relative `move_dir`, a `jump_buffer` and a `dodge_held` timer. Shift held for 0.25 s or longer means sprint; a shorter tap is reserved for the roll.
  - The controller runs in `FixedUpdate` and only reads `PlayerInput`. It uses avian's `MoveAndSlide` system param (new in avian 0.8, in `avian3d::character_controller`). Ground is any hit with normal.y > 0.7.
  - The player has `RigidBody::Kinematic` and a capsule (radius 0.35, length 1.1), plus `TransformInterpolation` for smooth rendering. The camera runs in `PostUpdate` before `TransformSystems::Propagate`.
  - The model is a child `PlayerModel` whose origin is at the feet and which faces -Z. `RightHandSocket` and `LeftHandSocket` are where weapons attach (for Phase 7). The facing angle is `CharacterController::facing`.
  - Cursor: grabbed at startup, Esc releases it, and a left click grabs it again. **Phase 5:** the attack system must ignore the left click that re-grabs the cursor, and should only fire while the cursor is grabbed.
- **N9 (2026-11-14) — Phase 3 action system** (`src/player/action.rs`, `stats.rs`):
  - **Locomotion** (idle/walk/sprint/airborne) means `PlayerAction.current == None`. Every other state is an `ActionKind` with an `ActionDef` (startup/active/recovery, a `cancel` time, stamina cost, an i-frame window, movement mode and ground rule), defined in `action_def()`. The numbers live in `tuning.rs`. Roll, Backstep, Light(n), Heavy, JumpAttack, DrinkAle, Hurt and Dead are already defined.
  - **Flow:** input calls `PlayerInput::buffer(ActionRequest)`, which stays valid for 0.2 s. `resolve_request` (pure, unit-tested) checks the cancel window, ground rule, stamina and the light combo index. `start_buffered_action` spends stamina and locks in the direction. `tick_action` advances time and inserts or removes `Invulnerable`.
  - **Movement modes:** Free, Slowed(f), Locked and Scripted(speed fn). The controller obeys them. Jump and sprint only work with no action running.
  - **Damage:** `DamageMessage { target, amount }`, sent with `MessageWriter`. It's ignored while `Invulnerable`. Otherwise the target goes to Hurt, or to Dead at 0 HP, which nothing can interrupt. Dead currently respawns at `PLAYER_SPAWN` after 3 s (Phase 6 replaces this with the Hearth).
  - **FixedUpdate order:** `PlayerSystems::{Damage, Actions, Stats, Movement}`, chained.
  - **What's left for Phase 4:** bind Shift tap to `ActionRequest::Roll` (release under 0.25 s, before sprint starts); it becomes a Backstep automatically without movement input and add a visual for the roll. **Phase 5:** bind LMB tap/hold and the airborne attack, and add swing visuals and hitboxes. **Phase 6:** bind R to DrinkAle, which starts healing in the active phase.
  - **Debug:** F3 toggles the overlay. In debug builds, H deals 15 damage and 1–4 request Roll (a backstep when standing still), Light (a jump attack in the air), Heavy and Drink. Autopilot now also runs a timeline: roll → a hit during the roll is ignored → a hit while free (Hurt, HP 90) → light combo 0,1,2 → jump attack (a light attack just after the t=6 s jump) → lethal hit at 7.5 s → respawn → backstep at 11 s (standing still); run it for 13 s. Grep the log for `action`, `autopilot` and `ignored`.
  - The default UI font has no "—" glyph, so use ASCII in UI text.
- **N10 (2026-11-14) — fixes from user feedback:**
  - **Jump attack:** there's no separate request for it any more. `ActionRequest::LightAttack` resolves to `ActionKind::JumpAttack` when the player isn't grounded (`resolve_request`). `HeavyAttack` in the air is rejected.
  - **"Airborne" while walking (bug):** grounding used to come only from move-and-slide hits. While walking, the sweep is mostly horizontal and often misses the floor, so `grounded` flickered false. Now a downward **ground probe** (`cast_shape`, 0.08 m) runs after moving. It's skipped while rising. It snaps the capsule onto the floor, keeping a 0.01 m skin gap.
  - **Backstep:** there's no `ActionRequest::Backstep` any more. `ActionRequest::Roll` resolves to `ActionKind::Backstep` when there's no movement input (`resolve_request` takes a `moving` flag). A backstep goes against the facing direction and is also ground-only.
  - The autopilot log now prints `airborne_ticks` per 0.5 s window. Walking on flat floor should show 0. Expect about 32 for a jump window, and a few when stepping over rubble near spawn.
- **N11 (2026-11-14) — Phase 4 (dodge roll):**
  - **Input:** Shift *released* after less than 0.25 s (`input::is_dodge_tap`) buffers `ActionRequest::Roll`. Holding longer sprints, and releasing then does nothing (rolling happens on release, as in the Souls games).
  - **On start:** the character turns instantly toward the action direction, for rolls and later for attacks too. A backstep keeps its facing and moves backwards.
  - **Measured by autopilot:** the roll covers 3.6 m (target 3.5) at 8.5 m/s during the active window, and the backstep covers 1.7 m (target 1.6).
  - **Animation:** `player/visuals.rs` does a procedural somersault. `model_pose()` returns a rotation, scale and lift that are applied around a pivot 0.9 m up. It does one forward flip with a tuck (scale 0.7) and a dip, finishing at about 0.5 s. The backstep leans back and hops a little. Phase 5 attack swings should go in this same file and function.
  - **Debug:** while F3 is on, a cyan ring at the feet shows i-frames. `FLOWERBOX_SHOT_TIME=secs` takes the screenshot at a given game time (for example 1.2 = mid-roll in the autopilot timeline). The autopilot now walks along world -Z regardless of camera yaw, so `FLOWERBOX_CAM="1.57,0.2,5"` gives a side view. It stands still from 10.8 to 11.6 s for the backstep test.
  - **Watch in Phase 6:** the placeholder death respawn teleports the player while `TransformInterpolation` is on, so the model may smear across the arena for one tick. When doing the Hearth respawn, check this and reset the interpolation if needed. **Resolved in Phase 6 (N17):** the respawn now teleports in `Update` while the screen is black. bevy_transform_interpolation resets easing when `Transform` changes outside the fixed schedules, so there's no smear.
- **N12 (2026-11-14) — Phase 5 (combat):**
  - **Attack input** (`input::AttackTracker`, unit-tested):
    - The press must start while the cursor is grabbed, so the click that re-grabs it never attacks.
    - Released under 0.25 s buffers `LightAttack`, which becomes `JumpAttack` in the air.
    - Held for 0.25 s buffers `HeavyAttack`, which becomes `HeavyCharge`. That costs 30 stamina, slows movement to 0.3×, and can be cancelled by a roll.
    - `release_heavy_charge` turns the charge into `HeavyAttack` on release, or automatically after 0.65 s. `charge` (0..1) scales damage from 1.6× to 2.0×.
  - **Jump attack:**
    - Movement mode `Plunge`: hover during the 0.2 s startup, then dive at 14 m/s.
    - Landing skips ahead to a 0.1 s slam window, but only after the 0.12 s downward swing has finished. Skipping mid-swing used to skip its hit samples too, which made hits random; that was a bug, now fixed.
    - There's no area-of-effect slam yet. It's a possible extra.
  - **Poses** (`player/pose.rs`, unit-tested): `arm_pose(action)` returns the shoulder and grip rotations for every action. The swings are: light 0 right→left, light 1 left→right, light 2 overhead, heavy overhead from the raised charge, and the jump attack downward. `animate_arm` applies them to `RightShoulder`/`RightHandSocket`, and `combat::blade_world` uses the same pose for the hitbox. The right arm is now a shoulder pivot with the arm, hand and sword socket as children.
  - **Hits** (`combat.rs`, `PlayerSystems::Combat`, after Actions):
    - During the active phase, the blade capsule (radius 0.12) is sampled every ~4 ms of action time between the last tick and now, and `shape_intersections` checks it against colliders that have `Hurtbox`.
    - The hit list is per `PlayerAction::serial`. `start()` increments it, so every action, including the heavy release, is a new swing.
    - Damage is `WEAPON_BASE_DAMAGE` (20, a placeholder until Phase 7 adds weapon stats) × the multiplier.
  - **Damage pipeline:** `DamageMessage { target, amount, from }` → `stats::apply_damage`, which works on any `Health` (players also go to Hurt or Dead) → `DamageTaken { killed, .. }`. Feedback reads `DamageTaken`: hitstop (virtual time × 0.05 for 60 or 100 ms of real time) and camera shake (trauma, decayed in real time).
  - **Dummy:** 120 HP, at the arena center (`DUMMY_POSITION`). It flashes, wobbles on a spring away from the hit, and breaks by falling away from the killing blow. It resets after 3 s.
  - **Debug:**
    - With F3 on, the blade hitbox is drawn in red during active frames. The overlay shows LMB held time, charge % and dummy HP.
    - `FLOWERBOX_AUTOPILOT=combat` places the player 1.5 m in front of the dummy and scripts the attack button. Expected over 13 s: light ×3 (20 each) → heavy (about 36) → jump attack (36, breaks the dummy) → reset → full heavy (40) → light ×2. Grep for `hit `, `dummy` and `action`.
    - The default autopilot (`=1`) now walks into the dummy, and its combo hits it.
- **N13 (2026-11-14) — humanoid and character interface (user request):**
  - **Adding a humanoid** (an enemy, say): spawn an entity with `character_bundle(transform, hp, stamina)`, then `spawn_humanoid(&mut commands, meshes, materials, entity, &HumanoidSpec, root_height)`. Insert the returned `HumanoidRig`, the `HumanoidSpec` and a `HumanoidAnimator`, plus a `Weapon` if it attacks and a `Hurtbox` if it can be hit. Drive it by writing `MoveIntent { dir, sprint, jump_buffer }` and `ActionIntent::buffer(ActionRequest)`. `PlayerInput`/`gather_player_input` is just the player's writer.
  - **Body:** `HumanoidSpec` holds proportions and colours (`HumanoidSpec::human()` = the player). The bones are `Pelvis → Chest → Head`, `Chest → UpperArm → Forearm → Hand` (L/R), and `Pelvis → Thigh → Shin → Foot` (L/R). Limbs hang along local -Y. +X rotation swings a limb forward; on the shin, -X bends the knee. The weapon is a child of `HandR`, with the blade along +Y.
  - **Animation** (`humanoid/anim.rs`): `locomotion_pose(LocomotionInput)` (the gait phase advances with distance travelled, walk↔run blends by speed, plus air/landing layers) and `action_pose(&ActiveAction) -> (pose, Full|UpperBody)`. The animator blends the action layer in over 0.06 s and out over 0.12 s.
  - **Hitbox** = `action_pose` + FK (`bone_character_transform`), so whatever the pose functions do, the hitbox follows. Unit tests check that the swings pass in front, the overheads come down in front, the guard is forward-down, the feet alternate when walking, and the roll stays above the floor.
  - **Movement additions:**
    - The `Scripted` lunges (`lunge_speed`, a sine bell) give light attacks 0.45 m and heavy attacks 0.7 m of forward motion.
    - `RunningAttack`: a light attack while `ctrl.sprinting` (and not mid-action) gives a 3.2 m leap, `ActionDef::hop` = 3.2 m/s upward, and 1.3× damage.
    - In the air, the target speed is at least your current horizontal speed, so momentum carries. `Plunge` keeps the horizontal velocity.
  - **Bevy quirk (workaround):** frustum culling hid the posed humanoid meshes mid-roll even though their transforms were correct (verified at runtime). All humanoid and weapon meshes therefore get `NoFrustumCulling`. The root cause hasn't been found; it may be Bevy visibility caching combined with child transforms that change every frame. Revisit with real models.
  - **The combat autopilot now also tests** a running attack (sprinting from z=4.4 at 12.7 s, tap at 13.15 s, 26 damage) and a walking jump attack (walking from z=4 and jumping at 14.7 s, 36 damage, breaks the dummy). Run it for 17 s.
- **N14 (2026-11-14) — possible improvement:** small rubble (for example the stone at (-0.5, 4.98)) lifts the capsule so it's briefly airborne, so a sprint attack over it becomes a jump attack. Consider a step-up/step-offset in the controller, treating short drops (under about 0.25 m) as grounded.
- **N15 (2026-11-14) — weapon classes (user request):**
  - **A class** (`WeaponClass`, `weapon/<class>.rs`) defines a `Moveset`:
    - `light: &[AttackSpec]`, the combo; its length is the combo length.
    - `heavy` plus charge max, cost and full-charge multiplier.
    - `running` plus `running_hop`.
    - `jump` plus `jump_strike_time`.
    - `guard: GuardSpec`, the idle arm/grip pose.
    - Each `AttackSpec` has the timings, cancel time, stamina cost, `lunge` distance, `damage_mult`, `windup`/`strike` `SwingKey`s (chest yaw/pitch, arm yaw/pitch, where pitch 0 = down, π/2 = forward, π = up) and a `Stance` (Lunge/Leap/Tucked).
  - **How it's used:** `ActiveAction` stores the class held when it started (`weapon`). `action_def(kind, class)`, `ActiveAction::attack()/damage_multiplier()`, `humanoid::anim::action_pose` (swing keys, stance, guard) and `combat::blade_world` all read it, so the timing, motion, animation and hitbox change together.
  - **Movement curves** are data (`MotionCurve::Dash/Lunge`) instead of fn pointers, so classes can tune them. Attack lunges run from 30% of startup to the end of active.
  - **Adding a class:**
    1. Add the variant and include it in `WeaponClass::ALL`.
    2. Copy `light_sword.rs` and tune it.
    3. Add a match arm in `moveset()`.

    The tests in `weapon/mod.rs` and `humanoid/anim.rs` loop over `ALL`. They check sane timings, that lunges cover their distance, that heavy damage scales, and that the strikes reach in front. A two-hander will also want the left hand on the grip: extend `GuardSpec`/`SwingKey` with off-hand data, or add an IK step, when that class gets built.
  - **Weapon definitions** (`WeaponDef`, static, for example `light_sword::LONGSWORD`) hold the name, class, `base_damage`, `blade: (start, end)` along the hand's +Y (the hitbox) and a `ModelSource` (`Parts(&[WeaponPart])` primitives, or `Gltf { path, transform }`). Equip with `EquipWeapon { character, def }`: the old model is despawned and the new one goes under `HandR`. The player starts with the Longsword through this message. Phase 7 items should reference `&'static WeaponDef`s.
  - The combat autopilot now ends with an F6-style swap to the Rusted Shortsword at 16 s and a light hit for 16 at about 18.7 s. Run it for 22 s: hitstops slow game time, so it lags behind real time.
- **N16 (2026-11-14) — wall stutter and the new gait (user request):**
  - **Cause:** pressed against the wall, `grounded` flickered almost every tick (logged with `FLOWERBOX_AUTOPILOT=wall`), so the air pose blended in and the landing crouch retriggered. There were three causes:
    1. `cast_shape` starting in contact with the wall returned no hit.
    2. A cast starting in contact with the floor can return a bad normal.
    3. `move_and_slide` sometimes returns a tiny +y velocity in the wall/floor crease, which skipped the probe.
  - **Fix** (`character/controller.rs`): the probe is now a sphere 0.8× the capsule radius, centred on the bottom hemisphere and lifted 5 cm (`GROUND_PROBE_*`). It's gated on the *intended* vertical velocity. The probe uses `PLAYER_RADIUS`/`PLAYER_CAPSULE_LENGTH`; make it per-character when enemies get other sizes.
  - **Animator:** 0.1 s air grace (`AIR_GRACE`), so a lost tick never shows. Speed comes from `ctrl.moved`. `blocked` = `wish_speed > 1` and actual speed < 35 % of it; this forces a slow-walk gait at 1.1 m/s with the free hand braced forward.
  - **Gait** (`humanoid/anim.rs`): the `Gait` params `SLOW_WALK`/`JOG`/`SPRINT` are blended by speed. `leg_cycle(u)` handles stance and swing with a duty factor (< 0.5 gives flight phases plus a bounce). Foot = knee − thigh keeps the feet level. Planting shifts the pelvis so the lowest heel or toe (`HumanoidSpec::sole_points`) touches the floor. Stride length comes from the gait. Tests: feet stay planted at every speed and phase, the sprint trails the blade, the braced hand reaches forward.
  - **`FLOWERBOX_AUTOPILOT=wall`:** starts at (-1.5, -4) in a lane clear of rubble. It sprints, jogs into the wall (contact at ≈ 4.75 s), pushes, then slides diagonally along it, logging every fixed tick. Expect `grounded=false` never to appear. For side-view shots use `FLOWERBOX_CAM="1.5708,-0.05,5.0"`, and run at most 2 instances in parallel or the shots time out.
- **N17 (2026-11-14) — Phase 6 (Healing Ale, Hearth, death, HUD):**
  - **Healing Ale** (`healing.rs`):
    - `HealingAle { charges, max }` lives on the player.
    - `drink_ale` runs in FixedUpdate between Actions and Stats. When `DrinkAle` enters Active, it uses the charge once per action serial and heals 40 % of max HP over 0.5 s. It only writes `Health` while healing, so `Changed<Health>` stays meaningful.
    - The tankard prop is a child of `HandL` and only visible while drinking.
  - **Hearth** (`hearth.rs`):
    - `Hearth { kindled }` is at `HEARTH_POSITION` (0, 12), in front of the pillar behind the spawn. It has a static collider (r 0.75) and a flickering fire; unlit, it only smoulders.
    - `hearth_interaction` (Update) sets `InteractPrompt` within `HEARTH_RANGE`, when grounded and with no action. E sets `PlayerInput::interact`, which is consumed here and turned into `ActionRequest::Rest`.
    - `rest_at_hearth` (FixedUpdate) turns the player toward the fire. At the start of Rest's active phase it restores everything, sets `RespawnPoint`, writes `WorldReset` (the dummy resets) and, the first time, a `Banner("HEARTH KINDLED")`.
  - **Death:**
    - `DeathSequence { t }` is added when the player is Dead. `death_screen(t)` gives the text and black alphas: the text from 1.1 s, black from 3.6 s, and the respawn at 4.5 s, while fully black.
    - On respawn the player teleports to `RespawnPoint`, or to the Hearth's `respawn_spot`: 1.6 m toward the centre, facing it. Then `revive()`, replenish and `WorldReset` run, followed by a 0.9 s fade-in.
  - **HUD** (`ui/hud.rs`):
    - The HP bar width scales with max HP (3.2 px per point). It has a lagging amber "damage chunk" that holds 0.7 s, then drains.
    - Stamina bar, plus an Ale icon and count (greyed out at 0).
    - Quadrant slots: top = spells, left = left hand (empty), right = a sword icon and the weapon name, bottom = Ale ×N.
    - Banner, YOU DIED band (PT Serif) and a full-screen black fade.
  - **Assets:** `cargo run` finds `assets/`. When running `./target/debug/flowerbox` directly (autopilots, screenshots), set `BEVY_ASSET_ROOT=.`, or the display font is missing.
  - **`FLOWERBOX_AUTOPILOT=hearth`** (run for about 24 s) gives this expected log:
    1. damage 50, then drink: HP 90 and "2 left".
    2. A drink interrupted by a hit keeps 2 charges.
    3. Two more drinks leave 0. The fifth drink logs "the tankard is empty".
    4. Damage 40, walk to the Hearth, E: "hearth kindled" and "rested…", HP 100, Ale 3/3.
    5. Lethal damage: "YOU DIED", then "respawned at [0, 0.91, 10.40]" with full HP and Ale.
  - **Default autopilot:** death at 7.5 s, respawn at the Hearth at about 12 s, backstep test at 13.5 s (standing still 13.3–14.1 s).
  - In this environment the log fills with audio (rodio/ALSA) errors; they're harmless, so filter them out.
