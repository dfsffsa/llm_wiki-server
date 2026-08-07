# 电子书批量复用设计(配置化驱动 + runbook)

> **性质**:已批准设计(2026-08-06)。
> **相关**:[2026-08-04-ebook-batch-ingestion-design.md](./2026-08-04-ebook-batch-ingestion-design.md)(首次入库设计)、[2026-08-04-ebook-batch-ingestion.md](../plans/2026-08-04-ebook-batch-ingestion.md)(11 任务计划)、[2026-08-06-ebook-ingestion-lessons.md](../../notes/2026-08-06-ebook-ingestion-lessons.md)(17 条经验教训)
> **动机**:首次电子书入库已跑通,但 `ebook_run.sh` 的书单/路径/项目是硬编码的。目标:新增一批电子书时,用一套固定流程 + 配置驱动复用,避免重复踩坑。

---

## 1. 目标与形态

把「电子书 → 切分 → LLM 检查 → 落地 raw/sources」固化为**配置驱动的可复用流程**,并配一份**新批次 runbook**,让下次加书 10 分钟内进入切分阶段。

| 决策项 | 结论 |
|--------|------|
| 驱动范围 | **只驱动 切分→检查→修复→落地**(split/check/fix/promote)。ingest 与 eval 保持通用脚本,由 runbook 调用 |
| 配置格式 | **JSON 批次配置**(支持含 `\|` 的正则与含空格文件名;python3 解析) |
| 自动正则探测 | **新增 `detect` 子命令**(转换 txt 后分析标题模式,提出候选 `headingRe`) |
| 测试用例覆盖 | **顺带修复** `generate_test_cases.py` 的跨书覆盖偏置(固定种子洗牌源文件) |
| runbook | `docs/新批次电子书入库.md`(固定 10 步 + 全部踩坑) |

## 2. 背景与现状

**已可复用**(通用工具,已提交在 `main`):
- `scripts/ebook_split.py` — 完全参数化(`--epub/--book/--source/--out/--heading-re/--max-chars/--dry-run`)
- `scripts/ebook_check.py` — 通用(`--chunks/--config/--cache/--report/--fix`,增量缓存 + 逐块 error 容错)
- `scripts/ingest-parallel.sh` — 通用并行 ingest(env 驱动 `LLM_WIKI_PROJECT/CONFIG/WORKERS`,空格安全)
- LLM 配置:Tencent tokenhub(`deepseek-v4-flash-202605` / `deepseek-v4-pro-202606`)

**批次特定,本次要消除**:
- `scripts/ebook_run.sh` 三处硬编码:`SRC_BASE`(源目录)、`PROJECT`(默认项目)、`BOOKS=(...)`(书单,且 `|` 分隔有正则含 `|` 的隐患)
- 每本书 `--heading-re` 需人工逐本探查(8 本 6 种格式)
- `generate_test_cases.py` 按目录序处理源文件,首本书独占用例

## 3. 配置格式(batch JSON)

- **实际批次配置**(含本机路径,gitignored):`.tools/ebooks/batches/<name>.json`
- **格式模板**(仓库内提交):`scripts/ebooks/batches/example.json`
- `ebook_run.sh -c <path>` 解析顺序:绝对路径直接用;相对路径依次试 `<repo>/scripts/ebooks/batches/<path>` → `.tools/ebooks/batches/<path>` → 当前目录。

```json
{
  "name": "books-2026-08",
  "sourceDir": "/mnt/c/Users/Lenovo/Downloads/电子书",
  "project": "ParentingBooks",
  "outBase": ".tools/ebooks",
  "maxChars": 2500,
  "books": [
    {
      "dir": "11153-好孕，从卵子开始",
      "epub": "HaoYunCongLuanZiKaiShi.epub",
      "book": "好孕从卵子开始",
      "source": "好孕，从卵子开始",
      "headingRe": ""
    }
  ]
}
```

字段约定:
- `name` — 批次名(用于 staging 目录与日志)
- `sourceDir` — 电子书根目录(每本书一个子目录)
- `project` — 目标项目名(用于 resolve `~/overseas-github/llm_wiki_projects/<project>`)
- `outBase` — staging 根(默认 `.tools/ebooks`)
- `maxChars` — 超长再切阈值(全局默认 2500,book 可覆盖)
- `books[]` — `dir`(源子目录)/`epub`(epub 文件名)/`book`(简化书名,文件名前缀)/`source`(原书名,frontmatter `source`)/`headingRe`(章节标题正则,空则用 `ebook_split` 默认)

## 4. `ebook_run.sh` 改造(配置驱动)

- 新增 `-c/--config <batch.json>`;从配置读 `sourceDir/project/outBase/maxChars/books[]`,去掉 `SRC_BASE/PROJECT/BOOKS` 三处硬编码。`PROJECT` 仍可用 `LLM_WIKI_PROJECT` env 覆盖。
- 子命令:
  - `split [BOOK...]` — 逐书 `ebook_split.py --epub sourceDir/dir/epub ... --heading-re`
  - `detect [BOOK...]` — 调 `ebook_detect.py` 探测候选正则(见 §5)
  - `check [BOOK...]` — 逐书 `ebook_check.py`
  - `fix [BOOK...]` — 逐书 `ebook_check.py --fix`
  - `promote [BOOK...]` — 拷 chunks → `~/overseas-github/llm_wiki_projects/<project>/raw/sources/`
  - `pipeline [BOOK...]` — 顺序串 `split → check → fix → promote`(每步可失败续跑,因 split/check 幂等)
- 默认 `split`(无子命令时)。
- **空格安全**:所有文件路径经 `while IFS= read -r` / `"$var"` 处理(沿用 `ingest-parallel.sh` 修复后的模式)。

## 5. 自动正则探测 `ebook_detect.py`

`ebook_run.sh detect <book>` 流程:
1. 调 `ebook_split.convert_epub(epub, txt)`(复用)得到纯文本。
2. 分析:统计**行首模式**(正则候选,如 `^第[0-9]+章　`、`^\d+\.`、`^CHAPTER [0-9]+`、`^第[一二三四五六七八九十百千]+章` 等),按「正文标题 vs TOC 行」区分(正文标题行短、TOC 行带页码/多标题)。
3. 输出:**候选正则列表 + 每个候选命中的前 5 个样本行**,供用户确认后填 `headingRe`。
- 纯启发式、无 LLM;输出人工确认,不自动写配置。

## 6. 测试用例覆盖修复

`overlay/eval/generate_test_cases.py::generate_v2_batch`:遍历 `sorted(glob(raw_dir/*.md))` 前,先**按固定种子洗牌**源文件列表(如 `random.Random(42).shuffle`),使用例分散到各书。同时保持 `--target-count` 语义。
- 验证:一次生成 60 条,`source_file` 前缀应覆盖 ≥3 本书。

## 7. Runbook `docs/新批次电子书入库.md`

固定流程(10 步,每步带命令 + 踩坑):
1. **建批次配置**:复制 `example.json`,填 `sourceDir/project/books[]`(书名等)
2. **逐本探测正则**:`ebook_run.sh -c batch.json detect` → 确认 `headingRe` 填入配置
3. **测 LLM 配额**:单文件 ingest 试跑(不同步骤可能用不同端点,各自测)
4. **切分**:`ebook_run.sh -c batch.json split`
5. **检查**:`ebook_run.sh -c batch.json check`(增量缓存,断点续跑)
6. **修复**:`ebook_run.sh -c batch.json fix`
7. **落地**:`ebook_run.sh -c batch.json promote`
8. **入库**:`INGEST_WORKERS=4 LLM_WIKI_PROJECT=... LLM_WIKI_CONFIG=... ./scripts/ingest-parallel.sh`(脱离会话:`setsid nohup ... &`;空格安全已内置)
9. **生成用例**:`generate_test_cases.py`(已修复跨书覆盖)
10. **eval**:启动 server → `run_eval.sh all --fix` + `rag_eval.py --project <名称> --token`

**踩坑速查**(从首次入库提炼):
- 文件名含半角空格(如 `CHAPTER 17`)→ 路径一律 `read -r`,勿裸拆词
- `rag_eval --project` 按**项目名**匹配,不是路径
- `run_eval.sh` 的 rag_eval 不带 token,需直接 `rag_eval.py --token`
- `pkill -f` 用 `[x]` 方括号防自匹配;杀 ingest 要连孤儿 node/tsx 子树
- 长时任务用 `setsid nohup` 脱离会话;监控 ok/FAILED 标记在**主日志**
- 10h+ 任务进度要断点续跑(增量缓存)

## 8. 测试与验收

- `ebook_run.sh -c example.json split --dry-run`(或单书)能正确解析配置并打印预期命令
- `ebook_detect.py` 在 2 本已知格式书(法伯 `第N章　`、西尔斯 `CHAPTER N`)上冒烟,候选含正确正则
- 配置解析单测:`ebook_run.sh` 对缺字段/坏 JSON 报清晰错误
- `generate_test_cases.py` 修复后:60 条用例覆盖 ≥3 本书
- runbook 全流程在 example.json(或最小 fake 批次)上跑通 split→promote(dry)

## 9. 风险与回滚

| 风险 | 缓解 |
|------|------|
| JSON 解析失败(坏配置) | 启动时校验必填字段,报错带字段名 |
| detect 探测不准 | 输出候选+样本人工确认,不自动写配置 |
| 改造破坏现有批次 | 保留旧调用兼容:无 `--config` 时报错并提示;example.json 可复现本次 8 本 |
| 测试用例洗牌改变已有行为 | 固定种子,输出可复现;旧用例文件不受影响 |

回滚:工具脚本均可从 git 历史恢复;批次配置在 `.tools/ebooks/batches/`(gitignored,删了重建即可)。
