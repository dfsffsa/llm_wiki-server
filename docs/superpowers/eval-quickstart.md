# LLM Judge 评估 — 操作指南

> 在另一台电脑上继续完成 ParentingBooks 全量评估及后续工作。

## 分支信息

| 项目 | 值 |
|------|-----|
| 分支 | `feat/llm-judge-phase2` |
| 最新 commit | `5521d7a` |
| 仓库 | `llm_wiki-server` |
| 项目数据 | `/home/li/code/personal/llm_wiki_projects/ParentingBooks/` |

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
`/tmp/parenting-eval.json`（167 条 report，已清洗字段别名）

---

## 当前进度

### 已完成
- [x] **Phase 1: 规则评估 + Auto-fix** — `ingest_check.py` + `fixers/` + `auto_fix.py`
- [x] **Phase 2: LLM-as-Judge 评估** — `judge/extractor.py` + `evaluator.py` + `llm_judge.py`
- [x] **Phase 3: Repairer 实现** — `judge/repairer.py` + `--auto-fix` CLI 参数
- [x] **全量评估已跑完** — 167 文件，结果在 `/tmp/parenting-eval.json`
- [x] **Score 字段别名修复** — 43 个 `factual_consistency` → `consistency` 等（已映射 + 清洗数据）
- [x] **Checkpoint 保存** — 每完成一个文件写 JSON，中断不丢数据
- [x] 64 个单元测试全部通过

### 评估结果（清洗后）
| 指标 | 值 | 说明 |
|------|----|------|
| 总文件 | 167 | |
| 有效评估 | **109** | 58 个错误 |
| 平均覆盖率 | **7.3 / 10** | 中位数 7.0，整体尚可 |
| 平均一致性 | **9.7 / 10** | 中位数 10.0，wiki 内容忠实于 source |
| 幻觉总数 | **275** | 平均每页 2.5，全部为 minor |
| 低质量 (coverage<6) | 14 页 (13%) | 需要修复 |
| 中质量 (6-8) | 45 页 (41%) | |
| 高质量 (>=8) | 50 页 (46%) | |

---

## 剩余工作

### Task 1: 重跑 47 个 429 限流文件

**原因**：47 个文件因 API 限流（`429 Too Many Requests`）未评估。这些文件集中在最后 9 个（郑玉巧婴儿卷-12~13 章），其余散布在前面的文件中。

**方法**：修改 `llm_client.py` 加 retry+backoff，或分批重跑。

**方案 A：简单分批重跑（推荐，20 分钟）**
```bash
python3 overlay/eval/llm_judge.py \
  --project /home/li/code/personal/llm_wiki_projects/ParentingBooks \
  --config overlay/config/llm.judge.json \
  --verbose --output /tmp/parenting-eval-retry.json \
  --sample 50
```
重复几次直到覆盖所有 47 个文件。手动合并结果。

**方案 B：加 retry（推荐，1 小时）**
修改 `overlay/eval/judge/llm_client.py`，在 `call_llm()` 加 retry 逻辑：

```python
import time
def call_llm(prompt, llm_config, system="", max_retries=3):
    for attempt in range(max_retries):
        try:
            resp = requests.post(url, headers=headers, json=payload, timeout=300)
            resp.raise_for_status()
            return resp.json()["choices"][0]["message"]["content"]
        except requests.exceptions.HTTPError as e:
            if e.response.status_code == 429 and attempt < max_retries - 1:
                wait = 2 ** (attempt + 1) * 10  # 20s, 40s, 80s
                time.sleep(wait)
                continue
            raise
```

然后重新跑全量评估（会自动覆盖 checkpoint）。

### Task 2: 排查 11 个 JSON 解析失败

**错误类型**：
| 错误类型 | 数量 | 原因 |
|----------|------|------|
| `Expecting value: line 1 column 1 (char 0)` | 5 | LLM 返回空响应 |
| `Unterminated string starting at: ...` | 2 | LLM 返回截断的 JSON |
| `unparseable_response` | 3 | LLM 返回了非 JSON 文本 |
| 其他 | 1 | |

**排查方法**：
```bash
# 看具体文件
python3 -c "
import json
with open('/tmp/parenting-eval.json') as f:
    d = json.load(f)
errs = [r for r in d['reports'] if 'error' in r.get('scores', {}) and '429' not in r['scores']['error']]
for e in errs:
    print(f'{e[\"source_file\"]}: {e[\"scores\"][\"error\"][:80]}')
"
```

**可能原因**：
- `max_tokens=16384` 对大页面不够 → LLM 输出被截断 → JSON 不完整
- 某些 source 文件包含特殊字符导致 LLM 输出异常

**修复建议**：
1. 增大 `max_tokens`（如 32768）
2. 对这几页单独跑 `--sample` 调试

### Task 3: Auto-fix 14 个低质量页面

**修复目标**（coverage < 6 的 14 个页面）：

```bash
python3 overlay/eval/llm_judge.py \
  --project /home/li/code/personal/llm_wiki_projects/ParentingBooks \
  --config overlay/config/llm.judge.json \
  --auto-fix --threshold 6 --dry-run
```

**注意**：先跑完 Task 1（429 重跑）确保数据完整后再 auto-fix。

**已知问题**：3 样本测试时 Repairer 的修复通过率 1/3（1 成功，2 被 reject 回退）。如果全量修复 reject 率太高，考虑：
- 调 prompt（`REPAIR_SYSTEM_PROMPT` 在 `repairer.py`）
- 降低阈值从 6 到 5
- 对低 coverage 和高幻觉分开处理

### Task 4: 幻觉全部 minor 的问题

**问题**：275 条幻觉全部标记为 `severity: "minor"`，没有一条 `major`。这可能不准确。

**原因**：LLM 的 Evaluator prompt 可能不够严格，或者 LLM 不愿判 major。

**修复**：修改 `overlay/eval/judge/evaluator.py` 的 `EVAL_SYSTEM_PROMPT`，强调需要区分 major/minor。

### Task 5: 跨机器继续

```bash
# 1. 确认环境
python3 -c "import requests; print('requests ok')"
python3 -m unittest overlay/eval/tests/test_llm_judge.py -v

# 2. 确认 API key
cat overlay/config/llm.judge.json

# 3. 确认项目数据
ls /home/li/code/personal/llm_wiki_projects/ParentingBooks/raw/sources/ | wc -l
ls /home/li/code/personal/llm_wiki_projects/ParentingBooks/wiki/sources/ | wc -l

# 4. 确认评估结果
python3 -c "import json; d=json.load(open('/tmp/parenting-eval.json')); print(f'{len(d[\"reports\"])} reports, {d[\"summary\"]}')"
```

---

## 文件结构

```
llm_wiki-server/
├── overlay/eval/                    # 评估模块根目录
│   ├── llm_judge.py                 # 主入口：CLI + 管线编排
│   ├── judge/
│   │   ├── __init__.py
│   │   ├── llm_client.py            # LLM API 调用封装（需要加 retry）
│   │   ├── models.py                # 数据模型
│   │   ├── extractor.py             # 角色 A：从 source 提取关键陈述
│   │   ├── evaluator.py             # 角色 B：逐条比对 wiki 覆盖度
│   │   └── repairer.py              # Phase 3：定向修复
│   ├── tests/
│   │   ├── test_llm_judge.py        # 38 个测试
│   │   └── test_repairer.py         # 26 个测试
│   └── config/
│       └── llm.judge.json           # LLM 配置
├── overlay/auto_fix/
│   ├── fixers/                      # 规则级修复器
│   └── auto_fix.py                  # 规则级修复管线
└── docs/superpowers/
    ├── eval-quickstart.md           # 本文件
    ├── plans/2026-07-27-repairer-phase3.md  # 设计文档
    └── specs/                       # Phase 1 设计文档
```

## 配置

### LLM 配置 (`overlay/config/llm.judge.json`)
```json
{
  "llmConfig": {
    "model": "deepseek-v4-flash",
    "apiKey": "sk-95maPEsHlsNjrg1ojeUb7Uw4BYIUXeLKtzznpb27RGIqrPhQ4QxnS84DbkqGzhY4",
    "endpoint": "https://opencode.ai/zen/go/v1/chat/completions",
    "max_tokens": 16384
  }
}
```

> **注意**：另一台机器上如果 `opencode.ai` 不可达，需要修改 `endpoint`。

## 运行命令速查

```bash
# 评估全部（带 checkpoint）
python3 overlay/eval/llm_judge.py \
  --project /home/li/code/personal/llm_wiki_projects/ParentingBooks \
  --config overlay/config/llm.judge.json \
  --verbose --output /tmp/parenting-eval.json

# 抽样 3 个
python3 overlay/eval/llm_judge.py \
  --project ... --config ... --sample 3 --verbose

# 评估 + 修复
python3 overlay/eval/llm_judge.py \
  --project ... --config ... --auto-fix --threshold 6

# 修复预览（不修改）
python3 overlay/eval/llm_judge.py \
  --project ... --config ... --auto-fix --threshold 6 --dry-run

# 后台运行
nohup python3 overlay/eval/llm_judge.py \
  --project ... --config ... --verbose \
  --output /tmp/parenting-eval.json \
  > /tmp/parenting-eval.log 2>&1 &

# 查看进度
tail -5 /tmp/parenting-eval.log
python3 -c "import json; d=json.load(open('/tmp/parenting-eval.json')); print(d['progress'], d['summary'])"

# 运行测试
python3 -m unittest overlay/eval/tests/test_llm_judge.py -v
python3 -m unittest overlay/eval/tests/test_repairer.py -v
```

## 输出格式

```json
{
  "project": "ParentingBooks",
  "timestamp": "...",
  "config": {"model": "deepseek-v4-flash", "sample": null},
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
    "avg_coverage": 7.3,
    "avg_consistency": 9.7,
    "total_hallucinations": 275,
    "low_quality": ["raw/sources/xxx.md", ...]
  }
}
```

## 已知问题

### 1. 429 Rate Limit
47 个文件因 API 限流未评估。见 Task 1。

### 2. JSON 解析失败
11 个文件因 LLM 返回格式异常未评估。见 Task 2。

### 3. 低覆盖率页面
13% 的页面覆盖率低于 6。见 Task 3。

### 4. 幻觉全部 minor
275 条幻觉全部标记为 minor，可能不准确。见 Task 4。

### 5. 项目数据路径
项目数据在 `/home/li/code/personal/llm_wiki_projects/`，不在 `llm_wiki-server` 仓库内。另一台机器上需要 rsync 或重新拉取。