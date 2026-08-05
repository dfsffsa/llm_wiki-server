# 电子书批量入库 — 进度交接(2026-08-04)

> **交接时间**:2026-08-04 晚(执行中断,待续);2026-08-05 已续跑
> **设计 spec**:`docs/superpowers/specs/2026-08-04-ebook-batch-ingestion-design.md`
> **实现计划**:`docs/superpowers/plans/2026-08-04-ebook-batch-ingestion.md`(11 任务)
> **执行方式**:subagent-driven-development(每任务 implementer + 规格评审 + 质量评审)

---

## 2026-08-05 续跑更新

- **Task 8 完成 ✅**:8 本 1256 块全部 LLM 语义检查完毕。`--fix` 自动修复 114 个截断块。最终判定:ok 1156 / truncated 53 / dangling 39 / duplicate 4 / error 4。**人工复核清单**:`.tools/ebooks/MANUAL_REVIEW.md`(100 项)。
- **过程中修的 3 个工具 bug**(均已提交):
  - `parse_json_response` 无法解析结尾引号/if-else → 加平衡大括号提取 + if-else 清理(commit `8360cac`)
  - `save_cache` 只整本写盘、崩溃丢进度 → 增量写(commit `f99031f`)
  - LLM 拒绝/非 JSON 输出崩整本 → `check_chunk` 逐块容错,记 `error` verdict(commit `dd228c2`)
- **Task 9 完成 ✅**:promote 到 `raw/sources/`(1437 文件 = 181 旧 + 1256 新),`purpose.md` 已更新。
- **LLM 全部切到 Tencent tokenhub**(MiniMax 配额耗尽,429 阻塞):
  - ingest + server chat:`server.local.json` llmConfig → `deepseek-v4-flash-202605`,`apiMode: chat_completions`(gitignored,不提交)
  - llm_judge 评估者 B:`llm.judge.b.json` → `deepseek-v4-pro-202606`(commit `3a8dabc`)
  - llm_judge 提取者 A:`llm.judge.a.json` → 保持 `deepseek-v4-flash-202605`
- **Task 10 进行中**:单文件 ingest ≈ 2m11s(7 个 wiki 文件/源)。顺序 1256 文件 ≈ 42h 不可行 → 新增 `scripts/ingest-parallel.sh`(按 `wiki/sources/$base` 跳过已入库,多 worker 并行,`INGEST_WORKERS`/`INGEST_LIMIT` 可配)。冒烟 2 worker×4 文件通过、缓存无损坏。缓存竞态可容忍(loadCache/saveCache 均 try/catch,损坏只退回空缓存;跳过靠 wiki/sources 存在性,不靠缓存)。
- **最终启动(脱离会话,可随会话关闭继续)**:
  ```bash
  setsid nohup env INGEST_WORKERS=4 LLM_WIKI_PROJECT="$HOME/overseas-github/llm_wiki_projects/ParentingBooks" \
    LLM_WIKI_CONFIG=overlay/config/server.local.json bash scripts/ingest-parallel.sh \
    > /tmp/ingest-parallel-detached.log 2>&1 &
  ```
  - **查看进度**:`tail /tmp/ingest-parallel-detached.log`、各 worker `/tmp/ingest-w{0..3}.log`、`ls wiki/sources/ | grep -cE "养育女孩|好孕|成就|定本|崔玉涛自然|法伯|西尔斯|第一次当奶爸"`
  - **续跑**:中断后重跑上面命令即可(按 wiki/sources 存在性跳过已入库)
  - **坑**:`pkill -f "llm-wiki ingest"` 会匹配到自身 shell(exit 144);杀进程用 `pkill -9 -f "[l]lm-wiki ingest"`(方括号防自匹配)。杀死会留孤儿 node/tsx 孙进程,需一并 `pkill -9 -f "[c]md-ingest"` 清掉,否则与新 run 双跑。
- **模型探测结论**:tokenhub 上 `deepseek-v4-flash-202605` 与 `deepseek-v4-pro-202606` 可用;`deepseek-v4-pro`(裸)未授权、`deepseek-v4-pro-202605` TokenPlan 不支持。

---

---

## ⚠️ 交接注意(先读)

- **全部工作产物在主 checkout `/home/ab/overseas-github/llm_wiki-server`(branch `main`),不是 worktree!**
  子代理实际在主 checkout 工作并直接提交到了 `main`。worktree `.claude/worktrees/feat+ebook-ingestion`(branch `worktree-feat+ebook-ingestion`)**是空的**(仅含 2 个已在 main 上的文档 commit),可删除。
- `CLAUDE.md` 有**未提交修改**(/init 时补充了 Web Adapter 6 个别名、测试脚本),待决定是否提交。
- 数据目录在仓库外:`~/overseas-github/llm_wiki_projects/ParentingBooks/`。`.tools/` 已 gitignore。

---

## 已完成:Task 1–7(全部提交在 `main`)

### 交付物(仓库内 `scripts/`)

| 文件 | 职责 |
|------|------|
| `scripts/ebook_split.py` | EPUB→txt(`ebook-convert`)+ 按章切分 + 超长章(>2500 字)段落边界再切;frontmatter(`type/source/chapter/title_text/tags/split_status`,yaml.dump 转义);命名 `书名-序号-标题.md`,子块 `-2/-3`;`--heading-re` 支持自定义章节正则 |
| `scripts/ebook_check.py` | LLM 语义检查(截断/悬空指代/重复),内容 sha256 hash 缓存断点续跑,`report.md`,`--fix` 把未完尾段搬到下一块正文开头 |
| `scripts/ebook_run.sh` | 编排:8 本书单 + `split`/`check`/`promote` 子命令;6 本自定义 `--heading-re` |
| `scripts/tests/test_ebook_split.py` `test_ebook_check.py` | **27 个单元测试全过** |

### 切分结果(8 本共 **1256 块**,在 `.tools/ebooks/<书>/chunks/`)

| 书 | 块数 | 章节正则(需覆盖的) |
|---|---|---|
| 定本育儿百科 | 684 | `^\d+\.`(651 个编号词条结构,合理) |
| 西尔斯育儿经 | 171 | `^(CHAPTER [0-9]+\|Part [IVX]+)　` |
| 崔玉涛自然养育法 | 121 | `^[0-9]{2} ` |
| 法伯睡眠宝典 | 71 | 默认 `^第[0-9]+章　` |
| 第一次当奶爸 | 62 | `^第[一二三四五六七八九十百千]+章 ` |
| 成就好爸爸 | 57 | `^[0-9]+．`(全角句点 U+FF0E) |
| 好孕从卵子开始 | 45 | 默认 |
| 养育女孩 | 45 | `^第[一二三四五六七八九十百千]+章$`(独立"第一章") |

### 验证记录

- **Task 3 冒烟(法伯)**:18 章 / 71 块;`book.txt` 字符数与参考值 155,461 **完全一致**(无内容丢失);frontmatter/命名/标题清洗正确。
- **Task 6 真实 LLM 冒烟(法伯 71 块全检)**:54 ok / 13 truncated / 4 dangling,耗时 4m04s;二次运行缓存秒回(`[C]` 命中);4 条 MANUAL_REVIEW 判定合理(悬空指代"前几节""见表13-2"、章节尾部混入湛庐广告等)。
- 每任务经两阶段评审(规格合规 + 代码质量),发现问题均已修复复核。

### Commits(`main` 分支,10 个)

```
af505ad feat(ebook): batch orchestration script (book list, split/check/promote)
feb7b89 fix(ebook): close test file handles, empty-tail guard, cache-pop assertion
3be12ae feat(ebook): --fix moves truncated tail to next chunk + report consistency
dda4005 fix(ebook): remove dead import, defensive save_cache, report tests
59cb307 feat(ebook): LLM semantic check with hash cache + report
418d90e fix(ebook): YAML-safe frontmatter via yaml.dump + dry-run test + regex cleanup
388828e feat(ebook): write chunk files with frontmatter + naming + main CLI
baac784 fix(ebook): separator budget off-by-one + edge-case tests + sentence-end param
4bff99b fix(ebook): revert subsplit to spec, correct paragraph-boundary test assertion
86cea04 feat(ebook): chapter parsing, title cleaning, sub-split core functions
```

---

## 待办:Task 8–11

### ⏳ Task 8(有**待决策项**)

**决策:LLM 全量检查范围**。8 本 1256 块全检预计 **~70 分钟 + 明显 token 成本**(法伯实测 24% 被标记,检查确能抓真问题;truncated 可由 `--fix` 自动修)。选项:
1. **全量检查 + `--fix`(推荐,按设计)**
2. 定本 684 块全检 + 其余 7 本每本抽样 10–15 块
3. 仅 `--only-long`(只检 >2500 字块,最快但漏短块)

**执行**(决策后):
```bash
cd /home/ab/overseas-github/llm_wiki-server
./scripts/ebook_run.sh check        # 全量检查(逐本,断点续跑)
# 自动修复截断块(若有):
for b in 崔玉涛自然养育法 好孕从卵子开始 法伯睡眠宝典 成就好爸爸 定本育儿百科 西尔斯育儿经 第一次当奶爸 养育女孩; do
  python3 scripts/ebook_check.py --chunks ".tools/ebooks/$b/chunks" \
    --config overlay/config/llm.judge.a.json \
    --cache ".tools/ebooks/$b/check-cache.json" --fix
done
./scripts/ebook_run.sh check        # 复检
grep -l "MANUAL_REVIEW" .tools/ebooks/*/report.md   # 人工复核清单
```
> 法伯的 71 块已检查过(缓存命中,Task 6)。

### Task 9:落地
```bash
./scripts/ebook_run.sh promote      # 拷 chunks → ~/overseas-github/llm_wiki_projects/ParentingBooks/raw/sources/
ls ~/overseas-github/llm_wiki_projects/ParentingBooks/raw/sources/ | wc -l   # ≈ 181 + 1256
```
更新 `purpose.md`:声明 8 本新书范围(目前只提郑玉巧/崔玉涛两本)。

### Task 10:ingest 批量入库
```bash
LLM_WIKI_PROJECT="$HOME/overseas-github/llm_wiki_projects/ParentingBooks" \
LLM_WIKI_CONFIG=overlay/config/server.local.json \
INGEST_LOG=/tmp/llm-wiki-ingest-ebooks.log \
./scripts/ingest-batch.sh
```
新文件全走 LLM 生成 wiki 页(1256 次调用,耗时较长);旧 181 文件自动 SKIP。验证 `wiki/sources/` 数量。

### Task 11:server + 新书测试用例 + eval/fix
```bash
# 1. 启动 server(:8080)
export LLM_WIKI_PROJECT="$HOME/overseas-github/llm_wiki_projects/ParentingBooks"
export LLM_WIKI_API_TOKEN="$(python3 -c 'import json; print(json.load(open("overlay/config/server.local.json")).get("apiConfig",{}).get("token",""))')"
export LLM_WIKI_CONFIG=overlay/config/server.local.json
export LLM_WIKI_STATIC=upstream/dist
nohup ./overlay/server/target/release/llm-wiki-server > /tmp/llm-wiki-server.log 2>&1 &

# 2. 为新书生成 v2 测试用例
python3 overlay/eval/generate_test_cases.py \
  --project "$HOME/overseas-github/llm_wiki_projects/ParentingBooks" \
  --config overlay/config/server.local.json --schema v2 --mode auto \
  --target-count 60 --output overlay/eval/test_cases/parenting_books_ebooks.json

# 3. ingest_check + auto_fix
./overlay/eval/scripts/run_eval.sh ParentingBooks all --fix

# 4. 新书用例 rag_eval(与旧指标分开看)
python3 overlay/eval/rag_eval.py \
  --project "$HOME/overseas-github/llm_wiki_projects/ParentingBooks" \
  --test-cases overlay/eval/test_cases/parenting_books_ebooks.json \
  --mode all --token "<server.local.json apiConfig.token>"
```

---

## 关键事实 / 坑(续跑前必读)

- **8 本书章节格式各异**:6 本需自定义 `--heading-re`(见上表,已写进 `ebook_run.sh`)。
- **定本育儿百科**是按 651 个编号词条组织的参考书,`^\d+\.` 切分合理,无正文过匹配;唯一坏块 `定本育儿百科-647-651.急救.md`(含出版社信息,632 字节,可忽略)。
- **已知待修**(Task 7 质量评审 I1):`ebook_run.sh` 的 `promote` 用 `cp "$OUT_BASE/$book/chunks/"*.md`,无 `nullglob` 守卫 —— 若某书 chunks 为空,`cp` 会因 `set -e` 中断整个 promote。建议加 `shopt -s nullglob` + 空目录跳过(见评审建议)。
- **环境**:`TENCENT_TOKEN` 已设(len 54,llm.judge.a.json 引用 `${TENCENT_TOKEN}`);`overlay/server/target/release/llm-wiki-server` 二进制已 build(Jun 23);`overlay/config/server.local.json` 有 llmConfig/embeddingConfig(ingest 可用)。
- **成本**:Task 8(检查)与 Task 10(ingest)是主要 token 消耗点;检查用缓存可断点续跑。
- **评测口径**:新书测试用例以「章节内容可检索」为准,与旧两本(月龄问答)风格不同,指标分开看(勿与历史混比,见 `overlay/eval/AUDIT_2026-06-23.md`)。
- 源文件 frontmatter `source` 字段用原书名(含标点,如 `好孕，从卵子开始`),文件名用简化名。
