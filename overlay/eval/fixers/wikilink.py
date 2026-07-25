import glob, os, re, shutil, sys
_eval_dir = __file__.rsplit("/", 2)[0]
if _eval_dir not in sys.path:
    sys.path.insert(0, _eval_dir)
from ingest_check import Finding, read_file, normalize_slug

def _build_slug_index(project_dir: str) -> dict:
    """slug -> 真实路径的映射，用于模糊匹配"""
    index = {}
    for f in glob.glob(os.path.join(project_dir, "wiki", "**", "*.md"), recursive=True):
        name = os.path.splitext(os.path.basename(f))[0]
        slug = normalize_slug(name)
        if slug not in index:  # 首次胜出
            index[slug] = os.path.relpath(f, project_dir)
    return index

def fix_wikilink(project_dir: str, finding: Finding) -> dict:
    page_path = os.path.join(project_dir, finding.page)
    if not os.path.exists(page_path):
        return {"fixed": False, "error": "file not found"}

    content = read_file(page_path)
    if content is None:
        return {"fixed": False, "error": "cannot read"}

    target = finding.detail.get("target", "")
    link_text = finding.detail.get("link_text", target)

    # 构建 slug 索引
    slug_index = _build_slug_index(project_dir)
    target_slug = normalize_slug(target)

    changes = []
    new_content = content

    if target_slug in slug_index:
        # 目标页面存在但链接路径不对 -> 修正路径
        real_path = slug_index[target_slug]
        new_link = f"[[{real_path}]]"
        new_content = content.replace(f"[[{link_text}]]", new_link, 1)
        changes.append(f"fixed wikilink: [[{link_text}]] -> {new_link}")
    else:
        # 目标页面不存在 -> 尝试模糊匹配
        fuzzy_matches = [k for k in slug_index if target_slug in k or k in target_slug]
        if fuzzy_matches:
            best = fuzzy_matches[0]
            real_path = slug_index[best]
            new_link = f"[[{real_path}]]"
            new_content = content.replace(f"[[{link_text}]]", new_link, 1)
            changes.append(f"fuzzy matched: [[{link_text}]] -> {new_link}")
        else:
            # 降级为纯文本
            new_content = content.replace(f"[[{link_text}]]", link_text.split("|")[0].strip(), 1)
            changes.append(f"removed broken wikilink: [[{link_text}]] (no match found)")

    if changes:
        backup_dir = os.path.join(project_dir, "fix_backups")
        os.makedirs(backup_dir, exist_ok=True)
        backup_path = os.path.join(backup_dir, finding.page.replace("/", "_") + ".bak")
        shutil.copy2(page_path, backup_path)
        with open(page_path, "w", encoding="utf-8") as f:
            f.write(new_content)

    return {"fixed": len(changes) > 0, "changes": changes, "summary": "; ".join(changes)}

from . import register
fix_wikilink = register("rule_wikilink")(fix_wikilink)
