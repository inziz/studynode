# StudyNodes — Agent Guidelines

Before making any significant UI, layout, or architectural change in this repository, read and follow these rules.

## 1. Analyze Existing Styling First

Before designing or modifying any screen, panel, card, or control:
- Inspect the existing design tokens and components in:
  - `crates/studynodes-app/src/theme.rs` (theme palettes, semantic surfaces `bg_canvas`, `bg_card`, `bg_control`, `bg_input`, borders, accents, and typography constants)
  - `crates/studynodes-app/src/icons.rs` (Phosphor Regular 256x256 SVG icons)
  - `crates/studynodes-app/src/settings_view.rs` (sidebar settings layout, switches, segmented controls, hairline rows)
  - `crates/studynodes-app/src/dashboard_view.rs`, `canvas_view.rs`, and `inspector_view.rs`
- Match the app's warm, minimal, high-craft aesthetic across **all** supported themes (`WarmCharcoal` default, `Dark`, `Light`, `TokyoNight`, `SolarizedDark`, `SolarizedLight`).

## 2. Interactive Controls — Never Use Basic Pill Labels for Options

**Do not** represent boolean settings or interactive option selectors as plain outlined text pills/badges (for example: `[• Wire Flow: Animated]`, `[Auto-Vertical: Off]`, or loose outlined word chips for small option sets).

Use proper, unmistakable interactive controls instead:
- **Boolean on/off settings** → Use a sliding **toggle switch** (`toggle_switch` / `toggle_switch_tx` in `settings_view.rs`), paired with a left-aligned row title and muted state description (`setting_row_with_desc`).
- **Mutually exclusive choice sets** (2–6 options like Language or Dock Position) → Use a unified **segmented control** (`segmented_track` + `segmented_option`): an inset track (`bg_input` + `border_subtle`) with a raised active segment (`bg_card` + `border_default`).
- **One-shot actions** (e.g., Check for updates, Import, Settings) → Use proper buttons (`bg_control` / `hover_subtle`) paired with crisp **Phosphor icons** from `icons.rs`.

## 3. Iconography & Chrome Consistency

- Top-bar, dashboard, dock, and overlay action buttons should use Phosphor icons from `icons.rs` (never duplicate an icon with an ASCII `+` or raw Unicode glyph next to it).
- Prefer compact icon buttons in dense toolbars (like the AI chat header or quiz overlay controls) and icon + label buttons in top-level navigation/actions (like Dashboard `Import` and `Settings`).

## 4. Runtime Performance & Storage Rules

- Keep `spec.poll` free of continuous `view()` rebuilds: use Rune `Live` bindings (`live_offset`, `live_scale`, `live_trim`) with `tx.live_only()` and idle auto-sleep so Rune enters `window::rest` when idle.
- Memoize heavy card subtrees (`rune::memo`) and keep long-lived caches bounded.
- Store persistent user projects and preferences in `<Documents>/StudyNodes` via `platform_paths.rs` (never hardcode `./study_projects` as the default). Keep tests isolated in `TempDir` via `AppState::with_projects_dir(...)`.
- Preserve `#![deny(unsafe_code)]` in `crates/studynodes-app/src/main.rs` (isolate any required platform FFI in dedicated modules).
- Always run `cargo test --workspace --no-fail-fast` and ensure **0 compiler warnings** before committing or releasing.
