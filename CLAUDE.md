# CLAUDE.md

Soulslike prototype in Rust (Bevy 0.20 + avian3d 0.8 + leafwing-input-manager 0.22).

- **Start every session by reading `ROADMAP.md`** — it holds the design spec, controls, architecture, phase checklist and decision log. Continue at the first unchecked task; tick boxes and log decisions as you go.
- Code map: `character/` = generic movement/actions/stats, `humanoid/` = reusable body + animations, `weapon/` = weapon classes (movesets) + weapon definitions (models), `player/` = the player built from these, `healing.rs` / `hearth.rs` = Healing Ale, rest and death/respawn, `ui/` = HUD (player-facing text in `ui/strings.rs`; the Hearth is drawn as a sunflower) (see ROADMAP §4, N13, N15, N17).
- `PROMPT.md` is the user's original request (cleaned up). Treat it as the source of truth for intent.
- Problems and findings go in the **Notes** section of `ROADMAP.md` (N1, N2, …). Check the open ones first.
- Commit every finished step with a descriptive message (no need to ask). Branch when it's useful.
- Keep the game runnable (`cargo run`) at the end of every phase. Gameplay constants go in `src/tuning.rs`; per-weapon-class attack numbers go in `src/weapon/<class>.rs`.
- When running the binary directly (autopilots, screenshots), set `BEVY_ASSET_ROOT=.` so `assets/` is found.
