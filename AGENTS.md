# PReferZ — Agent Instructions

## Stack

- **Rust** (stable, >= 1.75), edition 2021
- **GUI**: `egui` + `eframe` (glow backend)
- **2D geometry**: `euclid` (parameterized `CanvasSpace` / `ScreenSpace`)
- **Undo**: custom `undo` crate wrapping `History` / `Command`
- **File I/O**: `rusqlite` (`.bee` / `.prz` SQLite + sqlar), `image`, `rayon`
- **File dialog**: `rfd`; **clipboard**: `arboard`; **config**: `confy`
- **Workspace**: `crates/preferz` (binary), `crates/preferz-core`, `crates/preferz-fileio`

## Commands

```
cargo check --workspace          # fast typecheck
cargo test --workspace           # unit tests
cargo run -p preferz             # run app
cargo build --release -p preferz # release binary
```

## Architecture (3 crates)

```
preferz (binary, eframe::App)
  ├── preferz-core        Item, Transform, Scene, Selection, Commands, Arrange
  └── preferz-fileio      BeeFile, ImageLoader, Export, Assets
```

`PReferZApp` holds `Scene`, `UndoStack`, `ViewportState`. `update()` dispatches each frame.

## Must-Know Conventions

- **All item mutations go through the undo stack.** Never mutate `Item.transform` or `Scene.items` directly in the UI layer.
- **Item IDs are `uuid::Uuid`.** Do not use integer auto-increment ids.
- **Three coordinate systems**: `ScreenSpace` (pixels), `Viewport` (screen/zoom), `CanvasSpace` (world). Use `euclid` types — never cast between them with raw `f32`.
- **Texture ownership**: `egui::TextureHandle` lives on `egui::Context`. Create/release textures inside `update()`. Do not hold bare `TextureId` across frames without registration.
- **Undo "preview" mode**: interactive drag/scale rotates items directly, then `push(cmd)` on release with `skip_first_redo: true`. The `undo` crate has no built-in skip — this field is on the `Command` impl.
- **BeeRef `.bee` compat**: `USER_VERSION=2`, `sqlar` table with `sz` (uncompressed size) + `data` (compressed blob). `.prz` starts at `USER_VERSION=3` with a `metadata` table.
- **Image decode runs on background threads** (`std::thread::spawn` or `rayon`), post result via channel, call `egui_ctx.request_repaint()`.
- **Canvas rendering**: egui is immediate-mode — viewport culling is mandatory. LOD (thumbnail textures at small zoom) is Phase 5+.

## Gotchas

1. **egui has no retained mode.** Every visible item redraws every frame. If you add an item, call `request_repaint()` or rely on event-driven repaint.
2. **Text editing** uses an egui `TextEdit` widget overlaid at the item's screen position — not a native editor.
3. **`euclid::Transform2D`** uses column-major convention. Rotation is in radians. `with_anchor()` is the correct way to scale/rotate around a point.
4. **`rfd::FileDialog`** blocks the main thread on some platforms — wrap in a thread if opening large directories.

## File Conventions

- Source files: one public struct/enum per file, module re-exports in `lib.rs` / `mod.rs`.
- Tests: unit tests co-located in `src/` (use `#[cfg(test)] mod tests`), integration tests under `tests/`.
- Commit convention: conventional commits preferred.

## Spec

Full design spec with phases, data models, and API details: `.agents/preferz-spec.md`. Read it before implementing any feature beyond a bug fix.
