# 知识库可发现性实现计划(/lite 年龄导航 + 主题速查 + 质量徽标)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 `/lite/` 空状态从 3 条静态 starters 升级为「年龄段导航 + 主题速查卡 + 质量徽标」的可发现入口,数据来自 wiki 结构自动派生的 `discover.json`。

**Architecture:** `overlay/eval/generate_discover.py` 扫 `wiki/scenarios/`+`wiki/concepts/`,按 7 个年龄段桶 + 手工主题骨架(discover_topics.json)归类,派生示例问题(场景标题转问句),可选跑服务器检索算质量徽标 → 输出 `overlay/static/lite/discover.json`。`/lite/` 前端在空状态加载该 JSON 渲染年龄按钮/主题卡/问题 chips,点问题即发消息。

**Tech Stack:** Python 3.12(纯 stdlib)、Vanilla JS(/lite 现状)、复用 `overlay/eval/rag_eval.py::search_wiki`(质量检索)、unittest。

**设计 spec:** [2026-08-06-kb-discoverability-design.md](../specs/2026-08-06-kb-discoverability-design.md)

---

## 关键事实(执行前必读)

- 仓库根:`/home/ab/overseas-github/llm-wiki-server`(branch `main`)。
- 项目数据:`~/overseas-github/llm_wiki_projects/ParentingBooks`。
- **wiki 页面形态**:`wiki/scenarios/*.md`(510 个,标题即问题,frontmatter 含 `tags: [书, 主题, 月龄]`、`age-range: "1-1.5岁"`);`wiki/concepts/*.md`(按月龄组织的概念)。
- **/lite 前端**:`overlay/static/lite/app.js` 的 `renderEmptyState()`(约 427-445 行)读 `state.activeProject?.starters`(来自 `projects.meta.json` 的 3 条手写 starters)渲染 chip,点击 `sendMessage(text)`。`index.html` 的 `<main id="messages">` 是聊天区。fetch 模式见 `app.js` 约 238 行(meta 加载)。
- **服务器检索接口**:`POST /api/v1/projects/{id}/search`(Bearer token);`overlay/eval/rag_eval.py::search_wiki(query, project_id, token)` 可直接复用(返回 `{results:[{path,...}]}`)。
- **测试运行**:`cd /home/ab/overseas-github/llm-wiki-server && python3 -m unittest discover -s overlay/eval/tests -v`。
- 服务器二进制:`overlay/server/target/release/llm-wiki-server`;启动见 `docs/新批次电子书入库.md` 第 10 步(需 `LLM_WIKI_PROJECT` + token)。
- 本项目 eval 测试通过 `sys.path.insert(0, <overlay/eval>)` 后 `from generate_discover import ...` 导入(参考 `test_generate_test_cases.py`)。

---

## 文件结构

| 文件 | 职责 | 类型 |
|------|------|------|
| `overlay/eval/generate_discover.py` | 扫 wiki → 年龄/主题分桶 → 派生问题 → 可选质量徽标 → 输出 discover.json | 新建 |
| `overlay/eval/discover_topics.json` | 手工主题骨架(主题→tags 映射) | 新建 |
| `overlay/eval/tests/test_generate_discover.py` | 生成器单测 | 新建 |
| `overlay/static/lite/discover.json` | 构建产物(gitignore),/lite 加载 | 生成 |
| `overlay/static/lite/app.js` / `index.html` / `app.css` | 空状态渲染年龄/主题/徽标/问题 | 修改 |
| `.gitignore` | 加 `overlay/static/lite/discover.json` | 修改 |
| `docs/新批次电子书入库.md` | 加一步「刷新 discover.json」 | 修改 |

---

## Task 1: `generate_discover.py` 核心纯函数(分桶/归类/问题派生)

**Files:**
- Create: `overlay/eval/generate_discover.py`
- Create: `overlay/eval/tests/test_generate_discover.py`

- [ ] **Step 1: 写失败测试**

Create `overlay/eval/tests/test_generate_discover.py`:

```python
import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from generate_discover import (  # noqa: E402
    AGE_BUCKETS,
    build_discover,
    classify_age,
    classify_topic,
    parse_frontmatter,
    to_question,
)

TOPICS = [
    {"id": "sleep", "label": "睡眠", "tags": ["睡眠", "哄睡"]},
    {"id": "feeding", "label": "喂养", "tags": ["喂养", "辅食"]},
]


def make_page(tmpdir, name, fm, title=None):
    import json
    body = "\n".join(f"{k}: {v}" for k, v in fm.items())
    content = f"---\n{body}\n---\n\n# {title or name}\n\n正文\n"
    p = os.path.join(tmpdir, name)
    with open(p, "w", encoding="utf-8") as f:
        f.write(content)
    return p


class TestParseFrontmatter(unittest.TestCase):
    def test_tags_and_age(self):
        d = tempfile.mkdtemp()
        p = make_page(d, "s1.md", {"type": "scenario", "tags": "[睡眠, 1-2个月]",
                                   "age-range": "\"1-2个月\""})
        fm = parse_frontmatter(open(p, encoding="utf-8").read())
        self.assertIn("睡眠", fm["tags"])
        self.assertEqual(fm["age-range"], "1-2个月")

    def test_no_frontmatter_returns_empty(self):
        fm = parse_frontmatter("纯正文\n没有 frontmatter\n")
        self.assertEqual(fm, {})


class TestClassifyAge(unittest.TestCase):
    def test_exact_bucket(self):
        self.assertEqual(classify_age({"age-range": "1-2个月", "tags": []}), "0-3m")
        self.assertEqual(classify_age({"age-range": "3-4岁", "tags": []}), "3-6y")
        self.assertEqual(classify_age({"age-range": "备孕期", "tags": []}), "preconception")

    def test_from_tags(self):
        self.assertEqual(classify_age({"age-range": "", "tags": ["青春期"]}), "school")

    def test_unknown_returns_none(self):
        self.assertIsNone(classify_age({"age-range": "", "tags": []}))


class TestClassifyTopic(unittest.TestCase):
    def test_tag_match(self):
        self.assertEqual(classify_topic({"tags": ["睡眠", "定本育儿百科"]}, TOPICS), "sleep")
        self.assertEqual(classify_topic({"tags": ["辅食"]}, TOPICS), "feeding")

    def test_no_match_returns_none(self):
        self.assertIsNone(classify_topic({"tags": ["未知"]}, TOPICS))


class TestToQuestion(unittest.TestCase):
    def test_appends_question(self):
        self.assertEqual(to_question("1-1.5岁婴儿半夜起来玩"), "1-1.5岁婴儿半夜起来玩怎么办？")

    def test_keeps_existing_question(self):
        self.assertEqual(to_question("纯母乳需要补维生素D吗？"), "纯母乳需要补维生素D吗？")


class TestBuildDiscover(unittest.TestCase):
    def test_buckets_and_questions(self):
        d = tempfile.mkdtemp()
        os.makedirs(os.path.join(d, "wiki", "scenarios"), exist_ok=True)
        os.makedirs(os.path.join(d, "wiki", "concepts"), exist_ok=True)
        make_page(d, "wiki/scenarios/s1.md",
                  {"type": "scenario", "tags": "[睡眠, 1-2个月]", "age-range": "\"1-2个月\""},
                  title="宝宝半夜起来玩")
        make_page(d, "wiki/scenarios/s2.md",
                  {"type": "scenario", "tags": "[喂养, 0-3个月]", "age-range": "\"0-3个月\""},
                  title="要不要补维生素D")
        make_page(d, "wiki/concepts/c1.md",
                  {"type": "concept", "tags": "[睡眠]", "age-range": ""},
                  title="婴儿睡眠周期")
        data = build_discover(d, TOPICS, per_bucket=5)
        ages = {a["id"]: a for a in data["ages"]}
        topics = {t["id"]: t for t in data["topics"]}
        self.assertIn("0-3m", ages)
        self.assertTrue(any("维生素D" in q for q in ages["0-3m"]["questions"]))
        self.assertIn("sleep", topics)
        self.assertTrue(any("睡眠周期" in q for q in topics["sleep"]["questions"]))
        # 默认 quality 为 good(未跑 eval)
        self.assertEqual(ages["0-3m"]["quality"], "good")


if __name__ == "__main__":
    unittest.main()
```

- [ ] **Step 2: 运行确认失败**

Run: `cd /home/ab/overseas-github/llm-wiki-server && python3 -m unittest discover -s overlay/eval/tests -p 'test_generate_discover.py' -v`
Expected: FAIL —— `ImportError: cannot import name 'generate_discover'`

- [ ] **Step 3: 实现核心纯函数**

Create `overlay/eval/generate_discover.py`:

```python
#!/usr/bin/env python3
"""为 /lite 生成 discover.json(年龄导航 + 主题速查 + 质量徽标)。

用法:
  python3 overlay/eval/generate_discover.py --project <path> \
      [--topics overlay/eval/discover_topics.json] \
      [--out overlay/static/lite/discover.json] \
      [--with-eval] [--dry-run]
"""
import argparse
import json
import os
import re
import sys

AGE_BUCKETS = [
    {"id": "preconception", "label": "备孕/孕期", "match": ["备孕", "孕期"]},
    {"id": "0-3m", "label": "0-3个月",
     "match": ["新生儿", "0-1个月", "1-2个月", "2-3个月", "0-3个月", "0-6个月"]},
    {"id": "4-6m", "label": "4-6个月", "match": ["3-4个月", "4-5个月", "5-6个月", "4-6个月"]},
    {"id": "7-12m", "label": "7-12个月",
     "match": ["6-7个月", "7-8个月", "8-9个月", "9-10个月", "10-11个月", "11-12个月", "7-12个月"]},
    {"id": "1-2y", "label": "1-2岁", "match": ["1-1.5岁", "1-2岁", "1.5-2岁"]},
    {"id": "3-6y", "label": "3-6岁", "match": ["3-4岁", "4-5岁", "5-6岁", "3-6岁"]},
    {"id": "school", "label": "学龄/青春期",
     "match": ["6-7岁", "7-8岁", "10-18岁", "学龄", "青春期"]},
]
DEFAULT_TOPICS_PATH = os.path.join(os.path.dirname(os.path.abspath(__file__)),
                                   "discover_topics.json")


def parse_frontmatter(content):
    """提取 frontmatter 的 tags 与 age-range。返回 dict(简单解析,无 yaml 依赖)。"""
    fm = {}
    if not content.startswith("---"):
        return fm
    end = content.find("\n---", 3)
    if end == -1:
        return fm
    block = content[3:end]
    for line in block.splitlines():
        if ":" not in line:
            continue
        key, _, val = line.partition(":")
        key = key.strip()
        val = val.strip().strip('"').strip("'")
        if key in ("tags",):
            fm[key] = [t.strip() for t in val.strip("[]").split(",") if t.strip()]
        else:
            fm[key] = val
    return fm


def classify_age(fm):
    """按 age-range 或 tags 归入 AGE_BUCKETS 之一;未命中返回 None。"""
    text = (fm.get("age-range") or "") + " " + " ".join(fm.get("tags") or [])
    for b in AGE_BUCKETS:
        if any(m in text for m in b["match"]):
            return b["id"]
    return None


def classify_topic(fm, topics):
    """按 tags 命中主题骨架之一;未命中返回 None。"""
    tags = set(fm.get("tags") or [])
    for t in topics:
        if tags & set(t.get("tags", [])):
            return t["id"]
    return None


def to_question(title):
    t = title.strip()
    if t.endswith(("？", "?", "吗", "呢", "怎么", "什么")):
        return t
    return f"{t}怎么办？"


def load_topics(path):
    with open(path, encoding="utf-8") as f:
        return json.load(f)["topics"]


def _read_pages(project_dir):
    """扫 scenarios + concepts,返回 [{type, title, fm, path}]。"""
    pages = []
    for sub in ("scenarios", "concepts"):
        d = os.path.join(project_dir, "wiki", sub)
        if not os.path.isdir(d):
            continue
        for name in sorted(os.listdir(d)):
            if not name.endswith(".md"):
                continue
            p = os.path.join(d, name)
            content = open(p, encoding="utf-8").read()
            fm = parse_frontmatter(content)
            title = fm.get("title") or name[:-3]
            pages.append({"type": sub, "title": title, "fm": fm, "path": p})
    return pages


def build_discover(project_dir, topics, per_bucket=5):
    """构造 {ages, topics}。每桶收集问题(场景优先,概念补齐),默认 quality=good。"""
    pages = _read_pages(project_dir)
    ages = {b["id"]: {"id": b["id"], "label": b["label"], "quality": "good",
                      "questions": []} for b in AGE_BUCKETS}
    topics_out = {t["id"]: {"id": t["id"], "label": t["label"], "tag": t["tags"][0],
                            "quality": "good", "questions": []} for t in topics}

    for pg in pages:
        fm = pg["fm"]
        q = to_question(pg["title"])
        aid = classify_age(fm)
        if aid:
            ages[aid]["questions"].append(q)
        tid = classify_topic(fm, topics)
        if tid:
            topics_out[tid]["questions"].append(q)

    def _dedup_top(lst):
        seen, out = [], []
        for q in lst:
            if q not in seen:
                seen.append(q)
                out.append(q)
        return out

    return {
        "ages": [a for a in ages.values() if a["questions"]],
        "topics": [t for t in topics_out.values() if t["questions"]],
    }


def main():
    ap = argparse.ArgumentParser(description="生成 /lite discover.json")
    ap.add_argument("--project", required=True)
    ap.add_argument("--topics", default=DEFAULT_TOPICS_PATH)
    ap.add_argument("--out", default="overlay/static/lite/discover.json")
    ap.add_argument("--with-eval", action="store_true",
                    help="用服务器检索算质量徽标(需 server 在 :8080 + --token)")
    ap.add_argument("--token", default="")
    ap.add_argument("--per-bucket", type=int, default=5)
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()

    topics = load_topics(args.topics)
    data = build_discover(args.project, topics, args.per_bucket)
    data["project"] = os.path.basename(os.path.normpath(args.project))
    data["generatedAt"] = os.environ.get("DISCOVER_DATE", "2026-08-06")

    if args.with_eval:
        from generate_discover import compute_quality  # Task 2 定义
        compute_quality(data, args.token)

    if args.dry_run:
        for a in data["ages"]:
            print(f"age {a['label']}: {len(a['questions'])} 问题")
        for t in data["topics"]:
            print(f"topic {t['label']}: {len(t['questions'])} 问题")
        return

    os.makedirs(os.path.dirname(args.out) or ".", exist_ok=True)
    with open(args.out, "w", encoding="utf-8") as f:
        json.dump(data, f, ensure_ascii=False, indent=2)
    print(f"discover.json -> {args.out} ({len(data['ages'])} ages, {len(data['topics'])} topics)")


if __name__ == "__main__":
    main()
```

- [ ] **Step 4: 运行确认通过**

Run: `cd /home/ab/overseas-github/llm-wiki-server && python3 -m unittest discover -s overlay/eval/tests -v`
Expected: `Ran 118 tests ... OK`(113 + 5 新增;若总数不同以"全过"为准)

- [ ] **Step 5: 提交**

```bash
git add overlay/eval/generate_discover.py overlay/eval/tests/test_generate_discover.py
git commit -m "feat(discover): core functions — age/topic bucketing, question derivation"
```

---

## Task 2: `generate_discover.py` 质量徽标 + 输出完整 CLI

**Files:**
- Modify: `overlay/eval/generate_discover.py`(加 `compute_quality` + 完成 `main` 输出)
- Create: `overlay/eval/discover_topics.json`

- [ ] **Step 1: 创建主题骨架配置**

Create `overlay/eval/discover_topics.json`:

```json
{
  "topics": [
    { "id": "feeding", "label": "喂养", "tags": ["喂养", "辅食", "母乳", "配方奶"] },
    { "id": "sleep", "label": "睡眠", "tags": ["睡眠", "哄睡", "夜醒"] },
    { "id": "care", "label": "护理", "tags": ["护理", "洗澡", "抚触", "穿衣"] },
    { "id": "dev", "label": "发育", "tags": ["发育", "里程碑", "大运动", "能力"] },
    { "id": "illness", "label": "疾病", "tags": ["疾病", "发热", "感冒", "腹泻", "湿疹"] },
    { "id": "parenting", "label": "亲子关系", "tags": ["亲子关系", "管教", "情绪", "习惯"] },
    { "id": "fatherhood", "label": "父职", "tags": ["父职"] },
    { "id": "preconception", "label": "备孕", "tags": ["备孕", "孕期", "备孕期"] },
    { "id": "safety", "label": "安全", "tags": ["安全", "意外"] }
  ]
}
```

- [ ] **Step 2: 写失败测试(追加到 test_generate_discover.py)**

```python
class TestComputeQuality(unittest.TestCase):
    def test_quality_good_when_retrieved(self):
        from generate_discover import compute_quality
        bucket = {"id": "sleep", "label": "睡眠", "quality": "good",
                  "questions": ["宝宝睡不好怎么办？"]}
        calls = {"n": 0}

        def fake_search(q, pid, token):
            calls["n"] += 1
            return {"results": [{"path": "wiki/scenarios/x.md"}]}

        # 命中率 100% → good
        res = compute_quality([bucket], fake_search, sources={"宝宝睡不好怎么办？": "wiki/scenarios/x.md"})
        self.assertEqual(res, "good")

    def test_quality_weak_when_no_hits(self):
        from generate_discover import compute_quality
        bucket = {"id": "sleep", "label": "睡眠", "quality": "good",
                  "questions": ["某个冷门问题怎么办？"]}

        def fake_search(q, pid, token):
            return {"results": []}

        res = compute_quality([bucket], fake_search, sources={"某个冷门问题怎么办？": "wiki/scenarios/y.md"})
        self.assertEqual(res, "weak")
```

- [ ] **Step 3: 运行确认失败**

Run: `cd /home/ab/overseas-github/llm-wiki-server && python3 -m unittest discover -s overlay/eval/tests -p 'test_generate_discover.py' -v`
Expected: FAIL —— `ImportError: cannot import name 'compute_quality'`

- [ ] **Step 4: 实现 `compute_quality`**

在 `generate_discover.py` 加(放在 `build_discover` 之后):

```python
THRESHOLDS = {"good": 0.5, "medium": 0.25}


def compute_quality(buckets, search_fn, sources=None, k=10):
    """对每个桶的问题做检索,按来源命中率定 good/medium/weak。

    search_fn(query, project_id, token) -> {results:[{path}]}
    sources: {question: expected_source_path} 的映射;缺省时用"任何命中即算"。
    返回: {bucket_id: "good"|"medium"|"weak"}。
    """
    out = {}
    for b in buckets:
        qs = b.get("questions", [])
        if not qs:
            out[b["id"]] = "weak"
            continue
        hits = 0
        for q in qs:
            try:
                res = search_fn(q, "", "")
                paths = [r.get("path", "") for r in res.get("results", [])[:k]]
            except Exception:
                paths = []
            expected = (sources or {}).get(q)
            if expected and expected in paths:
                hits += 1
            elif not expected and paths:
                hits += 1
        rate = hits / len(qs)
        if rate >= THRESHOLDS["good"]:
            out[b["id"]] = "good"
        elif rate >= THRESHOLDS["medium"]:
            out[b["id"]] = "medium"
        else:
            out[b["id"]] = "weak"
    return out
```

然后改 `main()` 的 `--with-eval` 分支,让它真正调用服务器检索并回写 quality。先在 `main()` 的 argparse 里加 `--project-id`:

```python
    ap.add_argument("--project-id", default="",
                    help="服务器项目 UUID(如 a8f3c2e1-...);缺省时按 --project 名查 /api/v1/projects")
```

再改 `main()` 的 `--with-eval` 分支:

```python
    if args.with_eval:
        import sys
        sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__))))
        import rag_eval

        # 解析 project_id:显式 --project-id 优先,否则查服务器项目列表(按名匹配)
        project_id = args.project_id
        if not project_id:
            import urllib.request
            req = urllib.request.Request(
                "http://127.0.0.1:8080/api/v1/projects",
                headers={"Authorization": f"Bearer {args.token}"})
            with urllib.request.urlopen(req, timeout=10) as resp:
                body = json.loads(resp.read().decode("utf-8"))
            for p in body.get("projects", []):
                if p.get("name") == args.project:
                    project_id = p.get("id")
                    break
        if not project_id:
            print("error: 无法解析 project_id(--project 传项目名,或提供 --project-id)", file=sys.stderr)
            sys.exit(1)

        search_fn = lambda q, pid, tok: rag_eval.search_wiki(q, project_id, args.token)
        out = compute_quality(data["ages"] + data["topics"], search_fn)
        for b in data["ages"] + data["topics"]:
            b["quality"] = out.get(b["id"], "weak")
```

> 注意:`rag_eval.search_wiki` 需要 server 在 :8080 + `--token`。`--project` 传**项目名**(如 `ParentingBooks`),不是路径(与 `rag_eval --project` 一致)。

- [ ] **Step 5: 运行确认通过**

Run: `cd /home/ab/overseas-github/llm-wiki-server && python3 -m unittest discover -s overlay/eval/tests -v`
Expected: 全过(含 2 个新增)

- [ ] **Step 6: CLI dry-run 冒烟(真实项目,不跑 eval)**

Run: `python3 overlay/eval/generate_discover.py --project ~/overseas-github/llm_wiki_projects/ParentingBooks --dry-run`
Expected: 打印各 age/topic 桶与问题数(应非零、且多桶有内容)

- [ ] **Step 7: 提交**

```bash
git add overlay/eval/generate_discover.py overlay/eval/discover_topics.json overlay/eval/tests/test_generate_discover.py
git commit -m "feat(discover): quality badges via server retrieval + topic skeleton config"
```

---

## Task 3: 真实项目生成 discover.json + 质量徽标(server 检索)

**Files:** 无代码改动(验证)。

- [ ] **Step 1: 启动 server(若未运行)**

```bash
export LLM_WIKI_PROJECT="$HOME/overseas-github/llm_wiki_projects/ParentingBooks"
export LLM_WIKI_API_TOKEN="$(python3 -c 'import json; print(json.load(open("overlay/config/server.local.json")).get("apiConfig",{}).get("token",""))')"
export LLM_WIKI_CONFIG=overlay/config/server.local.json
export LLM_WIKI_STATIC=upstream/dist
setsid nohup ./overlay/server/target/release/llm-wiki-server > /tmp/llm-wiki-server.log 2>&1 &
sleep 2
curl -s http://127.0.0.1:8080/api/v1/health | head -c 100; echo
```
Expected: `{"ok":true,...}`。若已运行则跳过。

- [ ] **Step 2: 生成(含质量徽标)**

Run: `TOKEN=$(python3 -c 'import json; print(json.load(open("overlay/config/server.local.json")).get("apiConfig",{}).get("token",""))') && python3 overlay/eval/generate_discover.py --project ParentingBooks --token "$TOKEN" --with-eval --out overlay/static/lite/discover.json`
Expected: 生成 `discover.json`;检查内容:
```bash
python3 -c "import json; d=json.load(open('overlay/static/lite/discover.json')); print('ages:', [(a['id'],a['quality'],len(a['questions'])) for a in d['ages']]); print('topics:', [(t['id'],t['quality'],len(t['questions'])) for t in d['topics']])"
```
Expected: 7 个年龄段桶 + ~9 个主题,各带 quality(good/medium/weak 应能区分)与问题。

- [ ] **Step 3: 校验 JSON 合法 + 问题质量抽查**

`python3 -m json.tool overlay/static/lite/discover.json > /dev/null && echo json-ok`
抽查 2-3 个问题:应为自然问句、无重复、能对应知识库主题。

- [ ] **Step 4: 提交 gitignore 决定**

将 `overlay/static/lite/discover.json` 加入 `.gitignore`(构建产物);确保 `git status` 不显示它。

- [ ] **Step 5: 提交**

```bash
git add .gitignore
git commit -m "chore(discover): gitignore generated discover.json"
```

---

## Task 4: `/lite/` 前端渲染(年龄导航 + 主题卡 + 徽标)

**Files:**
- Modify: `overlay/static/lite/app.js`、`index.html`、`app.css`

- [ ] **Step 1: 读现有 renderEmptyState**

读 `overlay/static/lite/app.js` 约 425-450 行 `renderEmptyState()`,理解现状:空消息时渲染 `state.activeProject?.starters` 的 chips。

- [ ] **Step 2: 加 discover 加载与渲染**

在 `app.js` 中:
1. 加一个 `loadDiscover()` 异步函数:fetch `/lite/discover.json`(相对路径,`{ cache: "no-cache" }`),失败返回 `null`:
```js
async function loadDiscover() {
  try {
    const res = await fetch("/lite/discover.json", { cache: "no-cache" });
    if (!res.ok) return null;
    return await res.json();
  } catch (e) {
    return null;
  }
}
```
2. 改 `renderEmptyState()`(改为 `async`):加载 `discover`;若无(或项目不匹配)回退到现有 `starters`。有 `discover` 时渲染:
   - **年龄导航**:横向 chips(`state.discover.ages`),每 chip 显示 `label`,选中态。
   - **主题速查卡**:grid,每卡 `label` + 质量徽标(`quality-{quality}`)+ 前 3 个问题 chips。
   - 点问题 chip → `sendMessage(text)`(复用现有)。
   - 点年龄段 chip → 在该段下方展示该段问题 chips(或直接高亮)。
3. 关键:`renderEmptyState` 需在 `state.discover` 就绪后调用;或在空状态首次渲染时 `await loadDiscover()`。

实现骨架(放入 app.js,按现有代码风格):

```js
async function renderEmptyState() {
  const msgs = $("#messages");
  if (!msgs) return;
  if (state.currentMessages.length > 0) return;
  const discover = state.discover || await loadDiscover();
  state.discover = discover;
  msgs.innerHTML = "";
  const welcome = document.createElement("div");
  welcome.className = "empty-state";
  let html = `<h2 class="empty-title">${escapeHtml(I18N.t("lite.empty.title"))}</h2>`;
  if (discover && (discover.ages || discover.topics)) {
    html += `<div class="discover">`;
    if (discover.ages?.length) {
      html += `<div class="discover-section"><h3>按宝宝月龄</h3><div class="age-nav">` +
        discover.ages.map(a =>
          `<button class="age-chip" data-age="${escapeHtml(a.id)}" title="${escapeHtml(a.label)}">` +
          `${escapeHtml(a.label)}<span class="quality quality-${a.quality}">${escapeHtml(a.quality)}</span></button>`
        ).join("") + `</div></div>`;
    }
    if (discover.topics?.length) {
      html += `<div class="discover-section"><h3>按主题</h3><div class="topic-grid">` +
        discover.topics.map(t =>
          `<div class="topic-card"><div class="topic-head">` +
          `<span class="topic-label">${escapeHtml(t.label)}</span>` +
          `<span class="quality quality-${t.quality}">${escapeHtml(t.quality)}</span></div>` +
          `<div class="topic-questions">` +
          (t.questions || []).slice(0, 3).map(q =>
            `<button class="suggestion-chip" data-text="${escapeHtml(q)}">${escapeHtml(q)}</button>`
          ).join("") +
          `</div></div>`
        ).join("") + `</div></div>`;
    }
    html += `</div>`;
  } else {
    html += (state.activeProject?.starters || []).length
      ? `<div class="suggestion-list">` +
        state.activeProject.starters.map(s =>
          `<button class="suggestion-chip" data-text="${escapeHtml(s)}">${escapeHtml(s)}</button>`
        ).join("") + `</div>`
      : "";
  }
  welcome.innerHTML = html;
  msgs.appendChild(welcome);
  welcome.querySelectorAll("[data-text]").forEach(btn => {
    btn.addEventListener("click", () => sendMessage(btn.dataset.text));
  });
  welcome.querySelectorAll(".age-chip").forEach(btn => {
    btn.addEventListener("click", () => {
      const id = btn.dataset.age;
      const age = (discover.ages || []).find(a => a.id === id);
      if (age) {
        // 在该段下方渲染该年龄的问题 chips
        let list = welcome.querySelector(".age-questions");
        if (!list) {
          list = document.createElement("div");
          list.className = "age-questions suggestion-list";
          welcome.appendChild(list);
        }
        list.innerHTML = (age.questions || []).map(q =>
          `<button class="suggestion-chip" data-text="${escapeHtml(q)}">${escapeHtml(q)}</button>`
        ).join("");
        list.querySelectorAll("[data-text]").forEach(btn2 =>
          btn2.addEventListener("click", () => sendMessage(btn2.dataset.text)));
      }
    });
  });
}
```
> 注意:现有代码里 `renderEmptyState` 可能是同步被调用;改为 async 后,调用点需 `renderEmptyState()`(不用 await 也 OK,内部 await)。若 `state.activeProject` 未就绪(meta 未加载),discover 优先,starters 回退逻辑保持。

- [ ] **Step 3: index.html / app.css 补样式**

`index.html` 空状态结构不动(renderEmptyState 动态生成)。`app.css` 加:
```css
.discover-section { margin: 12px 0; }
.age-nav { display: flex; flex-wrap: wrap; gap: 8px; }
.age-chip { padding: 6px 12px; border-radius: 999px; border: 1px solid #ddd; cursor: pointer; }
.topic-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(200px, 1fr)); gap: 10px; }
.topic-card { border: 1px solid #eee; border-radius: 8px; padding: 10px; }
.topic-head { display: flex; justify-content: space-between; align-items: center; }
.quality { font-size: 10px; padding: 1px 6px; border-radius: 999px; margin-left: 6px; }
.quality-good { background: #e6f4ea; color: #137333; }
.quality-medium { background: #fef7e0; color: #b06000; }
.quality-weak { background: #f1f3f4; color: #5f6368; }
.topic-questions { display: flex; flex-direction: column; gap: 6px; margin-top: 8px; }
.age-questions { margin-top: 10px; }
```

- [ ] **Step 4: 语法检查(无构建)**

`node --check overlay/static/lite/app.js`(若 node 可用)或人工通读;`overlay/static/lite/` 是纯静态,改动无需重编译 server。

- [ ] **Step 5: 提交**

```bash
git add overlay/static/lite/app.js overlay/static/lite/index.html overlay/static/lite/app.css
git commit -m "feat(lite): discover empty-state — age nav + topic cards + quality badges"
```

---

## Task 5: 前端冒烟(本地 server + discover.json)

**Files:** 无代码改动(验证)。

- [ ] **Step 1: 确保 server 在 :8080 + discover.json 已生成**(Task 3 已做)
- [ ] **Step 2: 浏览器/curl 验证静态资源**

Run: `curl -s http://127.0.0.1:8080/lite/ | grep -o "discover.json" | head -1`(确认页面可访问;若 server 静态服务 /lite/)
Run: `curl -s http://127.0.0.1:8080/lite/discover.json | python3 -m json.tool > /dev/null && echo "discover served"`
Expected: `discover served`(server 静态文件服务可达;若 /lite/discover.json 404 属预期,见 Step 3)

- [ ] **Step 3: 前端渲染验证**

若本机有浏览器自动化(Playwright)则用;否则人工核对:
1. 打开 `http://127.0.0.1:8080/lite/` → 空状态应显示「按宝宝月龄」年龄 chips + 「按主题」主题卡(带质量徽标)。
2. 点一个年龄 chip → 显示该年龄问题。
3. 点一个问题 → 进入聊天并发消息。
4. 删除 `overlay/static/lite/discover.json` 后刷新 → 回退到旧 3 条 starters,不报错。
5. 还原 discover.json。

> 若 /lite 走 `upstream/dist`(构建产物)而非 `overlay/static/lite/`,则前端改动需先 `./scripts/build-web.sh` 再验证。执行时先确认 `LLM_WIKI_STATIC` 指向哪、`/lite/` 由哪个目录服务(见 `docs/代码结构总览.md`)。

- [ ] **Step 4: 报告冒烟结果**

- **Status:** DONE | DONE_WITH_CONCERNS | BLOCKED
- 各验证点结果 + 发现的问题(渲染错位、加载失败、回退失效等)

---

## Task 6: 刷新流程 + runbook 更新

**Files:**
- Modify: `docs/新批次电子书入库.md`
- Modify: `docs/notes/2026-08-04-ebook-ingestion-progress.md`

- [ ] **Step 1: runbook 加一步「刷新 discover.json」**

在 `docs/新批次电子书入库.md` 的「10. eval」之后加:

```markdown
## 11. 刷新 /lite 可发现入口(discover.json)
新批次入库后,重新生成 /lite 的年龄/主题/质量数据:
`python3 overlay/eval/generate_discover.py --project <项目名> --token <token> --with-eval --out overlay/static/lite/discover.json`
> 需要 server 在 :8080(质量徽标走检索命中率);`discover.json` 是构建产物(gitignore),随 `sync-artifacts.sh` 同步到服务器。
```

- [ ] **Step 2: 交接文档补一句**

在 progress 文档「遗留待办」补:discoverability 功能已上线(见 spec `2026-08-06-kb-discoverability-design.md`)。

- [ ] **Step 3: 提交**

```bash
git add docs/新批次电子书入库.md docs/notes/2026-08-04-ebook-ingestion-progress.md
git commit -m "docs(discover): refresh runbook step + handoff note"
```

---

## Task 7: 集成验收(全链路)

**Files:** 无代码改动。

- [ ] **Step 1: 生成器全链路(含质量)重跑**

Run: `TOKEN=$(python3 -c 'import json; print(json.load(open("overlay/config/server.local.json")).get("apiConfig",{}).get("token",""))') && python3 overlay/eval/generate_discover.py --project ParentingBooks --token "$TOKEN" --with-eval --out overlay/static/lite/discover.json`
Expected: 成功生成;`--dry-run` 再跑一遍确认可复现(同输入同输出)。

- [ ] **Step 2: 前端全链路人工过一遍**(同 Task 5 Step 3:年龄/主题/徽标/发消息/回退)

- [ ] **Step 3: 全量测试回归**

Run: `python3 -m unittest discover -s overlay/eval/tests -v`(118+) 与 `python3 -m unittest discover -s scripts/tests -v`(45)
Expected: 全过。

- [ ] **Step 4: 提交(若有遗留)**

如有 bug 修复则提交;否则说明"验收通过,无代码改动"。

---

## 自检

**Spec 覆盖:**
- §3 discover.json 结构 → Task 2,3 ✓
- §4 生成器(分桶/归类/问题/质量/输出)→ Task 1,2,3 ✓
- §5 /lite 前端 → Task 4,5 ✓
- §6 质量徽标(服务器检索)→ Task 2,3 ✓
- §7 落盘 gitignore → Task 3 ✓
- §8 主题骨架配置 → Task 2 ✓
- §9 测试/验收 → Task 1,2,3,5,7 ✓
- §10 风险(无 discover 回退)→ Task 4,5 ✓

**类型一致性:** `compute_quality(buckets, search_fn, sources=None, k=10)` 返回 `{id: good|medium|weak}`,`main` 用它回写 `data["ages"|"topics"]` 的 `quality` —— 一致。`build_discover` 返回的 ages/topics 每桶含 `{id,label,quality,questions}`(topics 多 `tag` 字段),前端读取这些字段 —— 一致。`parse_frontmatter` 返回 `{tags:[], age-range:"", ...}`;`classify_age/classify_topic` 消费 —— 一致。
