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
| Camera | **Mouse** | Third-person orbit camera; the cursor is locked during gameplay. Gamepad: right stick |
| Jump | **Space** | |
| Dodge roll / sprint | **Left Shift** | Tap to roll (i-frames). Hold to sprint (familiar element). Rolling while standing still (no WASD input) does a backstep. There's no separate backstep input. |
| Attack (right hand) | **Left Mouse** | **Tap** (released within 0.25 s) = light attack. **Hold** (0.25 s or longer) = heavy attack, released when charged or automatically at full charge. **Tap while airborne** = jump attack (the jump attack is the airborne light attack). A heavy attack can't be done in the air. |
| Left hand | **Right Mouse** | With a left-hand **weapon**: its attacks (tap light, hold heavy, airborne jump attack); with the **same weapon class** in both hands: the **dual attack** (both at once, 1.5× stamina). With a shield (or a weapon held in both hands): block / parry — |
| Two-hand | **Mouse wheel (hold) + Left / Right Mouse** | Holds that hand's weapon in both hands (+20 % damage, the other item put away, guard with the weapon); again: one hand. The wheel tapped alone locks on |
| Weapon ability | **F** (gamepad L2) | The main weapon's ability: swords *Lunging Thrust*, axes *Warcry* (+25 % damage, 10 s) |
| Block / parry | **Right Mouse** (hold, with a shield) | Needs a shield in the left hand. Raising takes 0.3 s; the last third of the raise is the parry window. Blocks frontal hits only (±57°, `GUARD_ANGLE`; hits without a direction aren't blockable); costs stamina, breaks when stamina runs out. Slower walking, no sprint |
| Drink Healing Ale | **R** | |
| Interact / rest at Hearth | **E** | Resting is a state: you stay kneeling; **E again or moving** stands you up. Online the rest completes only when every player rests |
| Lock-on target | **Q**, or tap the mouse wheel | Locks the target nearest the view centre (≤15 m, ±63°); the camera follows it, you face it and strafe (the hips turn toward where you walk), attacks aim at it. Again = release. Nothing to lock = camera recentres behind you. **Switch target:** flick the mouse or push the right stick left / right |
| Gamepad | Souls-like | Left stick move, right stick camera, **R1** attack (tap/hold), **L1** block, **B/Circle** roll/sprint, **A/Cross** jump, **X/Square** drink, **Y/Triangle** interact (tap; **hold Y + R1 / L1** two-hands that weapon), **R3** lock-on, **Start** inventory (inside: D-pad, L1/R1 filter, A select / options, X unequip, B back) |
| Inventory / Equipment | **Tab** or **I** | Blocks gameplay input (the world keeps running). **Slot first:** Left from the grid's first column (or click a slot) walks the equipment column; Enter on a slot lists only what fits it, Enter equips into exactly that slot ("Nothing in the bag fits this slot" otherwise); X / Backspace empties it. **Item first:** Enter / double-click / right-click on an item opens its options (Drink / Eat, Equip in right / left hand, Equip, Unequip, **Throw out** — it lands in front of you, anyone can take it). Q/E filter, L puts a weapon in the left hand, Esc / Backspace go back one step, Tab/I close |
| Online co-op | **O** | Online panel: **H** host (code copied to the clipboard), **J** join with the code in the clipboard, **C** copy code, **L** leave. Command line: `--host`, `--join <code>` |
| Use an item | Inventory: the item's options → Drink / Eat | Healing Ale = drink, Poppy = PvP on, Camomile = PvP off, Hop Cone = +1 Ale (short eating action) |
| Chat | **T** (rebindable) | A line opens at the bottom left (gameplay input blocked while typing); **Enter** sends, **Esc** throws it away. Messages appear in the notification feed: the speaker's little face (their tunic colour) and "Name: text", fading after 5–12 s by length. Online everyone sees them |
| Menu | **Esc** (gamepad: Select) | Resume / Settings (**name**: Enter, type, Enter; **skin**: Left / Right, the body changes at once; sensitivity, invert Y, stick speed, volume, notification corner) / Controls (rebind) / Delete save / Quit. The world keeps running |

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
- **Equipment slots:** Right Hand, Left Hand, Head, Chest, Hands, Legs, Ring ×2. The Healing Ale gets a quick-item slot. Which item goes where is decided by its kind in `Equipment::equip` (`items/inventory.rs`): Weapon → right hand, Shield → left hand, Armor(slot) → that slot, Ring → a free ring slot, Consumable → quick slot.
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
   ├─ textures.rs       # procedural textures (bricks, flagstones, mail, cloth, runes, flesh, bone, glass, rust, leather, marble) + tiled_box
   ├─ settings.rs       # Settings (camera, volume, notification corner, key bindings) ↔ settings.ron
   ├─ save.rs           # autosave / load (bag, Ale, sunflower, respawn point, taken items) ↔ save.ron
   ├─ sound.rs          # synthesised sounds (one-shots + loops), spatial playback triggers
   ├─ states.rs         # AppState (Loading, InGame, Inventory, Paused, Dead)
   ├─ input.rs          # leafwing actions, tap/hold detection, input buffer
   ├─ camera.rs         # third-person orbit camera, collision pull-in, lock-on (later)
   ├─ arena.rs          # circular dungeon, pillars, torches, lighting, fog
   ├─ character/       # GENERIC: anything that moves/acts/takes damage (player, future enemies)
   │  ├─ mod.rs         # CharacterPlugin, CharacterSystems, MoveIntent, ActionIntent, character_bundle()
   │  ├─ controller.rs  # kinematic movement from MoveIntent, gravity, grounding, jump, air momentum
   │  ├─ action.rs      # state machine: roll, attacks, drinking, timings, lunges, running attack
   │  └─ stats.rs       # Health, Stamina, Poise, regen, Invulnerable, DamageMessage/DamageTaken
   │                    # (mod.rs: BodySize capsules, Dormant, SolidExt::set_solid)
   ├─ quadruped/       # GENERIC: four-legged bodies (QBone, QuadrupedSpec::warg, pose + FK, gaits, action poses)
   ├─ grab.rs           # creature grabs: Pounce catches players, carry, unblockable bite, release
   ├─ humanoid/        # GENERIC: reusable humanoid body + shared animations
   │  ├─ mod.rs         # Bone enum, HumanoidSpec (proportions/look/textures/extra Parts, scaled()), spawn_humanoid(), HumanoidRig
   │  ├─ pose.rs        # HumanoidPose (+ blending/masks), forward kinematics
   │  └─ anim.rs        # locomotion (idle/walk/run/jump/land) + action poses, HumanoidAnimator
   ├─ player/
   │  └─ mod.rs         # Player = character_bundle + HumanoidSpec::human() + starting inventory + input
   ├─ ability.rs        # abilities (AbilityId → AbilityDef: strike, buff, spin, slam, shriek), AttackBuff, shockwaves
   ├─ weapon/          # weapon CLASSES (movesets + swing animations) and weapon DEFINITIONS (models)
   │  ├─ axe.rs         # Axe moveset (vertical chops) + Rusted Axe / Woodcutter's Hatchet
   │  ├─ claws.rs       # Claws (natural, humanoid): the Mourner's raking flurries
   │  ├─ greatsword.rs  # Greatsword: slow wide sweeps; Penitence (player) / the Saint's blade
   │  ├─ mod.rs         # WeaponClass, Moveset/AttackSpec/SwingKey/GuardSpec, WeaponDef, ModelSource, EquipWeapon
   │  ├─ light_sword.rs # LightSword moveset + Longsword / Rusted Shortsword / Rusted Axe / Fists
   │  └─ hollow_blade.rs # HollowBlade moveset (long wind-ups) + the Hollow's blade
   ├─ combat.rs         # blade hitbox via FK (uses the equipped WeaponDef), hitstop, camera shake, dummy
   ├─ debug.rs          # F3 overlay, debug keys, screenshots, autopilot scripts
   ├─ interact.rs       # InteractPrompt + InteractSystems order (pickups before the Hearth)
   ├─ lockon.rs         # LockOn, Targetable, pick_target, MoveIntent::face while locked
   ├─ pvp.rs            # Pvp flag, UseItem effects (Poppy/Camomile), PvP marker + HUD, death/respawn rules
   ├─ net/              # online co-op over iroh (N21)
   │  ├─ mod.rs         # NetPlugin, NetSession (roles), join codes, host/join/leave, event pump
   │  ├─ transport.rs   # network thread: iroh endpoint on tokio, channels to Bevy
   │  ├─ protocol.rs    # BodyState, Datagram (state/snapshot), Reliable (hello/welcome/damage/parry)
   │  ├─ sync.rs        # send states, puppets (Remote), forwarding hits/parries, parry stun routing
   │  ├─ world.rs       # the host's world: world state for joiners, pickup/sunflower requests, events → notifications
   │  └─ ui.rs          # online panel (O) + status corner
   ├─ enemy/
   │  ├─ mod.rs         # Enemy, EnemyKind, spawn points, death/corpses, drops, reset
   │  ├─ kinds.rs       # EnemyDef per kind (body, capsule, HP, poise, weapon, brain, boss); spawn_enemy, attach_body
   │  ├─ ai.rs          # shared brain helpers: Senses (target, sight, gaze), Rng, Steering, chain_combo
   │  ├─ fighter.rs     # humanoid brain (FighterProfile): combos, specials, stalker, enrage
   │  ├─ beast.rs       # four-legged brain (BeastProfile): chase, bite combos, pounce, circling
   │  ├─ boss.rs        # boss bar, fog gate, defeat banner, DefeatedBosses
   │  ├─ dormancy.rs    # enemies far from every player sleep (no AI/movement/animation/drawing)
   │  └─ warg.rs        # the warg's beast profile
   ├─ items/
   │  ├─ mod.rs         # ItemId → ItemDef registry (kind, icon, weight, absorption, block, HP bonus, weapon, model)
   │  ├─ inventory.rs   # Inventory, Equipment, EquipSlot, EquipRequest, EquipStats; weapon/model sync
   │  ├─ models.rs      # primitive shield/armour models on bones
   │  └─ pickup.rs      # glowing world pickups (E), ItemPickedUp
   ├─ healing.rs        # HealingAle charges, swallow + heal over time, tankard prop
   ├─ hearth.rs         # Hearth (kindle/rest), RespawnPoint, WorldReset, DeathSequence → respawn
   ├─ level/            # levels from data files (assets/levels/*.ron)
   │  ├─ mod.rs         # LevelPlugin: LEVELS (lvl1 north, lvl2 south of the arena), Levels; spawn floors/ceilings/walls/stairs/torches/sunflowers/spawn points per theme; line_of_sight
   │  ├─ layout.rs      # file format + pure geometry: grid rasterisation, wall runs, doors, stairs (unit-tested)
   │  ├─ door.rs        # Door: one-way + locked (key) + sealed + archways, swing, blocker collider; DoorRequest / DoorOpened / DoorRefused
   │  ├─ props.rs       # forgery props: anvil, forge (fire, hood, trough), sky grate (bars, sunbeam, dust), steam
   │  ├─ gothic.rs      # cathedral props: columns, stained glass, pews, altar, statue, bells, lectern, bones, candles, chandeliers, sarcophagi, fountain, fog gate
   │  └─ riddle.rs      # bells + lectern; ring in order to unseal a door (owner judges)
   └─ ui/
      ├─ mod.rs         # UiPlugin, Banner message, display font path
      ├─ hud.rs         # bars, Ale, equipment quadrant, prompt, banner, death band + fades
      ├─ icons.rs       # item icons drawn from UI nodes
      ├─ inventory_ui.rs # Tab/I screen: equipment, filters + grid, details, status preview
      ├─ markers.rs     # lock-on dot and enemy health bars (world → screen)
      ├─ notifications.rs # notification feed (Notify, NotificationSettings)
      ├─ damage_numbers.rs # numbers popping out of the dummy
      ├─ pause_menu.rs  # Esc menu: settings, controls (rebinding), delete save, quit
      └─ strings.rs     # all player-facing wording (COMPOSTED, SUN VIGORATED, prompts)
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
- [x] Future classes: add a variant plus `weapon/<class>.rs` (see N15) — done for Axe (8.12), Claws and Greatsword (8.13); more (daggers, spears) the same way

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
- [x] Item registry (in code first, `assets/items.ron` later), `Inventory`, `Equipment`
- [x] Equip/Unequip events, plus stat recalculation (damage, defense, equip load)
- [x] Inventory UI on Tab: grid, filters, details panel, equip/unequip, pauses gameplay input
- [x] Swap the visual weapon or shield in hand when equipment changes (also the hood)
- [x] World pickups (E to pick up) with a "Picked up: X" toast
- [ ] `assets/items.ron` instead of the code registry (when there are many items)
- [x] Chest/hand/leg armour items and models: Penitent's Hauberk, Ashen Gauntlets, Grave Greaves (8.13; paired models for both arms/legs)

### Phase 8 — Familiar extras (after the core works)
- [x] Lock-on (Q): the camera focuses the target and movement strafes around it (attacks aim at it; Q with nothing in view recentres the camera)
- [x] Block with a shield (RMB), which uses stamina (+ guard break, frontal arc only, slowed movement)
- [x] Parry window in the last third of raising the shield: negates the hit, sends `Parried` (user request, 2026-11-15)
- [x] Parry **stuns the attacker** (`ActionKind::Stunned`, 1.8 s); the next hit on it is a riposte (×2.5)
- [x] Roll weight classes based on equip load (Light fast roll / Medium / Heavy / Overloaded fat roll)
- [x] Gamepad bindings (play + inventory screen; untested without a pad)
- [x] Simple enemy: the Hollow (melee AI, telegraphed `HollowBlade` moveset, health bar, dies, resets on rest/respawn)
- [x] Dual wielding / weapons in the left hand (N19) — done in 8.12
- [x] Strafe animation while locked on: hips turn toward the travel, chest stays on the target, backward = reversed cycle (8.13)
- [x] Switching lock-on targets (mouse flick / right stick) (8.13)
- [x] Poise: optional `Poise` component, hits wear it down, only a break staggers (enemies: warg, acolyte, horrors, boss; the player and Hollows still flinch every hit) (8.13)

### Phase 8.5 — Online co-op (user request, 2026-11-15)
- [x] Research: transports that need no game server (iroh, Steam, WebRTC/matchbox, plain QUIC) → iroh (N21)
- [x] Peer-to-peer connection with a join code (host / join), direct with relay fallback
- [x] Players see each other (puppets: movement, actions, guard, equipment, coloured tunics)
- [x] Host-owned enemies replicated to guests; guests' hits and parries sent to the host; enemy hits on a guest decided on the guest's machine
- [x] Online panel (`O`), status corner, `--host` / `--join <code>`, join code via clipboard
- [x] Shared world events: guests resting / dying, item pickups, dummy state (done in Phases 8.6 / 8.7)
- [ ] Interpolation buffer + lag compensation (puppets currently ease toward the last state)
- [x] Windows build for friends: cross-compiled with mingw (`x86_64-pc-windows-gnu`; only system DLLs, no console window in release); `tools/package.sh` builds Linux + Windows and packs each with `assets/` and a README into `dist/`
- [ ] Optional: Steam transport (lobbies, invites) via `bevy_replicon_renet` + `renet_steam` or `aeronet_steam`

### Phase 8.6 — Shared world, notifications, opt-in PvP (user request, 2026-11-15)
Cleaned-up request:
1. **Shared sunflower revival.** Reviving the sunflower is a world event. The player who did it gets the big "SUN VIGORATED" banner; everyone else gets a small notification ("Guest 1 vigorated the Sun" + sunflower icon), and the sunflower stands up on every screen.
2. **Notification feed (new UI).** Small pop-ups about what happened: you picked up an item, another player picked something up / vigorated the Sun / joined / left / died / turned PvP on or off. Each has a distinct icon, no background but readable (shadowed text, outlined icon), slides in from outside the window, sits in the bottom-right corner by default. Several stack with the newest on top, older ones slightly shifted so they stay legible; each fades after 3 s. Everything (corner, margins, lifetime, fade, slide time, stack shift, sizes, max shown) is a setting.
3. **The host's world is the world.** A guest adds nothing: no own enemies or items; spawning and item drops are handled by the host.
4. **Shared pickups.** Items lie in the host's world; whoever takes one first gets it, it disappears for everyone.
5. **World-centred enemy spawning.** Enemies come from spawn points in the world: a spawn point spawns its enemy when *any* player enters its area, once (not again for players who join later), synchronised to everyone. (Fixes the reported invisible enemy: there is now exactly one enemy, owned by the host.)
6. **Opt-in PvP.** Players don't hurt each other unless both opted in. *Poppy* (consumable) turns PvP on, *Camomile* turns it off. Dying turns it off; on respawn you get a Poppy again if you have none.
7. **Investigate:** invulnerability after being hit (answer: no i-frames, only a tiny hit immunity), plus the open questions below.

Decisions (2026-11-15, from looking at how Souls games do it and at our code):
- Player names: "Host", "Guest 1"…"Guest 3" (by slot) until there's a name setting.
- Only the **host's rest** resets the world (enemies respawn, as in Souls where summons can't rest). A guest can still rest: it heals them, refills their Ale and sets their respawn point, and if the sunflower isn't revived yet, it revives it for everyone.
- A world reset (host rest / host death) **removes** spawned enemies and re-arms the spawn points; they spawn again when someone comes near.
- Enemy spawn point "tutorial Hollow" at (-4, -10), area radius 12 m.
- Pickups and kindling go guest → host as requests; the host decides (first come, first served) and tells everyone. Late joiners get the world state (revived sunflower, taken items) on arrival.
- Consumables stack (counts) and are used from the inventory (Enter / "Use"), with a short "eat" action (hand to mouth), so using one in a fight is a risk like drinking. Start with Poppy ×1, Camomile ×1.
- PvP needs **both** players to have it on. ~~The attacker's machine detects the hit~~ → changed while building: the **victim's machine** detects the hit from the attacker's puppet, exactly like enemy hits (no damage routing; the victim blocks/parries what it sees). A parry is routed via the host to the parried player's machine, which stuns it (riposte possible). PvP players carry a red poppy marker over their head; you see your own state next to the HP bar.
- Post-hit invulnerability: Souls games don't have any (each swing hits once, combos can trap you, poise matters). We keep that, plus a **0.1 s hit immunity** for players only (`HIT_IMMUNITY`) so two hits landing in the same instant (two attackers, network bunching) don't stack unfairly.
- The training dummy stays local to each machine (it's practice furniture).

Steps:
- [x] Plan, decisions (this list)
- [x] Notification feed (`ui/notifications.rs`, `NotificationSettings`), the pickup toast becomes a notification (F7 / `FLOWERBOX_AUTOPILOT=notify` for samples)
- [x] Player names; join / leave / death notifications
- [x] World state on the host (`net/world.rs`): revived sunflower, taken pickups, sent to joiners; kindle + pickup requests from guests; banner for the actor, notification for others
- [x] Spawn points (world-centred, area trigger, once per world cycle); guests never spawn
- [x] Consumables: stack counts, "Use" from the inventory, `UseItem` action; Poppy + Camomile items and icons
- [x] PvP flag (replicated), marker, damage routing between players, parry stun between players, off on death, Poppy on respawn
- [x] Hit immunity (0.1 s, players)
- [x] Two-instance tests (autopilots) + screenshots; ROADMAP notes (N22)

### Phase 8.7 — Souls-style respawns, shared rest, synced dummy, corpses (user request, 2026-11-16)
Request, cleaned up:
1. Enemies don't spawn when someone walks near: they exist from the start and **all respawn when resting** at the sunflower (and when the world's owner dies), like in Souls games. (Also removes the reported invisible second Hollow: a guest who joined after the first one died had spawned its own.)
2. **Shared rest:** online, the rest only "happens" when **every** player rests at the sunflower. Each resting player shows "<name> waiting for other players at rest site n/N". When all are resting: enemies respawn, then everyone's HP, stamina and Ale are restored.
3. **Resting is a state:** kneel and stay. **E again or moving** stands you up.
4. **Synced training dummy** (host-owned): everyone sees its wobble, falling over and **damage numbers** popping out of it; harder hits push it further.
5. **Dead bodies don't block:** corpses (enemies, players) and the fallen dummy no longer have a solid capsule.

Decisions:
- Offline the rest completes at once (1/1). A world reset (rest or the owner's death) despawns every enemy (including corpses) and respawns all spawn points.
- The rest's effect is split: *starting* to rest (kneel) revives the sunflower and sets your respawn point; the *completed* rest (everyone resting) respawns enemies and restores everyone. Standing up is its own short action (`StandUp`).
- The dummy follows the same pattern as enemies: guests' hits are forwarded to the host; the host broadcasts every dummy hit (amount, direction, broken), so every machine animates it and shows the number. Late joiners get its HP.
- Damage numbers: screen-space text over the hit point that pops up, drifts and fades (dummy only for now).
- Dead/broken bodies get `ColliderDisabled` (their own movement still uses the shape, so they don't fall through the floor).

Steps:
- [x] Enemies spawn at world start, all respawn on world reset (no area trigger)
- [x] Rest as a state (`Rest` stays active; `StandUp` on E or movement); effects split into started / completed
- [x] Shared rest online (host counts resting players, waiting notifications, completion event)
- [x] Synced dummy (forwarded hits, broadcast hits, HP for joiners), stronger push for bigger hits, damage numbers
- [x] Dead bodies and the fallen dummy don't block movement
- [x] Two-instance tests, notes

### Phase 8.8 — Level 1 "the sunken halls" (user request, 2026-11-16; design: `assets/levels/lvl1.png`)
Plan (measured from the drawing; 1 px = 1/14 m; origin px (409.5, 1228) = world (0, -18), the arena's north wall):
- [x] Several sunflowers: `Hearth { id, kindled, revive }`, reusable `hearth::spawn_sunflower(id, pos, facing)`, per-flower animation, respawn spot from the flower's facing and floor height
- [x] Level data file `assets/levels/lvl1.ron` + generic builder (`src/level/`): rooms = rectangles with a floor height; walls generated on a 0.25 m grid wherever a room borders nothing or another room (except doors / stair ends); floors; solid blocks; stairs = ramp collider + visible steps, always going down away from the start; torches; arena wall opening + filler walls
- [x] Rooms (px rects): corridor y0 [(370,1055,449,1228),(341,995,451,1055)]; middle −1.5 [(502,765,718,967),(502,967,567,1053),(451,992,567,1053)] + U block [(571,826,594,890),(594,868,634,890),(634,826,652,890)]; upper halls −1.5 [(308,765,502,826),(308,673,405,765),(329,633,380,673),(263,561,462,633),(152,686,308,746),(152,626,211,746),(45,567,211,626),(45,567,101,732)]; vault −3.5 [(521,564,701,720)]; stairs S1 (451,1023,502,1053) corridor→middle east, S2 (405,676,521,718) upper→vault east
- [x] Doors (interact + swing animation, stay open, saved, host-synced): D1 corridor south wall at (409.5,1228) 2.6 m; D2 middle west wall at (502,789) 2.4 m
- [x] Enemies (px): (353,1041),(360,1012),(580,791),(671,841 patrol N–S),(332,806),(354,782),(171,710),(320,579),(393,580),(619,644); line-of-sight for aggro
- [x] Treasures: (351,1024) Poppy, (617,854) Hop Cone (new, +1 Ale), (357,713) Camomile, (284,619) Thornbriar Ring (new, armour ring), (449,616) Hop Cone, (70,713) Hop Cone; sunflower id 1 at (72,726) facing north
- [x] Sync/save per sunflower id and door id (protocol v5), pickups with 3D positions
- [x] Autopilot `level` (walks the whole level), `still` (screenshots), `FLOWERBOX_POS`; lock-on needs a clear view (N25)
- [x] Two-instance test: host (`menu` autopilot, `--host`) + guest (`level`, `--join`): host logs "Guest 1 opened door 1 / door 2 / picked up Hop Cone / vigorated the Sun"; the guest walks through the host-opened doors
- [ ] Enemy navigation: Hollows walk straight at you and sidestep when stuck; they can snag on door frames and stair sides (no pathfinding yet)

### Phase 8.9 — The warg: a quadruped enemy, grabs, player HP bars (user request, 2026-11-16)
Request: replace the north-east enemy (the vault's) with a **warg**: more aggressive AI, light attacks (bites), and a **grab**: one second leaning back, a 3 m leap that carries everyone in its way and grabs them at the landing spot, then a bite of at least 100 damage that can't be blocked (must be dodged). Build the four-legged creature like the humanoid module so later creatures reuse its body, movement and AI. Online: an HP bar over each other player's head, shown 3 s after it changes, then fading.
- [x] `src/quadruped/`: `QBone` skeleton, `QuadrupedSpec` (proportions + look, `warg()`), `spawn_quadruped`, `QuadrupedPose` + FK, animator with walk / trot / gallop gaits and action poses (bites, pounce, hurt, stagger, death)
- [x] Natural weapons: `WeaponClass::WargFangs` (`weapon/warg_fangs.rs`: 3-bite combo, `PounceSpec`), `EquippedWeapon::natural`; bite hitbox = a sphere swept along the snout (`combat::bite_hits`, shared hit rules `may_hit`)
- [x] Generic actions `Pounce` (wind-up → leap → bite and shake) and `Grabbed` (held, `ActionMovement::Held`); `grab.rs` (catch during the leap, carry, unblockable bite, release); leaping creatures pass through characters
- [x] `enemy/beast.rs`: reusable beast AI with a `BeastProfile`; `enemy/warg.rs` (profile, spawn); `EnemyKind` in level files (`kind: Warg`); the vault enemy is a warg
- [x] Online: `BodyKind::Warg` puppets (protocol v6); grabs decided on the victim's machine
- [x] HP bars over other players (online), 3 s + 0.6 s fade
- [x] Autopilots `warg` / `wargroll` (+ `FLOWERBOX_WARG_TANK`), two-instance test
- [x] Poise: decided — the warg has 30 poise, so one light hit no longer breaks its pounce wind-up; two hits (or a heavy) still save a partner (8.13)
- [x] Refactor the Hollow AI onto the shared helpers the beast AI uses (`enemy/ai.rs`, `enemy/fighter.rs`, 8.13)

### Phase 8.10 — The forgery sector, enemy drops, one-way and locked doors (user request, 2026-11-16; new plan in `assets/levels/lvl1.png`)
Request, cleaned up:
1. **Build the new northern sector** from the updated plan. The vault's warg drops a **key**. A **locked door** in the vault's north wall leads to a room with three Hollows. From there a corridor runs north, then west along the top: a NW nook with a Hollow, a peaked hall with a Hollow, a V-shaped dip with a Hollow. It then runs down the west side, past an alcove with a treasure, into the **forgery**: a second warg, three Hollows, the rare item and two treasures. A door at the bottom leads back into the T-room of the upper halls. There is also a new door between stairs 2 and the vault.
2. **Enemy drops:** enemies can drop items when killed (chance; unique drops only once).
3. **Doors collide when open** (reported: you can walk through an opened door).
4. **One-way doors:** a door only opens from the side it swings away from. From the other side you can still interact, but you get a short notification that it only opens from the other side; online, everyone sees which player tried it ("PlayerX tried a door: …"). The door looping back into the T-room therefore only opens from the forgery.
5. **The rare item** is a *Well-Forged Sword*: a longsword with slightly better stats.
6. **The forgery look:** an anvil, a forge with a fire pit, and iron bars in the ceiling where sunlight shines in and the forge's steam escapes.

Decisions:
- The plan has no stairs in the new sector, but the vault (−3.5 m) and the T-room (−1.5 m) differ by 2 m. **Stairs in the narrow corridor north of the three-Hollow room** climb to −1.5 m (northward). The forgery and the loop are at −1.5 m.
- Diagonal walls in the plan (NW nook, peaked hall, V dip) are approximated with stepped rectangles (the builder is axis-aligned).
- Doors: `opens` gives the swing direction; you can open a door only from the opposite side. Locked doors name their key item, which isn't used up. Keys are unique drops.
- Treasures in the new sector (unspecified in the plan): a Hop Cone in the alcove; a Poppy and a Camomile in the forgery.
- (While building) Stairs 3 start at the top hall's south edge (overlapping it left a wall stub); floor slabs are cut away under every stair (a slab reaching under a ramp stuck up as a step). Light shafts are dust motes in the sunbeam, not translucent slabs (those read as grey walls; real volumetric light needs a directional light, which would also light the arena).

Steps:
- [x] Doors: collide when open; one-way rule + wrong-side notification (local + online "PlayerX…"); locked doors (key) + "Locked" notice
- [x] Doors on stair ends (the stairs 2 → vault door)
- [x] Enemy drops: level data (`drops`), unique drops remembered (save), host-owned online (`WorldEvent::Dropped`), Forge Key item + icon
- [x] Well-Forged Sword (weapon + item + icon)
- [x] New rooms, stairs, doors, enemies (second warg), treasures, torches in `lvl1.ron`
- [x] Forgery props: anvil, forge + fire pit (flicker light), ceiling grate with bars, sunlight shaft, rising steam
- [x] Autopilot through the new sector, screenshots, two-instance check, notes

### Phase 8.11 — Multiplayer robustness, 5 players (user report, 2026-11-16)
Report: with 4 players enemies didn't appear for everyone, spawning was weird, respawning when someone died was a problem; don't reset the world when the host dies (for now). Test up to 5 players, network stability.
- [x] Found: the host's snapshot (every body in one datagram) had grown to ~1150 bytes with one guest; with more players it exceeded the ~1200-byte QUIC datagram limit and was silently dropped (send errors were ignored)
- [x] Snapshots split into ≤ 900-byte datagrams; enemies sent only when changed (+ 1 s keepalive); roster twice a second; puppets removed by roster / 3.5 s silence instead of "missing from one snapshot"
- [x] 5 players (`MAX_PLAYERS`), a fifth tunic colour; datagram send failures logged; QUIC idle timeout 10 s
- [x] No world reset on death online (host included); resting together still resets
- [x] Joining guests appear beside the host (a free spot on a ring), not at the arena start
- [x] `mp` autopilot (+ `FLOWERBOX_MP_FIGHT`, `FLOWERBOX_MP_DIE_AT`), 5-instance tests through the relay
- [ ] Test across real networks (different homes, NAT) with 4–5 players; consider a self-hosted iroh relay if n0's public relays throttle

### Phase 8.12 — Axe moveset, two-handing, dual wielding, weapon abilities, door animation, feral warg (user request, 2026-11-16)
Request, cleaned up:
1. **Bug:** with Shift+Tab, the inventory's mouse cursor isn't visible, or the game goes back to gameplay.
2. **Axe class:** its own animations and combo — slower, more vertical, like natural axe swings.
3. **Two-handing:** hold the mouse wheel button and press the button of the item you want to hold in both hands.
4. **Dual wielding:** a weapon in each hand, each attacking with its own button and animations (mirrored for the left). Never both at once — except: if both weapons are of the **same class** (two axes, two light swords), the left button does a **dual attack** with both weapons at the same time for 1.5× the stamina. Jump attacks too.
5. **Modular:** done for the sword and axe classes, but so a new weapon class only needs its moveset file. Also the possibility of an **activated ability**.
6. **Feral warg:** jagged, uneven fur/plate edges sticking out of the body.
7. **Door opening animation** for the player.
8. **Notifications** when a player opens a door or dies (reported as not showing — recheck).

Decisions:
- **Buttons (Souls convention):** left mouse = right hand, right mouse = left hand (the shield arm). The left-hand item decides what the right button does: shield → block/parry (as now); weapon → left-hand attacks (tap light, hold heavy, airborne jump attack); same class as the right weapon → the **dual** attack instead. Two-handing: **wheel + left button** two-hands the right weapon, **wheel + right button** the left one (again to return to one hand). The wheel alone (released without a combo) still locks on, so lock-on now triggers on *release*; Q still locks on at once. If you meant left button = left hand, it's a binding swap.
- **Two-handed:** the other hand's item is put away (hidden), the free hand grips the handle (IK); +20 % damage; the right button guards with the weapon (`WeaponDef::guard_block`, weaker than a shield). Two-handing a shield isn't allowed.
- **Dual attack:** both arms swing (the right swing + its mirror), each blade can hit once; 1.5 × stamina (`DUAL_STAMINA_MULT`). Light, heavy, running and jump variants.
- **Data over objects:** weapon classes stay data (a `Moveset` per class file); hands and two-handing are generic (`Hand`, `AttackHand`, the `Wielded` component replacing the single `EquippedWeapon`); poses for the left hand / dual are derived by **mirroring** the right-hand pose, so new classes get left and dual animations for free.
- **Abilities:** per weapon class (`Moveset::ability`), key **F** (gamepad L2). Data-driven `AbilityDef` with effects: a strike (its own swing) or a buff. Sword: *Lunging Thrust* (a long, hard thrust). Axe: *Warcry* (+25 % damage for 10 s).
- **Inventory:** Enter / double-click equips weapons to the right hand as before; **L / right-click** equips a weapon to the left hand.
- **A second axe** (Woodcutter's Hatchet, axe class) lies in the arena, so dual axes are possible.
- **Gamepad:** dual attacks and the ability work (L1 / L2). Two-handing isn't mapped on the gamepad yet (Souls uses Y, which is interact here).
- **Shift+Tab:** can't be reproduced with injected input (`FLOWERBOX_KEYS`); the game logic is correct. Likely the desktop/compositor (GNOME Wayland) and the locked pointer. Fix attempt: the cursor follows the menu state every frame and after the window regains focus; a toggle can't fire twice within 0.25 s. Needs your confirmation.
- **Notifications:** door opened by another player → "Guest 1 opened a door"; deaths already notify others — recheck on screen.

Steps:
- [x] Shift+Tab robustness (cursor follows the menu state, focus, debounce) + input harness `FLOWERBOX_KEYS`
- [x] Notifications: door opened by others; recheck deaths on screen
- [x] Player door opening animation (`ActionKind::OpenDoor`, the door opens at the push)
- [x] Feral warg (jagged plates)
- [x] Hands: `Hand` / `AttackHand`, `Wielded`, left-hand weapon equip + models, two-handing (state, IK grip, damage, weapon guard), inventory UI, save, net
- [x] Per-hand attacks: requests / actions with a hand, left attacks (mirrored poses), dual attacks (1.5× stamina), jump/heavy/running variants, hit detection per blade
- [x] Axe class: vertical, slower combo + heavy / running / jump; Woodcutter's Hatchet
- [x] Abilities: `AbilityDef`, F / L2, Lunging Thrust, Warcry
- [x] Tests, autopilot, screenshots, notes
- [ ] Shift+Tab: confirm on the real desktop (couldn't be reproduced here)
- [x] Gamepad two-handing: hold Y + R1 / L1; Y alone interacts on release (8.13)
- [x] Refactor the Hollow AI onto shared AI helpers (8.13)

### Phase 8.13 — Corpse fix, slot-first inventory, item options, the Cathedral of Ashes (user request, 2026-11-16)
Request, cleaned up:
1. **Bug:** a dead enemy still has a hitbox: walking into it shoves the corpse around, and once the body vanishes an invisible wall stays.
2. **Slot-first equipping:** walk the equipment slots (right hand, left hand, …); choosing one moves the selection into the bag showing everything that fits; Enter equips it there. Nothing fits → a small notification.
3. **Item options:** Enter on an item opens a small popup with every possibility: consume (consumables), equip left / right (weapons), equip / unequip, and **throw out** for every item — it lands in the world where other players can take it too.
4. **Roadmap:** do as much unchecked as possible (strafe, target switching, poise, armour pieces, gamepad two-handing, the AI refactor…). Redesign the player and weapons with "crazy" textures and forms (no outside assets).
5. **A big new level** in a dark gothic medieval-fantasy style, behind a door from the circular arena: humanoid and animal enemies, **two new horror enemies** (scary, hard, few), a **riddle** midway, sunflowers and items placed with intent, loops back, and a **boss** at the end (a third new enemy: AOE spins, dashes …).
6. **Modular:** reuse the humanoid AI refactor for the new enemies and the boss.
7. **Performance:** check whether many more enemies cause problems; idea: only render enemies behind an opened door or within 20 m of a player.

Decisions:
- **Corpses:** the vanished-body wall was avian's collider tree: re-inserting `ColliderDisabled` fires `Discard` in Bevy 0.20, whose avian observer re-adds the collider to the tree with a frozen box. `SolidExt::set_solid` toggles idempotently; dead characters' own move-and-slide ignores characters (no shoving).
- **Inventory:** Souls-style both ways (slot first, item first). Throwing out *removes* the bag entry (equipment indices shift: `Equipment::forget`). The Healing Ale (your flask) can't be thrown out; keys can. Online the host owns the world: a guest's throw is a request, the host spawns the pickup and tells everyone. Thrown items aren't saved (they're the world's, like enemy drops).
- **Enemies are data** (`EnemyDef`): one spawner and one puppet body for every kind; new kinds = a table entry. Special moves are **abilities** any character may perform (they already replicate as `ActionKind::Ability`): Whirlwind (spin, one hit per turn), Dash Strike, Ground Slam (shockwave), Shriek (unblockable wave), Rending Turn. Waves are victim-side like every enemy hit.
- **Horrors:** the **Mourner** (2.5 m veiled, eyeless stalker; freezes while you look at it, rushes when you don't; shriek, rending spin, dash; 280 HP, poise 60) and the **Contorted** (a pale body bent backwards on all fours, upside-down glowing grin, hair to the floor; fast skittering beast with a grab; 220 HP). Two of each in the whole level.
- **Boss — the Thorned Saint** (2.9 m, 1300 HP, poise 140; Souls boss conventions: telegraphed wind-ups, punish windows after big moves, a fog gate you can't leave, a big name bar, a second phase): combos, Whirlwind, Dash Strike from range, Ground Slam; at 50 % it shrieks into phase two (faster, shorter recoveries, Shriek and a wider Whirlwind). Drops *Penitence* (greatsword, ability Penitent Whirl). Stays dead (`DefeatedBosses`, saved, sent to joiners).
- **Level 2** is south of the arena (level 1 is north): narthex (sunflower) → nave + aisles → transept → ossuary / cloister (sunflower) / bell chapel → apse → sealed crypt → antechamber (sunflower) → fog → the Choir of Thorns. **Loop:** the bell chapel's gallery has a one-way door back into the nave. **Riddle:** the apse lectern names three bells in an order (bones → drowned → glass sun); ringing them in order unseals the crypt; a wrong bell restarts the rite. Plan: `assets/levels/lvl2.png` (generated from the file).
- **Several levels:** a list of level files, each its own grid with its own theme, ceiling and id range.
- **Performance — dormancy by distance** (30 m wake / 36 m sleep), not by doors: see N31.
- **Redesign without assets:** procedural textures generated at startup; body extras (`Part`s) and `Styled` weapon models instead of glTF.

Steps:
- [x] Corpse collider fix + autopilot check (vanished body: no deflection)
- [x] Inventory: slot focus + picking, options popup, throw out (+ online), `FLOWERBOX_KEYS` arrows
- [x] Shared AI (`ai.rs`), `FighterAi` + profiles, beast refactor; `EnemyDef` table; `BodyKind::Enemy(kind)` (protocol v11)
- [x] Abilities: spin, slam, shriek, specials; poise; `BodySize`; scaled/textured/decorated bodies; `Styled` weapons; claws + greatsword
- [x] The Mourner, the Contorted, the Ashen Acolyte, the Thorned Saint; boss bar, fog gate, defeat
- [x] Multiple levels, gothic theme and props, riddle, archways, sealed doors; `lvl2.ron`; armour pieces, ring, Penitence; bell / boom / shriek sounds
- [x] Dormancy + measurement; strafe; target switching; gamepad two-handing; player + weapon redesign
- [x] Online: bells via the host, defeated bosses for joiners (protocol v12); save bosses
- [x] Tests (100), autopilots (`cathedral` route, `boss`, `bestiary`), two-instance runs, notes (N31)
- [x] Smooth puppet spins: the turn rate between two states carries a fast spin on between updates (`spin_rate`, > 6 rad/s while acting); ordinary turns still ease
- [x] Per-room ceilings: `height` on a room (above its floor; default the level's ceiling); columns, chimneys and grates reach their room's ceiling; a header closes the gap where a stair meets a lower room. Ossuary 6 m, crypt 5.5 m, the Choir of Thorns 10 m

### Phase 8.14 — Names and chat (user request, 2026-11-16)
Request, cleaned up:
1. **Name** set in the settings. Without one: **Host** (hosting), **GuestN** (guests), **PlayerX** (offline: Player1).
2. **Twins:** when a second player joins with a name already in use, append `_` and a random German noun (*gordelius* → *gordelius_Radieschen*).
3. **Chat** opened with **T** (default); a message pops up where the notifications are, the same way (slides in, fades later): a small face picture and "Name: I said something!". Other players see it.

Decisions:
- **The host owns the names** (like the world): `Hello` carries the wanted name, `Rename` a change; the host makes it unique (case-insensitive) and sends the whole list (`Reliable::Names`) to everyone. Notifications use the names everywhere ("gordelius opened a door"). Protocol v13.
- **Chat goes through the host** (`Chat` guest → host, `Said` host → the others); the sender sees its own line at once. Max 120 characters; names 16 (+ the noun); letters (umlauts too), digits, space, `_-.`.
- **Face:** drawn from UI nodes (head, hair, eyes, mouth, collar in the player slot's tunic colour, the same colour as their puppet).
- **Font:** notifications, the chat line and the name field use PT Serif — Bevy's default font is ASCII only (no umlauts).
- Text input (`ui/text_input.rs`) reads the logical key of `KeyboardInput` (keyboard layout, shift, repeats) — shared by the chat and the name field. `FLOWERBOX_KEYS` can type (`3.0:"Hello~you`).

Steps:
- [x] Names: settings field, defaults, unique names on the host, list to everyone, rename online; notifications use them
- [x] Chat: T line, send via the host, face + name in the feed, length-based lifetime; rebindable
- [x] Tests (names, twins, text input, chat cleaning); offline + host/two-guest runs (both named gordelius → `Gordelius_Hammer`; every message on all three screens)
- [x] Name tags over other players' heads (`ui/markers.rs`): within 25 m (`NAME_TAG_RANGE`, fading over the last 40 %), hidden behind walls (line of sight from the camera)

### Phase 8.15 — Player skins (user request, 2026-11-16)
Request, cleaned up: skins chosen in the settings, worn by your character; the default **Monobrow** is the current look. Several forms and sizes that stay humanoid — a cat person, a bear, a human, a skeleton, a cube-headed builder — in light cloth clothes or underwear (no armour: that's equipment, on top). Every skin shares the animations, the humanoid features and the hitboxes: plug and play.

Decisions:
- **A skin changes only the look; the skeleton is the human one.** Joint positions and bone lengths decide the animation, the weapon's FK hitbox (reach) and where the feet stand; the capsule decides being hit. So they stay identical — bulk is visual (the bear is broad, the skeleton thin, both reach as far and are hit the same). Test: `skins_change_only_the_look` (every bone, the soles, scale 1, fits the capsule). Sizes *taller/shorter* would change reach and the capsule: not done on purpose (fairness in PvP; a giant player would also need a bigger capsule through doors).
- **Body building blocks** (`HumanoidLook`, reused by enemies too): `Form::Round | Blocky` (cube head, box limbs), `Eyes::Bar | Dots | Squares | Sockets`, `Covered { chest, arms, legs, feet }` (what the clothes cover; the rest is skin / fur / bone), plus extras (ears, tails, snouts, caps, ribs, shorts, braces, sleeves). New texture: `Tex::Fur`.
- **Six skins** (`humanoid/skins.rs`): Monobrow (as before), Blockhead (cube head, teal shirt, blue trousers), Tabby (cat: ears, whiskers, curled tail, sleeveless linen shirt, trousers, sash, bare paws), Bruin (bear: broad, snout, belly patch, red shorts with braces, claws), Rattlebones (skeleton: ribs, collarbones, jaw, glowing sockets, loincloth, rope belt), Toadstool (mushroom: red spotted cap, mossy smock, short trousers).
- **Live swap** (`player/skin.rs`): the old model (with weapon / armour models and the tankard on its bones) is despawned, the new body spawned on the same character, gear re-attached the next frame. Gameplay state stays.
- **Armour fits** the body but at least a human build (`HumanoidSpec::armour_fit`): mail hangs on a skeleton like on a person, and grows for the bear.
- **Online:** the skin travels in `BodyKind::Player(skin)` (protocol v14); puppets rebuild when it changes. Monobrows keep the slot's tunic colour; other skins keep their own clothes.

Steps:
- [x] Look building blocks (form, eyes, coverage), fur texture, six skins, settings row, live swap with gear
- [x] Armour fit check (`FLOWERBOX_AUTOPILOT=skins FLOWERBOX_SKIN_ARMOR=1`), animation check (`FLOWERBOX_SKIN_MOVE`), host/guest run (host cycles Blockhead → Tabby → Bruin, the guest's puppet follows; the guest's Rattlebones on the host)
- [x] Chat faces per skin (drawn from UI boxes: monobrow, cube head, cat ears and whiskers, bear ears and snout, skull with glowing sockets, spotted cap); others' skins come from their puppets
- [x] Idle: besides the breathing chest, a slow look around and shoulders rising with each breath; every body on its own clock (not in lockstep)
- [x] Optional humanoid tail (`HumanoidLook::tail`): a chain of joints off the pelvis swinging in a travelling sine wave (`sway_tails`); Tabby (long, curled) and Bruin (stub)
- [ ] Ears twitching (still rigid)

### Phase 8.16 — Skids, pivots, momentum (user request, 2026-11-16)
Request, cleaned up:
1. **Hard stop:** running, then letting go, the body leans back against the direction it was going, as if braking hard.
2. **Pivot:** reversing the direction brakes first and turns the body round 180° with a turn animation.
3. **Momentum fades:** speed runs out over a short slide ("schlittern") instead of stopping dead — but **not clunky**.
4. Investigate how Souls games do it and pick a fitting implementation (N32).

Decisions (see N32):
- **Asymmetric speed change:** gaining speed stays quick (`GROUND_ACCEL` 45 m/s²), shedding it — letting go or reversing by more than ~110° — is gentler (`GROUND_DECEL` 30): a jog stop slides ~0.3 m in 0.15 s, a sprint stop ~0.8 m in 0.23 s. Sharp turns (< 110°) steer as before.
- **Never locks input:** skids and pivots are a state of normal movement, not actions: a roll, attack or jump takes over on the same frame (their actions drive movement), and steering out of a skid works at once.
- **Pivot** (reversing faster than `PIVOT_MIN_SPEED` 3 m/s, not while strafing locked on): brakes at `GROUND_DECEL` while the facing swings round (180° in ~0.2 s at `TURN_SPEED`), then runs off — ~0.25 s in all. Pose: the hips lead the turn, chest and head follow, knees dip.
- **Skid pose** (weight from the speed being shed, eased in fast, out slowly so it settles): lean back against the slide in whatever direction it goes relative to the body, the lead leg braced, the trailing knee bent, arms thrown forward, head up. The controller exposes `brake` and `pivot`; online they're replicated (`BodyState::skid`, protocol v15) so puppets skid too. Enemies share it (same controller).

Steps:
- [x] `ground_rate` (accel vs decel), `brake` / `pivot` on the controller, skid and pivot poses; tests (stop distances, reversal rate, pose directions)
- [x] `FLOWERBOX_AUTOPILOT=skid` (sprint → let go → sprint → reverse → jog → let go, logged); screenshots; level walk still completes
- [x] Turning on the spot (locked on, the target circling): small alternating shuffle steps, a step per ~35° turned

### Phase 9 — Polish
Order (2026-11-16): first everything that needs no outside assets, then the art (needs a decision on downloading CC0 models).
- [x] Pause menu (Esc): Resume / Settings / Quit; the world keeps running (Souls), gameplay input blocked
- [x] Settings: mouse sensitivity, invert Y, stick speed, volume, notification corner; saved to the OS config folder (`settings.ron`)
- [x] Key rebinding (keyboard + mouse), saved with the settings
- [x] Save and load (autosave): inventory + equipment + consumables, Ale, revived sunflower, respawn point, taken items; `--new-game` / "Delete save" in the menu. A guest saves only its own character
- [x] Sound, generated in code (no asset licensing): footsteps, swings, hits, block, parry, drink gulp, sunflower hum, torch crackle; spatial, volume from settings
- [ ] Weapon models from glTF via `ModelSource::Gltf` (already wired, untested) — needs assets
- [ ] Replace the primitives with a rigged glTF character (CC0). Map its skeleton onto `humanoid::Bone` so the shared poses and the FK hitbox keep working, or blend its clips in `humanoid::anim` (see N13) — needs assets

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
| 2026-11-14 | Debug overlay on F3 is always compiled in (it's cheap). Debug keys (H, J, K, 1-4, F6) only exist in debug builds. Not tied to the `dev` feature. |
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
| 2026-11-15 | The Hearth is shown as a **sunflower**: drooping and wilted until first rested at, then it stands up over 2.2 s (`STAND_UP_TIME`), turns gold and lights the area. Death text is "COMPOSTED", the revive banner "SUN VIGORATED". All player-facing wording lives in `src/ui/strings.rs`; code keeps the names `Hearth`/`kindled`. |
| 2026-11-15 | Phase 7: the world does **not** pause in the inventory (Souls-style); the player just gets no gameplay input (`GameplayBlocked`). Equipment points at inventory indices (items are never removed yet). Defence = multiplicative armour **absorption** (generic `DamageAbsorption` on characters). Empty right hand = **Fists** (a `WeaponDef` with no model). The Rusted Axe uses the LightSword moveset until it gets its own class. Shields only add weight and show a block value until blocking (Phase 8). |
| 2026-11-15 | Guard (user request): hold RMB with a shield. The shield blocks from the moment you start raising it; the **last third of the 0.3 s raise is the parry window** (negates the hit). Every new raise (also after a roll/attack/stagger while still holding) has its own window. Blocked hits don't stagger; a guard break (stamina cost > stamina) does. Without a shield there is no guard (weapon blocking may come later). |
| 2026-11-15 | Phase 8: enemies are generic characters driven by an AI that writes `MoveIntent`/`ActionIntent` (same pipeline as the player). Enemy attack readability comes from their own **weapon class** (`HollowBlade`, 0.55 s wind-ups) rather than AI pauses. Teams (`combat::Team`) stop friendly fire; the dummy has no team. Enemies respawn on `WorldReset`. The older autopilots run without enemies (`FLOWERBOX_AUTOPILOT` ≠ `enemy`). Roll weight = load ratio classes; the fast roll keeps the full i-frames. |
| 2026-11-15 | Online: **iroh** (QUIC, direct connections with hole punching, n0's free public relays as fallback, connect by an address code) instead of Steam (needs Steam on both sides + $100 app fee to ship), matchbox/WebRTC (needs a signalling server) or plain sockets (port forwarding). Own small protocol over iroh instead of `bevy_replicon` (+ `bevy_replicon_iroh`, a tiny one-person crate): the state is small and the game already has a clean "intent → character" split. Host-authoritative world, each player authoritative over its own character; hits are decided on the victim's or the local attacker's machine. |
| 2026-11-16 | Levels are data (`assets/levels/lvl1.ron`) in **drawing pixels** of the design sketch, built by a generic grid builder (`src/level/`): walls are generated, not drawn. One shared ceiling height (5 m), so deeper rooms are taller. Stairs = invisible convex ramp + visual steps. Doors open from either side and stay open; online the host decides (like pickups). The arena stays, with a gap in its ring wall where the level's first door sits. Hollows notice you only with a clear line of sight (and within 2.5 m height); lock-on also needs a clear view. |
| 2026-11-16 | Hop Cone (consumable): +1 Healing Ale for good when eaten (Estus-shard equivalent, no sunflower needed). Thornbriar Ring: 10 % absorption. Sunflower 1 moved ~1 m north of the drawing's dot so its bed clears the wall; the Hop Cone beside it moved too. |
| 2026-11-16 | Quadrupeds mirror humanoids: own skeleton/spec/pose/animator module (`quadruped/`), driven by the same character actions. A beast's jaws are a **natural weapon class** (moveset + no model) so bites reuse the action/attack pipeline; the grab is a generic `Pounce` action available to any moveset with a `PounceSpec`. Beast AI = one system + a data profile per species. |
| 2026-11-16 | Grabs are **victim-side** (like enemy hits online): each machine checks only its own player against pouncing creatures. Rolling i-frames at the first contact = dodged for that pounce. The bite is unblockable because a held character has no guard. The warg's bite is 120 (≥ 100 after the best armour). Each declined pounce chance raises the next (`pounce_urge`). Player HP bars show only over *other* players (your own is the HUD bar). |
| 2026-11-16 | Doors are one-way by their swing (`opens` points away from the side that can open them); locked doors name a key item that stays in the bag. A door's collider is a separate static blocker spawned where the leaf rests (closed or open), never moved. Enemy drops come from the level file (`drops`), roll on the machine that owns the enemy (offline/host) and are broadcast; unique drops (keys) are remembered once taken (`ClaimedDrops`, saved). |
| 2026-11-16 | Online, dying (anyone's, the host's too) no longer resets the world; only the shared rest does (user request, "for now"). Offline death still resets (Souls). Up to 5 players. The host sends enemies only when they change; guests learn what exists from a roster. |
| 2026-11-16 | Hands are generic: `Wielded { right, left, two_hand }` replaces `EquippedWeapon`; attacks carry an `AttackHand` (Right / Left / Both); left-hand and dual poses are the **mirror** of a class's right-hand swing keys, so every class gets them for free. Left mouse = right hand, right mouse = left hand (Souls); the left item decides: shield → guard, weapon → attacks, same-class pair → dual attack. Abilities are data per class (`Moveset::ability`). |
| 2026-11-16 | Test runs (autopilot / screenshot) render without vsync: GNOME throttled the (often hidden) test windows to ~4 FPS. |
| 2026-11-16 | Enemies are data (`EnemyDef` per kind: body, capsule, HP, poise, weapon, brain profile, boss); special moves are abilities; brains share `enemy/ai.rs`. Horrors: Mourner (stalker) and Contorted; boss: the Thorned Saint (fog gate, name bar, enrage, stays dead). |
| 2026-11-16 | Inventory: slot-first and item-first equipping; throwing out removes the entry (indices shift) and puts a pickup into the host's world. |
| 2026-11-16 | Level 2 "The Cathedral of Ashes" south of the arena; several levels load side by side. Bell riddle unseals the crypt; a one-way gallery door is the loop. |
| 2026-11-16 | Enemy dormancy by distance to every player (30 / 36 m), not by opened doors (N31). |
| 2026-11-16 | Movement: quick to speed up, gentler to slow down (short skid), pivots on hard reversals with a lean / hip-turn pose; never locks input (N32). |
| 2026-11-16 | Player skins change only the look on the human skeleton (same animation, reach, feet, capsule); six skins; armour fitted to at least a human build. |
| 2026-11-16 | Player names (settings; Host / GuestN / Player1 without one), made unique by the host with a German noun; chat on T through the host into the notification feed (face + name). |
| 2026-11-17 | Test runs (`FLOWERBOX_AUTOPILOT` / `FLOWERBOX_SCREENSHOT`) never capture the mouse (`input::cursor_lock_allowed`) and open their window without taking focus, so tests can run while you keep using the desktop. Scripted attack/guard buttons already ignore the cursor state. |
| 2026-11-17 | Robes, skirts and capes are cloth meshes rebuilt each frame (`humanoid/skirt.rs`): the hem is pushed out past knees and feet, so striding legs never poke through (a fixed cone narrow enough to look like a robe always failed). Hoods wrap the head (a cowl with the face in its opening) instead of sitting on top. Online, every visible equipment slot (hands, head, chest, gauntlets, legs: `SHOWN_SLOTS`) is sent, not only weapons and the hood (protocol 16). |
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
- **N18 (2026-11-15) — Phase 7 (inventory and equipment):**
  - **Data:** `items::ItemId` → `ItemDef` (static, in `items/mod.rs`): name, kind (Weapon/Shield/Armor(slot)/Ring/Consumable/Key), description, `ItemIcon`, weight, absorption, block, `max_hp_bonus`, `weapon: Option<&'static WeaponDef>`, `model: Option<ItemModel { bone, build }>`.
  - **State:** `Inventory { items }` (bag order, equipped items stay in it) and `Equipment` (slot → inventory index; slots: right/left hand, head/chest/hands/legs, 2 rings, quick item). `Equipment::equip/unequip` are pure and unit tested; rings fill the free ring slot, else replace the first; the quick item (Healing Ale) can't be removed.
  - **Flow:** anything sends `EquipRequest { character, index, equip }` → `handle_equip_requests` → on `Changed<Equipment>`: `EquipStats` recomputed (attack, absorption, block, weight / `PLAYER_MAX_LOAD`, load class, max HP bonus → `Health::max` and `DamageAbsorption`), `sync_weapon` sends `EquipWeapon` (Fists when empty), `sync_models` attaches shield/armour parts to bones.
  - **UI:** `ui/inventory_ui.rs`, rebuilt whenever what it shows changes. Mouse via picking observers (`On<PointerOver>`, `On<PointerClick>`, double click equips). The default font lacks "—", "→", "·": use ASCII in body text.
  - **Interact:** `interact.rs` clears `InteractPrompt` each frame; pickups claim E before the Hearth.
  - **Pickups:** Rusted Shortsword (-7, -6), Studded Buckler (9, -7), Sunseed Ring (-9, 7.5). They don't respawn.
  - **Debug:** F6 equips the next weapon in the bag. `FLOWERBOX_AUTOPILOT=inventory` (see `inventory_script`): menu with the axe selected (1.6 s), gear on (3.3 s), toast (4.4 s), 50 damage → HP 53 (6% absorbed), shortsword equip/unequip → Fists. The combat autopilot now adds the shortsword to the bag and equips it through `EquipRequest`; hits unchanged.
  - **Not tested by script:** real Tab/I/Esc presses and mouse clicks (the autopilot drives the `InventoryMenu` resource directly).
- **N19 (2026-11-15) — guard, block and parry (user request):**
  - **Slots:** an item's kind decides its slot (`Equipment::equip`). The left hand takes **only shields**; there is no dual wielding yet. To add it: let `ItemKind::Weapon` go to the left hand too (e.g. equip into the left hand when chosen from a left-hand slot in the UI), give `Equipment` a left-hand weapon, attach its model to `HandL`, and add left-hand attack actions / a two-handed toggle in the movesets.
  - **Code:** `character/guard.rs` (generic, works for future enemies). `Guard { strength, raised_for }` on every character (`character_bundle`). `strength` = the shield's block value via `EquipStats` (0 without a shield = can't guard). `ActionIntent::guard_held` comes from RMB (`Action::Block`) or the autopilot's `guard_button`. `update_guard` (FixedUpdate, after the action state machine) raises/lowers it; any action lowers it.
  - **Damage order** (`stats::apply_damage`): i-frames → `Guard::outcome` (pure, unit tested) → armour absorption. Outcomes: `Unguarded` (down, no shield, or attacker outside `GUARD_ANGLE` of the facing), `Parried` (in the window: no damage, `Parried { defender, attacker }` message, hitstop + shake), `Blocked { through, stamina_cost }` (no stagger), `Broken { through }` (stamina → 0, guard lowered, stagger). `DamageMessage` gained `attacker: Option<Entity>` (combat fills it) so a parry knows whom to stun.
  - **Tuning:** `GUARD_RAISE_TIME 0.3`, `PARRY_WINDOW_START 2/3`, `GUARD_ANGLE 1.0 rad` (was 1.4), `GUARD_STAMINA_PER_DAMAGE 0.6`, `GUARD_MOVE_FACTOR 0.55`, `GUARD_REGEN_FACTOR 0.35`. Shields: Wooden 60%, Buckler 40%.
  - **Animation:** `humanoid::anim::guard_arm` blends the left arm to an upright forearm in front of the chest (shield face forward), eased with the raise (unit tested via FK).
  - **To do — the stun:** when enemies exist, react to `Parried { attacker: Some(e) }` by forcing a long `Stunned` action on `e` (open to a riposte). Possibly give a parry its own short animation (shield bash) instead of reusing the raise.
  - **Fix (user report):** blocking looked like it stopped everything because the debug key H sent damage with `from: None`, which counted as frontal. Now a hit with no direction can't be guarded, H hits from the front and J from behind, and with F3 on the guarded arc is drawn on the floor (blue, gold in the parry window).
  - **Fix (user report):** the pickup prompt flickered: the HUD's `update_prompt` wasn't ordered after `InteractSystems`, so on some frames it read the prompt right after it was cleared. It now runs after `InteractSystems::Hearth`.
  - **Debug:** F3 shows the guard (down / up / PARRY WINDOW). `FLOWERBOX_AUTOPILOT=guard`: see `guard_script` for the expected log (block, parry, guard break, hit from behind, guard down).
- **N20 (2026-11-15) — Phase 8 (lock-on, roll weight, gamepad, the Hollow, parry stun):**
  - **Roll weight:** `character::action::RollWeight` (component in `character_bundle`, set from `EquipStats::roll_weight()`): Light ≤30% load, Medium ≤70%, Heavy ≤100%, Overloaded. `RollWeight::roll_def()` scales the base roll (Medium = `ROLL_*`): Light 0.85× time, 1.1× distance, full i-frames; Heavy 1.2×/0.8×, 80% i-frames, +0.12 s recovery; Overloaded 1.4×/0.45×, 45% i-frames, +0.45 s. The starting gear is Light (fast roll). The inventory shows the class with its roll.
  - **Lock-on:** `lockon.rs`. `LockOn { target }` on the player, `Targetable { height }` on the Hollow and the dummy. `pick_target` (pure, tested). While locked `MoveIntent::face` = toward the target: the controller faces it unless an action runs or you sprint (sprinting runs where you steer), and `start_buffered_action` aims attacks at it (rolls still follow the stick). The camera turns to look past the player at the target (`CAM_LOCK_TURN`, `CAM_LOCK_PITCH`). Markers: `ui/markers.rs` (PostUpdate, world → viewport).
  - **Gamepad:** leafwing bindings in `input::default_input_map` (layout in §2) plus `Action::LookStick` (rate-based camera). The inventory screen reads `Gamepad` buttons directly. **Not tested** (no pad in this environment).
  - **The Hollow** (`enemy/mod.rs`): `character_bundle` + `HumanoidSpec::hollow()` (thinner, grey, rags, emissive eyes via `HumanoidLook::eye_glow`) + `HOLLOW_SWORD` (class `HollowBlade`: light combo of 2 with 0.55 s wind-ups; heavy/running/jump from the light sword). AI states Idle → Chase → Attack (1–2 light swings or a charged heavy, `HOLLOW_HEAVY_CHANCE`) → Recover (circle / back off 1–2 s) → … → Home (leash 16 m or the player died). Hit while idle → Chase. Walking into rubble: sidestep for 0.7 s (`stuck` / `detour`). The AI ticks its own `ActionIntent` buffer. Deterministic xorshift RNG.
  - **Death/reset:** a dead enemy loses `Hurtbox` + `Targetable` (the lock breaks), lies `CORPSE_TIME`, then hides and gets `ColliderDisabled`. `WorldReset` (rest or respawn) restores all enemies at home.
  - **Combat:** the player now has a `Hurtbox` and `Team::Player`, the Hollow `Team::Hostile`; `sword_hits` skips same-team targets. `guard::stun_parried` (FixedUpdate, right after damage) forces `Stunned` on the parried attacker; a hit on a `Stunned` target deals `RIPOSTE_MULT` (logged as "RIPOSTE").
  - **Autopilot `enemy`:** the Hollow is moved 4 m in front of the player (and walks around the rubble at (0.7, 6.8)), lands its 2-swing combo (HP 72), lock-on at 3 s, then the script raises the shield timed to the swing (`raise shield`) → "parried" → "attacker stunned" → "RIPOSTE … for 50" → "hit … for 20" → "a Hollow falls" → "lock-on lost" → 5 s later "enemies reset" → it repeats. Logs the Hollow's AI state every 0.5 s.
  - **Known gaps:** no strafe leg animation; no poise (every hit flinches, so a fast player can stun-lock a Hollow); one enemy type; no target switching.
- **N21 (2026-11-15) — online co-op (user request: "peer to peer, ideally without hosting a server"):**
  - **Research (crates.io, Nov 2026):**
    | Option | Server needed? | Notes |
    |---|---|---|
    | **iroh 1.4** (n0) | No. Direct UDP hole punching; n0's free public relays as fallback and for address lookup | Connect by an endpoint address (code). Public relays are free but rate-limited, meant for development/small use; for a release you can run your own `iroh-relay` on a domain/VPS (that's where a domain would help) |
    | Steam (`steamworks` 0.13 + `renet_steam` 4 / `aeronet_steam` 0.22; `bevy_steamworks` is still on Bevy 0.19) | No (Valve's relay network) | Both players need Steam running; dev with test App ID 480 is free, shipping needs a Steam Direct fee ($100) and the SDK lib next to the binary. Gives lobbies, friend invites, overlay |
    | matchbox / WebRTC (`bevy_matchbox` 0.15, Bevy 0.19) | A small signalling server (could run on your domain) + STUN/TURN | Good with GGRS rollback, which needs a deterministic simulation (avian + our systems aren't) |
    | Plain QUIC/UDP (`bevy_quinnet`, `renet`) | No, but the host must open/forward a port | Simplest, but friends behind routers can't connect without port forwarding |
    | Replication libs: `bevy_replicon` 0.45, `lightyear` 0.31 (both Bevy 0.20) | — | Transport-agnostic replication; `bevy_replicon_iroh` 0.20 exists (very new). Worth adopting when the networked state grows |
  - **Chosen:** iroh, `presets::N0`, ALPN `flowerbox/1`. The join code is `FBX1-` + base64url(postcard(`EndpointAddr`)) (endpoint id + home relay + local IPs, ~135 chars). Local IPs make LAN play work without internet; the relay makes it work across routers. Tested: two instances on one machine, also with `FLOWERBOX_NET_RELAY_ONLY=1` (the code's IPs stripped, so the connection starts through the n0 relay).
  - **Threading:** `net/transport.rs` runs a tokio runtime on its own thread; Bevy talks to it through channels (`NetLink::send` / `poll`). Per connection: QUIC datagrams for state; one bidirectional stream (opened by the guest, length-prefixed postcard frames) for reliable messages.
  - **Sessions:** `NetSession.role` Offline → StartingHost → Hosting, or Offline → Joining → Client (after `Reliable::Welcome`). The host admits up to `MAX_PLAYERS` (4) with ids `PLAYER_ID_BASE + slot` (host = 1) and refuses other protocol versions (`PROTOCOL_VERSION`). On disconnect the guest goes back offline; its own enemies come back.
  - **State:** every machine sends its player's `BodyState` (position, facing, movement, grounded, action kind/elapsed/dir/charge/class, guard time, HP, right/left hand + head items) at 30 Hz. The host sends a snapshot of all bodies (its player, the guests' last states, its real enemies: id `ENEMY_ID_BASE + entity index`).
  - **Puppets** (`net/sync.rs`): spawned per remote body with `Remote` + the normal components (`CharacterController`, `CharacterAction`, `Guard`, `Health`, rig, animator, kinematic collider; players: tiny `Inventory`/`Equipment` so weapon/shield/hood models follow; Hollows: `Enemy`, `Hurtbox`, `Team::Hostile`, `Targetable`). They ease toward the state (extrapolated ≤0.2 s), keep their action ticking locally and resync when it drifts > 0.12 s. Guests' tunics are coloured by slot.
  - **Hit rules** (`combat::sword_hits`): a `Remote` target is only hit by *our* player (`apply_damage` skips `Remote`; `forward_hits` sends `Reliable::Damage` to the host); a `Remote` attacker only hits *our* player. So each hit is decided exactly once: your hits on enemies on your machine, enemy hits on you on your machine (blocking/parrying are local; a parry sends `Reliable::Parried`, the host stuns the real enemy).
  - **Host side:** guests' damage → `DamageMessage` on the real enemy (attacker = the guest's puppet); enemies chase the nearest living `Team::Player` (local or puppet). Guests don't spawn enemies (`EnemiesSuppressed`).
  - **Found while testing:** spawning characters and sending `EquipWeapon` in the same `Update` could drop the request (the entity didn't exist yet) → `WeaponSystems::Equip` set; spawners run `.before` it. Duplicate puppets when two snapshots arrived in one frame → dedupe by id.
  - **Autopilots:** `net` (walk, attack, walk back) and `netfight` (guest walks to the Hollow and keeps swinging). Two-instance recipe: host with `FLOWERBOX_NET_CODE_FILE=code.txt … --host`, then guest `… --join "$(cat code.txt)"`.
  - **Not shared yet:** guest deaths/rests (local only; the host's world doesn't reset for them), pickups, the dummy, banners. No interpolation buffer or lag compensation. Untested across two real networks/NATs (only same machine, direct and relay-started).
  - **Giving the game to a friend:** `cargo build --release`, then send `target/release/flowerbox` together with the `assets/` folder (same directory). A Windows friend needs a Windows build (build on Windows, or cross-compile with `x86_64-pc-windows-gnu`).
- **N22 (2026-11-15) — shared world, notifications, opt-in PvP (Phase 8.6):**
  - **Notifications** (`ui/notifications.rs`): send `Notify { icon: NoticeIcon, text }`. `NotificationSettings` (resource): `corner` (default bottom-right), `margin`, `lifetime` 3 s, `fade` 0.6 s, `slide_in` 0.35 s (from beyond the window edge), `stack_step` (10, 28) px per older notice (slight overlap), `older_alpha` 0.8, `max_shown` 5, `font_size`, `icon_scale`. No background: text with a hard shadow, icons outlined; colours (incl. shadows) faded together. F7 / `FLOWERBOX_AUTOPILOT=notify` show samples. Icons (`ui/icons.rs`): items, Sunflower, Joined (+), Left (−), Died (×), Poppy, Camomile.
  - **World authority** (`net/world.rs`): a joining guest stashes its own world (sunflower state, taken layout items), despawns its items and receives `Reliable::World` from the host (sunflower + every item on the floor, so later drops work too); leaving restores the stash. Pickups have stable ids (`Pickup { id, item }`); E writes `PickupRequest`; `PickupAuthority` (offline/host) grants locally, a guest's request goes to the host, which answers with `WorldEvent::PickedUp` to all. The sunflower: the reviver's own game shows the banner at once (`SunRevived` message from `hearth.rs`); the host records it and broadcasts; others get a notification and their sunflower stands up. Join/leave/death/PvP events → notifications ("Host", "Guest n").
  - **Spawn points** (`enemy/mod.rs`): `SpawnPoint { pos, radius, spawned }`; any `Team::Player` body (local or a guest's puppet) inside the radius spawns its enemy once; `WorldReset` (host rest / host death / offline rest) despawns them and re-arms. Guests never spawn (`EnemiesSuppressed`). The reported "invisible enemy" couldn't be reproduced on one machine (the guest saw the host's Hollow in every test); with this change there is exactly one enemy, owned by the host.
  - **Consumables:** `Inventory::counts` (stacking, the entry stays at 0 so equipment indices never shift), `add/count/consume`, `Inventory::from_items`. `ItemDef::effect` (`PvpOn` / `PvpOff`). Non-Ale consumables can't be equipped; Enter / double-click / "Eat" in the inventory writes `UseItemRequest` → `UsingItem` + `ActionKind::UseItem` (drink pose, `USE_ITEM_*`), effect at the start of the active phase (`pvp::apply_item_use`).
  - **PvP:** `Pvp(bool)` on players and player puppets (`BodyState::pvp`, protocol v3). `combat::sword_hits`: same team is skipped unless both have PvP; a remote *player* is never hit from our side (its own machine decides from our puppet). Parries of a player go `Reliable::Parried` → host → `Reliable::Stun` to the parried player's machine (the host stuns itself directly). Death turns PvP off; respawning (`DeathSequence` removed) adds a Poppy if there is none. Red poppy marker over PvP players (child of the body, not the head bone, so it doesn't swing); "PvP" next to your Ale counter.
  - **Hit immunity:** `HitImmunity` on the player, `HIT_IMMUNITY` 0.1 s after any applied hit (Souls has no post-hit i-frames; this only stops simultaneous hits from stacking).
  - **Camera fix:** the wall check ignores characters (other players, enemies, dummy) — before, another player standing behind you pulled the camera into their head.
  - **Autopilots:** `netfight` (guest: walks into the spawn area → the host spawns the Hollow, fights, takes the Shortsword, revives the sunflower; host logs "world event: Guest 1 picked up Rusted Shortsword / vigorated the Sun"), `pvp` (both eat a Poppy; guest hits host 3× for 20; guest eats a Camomile; further attacks do nothing; host dies → PvP off, "respawned with a fresh Poppy"), `enemy` (now steps into the spawn area first), `notify`.
  - **Still local per machine:** the training dummy; guests' rests reset only their own state (not the world).
- **N23 (2026-11-16) — Souls-style respawns, shared rest, synced dummy, corpses (Phase 8.7):**
  - **Reported bug explained:** the "invisible enemy" came from a guest that joined after the first Hollow had died — with the area trigger, the guest's arrival spawned a second Hollow in the host's world. Spawn points no longer trigger by area: every spawn point's enemy exists from the world's start (`trigger_spawn_points` spawns whatever is missing; never on guests), and a `WorldReset` despawns all enemies (alive or corpses) so they respawn next frame. N22's area-trigger note is superseded.
  - **Rest:** `ActionKind::Rest` = kneel (`REST_STARTUP`) then an endless active phase; `ActionKind::StandUp` (`REST_RECOVERY`, rest pose reversed) on E or movement (`hearth_interaction`, prompt "[E] Stand up"). Once down, `rest_at_hearth` revives the sunflower (if needed) and sets the respawn point; the **completion** is `hearth::RestComplete` → `complete_rest`: `WorldReset` (enemies respawn, dummy reset) and the resting player's HP/stamina/Ale restored. Offline (`SharedRest(false)`) the completion follows at once.
  - **Shared rest** (`net/world.rs::host_rest_circle`): the host counts resting players (itself + guests' puppets whose replicated action is `Rest` past `REST_STARTUP`). Each newcomer: `WorldEvent::RestWaiting { who, resting, total }` → "Host waiting for other players at rest site 1/2" for everyone. All resting → `WorldEvent::RestComplete` to everyone (+ "Everyone rests at the sunflower: the world stirs"). A rest completes once per gathering (only a newly sitting player arms a new completion; someone leaving the session doesn't).
  - **Dummy sync:** `combat::DummyStruck { amount, from, killed }` drives the wobble/break (`dummy_react`) and the damage numbers; offline/host it comes from the applied `DamageTaken`, guests get it from `WorldEvent::DummyHit { amount, from, killed, hp }`. A guest's dummy is `Remote` (damage isn't applied there); its hits go to the host as `Reliable::Damage { target: DUMMY_ID }`. Joiners get the dummy's HP in `Reliable::World`. Push: `1.2 + DUMMY_PUSH_PER_DAMAGE × damage`, capped at `DUMMY_PUSH_MAX`.
  - **Damage numbers** (`ui/damage_numbers.rs`): pop in 40 % larger, rise ~0.9 m while drifting sideways (alternating sides), fade in the last 40 % of 1.1 s; white, orange ≥ 30, red when it breaks the dummy.
  - **Hit feedback fix:** `DamageTaken` carries `attacker`; hitstop/shake only for our own hits or hits on us (before, the host felt a guest's hits).
  - **Corpses:** `character::corpses_dont_block` gives dead characters (enemies, players, puppets) `ColliderDisabled` and removes it on revival; the broken dummy too, until it stands up. Verified: the enemy autopilot walks the player straight over the Hollow's body ("passed the corpse").
  - **Protocol v4.** Autopilot `netrest` (two instances): guest hits the dummy (host logs "a guest hit the dummy for 20/20/32", both see numbers), host rests ("Host waiting … 1/2"), guest rests → both "rest complete", guest walks off → `StandUp`. Hearth autopilot: E at 11 s stands up.
- **N24 (2026-11-16) — Phase 9, part 1 (menu, settings, rebinding, save, sound):**
  - **Menu** (`ui/pause_menu.rs`): Esc / Select opens it only if no other menu had the input in the previous frame (Esc closing the inventory doesn't open it). Rebuilt on change like the inventory. `PauseMenu::show(page)` for scripts (`FLOWERBOX_AUTOPILOT=menu`, `FLOWERBOX_MENU_PAGE=main|settings|controls`). The default font lacks "…", "—", "→", "·": use ASCII.
  - **Settings** (`settings.rs`): `Settings` resource (`#[serde(default)]`, so older files load), saved to `dirs::config_dir()/flowerbox/settings.ron` on every change. Bindings: one input per `Bind` (movement keys only), conflicts swap. `input::input_map(&bindings)` builds the leafwing map (plus fixed extras: I, middle mouse, mouse look, gamepad); `rebind_player` applies changes at once. Camera reads sensitivity / invert Y / stick speed; `GlobalVolume` and `NotificationSettings::corner` follow. Bevy features `serialize` (KeyCode/MouseButton serde) and `wav`.
  - **Save** (`save.rs`): `SaveData` (version 1) in `dirs::data_dir()/flowerbox/save.ron`, written atomically (temp + rename) at most once a second when the bag/equipment/Ale or the own world (sunflower, respawn point, taken layout items) changed, and on quit. Loaded in `PostStartup`: bag + counts, equipment (`Equipment::from_slots`), Ale, sunflower, taken items, and the player placed at the respawn point. A guest doesn't update the world fields. `--new-game`, menu "Delete save" (stops autosave for the session).
  - **Test isolation:** with `FLOWERBOX_AUTOPILOT` set, settings and saves are neither read nor written unless `FLOWERBOX_CONFIG_DIR` / `FLOWERBOX_DATA_DIR` point somewhere. Verified: inventory autopilot → save file with the Shortsword, equipment and taken id 0; the next run loads it.
  - **Sound** (`sound.rs`): synthesised at start (deterministic noise, one-pole filters) into 16-bit WAV `AudioSource`s: 4 footsteps, swing, heavy swing, roll whoosh, hit, straw hit (dummy), block clang (`guard::Blocked` message, new), parry ring, gulp (Ale and eating), sunflower hum (seamless 4 s loop, louder once revived), 3 torch crackle loops. All spatial (`SpatialListener` on the camera). Triggers: footsteps from `CharacterController::moved` (any character, also puppets), swings/rolls/drinks from actions reaching their moment, hits from weapon `DamageMessage`s (not the dummy), dummy hits from `DummyStruck` (so guests hear them too). Unit tests check loudness, clipping, the loop seam and that rodio decodes the WAV. **Not listened to** (no speakers here); the enemy autopilot logs "audio: N started so far".
- **N25 (2026-11-16) — Level 1 "The Sunken Halls" (Phase 8.8):**
  - **Format** (`src/level/layout.rs`): `px_per_m`, `origin_px` ↔ `origin_world` (px (409.5, 1228) = world (0, −18)); `rooms` (name, floor, rects), `blocks`, `stairs` (rect, `down`, top, bottom), `doors` (id, `at` on the wall line, width, `hinge` side, `opens` direction — also gives the wall's orientation), `torches` (point + which wall), `enemies` (`patrol` optional), `treasures` (pickup ids ≥ 100; the arena's are 0–2), `sunflowers` (id, facing). Read from `BEVY_ASSET_ROOT`/crate dir/next to the binary, embedded copy as fallback.
  - **Builder:** rects snap to a 0.25 m grid; cells are Empty / Solid / Room(n) / Stair(n). A wall stands on a cell boundary between walkable and not, between different rooms, between different stairs, and between a stair and a room unless it's the stair's top/bottom end. Door spans turn walls into lintels (from `DOOR_HEIGHT` above the door's floor). Runs are merged (58 for level 1), 0.5 m thick, from −5 to 6 m, each a box with a collider. Floors and ceilings per rect. Doors on the arena circle (±0.75 m) remove ring segments (`ArenaOpenings`); the level's own wall fills the gap.
  - **Stairs:** walking uses a convex-hull wedge (the controller's slide + 0.08 m snap-down handles 23° and 14° fine; verified walking down both); steps (rise ≤ `STAIR_RISE`) are visual only, the ramp passes through their middles.
  - **Doors** (`level/door.rs`): the leaf is the rigid body (static), positioned from the hinge each frame while swinging; `ColliderDisabled` during the swing. `Door::set_now` for saves / joined worlds. Prompt "[E] Open" (`InteractSystems::Doors`, between pickups and the Hearth).
  - **Enemies:** `SpawnPoint { pos: Vec3, patrol }`; the Hollow idles or paces (`HOLLOW_PATROL_FACTOR`); noticing needs `level::line_of_sight` (ray ignoring characters) and |Δy| < 2.5 m. Once chasing, it follows without sight (leash as before).
  - **Sync/save:** protocol v5: `World { suns, doors, pickups (3D), dummy_hp }`, `RequestDoor`, `WorldEvent::SunRevived { by, id }`, `DoorOpened { by, id }`. Save gains `suns` / `doors` (older saves' `sun_revived` → sunflower 0). `PickupLayout` resource replaces the arena-only `PICKUPS` (taken ids = layout minus present). Sunflower hum volume is per flower now.
  - **Tests:** unit tests for the grid/walls/stairs/doors, the shipped file (every object on a floor, torches near walls, door 1 on the arena wall). `FLOWERBOX_AUTOPILOT=level` walks arena → door 1 → corridor → stairs 1 → middle hall → door 2 → upper halls (T room, west wing) → Hop Cone → rests at sunflower 1 → stairs 2 → vault: all 33 waypoints reached, floors 0 / −1.5 / −3.5, "revived sunflowers [1]; open doors [1, 2]". Screenshots: `FLOWERBOX_AUTOPILOT=still FLOWERBOX_POS=x,z,yaw FLOWERBOX_SCREENSHOT=…` (no enemies, doors open). Screenshot runs no longer read or write the real save.
  - **Performance:** 26 shadow-casting torches dropped the release build to 20–30 FPS (RX 6800 XT). Only the `TORCH_SHADOW_LIMIT` (3) torches nearest the camera cast shadows now (`arena::limit_torch_shadows`) → 60 FPS (vsync) along the whole walk; 4 dips to 43, and a 512 shadow map didn't help (it's the per-light passes/draw calls). Shadowless torches can light a little through walls; level meshes aren't batched (one mesh per box) — merging them would make more shadowed torches affordable.
  - **Look:** dark and warm (18 extra torches: `LEVEL_TORCH_*`). Walls are flat boxes; texture/trim would help later.
  - **Autopilot fix:** `enemy` picks the arena's Hollow (there are 11 now). A `hearth` run once failed because of stray real input (the window takes focus): rerun before suspecting a regression.

- **N26 (2026-11-16) — menus leaked input (user report: "in the pause menu W still moves the character and the selection; the mouse doesn't work in pause / inventory"):**
  - **Cause:** the inventory, the pause menu and the online panel each wrote `GameplayBlocked` and grabbed/freed the cursor themselves. The *closed* inventory's toggle reset `GameplayBlocked` to false and re-grabbed the cursor every frame, while the open pause menu set it back. Depending on system order, W / attacks reached the character and the cursor kept getting locked (so menu hover/clicks didn't work).
  - **Fix** (`input.rs`): each menu only sets its own flag in `MenuInput { inventory, pause, online }` (system set `InputSystems::Menus`); `apply_menu_block` (`InputSystems::Apply`) derives `GameplayBlocked` and grabs/frees the cursor only when that changes; then `gather_player_input` (`Gather`). Order: `Cursor → Menus → Apply → Gather`. Menus don't open over each other. The frame a menu closes still counts as blocked, so clicking "Resume" doesn't swing the sword.
  - **Check:** `check_menu_block` (menu / inventory autopilots) warns on any frame a menu is open but input isn't blocked or the cursor is locked: pause menu 0 of 720 frames. The inventory autopilot shows 1 frame per opening, because the script sets `InventoryMenu.open` directly (bypassing the toggle); real Tab/I presses go through the toggle.
- **N27 (2026-11-16) — the warg (Phase 8.9):**
  - **Body** (`quadruped/`): 18 bones (hips root → chest → neck → head → jaw, tail, 4 × upper/lower/paw). The collider is the standard character capsule around the chest (the controller's ground probe assumes it); the hindquarters and head stick out of it, so hits on the rear don't register. Gaits by speed: walk < 2.5 m/s (lateral sequence), trot ≤ 6, gallop above (front pair, hind pair, body rock). The animator layers action poses on the gait like the humanoid one.
  - **Bites:** `LightAttack(n)` of the `WargFangs` moveset (startup 0.26–0.42 s, 17 base damage, lunges); blockable and parryable. `combat::bite_hits` sweeps a 0.28 m sphere along the snout tip (`QuadrupedSpec::snout_tip`) with the same FK the animation uses.
  - **Pounce** (`PounceSpec`: wind-up 1.0 s, leap 0.42 s over 3 m, recovery 1.5 s): during the wind-up `beast::pounce_tracking` keeps turning toward the target (2.6 rad/s), so stepping aside isn't enough. During the leap the warg has `PassThroughCharacters`. `grab::pounce_grabs`: the local player within `GRAB_REACH` + radius of the jaws (|Δy| < 1.2) is caught (`Grabbed`, carried at `GRAB_HOLD_DISTANCE` via `ActionMovement::Held`, walls still stop them). `GRAB_BITE_AT` (0.6 s) later the bite deals 120. If the warg stops pouncing first, the victim is dropped with a stagger. Invulnerable at first contact → dodged for that pounce (without this the roll got blocked by the warg's body and the victim was caught when the i-frames ended).
  - **AI** (`BeastProfile` for the warg: aggro 13 m, leash 24, chases at 0.95× walk, sprints beyond 6 m, recovers 0.35–0.9 s): after a combo, with the pounce ready, it backs off into pounce range (2.2–4.8 m) and decides (55 % + 20 % per declined chance, cooldown 5 s). Without backing off it never pounced (combos end at < 1 m).
  - **Tests:** unit tests for the skeleton, gaits, bite and pounce poses, HP bar fade. `warg` autopilot: two bites, back off, pounce, grab 0.08–0.16 s into the leap, "the jaws close: 120 damage"; with `FLOWERBOX_WARG_TANK=1`: 4 pounces, 4 grabs in 45 s. `wargroll`: 4 of 5 runs dodged every pounce; the one grab came from a roll pressed during a bite's stagger, which starts too late (fair). Online (host `warg` in the arena, guest `warg` placed in the vault after joining): the guest's machine detects the host warg's bites and grabs from its puppet ("grabbed … the jaws close: 120"). HP bar: guest screenshot 0.7 s after the host was hit shows the bar; 4.5 s after, it's gone.
  - **Screenshots in test runs** can catch real keystrokes: the test window takes focus, and typing elsewhere opened the Online panel / Esc menu in two warg shots.
- **N28 (2026-11-16) — the forgery sector, drops, one-way and locked doors (Phase 8.10):**
  - **Open doors you could walk through (reported):** reproduced with `FLOWERBOX_AUTOPILOT=still FLOWERBOX_SWING=1 FLOWERBOX_WALK=-1,0` (the player walked through door 1's open leaf from x 2.14 to −2.14; opened instantly it blocked at 1.67). The leaf was a static body moved by its transform with `ColliderDisabled` toggled during the swing, and after the swing physics kept the old pose. Now a separate static `DoorBlocker` is spawned wherever the leaf comes to rest and despawned while it swings; verified it stops at 1.67 after a swing.
  - **One-way doors:** `Door::opens_from(pos)`: the opener must be on the side opposite `opens`. From the wrong side E shows "This door only opens from the other side" (door icon); online `WorldEvent::DoorTried` → others get "Guest 1 tried a door: it only opens from the other side". Locked doors (`key: Some(ForgeKey)`): "Locked: it needs the Forge Key"; with it, "Unlocked with the Forge Key". Door 1 opens N, 2 W, 3 E (stairs 2 → vault), 4 N (locked), 5 S (forge loop → T room: only from the forgery).
  - **Builder:** a door also cuts a lintel where no wall would be (door 3 sits on stairs 2's lower end). Ceilings get holes (`subtract_rect`) for sky grates; floor slabs are cut away under stairs.
  - **Drops** (`enemy::DropDef { item, chance, unique }` in the level file, `Drops` on the enemy): rolled when a non-remote enemy becomes a corpse (`drop_loot`), spawned as a pickup by the body; online `WorldEvent::Dropped` (protocol v7). Unique ids are listed in `UniqueDrops`; taking one (local grant or a guest's request on the host) records it in `ClaimedDrops` (saved as `claimed_drops`). Tested (`wargkill`): kill → "dropped Forge Key" → take → world reset → kill again → no drop; after a restart with the save, no drop either. Online: the guest sees the drop and "Host picked up Forge Key".
  - **Items:** Well-Forged Sword (LightSword class, base 24 vs the Longsword's 20, blade 1.08 vs 1.03, brass guard, fuller; rare), Forge Key (Key kind; rare). Rare pickups glow cold blue and brighter.
  - **Forgery props** (`level/props.rs`): anvil on a stump with a hammer; forge = stone hearth with glowing coals, five flickering flames and a fire light (`arena::spawn_fire`, counted with the torches for shadows), brick hood and chimney, quench trough; a 4.4 × 3.4 m sky grate (iron bars, an emissive daylight plane above, a slanted shadow-casting `SpotLight` sunbeam, 90 drifting dust motes). Steam: vents on the forge and trough emit puffs that rise, swell, fade (alpha steps) and drift toward the nearest grate.
  - **Level walk** (`level` autopilot, now 59 waypoints): door 5 refused from the T room, door 3, door 4 locked → key → opens, stairs 3 up, peak, dip, nook, alcove, sword picked up, door 5 opens from the forgery, back in the T room: all reached ("open doors [1, 2, 3, 4, 5]; has the sword: true"). Online (host `menu`, guest `level`): the host logs every door, the refused door, the sword pickup.
- **N29 (2026-11-16) — multiplayer with 4–5 players (Phase 8.11):**
  - **Cause of the missing enemies:** `Datagram::Snapshot` carried every body (players + 21 enemies) in one QUIC datagram: 1152 bytes with host + 1 guest (measured), more with each player. QUIC datagrams over the internet / relays fit ~1150–1200 bytes; bigger ones fail, and `send_datagram` errors were ignored, so guests got no updates at all once the session grew. Guests also despawned every puppet missing from a snapshot, so any lost or partial snapshot made bodies vanish and respawn ("weird spawning"), restarting their animations. The 5th player was refused (`MAX_PLAYERS` was 4).
  - **Now** (protocol v8): `chunk_bodies` splits snapshots into ≤ `MAX_SNAPSHOT_BYTES` (900) datagrams; players are sent every tick (30 Hz), enemies only when they changed (`changed`: moved, turned, acting, HP, grounded; a corpse after 2 s counts as still) or every `ENEMY_KEEPALIVE` (1 s). `Datagram::Roster` (all ids) every 0.5 s; guests drop puppets not on it or silent for `PUPPET_TIMEOUT` (3.5 s). Average snapshot datagram: 304 bytes (was 1152 per tick). Send failures are logged (`datagram of N bytes not sent`). QUIC idle timeout 10 s (default 30) so a crashed player is noticed sooner.
  - **Deaths:** `run_death_sequence` writes `WorldReset` only offline. Verified with 5 players: the host died mid-fight and respawned; no "enemies reset", killed Hollows stayed dead.
  - **Joining:** `PlaceNearHost` → `place_guest` puts the guest on a ring around the host's body (first direction free of walls for 1.8 m), once the host's state arrives. Before, guests always appeared at the arena start.
  - **Tests** (`mp.sh`-style: host + 4 guests on one machine, `FLOWERBOX_NET_RELAY_ONLY=1` for the guests): all 5 admitted (ids 2–5), every guest sees 4 players + 21 enemies, exactly 25 puppets appeared per guest over 60 s (no flicker), 0 datagram failures, 0 puppet timeouts; with `FLOWERBOX_MP_FIGHT=1` the host and guests kill Hollows in the middle hall, guests' hits forwarded; shared rest (`netrest`) still completes. Not tested: real separate networks. n0's public relays logged one "rate-limiting" warning per instance; before, the host was throttled continuously.
- **N30 (2026-11-16) — hands, axe, abilities, door push, feral warg (Phase 8.12):**
  - **Shift+Tab** (report: the inventory's cursor invisible / back to the game) couldn't be reproduced: injected input (`FLOWERBOX_KEYS`, real `KeyboardInput` / `MouseButtonInput` messages) opens and closes it correctly with Shift held. Suspect: the GNOME Wayland pointer lock. Now `toggle_cursor` makes the cursor match the state every frame while focused (and re-applies it when focus returns); the inventory toggle ignores a second toggle within 0.25 s. Needs confirmation on the real desktop.
  - **Wielding** (`weapon::Wielded`): `right` (fists when empty), `left` (a weapon, not a shield), `two_hand`. `EquipWeapon { right, left, two_hand }` rebuilds the models (two-handed: only that weapon, in the right hand). `Equipment::two_hand` (saved, replicated `BodyState::two_hand`), `equip_to` (a weapon into the left hand), `TwoHandRequest` (refuses shields: "Only weapons can be held in both hands"). Stats: two-handed attack ×`TWO_HAND_DAMAGE_MULT` (1.2), guard = the weapon's `guard_block` (swords 0.35–0.5, axes 0.25–0.3), the shield is put away.
  - **Attacks with hands:** `ActionRequest::LightAttack(AttackHand)` / `HeavyAttack(AttackHand)`; `ActiveAction::hand` (`with_hand(Both)` × `DUAL_STAMINA_MULT` 1.5); combos continue per hand; left / both refused without a left weapon / a matching pair. Hit detection sweeps each blade (`Wielded::blades`: HandR / HandL), each can hit a target once per swing. Replicated (`NetAction::hand`).
  - **Poses:** `HumanoidPose::mirrored` (swap L/R bones, reflect rotations q → (x, −y, −z, w)); left = mirrored right swing; dual = right arm + mirrored left arm, chest twist averaged out. Idle with a left weapon: the mirrored guard of its class. Two-handed: `reach_left_hand` (two-bone IK; elbow twist chosen down/out) puts the left hand 12 cm below the right fist on the handle; guarding raises the weapon across the body (`weapon_guard_arm`).
  - **Input:** the right button = `Offhand::of(wielded)`: guard / left attacks / dual. The wheel (`Action::TwoHand`, rebindable) + a button toggles that hand; tapped alone → `PlayerInput::lock_on_tap` (lock-on on release). F / L2 = `Action::Ability`.
  - **Axe class** (`weapon/axe.rs`): combo overhead chop → diagonal chop → heavy overhead (startups 0.44–0.62 s, recoveries 0.44–0.6 s; sword: 0.15 / 0.25), heavy from far behind the head, leaping running chop, plunging jump chop. Test: the tip starts above 2 m and ends below 1 m in front with < 0.5 m sideways travel. Rusted Axe moved to the class; Woodcutter's Hatchet (axe) lies in the arena (pickup 3).
  - **Abilities** (`ability.rs`): `AbilityDef { name, icon, stamina, effect }`, `AbilityEffect::Strike(AttackSpec)` (runs through the attack pipeline: `ActiveAction::attack` returns it) or `Buff { timings, attack_mult, duration }` → `AttackBuff` (applied at the active phase, ticks down, multiplies weapon damage). Lunging Thrust (32 stamina, ×1.9, 2.6 m lunge), Warcry (25 stamina, ×1.25 for 10 s, arms-up roar pose).
  - **Verified** (`FLOWERBOX_AUTOPILOT=weapons`, at the dummy): right 20, left (Shortsword) 16, dual 22.5 stamina → hits 20 + 16, dual jump 37.5 stamina → 36 + 29, dual axes 27 stamina → 24 + 20, two-handed axe 29 (= 24 × 1.2), after Warcry 36 (× 1.25), Lunging Thrust 48. Online (guest runs it, host `menu`): every hit forwarded with the same numbers; the host's puppet shows the guest's hands and two-handing. Screenshots: dual axes raised overhead, two-handed grip (idle and chop), Warcry.
  - **Door push:** `ActionKind::OpenDoor` (0.4 s reach, 0.15 s push, 0.5 s back; the free left palm and shoulder; a small step in); `PushingDoor` sends the `DoorRequest` at the push; a hit cancels it. Others get "Guest 1 opened a door". Death notifications were already working (screenshot: "Guest 1 was composted" on the host).
  - **Feral warg:** `QuadrupedLook::shag` / `shag_length`: flattened cones, swept back, random length / tilt / lean / width from a fixed seed (the same on every machine): a ragged spine ridge, a thick mane around shoulders and neck, two rows on the flanks, tufts on haunches and shoulders, a ragged tail.
  - **Test runs without vsync:** after the reboot every run (also an older release build) crawled at 4 FPS: GNOME throttles hidden windows. Autopilot / screenshot runs now use `PresentMode::AutoNoVsync` (160 FPS debug); normal play keeps vsync.
- **N31 (2026-11-16) — corpses, inventory, the cathedral, enemies as data, dormancy (Phase 8.13):**
  - **Corpse bug:** `corpses_dont_block` inserted `ColliderDisabled` at death and `enemy_death` inserted it *again* when the body vanished. A re-insert fires `Discard` (Bevy 0.20 lifecycle), and avian's `add_to_tree_on::<Discard<ColliderDisabled>>` re-added the collider to the collider tree while it kept the marker, so its box was never updated: an invisible capsule where the body vanished (the hidden corpse isn't despawned until a world reset). Fix: `EntityCommands::set_solid` (checks at apply time). Also dead characters' own move-and-slide excludes characters (the living shoved bodies around). Test: `enemy` autopilot walks through the vanished body — the old code deflected the player 0.51 m sideways, now 0.03.
  - **Inventory:** `InventoryMenu { focus, slot, picking, popup }`; `EquipSlot::accepts / candidates`; `item_options`; `ItemActions` system param shared by keys and mouse observers. Hover selection uses `PointerMove` (rebuilt nodes under a resting mouse fired `PointerOver` and snapped the keyboard selection back). Real-key test: `FLOWERBOX_KEYS` (now with arrows) → slot → left hand → shield → legs ("nothing fits") → axe → options → throw out. Two windows: the unfocused one gets no keys, so the online throw test uses `FLOWERBOX_THROW="9:3"`: host logs "Guest 1 threw out Rusted Axe", both see it at the same spot.
  - **AI:** `Senses::gather / sees / watched`, `Steering::patrol / unstick`, `chain_combo`, `Rng`. `FighterProfile` (Hollow = the old constants; behaviour verified unchanged by the `enemy` and `warg` autopilots). Specials roll their chance per decision with a short retry delay; a dash may fire during recovery from afar. Stalker: frozen while the target faces it within ~50° beyond 3.2 m.
  - **Swing angles vs. body size:** the Saint (×1.6) swung greatsword keys authored for humans: the blade passed at 1.9–2.3 m, over a player's head — it never hit. Greatsword sweeps now descend; `big_blades_reach_a_player_in_front` checks the FK sweep of Hollow, Acolyte and Saint passes through a person in front. Lesson: a scaled body needs its own (lower) swing keys, or the AttackSpec must aim.
  - **Boss test** (`FLOWERBOX_AUTOPILOT=boss FLOWERBOX_POS=0,138,3.14`, 5000 HP, +12 scripted damage / 0.5 s): poise breaks every ~5 hits, Whirlwind (33 per turn), Ground Slam (54 + wave 30), Shriek (14, unblockable), enrage at 50 %, death after ~36 s, "HALLOWED FOE UNMADE", the mist clears, Penitence drops. For a 100 HP player a full slam nearly kills: hard, but every special has a ≥ 0.6 s wind-up and rolls through.
  - **Cathedral walk** (`FLOWERBOX_AUTOPILOT=level FLOWERBOX_ROUTE=cathedral`): 43 waypoints, doors 11–15, three bells in order → "riddle 1 solved", the shortcut, the lectern, the crypt, sunflowers 11–13, the fog gate. Found on the way: the fog gate didn't work in enemy-less autopilots (`BossPlugin` lived behind `EnemyPlugin`'s autopilot early-return) — it now always loads. Online (host idle, guest walks): the guest's rings solve the rite on the host and the door opens for the guest.
  - **Performance** (release, uncapped, RX 6800 XT, `FLOWERBOX_FPS_LOG=1`, `FLOWERBOX_NO_DORMANCY=1` to compare): arena 146 → 168 FPS with dormancy (44 of 46 enemies asleep), cathedral narthex 126 → 143 (41 asleep). Both levels load at once (46 enemies, 158 point lights). The user's idea (render only behind opened doors or within 20 m) was considered: a door/room graph would need room membership for every enemy and break for open halls; the nave is 40 m long, so a 20 m cut-off makes enemies pop in where you can see them. Distance with hysteresis needs no level knowledge, works for any number of co-op players (the host decides by every player, a guest only draws/animates near itself) and costs nothing when asleep (no brain, controller, animator or draw). Lights aren't culled; at 158 lights the frame is still ~6 ms.
  - **Not done / known:** puppet spins on guests step at 30 Hz; one ceiling per level (crypts are tall); no pathfinding (enemies sidestep obstacles); the player has no poise; chest/hand/leg armour isn't replicated to puppets (only hands + head); thrown items vanish on restart.
- **N32 (2026-11-16) — stops, pivots and momentum, Souls-style (Phase 8.16):**
  - **What Souls games do** (player accounts and modding notes, no official frame data): Dark Souls (since the first) and its successors have a short **skid-to-stop** when you stop or change direction while sprinting, and a turn when reversing; rolls stay available, and an Elden Ring mod widens the roll cancel during turn animations, which suggests even that is felt as too sticky. At a jog there's barely any slide. The lesson matches general locomotion design: a good stop *absorbs momentum* (a lean that corrects back to neutral), a pivot plants a foot instead of snapping, and every bit of realism costs responsiveness ("floaty", the Witcher 3 complaint).
  - **So:** small, physical slides (≤ 0.8 m), asymmetric rates (only slowing down is softened), a pivot that's over in ~0.25 s, and *nothing* that blocks input — the animation sells the weight, the controller stays responsive. Rejected: root-motion stop / turn animations as actions (they'd lock input like the criticised turn animations), and global momentum (slow acceleration too — sluggish dodging into position).
  - **Measured** (`FLOWERBOX_AUTOPILOT=skid`, 60 Hz): sprint stop 7 → 0 m/s in 0.23 s over 0.8 m; jog stop 0.3 m; sprint reversal: facing 90° → 270° in 0.2 s, 0.7 m of slide, back at full speed 0.15 s later. The level route still walks through.
  - Sources: [Steam: DS3 sprint skid](https://steamcommunity.com/app/374320/discussions/0/1290691937720311197), [Nexus: Elden Ring turn-roll mod](https://www.nexusmods.com/eldenring/mods/3049), [Game Developer: analyzing core character movement](https://gamedeveloper.com/design/analyzing-core-character-movement-in-3d), [responsiveness vs realistic animation](https://engineered.at/articles/striking-the-balance-exploring-the-challenges-of-responsive-movement-and-realistic-animations-in-video-games), [UE forums: starts, stops, pivots with inertia](https://forums.unrealengine.com/t/how-to-implement-realistic-acceleration-starts-stops-and-pivot-animations-with-inertia-in-ue5/2744897).
