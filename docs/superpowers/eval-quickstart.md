# LLM Judge 评估 — 操作指南

> 在另一台电脑上继续完成 ParentingBooks 全量评估及后续工作。

## 分支信息

| 项目 | 值 |
|------|-----|
| 分支 | `feat/llm-judge-phase2` |
| 最新 commit | `8955a99` |
| 仓库 | `llm_wiki-server` |
| 项目数据（本机） | `~/overseas-github/llm_wiki_projects/ParentingBooks/` |
| 项目数据（另一台） | `/home/li/code/personal/llm_wiki_projects/ParentingBooks/` |

### 仓库同步
```bash
# 另一台电脑上
git clone --recurse-submodules git@github.com:dfsffsa/llm_wiki-server.git
git checkout feat/llm-judge-phase2
```

### 项目数据同步
另一台电脑上需要相同的目录结构：
```bash
rsync -avz user@current-machine:/home/li/code/personal/llm_wiki_projects/ParentingBooks/ /home/li/code/personal/llm_wiki_projects/ParentingBooks/
```

### 评估结果文件
`docs/superpowers/ParentingBooks_llmjudge_results.json`（167 条 report，已清洗字段别名，500 KB）

```bash
python3 -c "import json; d=json.load(open('docs/superpowers/ParentingBooks_llmjudge_results.json')); print(f'{len(d[\"reports\"])} reports')"
```

---

## 当前进度

### 已完成
- [x] **Phase 1: 规则评估 + Auto-fix** — `ingest_check.py` + `fixers/` + `auto_fix.py`
- [x] **Phase 2: LLM-as-Judge 评估** — `judge/extractor.py` + `evaluator.py` + `llm_judge.py`
- [x] **Phase 3: Repairer 实现** — `judge/repairer.py` + `--auto-fix` CLI 参数
- [x] **全量评估已跑完** — 167 文件，结果在 `docs/superpowers/ParentingBooks_llmjudge_results.json`
- [x] **Score 字段别名修复** — 43 个 `factual_consistency` → `consistency` 等（已映射 + 清洗数据）
- [x] **Checkpoint 保存** — 每完成一个文件写 JSON，中断不丢数据
- [x] 64 个单元测试全部通过
- [x] **Task 1: 429 retry+backoff** — `call_llm()` 加指数退避（20s/40s/80s）
- [x] **Task 2: JSON 解析失败修复** — `max_tokens` 16384→32768，加截断恢复
- [x] **Task 3: Auto-fix 23 个 coverage<5 页面** — 5 修复成功，18 回退（22% 通过率）
- [x] **Task 4: major/minor 幻觉 prompt** — `EVAL_SYSTEM_PROMPT` 强调区分严重度

### 增量重跑（58 个失败文件 → 1 个剩余）

用 `overlay/eval/rerun_failed.py` 重跑了原 58 个失败项（47 个 429 + 7 个 JSON 解析 + 3 个 unparseable + 1 个 503），合并结果在 `/tmp/parenting-eval-merged.json`：

```bash
python3 overlay/eval/rerun_failed.py \
  --project ~/overseas-github/llm_wiki_projects/ParentingBooks \
  --input docs/superpowers/ParentingBooks_llmjudge_results.json \
  --config-a overlay/config/llm.judge.a.json \
  --config-b overlay/config/llm.judge.b.json \
  --output /tmp/parenting-eval-merged.json --verbose
```

剩余 1 个错误：`郑玉巧婴儿卷-11_10-11个月的婴儿(300-329天)-03-能力增长与潜能开发.md` — LLM 返回的 JSON 中含 `)` 代替 `}`（结构性损坏，截断恢复无法救），可手动重跑或忽略（1/167 = 0.6%）。

### 评估结果（增量重跑后）

| 指标 | 原始 | 重跑后 | 说明 |
|------|------|--------|------|
| 总文件 | 167 | 167 | |
| 有效评估 | 109 | **166** | 错误 58 → 1 |
| 平均覆盖率 | 4.3 | **6.8** | 重跑前缺低分文件拉低均值 |
| 平均一致性 | 3.1 | **8.8** | 同上 |
| 幻觉总数 | 275 | **469** | 更多文件有有效幻觉数据 |
| 低质量 (coverage<5) | 23 | 23 | 已尝试 auto-fix |
| 中质量 (5-8) | — | 84 | |
| 高质量 (>=8) | — | 59 | |

### Auto-fix 结果（coverage<5 的 23 个页面）

通过率 22%（5/23），符合 Task 3 文档预警的 1/3 区间。

| 状态 | 数量 | 处置 |
|------|------|------|
| ✓ 修复成功 | 5 | wiki 已实际写入新内容，验证通过 |
| ✗ 验证失败回退 | 18 | 回退到原状，待人工处理 |
| 失败 | 0 | |

成功修复的 5 个页面：

| 页面 | coverage | halls |
|------|----------|-------|
| 崔玉涛-20 哪些原因会引起宝宝发热 | 3 → 4.4 | 2 → 2 |
| 崔玉涛-40 髋关节滑囊炎 | 4 → 5.5 | 6 → 3 |
| 郑玉巧-07-08 护理中常见问题 | 4 → 6 | 3 → 3 |
| 郑玉巧-12-08 其他常见护理问题 | 0 → 1 | 8 → 8 |
| 郑玉巧-13-01 新生儿常见疾病 | 3.2 → 5.9 | 6 → 5 |

详情见 `/tmp/parenting-eval-merged-autofix.json`。wiki 备份在 `/tmp/parenting-wiki-backup-20260731-1540.tar.gz`。

---

## 文件结构

```
llm_wiki-server/
├── overlay/eval/                    # 评估模块根目录
│   ├── llm_judge.py                 # 主入口：CLI + 管线编排
│   ├── rerun_failed.py              # 增量重跑：只跑旧 JSON 中失败的文件
│   ├── run_auto_fix.py              # 加载已有 JSON 跑 auto-fix（可预过滤 coverage<threshold）
│   ├── judge/
│   │   ├── __init__.py
│   │   ├── llm_client.py            # LLM API 调用封装（含 429 retry+backoff）
│   │   ├── models.py                # 数据模型
│   │   ├── extractor.py             # 角色 A：从 source 提取关键陈述
│   │   ├── evaluator.py             # 角色 B：逐条比对 wiki 覆盖度（含 major/minor 判定）
│   │   └── repairer.py              # Phase 3：定向修复
│   ├── tests/
│   │   ├── test_llm_judge.py        # 38 个测试
│   │   └── test_repairer.py         # 26 个测试
└── docs/superpowers/
    ├── eval-quickstart.md           # 本文件
    ├── ParentingBooks_llmjudge_results.json  # 原始全量评估结果（167 条）
    ├── plans/2026-07-27-repairer-phase3.md  # 设计文档
    └── specs/                       # Phase 1 设计文档
```

## 配置

### 双模型 LLM 配置

角色 A（Extractor，便宜模型）和角色 B（Evaluator/Repairer，强模型）使用独立配置，便于成本控制：

- `overlay/config/llm.judge.a.json` — 角色 A
- `overlay/config/llm.judge.b.json` — 角色 B

```json
{
  "llmConfig": {
    "model": "deepseek-v4-flash-202605",
    "apiKey": "${TENCENT_TOKEN}",
    "endpoint": "https://tokenhub.tencentmaas.com/plan/v3/chat/completions",
    "max_tokens": 32768
  }
}
```

- `apiKey` 用 `${ENV_VAR}` 占位，运行时由 `load_llm_config()` 从环境变量展开
- `endpoint` 是腾讯 MaaS OpenAI 兼容接口
- `max_tokens=32768`（原 16384 对大页面不够，导致 JSON 截断）

> **注意**：旧的单配置文件 `overlay/config/llm.judge.json` 已删除，改用 `--config-a`/`--config-b` 两个文件。如两角色用同模型，可仍用 `--config` 同时指定。

## 运行命令速查

```bash
# 全量评估（带 checkpoint，每文件写一次）
python3 overlay/eval/llm_judge.py \
  --project ~/overseas-github/llm_wiki_projects/ParentingBooks \
  --config-a overlay/config/llm.judge.a.json \
  --config-b overlay/config/llm.judge.b.json \
  --verbose --output /tmp/parenting-eval.json

# 抽样 3 个
python3 overlay/eval/llm_judge.py \
  --project ... --config-a ... --config-b ... --sample 3 --verbose

# 评估 + 修复（评估完直接 auto-fix）
python3 overlay/eval/llm_judge.py \
  --project ... --config-a ... --config-b ... \
  --auto-fix --threshold 6

# 修复预览（不修改文件）
python3 overlay/eval/llm_judge.py \
  --project ... --config-a ... --config-b ... \
  --auto-fix --threshold 6 --dry-run

# 增量重跑：只跑旧 JSON 中失败的文件
python3 overlay/eval/rerun_failed.py \
  --project ~/overseas-github/llm_wiki_projects/ParentingBooks \
  --input docs/superpowers/ParentingBooks_llmjudge_results.json \
  --config-a overlay/config/llm.judge.a.json \
  --config-b overlay/config/llm.judge.b.json \
  --output /tmp/parenting-eval-merged.json --verbose

# 在已有评估 JSON 上跑 auto-fix（预过滤 coverage<threshold）
python3 overlay/eval/run_auto_fix.py \
  --project ~/overseas-github/llm_wiki_projects/ParentingBooks \
  --config-b overlay/config/llm.judge.b.json \
  --input /tmp/parenting-eval-merged.json \
  --threshold 5 --dry-run   # 去掉 --dry-run 实际修改

# 后台运行
nohup python3 overlay/eval/llm_judge.py \
  --project ... --config-a ... --config-b ... --verbose \
  --output /tmp/parenting-eval.json \
  > /tmp/parenting-eval.log 2>&1 &

# 查看进度
tail -5 /tmp/parenting-eval.log
python3 -c "import json; d=json.load(open('/tmp/parenting-eval.json')); print(d.get('progress'), d['summary'])"

# 运行测试
python3 -m unittest overlay/eval/tests/test_llm_judge.py -v
python3 -m unittest overlay/eval/tests/test_repairer.py -v
```

## 输出格式

```json
{
  "project": "ParentingBooks",
  "timestamp": "...",
  "config": {"model_a": "...", "model_b": "...", "sample": null},
  "progress": "85/167",          // 仅 checkpoint 有
  "reports": [
    {
      "source_file": "raw/sources/xxx.md",
      "wiki_page": "wiki/sources/xxx.md",
      "coverage_claims": [...],
      "hallucinations": [...],
      "scores": {"coverage": 6, "consistency": 8}
    }
  ],
  "summary": {
    "sources_evaluated": 167,
    "avg_coverage": 6.8,
    "avg_consistency": 8.8,
    "total_hallucinations": 469,
    "low_quality": ["raw/sources/xxx.md", ...]
  }
}
```

## 已知问题

### 1. Auto-fix 通过率偏低（22%）
23 个 coverage<5 的页面只修复成功 5 个。原因：Repairer 的 verify 阈值仍用 coverage<threshold 判定，但 LLM 一次修复常无法把 coverage 提到 5 以上（尤其 source 内容少、wiki 信息密度低的页面）。后续可考虑：
- 放宽 verify 阈值（如只要 coverage 提升 ≥1 就接受）
- 多轮迭代修复（同一页跑 2-3 次 repair）
- 调 `REPAIR_SYSTEM_PROMPT` 让 LLM 更激进地补全缺失要点

### 2. 1 个 unparseable_response
`郑玉巧婴儿卷-11_10-11个月的婴儿(300-329天)-03-能力增长与潜能开发.md` — LLM 返回的 JSON 中含 `)` 代替 `}`（结构性损坏）。可单独手动重跑或忽略。

### 3. 项目数据路径
项目数据在 `~/overseas-github/llm_wiki_projects/`（本机）或 `/home/li/code/personal/llm_wiki_projects/`（另一台机器），不在 `llm_wiki-server` 仓库内。跨机器协作需要 rsync。

### 4. 临时结果文件不进 git
`/tmp/parenting-eval-merged.json`、`/tmp/parenting-eval-merged-autofix.json`、`/tmp/parenting-wiki-backup-*.tar.gz` 都在 `/tmp`，重启会丢。如需持久化，复制到 `docs/superpowers/` 或项目数据目录。