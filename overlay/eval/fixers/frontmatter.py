import json, os, shutil, datetime, sys
# 确保 eval 目录在路径中以便导入 ingest_check
_eval_dir = __file__.rsplit("/", 2)[0]
if _eval_dir not in sys.path:
    sys.path.insert(0, _eval_dir)
from ingest_check import Finding, read_file, parse_frontmatter

def _serialize_frontmatter(fm: dict) -> str:
    """将 dict 序列化为 YAML frontmatter 字符串"""
    lines = ["---"]
    for k, v in fm.items():
        if isinstance(v, list):
            lines.append(f"{k}:")
            for item in v:
                lines.append(f"  - {json.dumps(item, ensure_ascii=False)}")
        elif isinstance(v, str):
            if any(c in v for c in [":", "#", "[", "]", "{", "}", "'", '"']) or v.lower() in ("true", "false", "null", "yes", "no"):
                lines.append(f"{k}: {json.dumps(v, ensure_ascii=False)}")
            else:
                lines.append(f"{k}: {v}")
        else:
            lines.append(f"{k}: {v}")
    lines.append("---")
    return "\n".join(lines)

def fix_frontmatter(project_dir: str, finding: Finding) -> dict:
    page_path = os.path.join(project_dir, "wiki", finding.page)
    if not os.path.exists(page_path):
        return {"fixed": False, "error": "file not found"}

    content = read_file(page_path)
    if content is None:
        return {"fixed": False, "error": "cannot read"}

    fm, body = parse_frontmatter(content)
    changes = []

    # 补缺失字段
    today = datetime.date.today().isoformat()
    for key in finding.detail.get("missing_keys", []):
        defaults = {
            "type": "note",
            "title": os.path.splitext(os.path.basename(finding.page))[0],
            "created": today,
            "updated": today,
        }
        if key not in fm:
            fm[key] = defaults.get(key, "")
            changes.append(f"added frontmatter {key}={fm[key]}")

    # 当有实际新增字段时，确保 updated 总是最新
    if changes and "updated" not in finding.detail.get("missing_keys", []):
        fm["updated"] = today
        changes.append("bumped updated date")

    if not changes:
        return {"fixed": False, "changes": []}

    # 备份原文件
    backup_dir = os.path.join(project_dir, "fix_backups")
    os.makedirs(backup_dir, exist_ok=True)
    backup_path = os.path.join(backup_dir, finding.page.replace("/", "_") + ".bak")
    shutil.copy2(page_path, backup_path)

    # 写回
    new_content = _serialize_frontmatter(fm) + "\n" + body
    with open(page_path, "w", encoding="utf-8") as f:
        f.write(new_content)

    return {
        "fixed": True,
        "backup": os.path.relpath(backup_path, project_dir),
        "changes": changes,
        "diff": f"bumped {len(changes)} frontmatter field(s)",
    }

from . import register
fix_frontmatter = register("rule_frontmatter")(fix_frontmatter)
