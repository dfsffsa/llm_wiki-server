# LLM Judge 评估 — 操作指南

> 在另一台电脑上继续完成 ParentingBooks 全量评估。

## 当前进度

| 项目 | 值 |
|------|-----|
| 项目 | ParentingBooks |
| 总文件 | 167 |
| 已完成 | 167 ✅（评估已跑完，数据在 `/tmp/parenting-eval.json`） |
| 错误数 | 58（47 个 429 限流，11 个 JSON 解析失败） |
| **有效评估** | **109** |
| **平均覆盖率** | **7.3 / 10**（中位数 7.0） |
| **平均一致性** | **9.7 / 10**（中位数 10.0） |
| **幻觉总数** | **275**（平均每页 2.5，全部 minor） |
| **低质量 (coverage<6)** | **14 页 (13%)** |

## 文件结构

```
llm_wiki-server/
├── overlay/eval/                    # 评估模块根目录
│   ├── llm_judge.py                 # 主入口：CLI + 管线编排
│   ├── judge/
│   │   ├── __init__.py
│   │   ├── llm_client.py            # LLM API 调用封装
│   │   ├── models.py                # 数据模型：JudgeReportItem, CoverageClaim, Hallucination
│   │   ├── extractor.py             # 角色 A：从 source 提取关键陈述
│   │   ├── evaluator.py             # 角色 B：逐条比对 wiki 覆盖度
│   │   └── repairer.py              # Phase 3：定向修复（可选）
│   ├── tests/
│   │   ├── test_llm_judge.py        # 38 个测试
│   │   └── test_repairer.py         # 26 个测试
│   └── config/
│       └── llm.judge.json           # LLM 配置
├── overlay/auto_fix/
│   ├── fixers/                      # 规则级修复器
│   └── auto_fix.py                  # 规则级修复管线
└── docs/superpowers/plans/
    └── 2026-07-27-repairer-phase3.md  # 设计文档
```

## 依赖

### Python 标准库（无需额外安装）
- `json`, `os`, `sys`, `random`, `glob`, `datetime`, `logging`, `unittest`
- `requests`（第三方，通常已安装）

### 检查 requests 是否可用
```bash
python3 -c "import requests; print('ok')"
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

> **注意**：API key 在 `llm.judge.json` 中是明文。部分机器可能通过 `LLM_WIKI_API_TOKEN` 环境变量有不同 key，但 **`llm.judge.json` 优先**。如果 `opencode.ai` 在另一台机器上不可达，需要修改 `endpoint`。

## 运行方式

### 1. 确认项目数据存在
```bash
ls /home/li/code/personal/llm_wiki_projects/ParentingBooks/raw/sources/  | wc -l
ls /home/li/code/personal/llm_wiki_projects/ParentingBooks/wiki/sources/ | wc -l
```
两个目录都应该有 167 个 `.md` 文件，文件名一一对应。

### 2. 运行评估
```bash
cd /path/to/llm_wiki-server

# 评估全部（带 checkpoint，每完成一个文件写入 JSON）
python3 overlay/eval/llm_judge.py \
  --project /home/li/code/personal/llm_wiki_projects/ParentingBooks \
  --config overlay/config/llm.judge.json \
  --verbose \
  --output /tmp/parenting-eval.json

# 抽样 3 个测试
python3 overlay/eval/llm_judge.py \
  --project /home/li/code/personal/llm_wiki_projects/ParentingBooks \
  --config overlay/config/llm.judge.json \
  --sample 3 --verbose
```

### 3. 评估 + 自动修复
```bash
# 预览（不修改文件）
python3 overlay/eval/llm_judge.py \
  --project /home/li/code/personal/llm_wiki_projects/ParentingBooks \
  --config overlay/config/llm.judge.json \
  --sample 3 --auto-fix --threshold 6 --dry-run

# 实际修复
python3 overlay/eval/llm_judge.py \
  --project /home/li/code/personal/llm_wiki_projects/ParentingBooks \
  --config overlay/config/llm.judge.json \
  --auto-fix --threshold 6
```

### 4. 后台运行（推荐全量）
```bash
nohup python3 overlay/eval/llm_judge.py \
  --project /home/li/code/personal/llm_wiki_projects/ParentingBooks \
  --config overlay/config/llm.judge.json \
  --verbose \
  --output /tmp/parenting-eval.json \
  > /tmp/parenting-eval.log 2>&1 &

# 查看进度
tail -5 /tmp/parenting-eval.log

# 查看 checkpoint
python3 -c "import json; d=json.load(open('/tmp/parenting-eval.json')); print(d['progress'], d['summary'])"
```

### 5. 运行测试
```bash
python3 -m unittest overlay/eval/tests/test_llm_judge.py -v
python3 -m unittest overlay/eval/tests/test_repairer.py -v
```

## 输出格式

### 评估结果 JSON
```json
{
  "project": "ParentingBooks",
  "timestamp": "2026-07-27 11:00:00",
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
    "avg_coverage": 4.3,
    "avg_consistency": 3.1,
    "total_hallucinations": 275,
    "low_quality": ["raw/sources/xxx.md", ...]
  }
}
```

### 完整输出路径
- **评估结果 JSON**: `/tmp/parenting-eval.json`（167 文件，约 2.3 MB）
- **日志**: `/tmp/parenting-eval.log`

### 产出文件
| 文件 | 大小 | 说明 |
|------|------|------|
| `/tmp/parenting-eval.json` | ~2.3 MB | 完整评估结果（167 条 reports） |
| `/tmp/parenting-eval.log` | ~215 行 | 运行日志，含每文件覆盖率和错误 |
| `fix_backups/` | 在项目目录下 | 修复备份（如果跑了 auto-fix） |

## 已知问题

### 1. 429 Rate Limit
最后 47 个文件遇到 `429 Too Many Requests`。初步判断是 API 速率限制。
建议：
- 在 `llm_client.py` 中加入重试 + 指数退避
- 或分批运行，每次 --sample 20 个

### 2. JSON 解析失败
约 49 个文件因 LLM 返回格式异常导致解析失败（`unparseable_response`、`Unterminated string` 等）。可能原因：
- LLM 输出截断（max_tokens=16384 可能不够）
- LLM 返回了非 JSON 文本

### 3. 低 avg_coverage (7.3/10)
13% 的页面覆盖率低于 6，需要修复。这些页面是 auto-fix 的目标。

### 4. 项目数据路径
项目数据在 `/home/li/code/personal/llm_wiki_projects/`，不在 `llm_wiki-server` 仓库内。另一台机器上需要 rsync 或重新拉取。