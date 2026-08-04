# 电子书批量入库流水线设计(2026-08-04)

> **性质**:已批准设计(2026-08-04)。实现前请先通读。
> **相关**:[2026-06-25-retrieval-eval-design.md](./2026-06-25-retrieval-eval-design.md)(eval 体系)、[代码结构总览.md](../../代码结构总览.md)(ingest 调用链)、[新项目指引.md](../../新项目指引.md)(项目结构)
> **数据所在**:Wiki 数据在 `~/overseas-github/llm_wiki_projects/ParentingBooks/`,不进本仓库。

---

## 1. 目标与形态

把 8 本育儿新书从电子书(Epub)转为结构化 markdown 源文件,纳入 ParentingBooks 知识库,再走 ingest + eval/fix 全流程,扩大可检索、可问答的内容覆盖面。

| 决策项 | 结论 |
|--------|------|
| 电子书格式 | **EPUB 为转换源**(每本目录齐备 epub/azw3/mobi,calibre 对 epub 支持最好) |
| 转换工具 | **ebook-convert**(calibre 7.6.0,已装 `/usr/bin/ebook-convert`) |
| 切分粒度 | **按章节 + 超长章(>2500 字)在段落边界再切成子块** |
| 语义把关 | **LLM 逐块检查切分后文本完整性**(截断/语义自包含/重复缺失),超长块必查 |
| 命名 | 简化:`书名-序号-标题.md`(书名去数字前缀/副标题) |
| frontmatter | 对齐现有 `type: source_lesson` 规范,`split_status: ebook_split` 区分来源 |
| 重复书 | **跳过**「52-崔玉涛:宝贝健康公开课」(已以 72 课形态入库) |
| 落地 | 检查通过后写入 `raw/sources/`,并更新 `purpose.md` 声明新书范围 |
| ingest | `ingest-batch.sh`(按 `wiki/sources/$base` 存在与否跳过已入库) |
| eval/fix | 新书生成测试用例(`generate_test_cases.py`)→ `run_eval.sh <proj> all --fix` |

## 2. 背景与现状

- `raw/sources/` 现有 **181 个源文件**,全部来自 2 本书:崔玉涛《宝贝健康公开课》(72 课)+ 郑玉巧《婴儿卷》。frontmatter 规范:
  ```yaml
  type: source_lesson
  source: <书名>
  part: <可选>
  chapter: <章>
  lesson: <课/节>
  title_text: <标题>
  age_hint: <可选>
  tags: [source_lesson, <书名>]
  split_status: raw_split | ebook_split
  ```
- `ingest_check.py` 校验的是 ingest 生成的 `wiki/sources/*.md`(要求 `type`+`title`),raw 源 frontmatter 保持与现有文件一致即可,无强约束。
- ingest 链路:`ingest-batch.sh` → `scripts/llm-wiki ingest` → Node shim(`cmd-ingest.ts`)→ 上游 `autoIngest()`(Zustand 耦合,仍需 Node)。用 `LLM_WIKI_PROJECT` + `LLM_WIKI_CONFIG`(`server.local.json`,已有 llmConfig/embeddingConfig)。
- eval 体系:`overlay/eval/{ingest_check,rag_eval,generate_test_cases,llm_judge}.py` + `run_eval.sh`。rag_eval 需要 server 在 `:8080` 运行。测试用例在 `test_cases/parenting_books.json`(v2 schema:`expected_sources.{must,should}`)。
- LLM 调用已有可复用基建:`overlay/eval/judge/llm_client.py::call_llm(prompt, config, system)`,支持 OpenAI 兼容 `/chat/completions`;配置样例 `overlay/config/llm.judge.a.json`(Ark `deepseek-v4-flash`,custom endpoint)。

## 3. 输入与范围

源目录:`/mnt/c/Users/Lenovo/Downloads/电子书/`(Windows C 盘,经 WSL `/mnt/c/` 可达)。每个子目录齐备 epub/azw3/mobi + `免责声明.txt` + `.url`(后两者跳过)。

**处理 8 本**(跳过「52-崔玉涛:宝贝健康公开课」):

| 目录 | 简化书名(文件名用) | 原书名(frontmatter `source` 用) | 备注 |
|------|----------------------|-------------------------------|------|
| 11063-崔玉涛自然养育法 | 崔玉涛自然养育法 | 崔玉涛自然养育法 | 崔玉涛另一本 |
| 11153-好孕,从卵子开始 | 好孕从卵子开始 | 好孕,从卵子开始 | 备孕/孕前 |
| 1454-法伯睡眠宝典 | 法伯睡眠宝典 | 法伯睡眠宝典 | 婴儿睡眠 |
| 2548-成就好爸爸:男人一生最重要的工作 | 成就好爸爸 | 成就好爸爸:男人一生最重要的工作 | 父职 |
| 290-定本育儿百科 | 定本育儿百科 | 定本育儿百科 | **大部头**,章可能超长 |
| 291-西尔斯育儿经 | 西尔斯育儿经 | 西尔斯育儿经 | 亲密育儿 |
| 738-第一次当奶爸 | 第一次当奶爸 | 第一次当奶爸 | 新手爸爸 |
| 9028-养育女孩(成长版) | 养育女孩 | 养育女孩(成长版) | 女孩养育 |

中间产物放 `.tools/ebooks/`(已 gitignore):staging、chunks、报告。脚本放 `scripts/`(可复用,入库)。

## 4. 阶段 1:转换(EPUB → txt)

对每本执行 `ebook-convert <书名>.epub <书名>.txt`,staging 为 `.tools/ebooks/<书名>/book.txt`。

- calibre 默认在章节边界插入分隔标记(文本化时按 TOC/标题结构),txt 是干净纯文本。
- 转换后用 `wc -m` 记录字符数,**与源书页数/内容量做数量级比对**,防止静默丢内容(纯图页、被忽略的脚注等)。偏差异常的书单独标记,进入 LLM 检查重点。

## 5. 阶段 2:切分(新脚本 `scripts/ebook-split.py`)

输入每本书的 `book.txt`,输出 `.tools/ebooks/<书名>/chunks/*.md`。

1. **章节解析**:从 txt 识别章节标题(优先解析 epub 的目录结构/分隔标记;退化用标题行正则,如 `第X章`/`Chapter N`/黑体标题)。
2. **按章切分**:一章一块。
3. **超长再切**:章 > 2500 字时,在**段落边界**(空行处,不打断句子)切成 ≤2500 字的子块,子块文件名带 `-N` 后缀。
4. **生成 frontmatter**(对齐现有规范;`source` 用**原书名**含标点,如 `好孕,从卵子开始`):
   ```yaml
   type: source_lesson
   source: <原书名>
   chapter: <章>
   title_text: <标题>
   tags: [source_lesson, <原书名>]
   split_status: ebook_split
   ```
5. **命名**:`<简化书名>-<两位序号>-<标题>.md`。标题超长截断;文件内首行 `# <标题>` 供 LLM 读取。序号按章内顺序。同一章切出的子块**共用同一序号**,在标题后追加 `-2`、`-3`(如 `定本育儿百科-05-婴儿喂养.md`、`定本育儿百科-05-婴儿喂养-2.md`),保证字典序即阅读序。
6. 可复现:幂等(同一 txt 输出同一批文件),带 `--dry-run` 只打印清单不写盘。

## 6. 阶段 3:LLM 语义检查(新脚本 `scripts/ebook-check.py`)

复用 `overlay/eval/judge/llm_client.py::call_llm`,配置按 `llm.judge.a.json` 同款(Ark `deepseek-v4-flash`,OpenAI 兼容 endpoint)。对每块判定:

1. **是否截断**:句子/段落是否在中间断开、上下语义是否衔接得上。
2. **语义自包含**:脱离上下文能否独立理解;是否出现「见上文/如前述」这类悬空指代。
3. **重复/缺失**:块内与相邻块是否有明显重复片段,或整段丢失(对比 `book.txt` 总量)。

行为:
- 默认**全块检查,超长块必查**;`--sample N` 抽样降本,`--only-long` 只查超长块。
- 输出 `.tools/ebooks/report.md`:每块结论(OK / 截断 / 悬空 / 重复),带修复建议。
- **自动修复**:对「截断」块,调整切分边界(向相邻块借/还段落)后重切;仍可疑的标 `MANUAL_REVIEW`,报告列出供人工复核。
- 断点续跑:已检块缓存结论(`check-cache.json`),重跑跳过,增量省钱。

**与 eval 的关系**:阶段 3 验**切分本身质量**(语义不丢);阶段 6 的 rag_eval 验**知识库检索/问答质量**。两者目的不同,不互相替代。

## 7. 阶段 4:落地

- 每本书检查通过后,把 chunks 拷入 `~/overseas-github/llm_wiki_projects/ParentingBooks/raw/sources/`。
- 更新 `purpose.md`:声明 8 本新书与项目定位(原仅郑玉巧/崔玉涛两本),并标注内容侧重(睡眠/备孕/父职/大部头百科等)。
- 如需保留原始 txt 供复核,留在 `.tools/ebooks/`。

## 8. 阶段 5:ingest

```bash
export LLM_WIKI_PROJECT=~/overseas-github/llm_wiki_projects/ParentingBooks
export LLM_WIKI_CONFIG=overlay/config/server.local.json
./scripts/ingest-batch.sh
```

- 自动跳过 `wiki/sources/` 已存在的旧文件名;新增源文件(新书名开头)全部走 LLM 生成 wiki 源页。
- 日志 `/tmp/llm-wiki-ingest-*.log`,中途失败可用 `ingest-cache` / 已存在判断续跑。

## 9. 阶段 6:eval/fix

1. 启动 server(:8080,带 `server.local.json`,`LLM_WIKI_PROJECT` + `LLM_WIKI_API_TOKEN` + `LLM_WIKI_STATIC`):
   ```
   ./overlay/server/target/release/llm-wiki-server
   ```
2. 为新书生成测试用例:`overlay/eval/generate_test_cases.py --project <路径> --mode auto --schema v2`(按书/按章节生成,`expected_sources.{must,should}`)。
3. `./overlay/eval/scripts/run_eval.sh ParentingBooks all --fix`(health → ingest_check → rag_eval;`--fix` 触发 auto_fix 修复 frontmatter/wikilink)。
4. 按需追加 `llm_judge.py` 内容级评测(新书样本)。

**评测口径注意**:新书测试用例以「章节内容可检索」为准,与现有两本书的月龄问答风格不同,指标分开看、不与旧指标混比(历史数据不可信见 `overlay/eval/AUDIT_2026-06-23.md`)。

## 10. 成本控制与可复现性

- LLM 检查是最大成本项:默认全块但可用 `--only-long` / `--sample N` 控制;缓存已检结论,增量重跑不重复计费。
- 所有脚本幂等、可断点续跑、`--dry-run` 预览。
- 密钥不落库:检查脚本读 `llm.judge.a.json`(`${TENCENT_TOKEN}` 由环境注入),ingest 读 `server.local.json`,均 gitignored(`*.local.json`)。

## 11. 风险与回滚

| 风险 | 缓解 |
|------|------|
| ebook-convert 转换丢内容(纯图/表格/脚注) | 字符数比对 + LLM 检查兜底 |
| 章节解析不准(个别书 TOC 结构异样) | 退化正则 + 人工复核标记 |
| 超长章再切打断语义 | 段落边界切 + LLM 检查 + 自动重切 |
| LLM 检查成本高 | `--only-long`/`--sample`/缓存 |
| 与现有 181 文件冲突 | 新文件名前缀与旧书不同,`ingest-batch.sh` 天然跳过已入库 |
| rag_eval 指标污染 | 新书测试用例独立生成、指标分开看 |

回滚:源文件都在 `raw/sources/`(可 git/rsync 恢复),chunks 在 `.tools/` 可整体删掉重来;wiki 生成页可由 ingest 重新生成。

## 12. 交付物清单

- `scripts/ebook-split.py`(转换→切分,可单独跑)
- `scripts/ebook-check.py`(LLM 语义检查 + 报告 + 自动重切)
- 本设计文档 + 实现计划(`docs/superpowers/plans/`)
- 落地后的 `raw/sources/` 新源文件 + 更新的 `purpose.md`
- ingest 日志 + eval 报告(`overlay/eval/results/`)
