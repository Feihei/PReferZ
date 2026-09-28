# Release Notes

每个版本一份 release note，**文件名必须与发布 tag 完全一致**（`cargo-release` 默认 `v` 前缀）：

```
.agents/release-notes/
├── README.md       # 本文件，不参与发布
├── v0.1.1.md       # → tag v0.1.1
└── v0.1.2.md       # → tag v0.1.2
```

`release.yml` 的 `release` job 读取 `${GITHUB_REF_NAME}.md` 作为 Release body；
**文件不存在则构建直接失败**，这是刻意的——避免发布出一个空 note 的 Release。

## 写 note 的时机

release note 必须在**打 tag 之前**写好并提交。tag 一旦推送，CI 立刻开始构建，
没有机会补文件。

推荐流程：

1. 写 `.agents/release-notes/v<next>.md`，从 [TEMPLATE.md](TEMPLATE.md) 复制
2. 提交（可与功能提交分开，便于 `git log v0.1.0..HEAD --oneline` 归纳）
3. `.\scripts\release.ps1 patch` —— 脚本会先校验 note 文件存在再 bump + tag
4. 推 tag → CI 构建三平台产物并创建 Release，note 自动填入

## 模板

见 [TEMPLATE.md](TEMPLATE.md)。要写的中英文均可，但保持单一语言；
`## 亮点` / `## 修复` 用祈使句现在时（`Add ...` / `Fix ...`），不用过去式。

## 历史

`v0.1.0-alpha`（2026-07-22）发布于本目录建立之前，无 note 文件。
补写历史版本 note 需要手动改 Release body，不走本流程。
