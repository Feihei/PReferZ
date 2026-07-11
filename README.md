# PReferZ

Picture Reference Zoomer - 一个用 Rust 编写的极简参考图聚合桌面应用。

## 特性

- 无限画布，支持平移和缩放
- 图片导入、移动、缩放、旋转
- 文本便签
- 选中与变换手柄
- 撤销/重做
- 图层管理
- 保存/加载 `.bee` / `.prz` 格式

## 技术栈

- **Rust** (stable, >= 1.75)
- **GUI**: `egui` + `eframe` (glow backend)
- **2D geometry**: `euclid`
- **File I/O**: `rusqlite`, `image`, `rayon`

## 开发命令

```bash
cargo check --workspace          # 快速类型检查
cargo test --workspace           # 单元测试
cargo run -p preferz             # 运行应用
cargo build --release -p preferz # 构建发布版本
```

## 项目结构

```
preferz/
├── crates/
│   ├── preferz/            # 二进制入口
│   ├── preferz-core/       # 核心领域模型
│   └── preferz-fileio/     # 文件 I/O
└── tests/                  # 集成测试
```

## 开发计划

- Phase 1: 骨架窗口
- Phase 2: 图片项
- Phase 3: 交互
- Phase 4: 持久化
- Phase 5: 撤销与高级操作
- Phase 6: 打磨

## 许可

MIT
