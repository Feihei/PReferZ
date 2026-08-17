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

## 必跑检查（提交前 / CI 强制）

`cargo fmt` 和 `cargo clippy` 已接入 CI（`.github/workflows/ci.yml`），PR 不通过会失败。本地提交前请先跑一遍：

```
cargo fmt --all --check         # 格式化检查，未通过时运行 cargo fmt 自动修正
cargo clippy --workspace --all-targets -- -D warnings   # 把警告当错误，必须零警告
```

约定：
- 所有代码必须保持 `cargo fmt` 干净（提交前 `cargo fmt`）。
- `cargo clippy -D warnings` 必须零警告；新增代码不要引入新的 lint 警告。
- 不要用 `#[allow(...)]` 静默绕过 lint，除非有充分理由并注明。

本地已安装 git pre-commit 钩子（`.git/hooks/pre-commit`），`git commit` 前自动跑 `cargo fmt --check`，不通过会阻止提交。注意 `.git/hooks/` 不入库，clone 到新机器后需重新放置该钩子（可运行 `cargo fmt --all --check` 替代）。

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

## 发布流程

使用 `cargo-release` + GitHub Actions 自动化发布。

### 前置条件

- `cargo install cargo-release`（已安装 v1.1.4）
- 有权限 push 到 `origin` 远程仓库
- 本地网络环境限制：需要设置 `CARGO_NET_OFFLINE=true`（见下方说明）

### 发布步骤

确保当前工作区干净（无未提交更改）后，运行发布脚本：

```powershell
.\scripts\release.ps1 patch          # 发布 patch 版本（0.1.0 → 0.1.1，推荐）
.\scripts\release.ps1 minor          # 发布 minor 版本（0.1.0 → 0.2.0）
.\scripts\release.ps1 major          # 发布 major 版本（0.1.0 → 1.0.0）
.\scripts\release.ps1 0.2.0          # 直接指定版本号
.\scripts\release.ps1 patch -DryRun  # 仅预览，不实际发布
```

或者手动执行（不通过脚本）：

```bash
CARGO_NET_OFFLINE=true cargo release patch --no-confirm
```

### 发布流程说明

1. 脚本先运行 `cargo fmt --all --check` + `cargo clippy` 确保代码质量
2. `cargo release` 自动执行：版本 bump → 提交 → 打 tag → push
3. push 触发 GitHub Actions `.github/workflows/release.yml`，构建三平台产物并创建 GitHub Release

### 关于 `CARGO_NET_OFFLINE=true`

本项目 `publish = false`（不发布到 crates.io），但 `cargo-release` 在版本 bump 阶段会尝试访问 `index.crates.io` 检查版本冲突。本地网络无法直连 crates.io，且该检查对不发布的项目无意义，因此设置 `CARGO_NET_OFFLINE=true` 跳过。

对应配置：

- `Cargo.toml` — `[workspace.metadata.release]` 段
- `scripts/release.ps1` — 发布脚本
- `.github/workflows/release.yml` — GitHub Actions 构建 + Release

## Spec

Full design spec with phases, data models, and API details: `.agents/preferz-spec.md`. Read it before implementing any feature beyond a bug fix.
