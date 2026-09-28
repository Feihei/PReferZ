# Plan: rusqlite 0.31 → 0.40 升级

- **状态**: ✅ **已结项**（2026-09-28）——已实施（2026-09-20，`4e40667` 修订 + `a72c508` 升级）、静态门全绿，手测验收（真实 `.prz` 往返）**已通过**。交付归档见 [`.agents/CHANGELOG.md`](../CHANGELOG.md) §依赖大版本升级 / §人工复验清账（本文档保留作破坏性变更核对表参考）
- **创建日期**: 2026-09-20
- **目标版本**: rusqlite 0.40.2（当前 0.31.0，`features = ["bundled"]`）
- **跨度**: 0.31 → 0.40，跨 9 个 minor（0.32…0.40）
- **前置调研**: 已逐条比对 rusqlite 官方 GitHub Releases（0.32.0–0.40.2）破坏性变更，并对照全仓 rusqlite 实际用法（见 §2）

## 1. 背景与目标

`rusqlite` 是 `.prz`（SQLite + sqlar）文件读写的底座，blast radius 理论上最大，但**本项目对它的用法极窄**（仅 `preferz-fileio/src/prz.rs` 一个文件，约 20 个调用点，全是最基础的 API），因此逐条核对后判断：**API 层面近乎零改动，主要实质变化是捆绑（bundled）SQLite 引擎从 ~3.45 跳到 3.53.2。**

**目标**：升到 0.40.2，获得更新的 SQLite 引擎（错误修复 / 性能 / 安全），保持行为等价，不改任何文件格式语义。

**非目标**：不引入新特性（vtab / serde 扩展 / 异步）；不改 `.prz` schema；不换后端。

## 2. 现状用法盘点（唯一使用点 `crates/preferz-fileio/src/prz.rs`）

| API | 位置（file:line） |
|---|---|
| `use rusqlite::{params, Connection}` | prz.rs:4 |
| `Connection::open` | 48 / 87 / 577 / 717（测试） |
| `conn.execute_batch(schema)` | 88 / 578 / 718 |
| `conn.query_row(sql, params, closure)` + `row.get::<_,String>(0)` | 50-53 / 70 |
| `conn.execute(sql, [])`（含 `ALTER TABLE`、`VACUUM`、`DELETE … LIKE`） | 76 / 211 / 619 |
| `conn.transaction()` → `tx.execute` / `tx.prepare` / `stmt.execute(params![…])` / `stmt.query_map([], \|r\| …)` | 133-205（保存） |
| `conn.prepare(sql)` → `stmt.query_map([], \|row\| {row.get…})` | 228-230 / 279-280 / 299-300（加载） |
| 绑定类型 | 仅 `String`/`&str`(TEXT)、`i64`（含 `bytes.len() as i64`）、`Vec<u8>`/`&[u8]`(BLOB)、空 `[]` |

**确认未使用**（逐条 grep 过）：`prepare_cached`、`params_from_iter`、`types::Value`、`limits()`、`load_extension`、`create_scalar_function`/`hook`、`busy_timeout`、`open_in_memory`、任何 `get::<_, usize/u64>` 绑定。
`prz.rs:376/633` 的 `42u64`/`7u64` 是应用层纹理 id（进 serde JSON 存进 transform 列），**不经 SQL 绑定**。

## 3. 破坏性变更逐条对照（0.32→0.40，取官方 Releases）

| 版本 | 破坏性变更 | 是否触及本项目 |
|---|---|---|
| 0.33.0 | `execute` 校验不得含尾随语句（#1679） | **需核**：所有 `execute`/`execute_batch` 语句确保单句；`execute_batch(schema)` 允许多句（batch 语义未变） |
| 0.33.0 | `prepare` 校验多语句报错（#1680） | **需核**：`insert_item_query()`/`select_*_query()` 均为单语句（大概率满足） |
| 0.34.0 | 语句缓存改为可选，`prepare_cached` 受 `cached` 默认特性门控（#1682） | 否（只用 `prepare`） |
| 0.34.0 | 默认禁用 `u64`/`usize` 的 `ToSql`/`FromSql`（#1732） | 否（不绑定 usize/u64，`len` 已 cast i64） |
| 0.34.0 | 最低 SQLite 版本抬到 3.34.1（#1733） | 否（`bundled`，随包 3.4x/3.5x ≫） |
| 0.35.0+ | vtab 系列 breaking（connect/create/best_index/宏改构造器） | 否（无虚拟表） |
| 0.35.0+ | 注册闭包 hook 要求 `Connection` 被独占（#1764） | 否（无 hook） |
| 各版本 | 多次 bundled SQLite 版本抬升（3.46.0@0.32 → **3.53.2@0.40**） | **是**：这是本次唯一实质变化，见 §4 |
| 0.40.0 | MSRV 抬到 1.88 | 否（本机 rustc 1.98 ✓） |

**结论**：`prz.rs` 用到的 `Connection::open/execute/execute_batch/query_row/transaction/prepare` 与 `params!` 签名在 0.31→0.40 间**均无破坏**；唯二"需核"是 0.33 的单语句校验（预期满足）。

## 4. 真正的风险与验证点

1. **bundled SQLite 3.45 → 3.53.2**（行为/默认变化）：
   - schema/`PRAGMA`/`VACUUM`/`INSERT OR REPLACE`/`LIKE` 都是极稳定语义，预期不变；但要跑既有 `prz::tests::*`（往返 + 旧 items 无 group 列迁移 + orphan sqlar 清理 + viewport 回落默认）确认全绿。
   - **重新打开用户机器上已存的 `.prz`** 做真实往返（引擎只向下兼容读，写出的库文件 SQLite 格式跨版本稳定）。
2. **0.33 单语句校验**：`cargo check` 不会报（运行时行为），故必须**靠测试覆盖** save/load/legacy-migration 全流程；手测导入含多图的项目文件。
3. 版本要求（features）不变：仍 `bundled`，无需新增 feature（未用 `prepare_cached`）。

## 5. 前置条件 / 环境

1. **网络**：`cargo update` 走 tuna 镜像；如遇 `CRYPT_E_REVOCATION_OFFLINE`，临时 `CARGO_HTTP_CHECK_REVOKE=false`。
2. **分支**：不开分支，直接在 main 上实施——改动预期为单 commit（`Cargo.toml` + `Cargo.lock`），回滚用 `git revert` 即可，分支不提供额外保护。唯一前提：动手前工作区干净（"先提交再实现"惯例已覆盖）。
3. **基线**：改前在 HEAD 跑通 `fmt/clippy/test` 与 `cargo run` 打开/保存一个真实 `.prz`，作为回退对照。
4. **工具链**：rustc 1.98 ≥ 0.40 MSRV 1.88 ✓。

## 6. 实施步骤（不开分支，直接在 main；尽量单 commit，API 若需改再拆分）

1. `Cargo.toml`：`rusqlite = { version = "0.40", features = ["bundled"] }`；`cargo update -p rusqlite`。
2. `cargo check --workspace` → 若编译器报签名/类型不符再定点改（预期不报，因用法未变）。
3. 按报错修 `prz.rs`（若有）；重点确认 0.33 单语句校验：`insert_item_query()`、`select_all_items_query()`、`select_metadata_query()` 内部不含第二条 `;` 语句。
4. 收尾：`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`（尤其 `prz::tests`）。
5. commit：`chore: bump rusqlite to 0.40`（如涉及源码再补一条 `fix:`）。

## 7. 验证

- 静态门：`cargo fmt --all --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`（含 rusqlite 相关集成测试）全绿。
- 手测（`cargo run -p preferz`）：新建 → 导入图片/多元素 → 保存 `.prz` → 关闭 → **重新打开该 `.prz`** 校验元素/图片/视口/分组/编号全还原；旧版本保存的 `.prz`（若留有）也重开一次；VACUUM 后文件正常。

## 8. 验收清单

- [x] `Cargo.toml`/`Cargo.lock` 中 rusqlite = 0.40.2，`bundled` 保留
- [x] 无新增 feature（未用 `prepare_cached`；hashlink 0.12 新传递依赖 `rsqlite-vfs`/`sqlite-wasm-rs` 为 lockfile 解析产物，非本项 feature）
- [x] `cargo clippy -D warnings` 零警告、`cargo test --workspace` 全绿（202 tests）
- [ ] 真实 `.prz` 保存→重开往返无损（含旧文件）
- [x] `.prz` 文件格式语义未改（schema/`metadata.format` 仍 `prz`，schema.rs 零改动）

## 9. 回滚

不开分支；`rusqlite` 版本改动仅触及 `Cargo.toml` + 可能的 `prz.rs` 微调，回滚 = `git revert`。bundled 引擎降版本不影响已写出的 `.prz`（SQLite 库文件跨版本兼容读）。
