# ADR-0004: `.prz`（SQLite + sqlar）是唯一文件格式

- 状态：已接受
- 日期：2026-08（`.bee` 兼容层移除，commit `62692a8`）
- 参考：[spec §5.4](../specs/preferz-spec.md)

## 背景

项目初期为迁移存量用户兼容 BeeRef 的 `.bee`。实际无人有存量，兼容层反而把两套
schema（9 列 INTEGER 主键 vs 5 列 UUID 主键、分列 vs JSON transform）搅在一起。

## 决策

- `.prz` 为**唯一格式**，`BeeFile::open()` 校验 `metadata.format == 'prz'`，不符直接报错
- `.prz` = SQLite，三张表：
  - `items`：5 列，UUID 字符串主键，transform 存单列 JSON
  - `sqlar`：`name` / `sz`（未压缩大小）/ `data`（压缩 blob）——嵌入图片资产
  - `metadata`：`format` 恒为 `prz`、视口状态、`next_z`
- Item ID 一律 `uuid::Uuid`，不用自增整数
- 未来线性对象新字段走 `#[serde(default)]` 缺省迁移（transform JSON），不做版本号升级脚本

## 后果

- ✅ 单格式单 schema，fileio 面积减半
- ⚠️ `.bee` 文件不再可打开（属有意为之）
- ⚠️ `PRAGMA user_version` 等 BeeRef 特有行为不复存在
