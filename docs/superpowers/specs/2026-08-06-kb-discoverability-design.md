# 知识库可发现性设计(/lite 年龄导航 + 主题速查 + 强项徽标)

> **性质**:已批准设计(2026-08-06)。
> **相关**:[2026-08-04-ebook-batch-ingestion-design.md](./2026-08-04-ebook-batch-ingestion-design.md)(知识库扩容)、[2026-06-25-retrieval-eval-design.md](./2026-06-25-retrieval-eval-design.md)(eval 体系)、`overlay/static/lite/README.md`(/lite 页)
> **问题**:知识库已从 2 本书扩到 10 本(1437 源页 + 510 场景页 + 4600+ 概念/实体/教训页),但 `/lite/` 页空状态只显示 `projects.meta.json` 里**手写的 3 条示例问题**,用户不知道能问什么。

---

## 1. 目标与决策

让用户在 `/lite/` 打开就知道「这个知识库能问什么」:按**年龄段**和**主题**浏览可问的问题,并看到每个方向的**回答质量**。

| 决策项 | 结论 |
|--------|------|
| 呈现面 | **`/lite/` 页**(空状态从 3 条静态 starters 升级为导航) |
| 年龄导航 | **阶段制 7 桶**:备孕/孕期、0-3月、4-6月、7-12月、1-2岁、3-6岁、学龄/青春期 |
| 主题来源 | **手工骨架 + 自动填问题**:人工定 8-10 个主题(喂养/睡眠/护理/发育/疾病/亲子关系/父职/管教/安全/备孕),每个主题的示例问题从 wiki 按 tag 自动派生 |
| 强项地图 | **质量徽标**(good/medium/weak):对每主题/年龄段跑小批 rag_eval 得出 |
| 数据形态 | **静态 `discover.json`**(构建产物,生成器产出;`/lite/` 加载渲染;无需改 Rust server) |

## 2. 背景与现状

- `/lite/` 空状态:`overlay/static/lite/app.js` `renderEmptyState()` 读 `state.activeProject.starters`(来自 `projects.meta.json`,每个项目 **3 条手写 starters**)→ 点 chip 直接发消息。
- 知识库素材(天然的问题形态):
  - **510 个 scenario 页**:标题即问题(如「1-1.5岁婴儿半夜起来玩」),frontmatter 含 `age-range` + 主题 tag。
  - **概念页**:按月龄组织(如「1-2个月婴儿发育里程碑」「1-2个月婴儿四季护理指南」)。
  - 主题 tag 分布(场景/概念页):喂养 380、睡眠 280、亲子关系 207、护理 195、父职 171、管教 124、安全 114…
  - 年龄段覆盖:`age-range` 从备孕期到 3-6 岁+(定本/西尔斯到学龄、养育女孩到青春期)。
- eval 基建:`overlay/eval/rag_eval.py`(检索/chat 评测)、`generate_test_cases.py`、测试用例 `test_cases/*.json` 可复用。

## 3. `discover.json` 结构

生成到 `overlay/static/lite/discover.json`(随部署同步;gitignore 与否见 §7):

```json
{
  "project": "ParentingBooks",
  "generatedAt": "2026-08-06",
  "ages": [
    {
      "id": "0-3m",
      "label": "0-3个月",
      "range": "0-3个月",
      "quality": "good",
      "questions": ["纯母乳需要补维生素D吗？", "3个月宝宝夜里频繁醒怎么办？"]
    }
  ],
  "topics": [
    {
      "id": "sleep",
      "label": "睡眠",
      "tag": "睡眠",
      "quality": "good",
      "questions": ["宝宝睡不好怎么办？", "怎么训练婴儿自主入睡？"]
    }
  ]
}
```

- `ages[]` — 7 个阶段桶,每桶 `quality` + 示例问题。
- `topics[]` — 手工骨架(8-10 个),每主题 `tag`(用于 wiki 匹配)+ `quality` + 示例问题。
- 问题点击行为:前端把问题字符串发给聊天(与现有 starters 一致)。

## 4. 生成器 `overlay/eval/generate_discover.py`

输入:项目路径(`--project`);输出:`discover.json`。

1. **扫 wiki**:`wiki/scenarios/` + `wiki/concepts/`(可选 `entities/`),读 frontmatter(`type/tags/age-range`)与标题。
2. **年龄分桶**:按 `age-range`/月龄 tag 归入 7 个阶段桶(见 §1);无年龄标记的归「通用」。
3. **主题归类**:手工骨架定义 `topic → [tags]` 映射(如 `睡眠 → [睡眠, 哄睡]`);页面 tag 命中即归该主题。
4. **示例问题派生**:优先用 scenario 标题(转问句:`「1-1.5岁婴儿半夜起来玩」` → `「1-1.5岁婴儿半夜起来玩怎么办？」`);不足时用概念页标题;每桶取 top-N(默认 5),去重、按书名去重。
5. **质量徽标**:对每主题/年龄段生成少量测试用例(每桶 10-15 条,复用 `generate_test_cases.py` 或直接构造),跑 `rag_eval` retrieval → 按 `source_hit@10`/`derived_hit@10` 定 `good(≥0.5) / medium(≥0.25) / weak(<0.25)`。
6. 输出 JSON;`--dry-run` 只打印统计。

## 5. `/lite/` 前端改动

`overlay/static/lite/app.js` + `index.html` + `app.css`:

- 空状态渲染:加载 `discover.json`(fetch 相对路径 `/lite/discover.json`;失败回退到现有 `starters`)。
- 交互:年龄阶段按钮(chips/横向滚动)→ 点开显示该阶段主题 + 示例问题(带质量徽标);主题速查卡区(grid,每卡 label + 徽标 + 问题 chips)。
- 点问题 chip → `sendMessage(text)`(复用现有机制)。
- 徽标样式:`.quality-good`(绿)/`.quality-medium`(黄)/`.quality-weak`(灰)。
- 保留搜索框与现有对话流程;新逻辑只在空状态(无消息)时展示。

## 6. 质量徽标数据流

`generate_discover.py --with-eval` → 每主题/年龄段跑小批 rag_eval(复用 `overlay/eval/rag_eval.py` 逻辑,按主题构造用例)→ 质量分写入 `discover.json`。`--without-eval` 时徽标默认 `good`(仅结构)。

## 7. 落盘与刷新

- `discover.json` 生成到 `overlay/static/lite/`(与 `/lite/` 其它文件一起 `sync-artifacts.sh` 同步到服务器)。
- **刷新时机**:每批次 ingest + eval 后重跑 `generate_discover.py --with-eval`。
- **gitignore**:`overlay/static/lite/discover.json` 默认 gitignore(构建产物);仓库内提交 `scripts/ebooks/...` 无关,提供一个 `discover.example.json` 或生成器可独立跑。
  - 决定:discover.json **gitignore**,生成器 + 主题骨架配置**提交**。

## 8. 主题骨架配置

主题骨架(手工部分)放 `overlay/eval/discover_topics.json`(提交):

```json
{
  "topics": [
    { "id": "sleep", "label": "睡眠", "tags": ["睡眠", "哄睡"] },
    { "id": "feeding", "label": "喂养", "tags": ["喂养", "辅食", "母乳"] },
    { "id": "care", "label": "护理", "tags": ["护理", "洗澡", "抚触"] },
    { "id": "dev", "label": "发育", "tags": ["发育", "里程碑", "大运动"] },
    { "id": "illness", "label": "疾病", "tags": ["疾病", "发热", "感冒", "腹泻"] },
    { "id": "parenting", "label": "亲子关系", "tags": ["亲子关系", "管教", "情绪"] },
    { "id": "fatherhood", "label": "父职", "tags": ["父职"] },
    { "id": "preconception", "label": "备孕", "tags": ["备孕", "孕期"] }
  ]
}
```
(可扩展;`tag` 与 wiki 页 `tags` 精确匹配)

## 9. 测试与验收

- `generate_discover.py` 单测:年龄分桶、主题归类、问题派生与去重、`--dry-run`。
- `/lite/` 冒烟:加载 `discover.json` → 年龄按钮 + 主题卡 + 徽标渲染 → 点问题发消息。
- 质量徽标:对已知强项(如睡眠/喂养)与弱项区分出 good/weak。
- 回归:`/lite/` 无 discover.json 时回退 starters 不报错。

## 10. 风险与回滚

| 风险 | 缓解 |
|------|------|
| 生成器扫 wiki 慢 | 只扫 scenarios+concepts(几千页,ms 级);缓存 |
| 问题派生质量差(标题不是问句) | 模板转问句 + 人工可在 topics 配置里覆写精选问题 |
| eval 徽标成本 | 每桶仅 10-15 条用例,`--without-eval` 可跳过;徽标是快照 |
| discover.json 与知识库脱节 | 每次 ingest 后重跑;gitignore 后部署时同步 |
| `/lite/` 改动破坏现有流程 | 只在空状态加渲染,回退 starters;手动冒烟 |

回滚:前端改动从 git 恢复;`discover.json` 删除即回退到旧 starters。
