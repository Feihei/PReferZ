# 字体许可说明

PReferZ 内嵌以下字体文件：

## Source Han Sans CN (思源黑体)

- **文件**: `SourceHanSansCN-Regular.ttf`
- **作者**: Adobe + Google
- **许可**: SIL Open Font License 1.1 (OFL-1.1)
- **用途**: 默认正文字体（Proportional + Monospace 族首位）
- **许可摘要**: 允许商用、修改、再分发；禁止单独出售字体文件。

## 851远星夜行手写体

- **文件**: `851LakeusNightWriting-Regular.ttf`
- **改作者**: Lakejason0（中国大陆）
- **原作者**: 8:51:22 pm（日本）— 原始字体「851手写杂书体」
- **许可**: 作者声明免费商用
- **来源**: https://www.maoken.com/freefonts/27883.html
- **用途**: 手写文字字体（FontFamily::Name("Handwriting851")）
- **原始许可**: https://pm85122.onamae.jp/851fontTerm.html

### 原始 851 字体许可摘要（8:51:22 pm 声明）

- 改造/再分发：**自由**
- 商用/非商用：**均可**
- 嵌入软件/游戏：**允许**
- 署名：**不需要**（"クレジットも特段必要ありません"）
- 唯一禁止：不得将字体文件**单品**直接销售

### 改作者声明

Lakejason0 基于原字体修改，声明企业/个人均可免费商用，允许嵌入系统/软件/APP、修改、商标注册。
### 子集化说明

repo 内 `851LakeusNightWriting-Regular.ttf` 是 **GB2312+ASCII 子集版**（6.2MB，7543 字符）；
完整版 `851LakeusNightWriting-Full.ttf`（28.9MB，34332 字符）不进 git（见 .gitignore），
从来源页重新下载或联系维护者获取。子集缺字（生僻字、CJK 扩展区、颜文字符号等）
在运行时回落到思源黑体（见 main.rs 手写族字体列表），不会渲染成豆腐块。

重新生成子集（需 uv/Python + fonttools）：

```bash
# 1. 生成字符集：GB2312 全表（6763 汉字 + 682 符号）+ ASCII 可打印区
python - <<'EOF'
import sys
chars = set()
for c in range(0x20, 0x7F):
    chars.add(chr(c))
for hi in range(0xA1, 0xF8):
    for lo in range(0xA1, 0xFF):
        try:
            chars.add(bytes([hi, lo]).decode("gb2312"))
        except UnicodeDecodeError:
            pass
sys.stdout.write("".join(sorted(chars)))
EOF
# 2. 子集化（输出替换 assets/851LakeusNightWriting-Regular.ttf）
uvx fonttools subset 851LakeusNightWriting-Full.ttf \
  --text-file=charset.txt \
  --output-file=851LakeusNightWriting-Regular.ttf \
  --no-hinting
```

许可允许修改/再分发，子集化属于允许的改造范畴，无需额外授权。