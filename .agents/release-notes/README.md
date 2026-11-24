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

见 [TEMPLATE.md](TEMPLATE.md)。主体语言**中文**（与 CHANGELOG 一致，也是应用默认界面语言）；
顶部可加一行英文摘要面向更广受众。要写的中英文均可，但正文保持单一语言。
`## 亮点` / `## 修复` 用祈使句现在时（`Add ...` / `Fix ...`），不用过去式。
文末的「## macOS 安装（首次运行需放行）」段落为固定内容（中英双语），每个版本的 note
都保留（把 `<版本>` 替换成实际版本号即可），因为 macOS 用户首次打开必须过 Gatekeeper。

面向发布对象（GitHub Release body）而非开发者：写**用户能感知的能力**（`Alt+单击` 删顶点、
`Ctrl+Shift+E` 导出选区），不写内部函数名 / 文件行号 / 测试计数——那些归 CHANGELOG。
发布前应已过一轮人工观感验收；若确有未过验收的条目，在末尾加「## 已知」如实列出。

**排版硬规则：一个要点 = 一个物理行，不要手工断行。** GitHub Release body 会保留
单个换行（v0.6.0 曾因按 80 列排版硬换行，页面出现句中折行）。行写长了没关系，
Markdown 会自动流式排版；超过 ~200 字符再考虑拆分成两个要点。

## 历史

`v0.1.0-alpha`（2026-07-22）发布于本目录建立之前，无 note 文件。
补写历史版本 note 需要手动改 Release body，不走本流程。
