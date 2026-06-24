#!/usr/bin/env python3
"""
Ingest 质量检查工具

用法:
    python ingest_check.py --project <项目路径> [--verbose]
    python ingest_check.py --project <项目路径> --check-sources
"""

import argparse
import json
import os
import re
import glob
from pathlib import Path
from typing import Dict, List, Tuple, Optional

try:
    import yaml
except Exception:  # pragma: no cover - 允许无 PyYAML 的环境回退
    yaml = None  # type: ignore

# ============ 配置 ============

SCHEMA_KEYS = ['type', 'title', 'created', 'updated']

# ============ 工具函数 ============

def read_file(path: str) -> Optional[str]:
    """读取文件内容"""
    try:
        with open(path, 'r', encoding='utf-8') as f:
            return f.read()
    except Exception as e:
        print(f"[WARN] 无法读取 {path}: {e}", file=__import__('sys').stderr)
        return None

def parse_frontmatter(content: str) -> Tuple[Dict, str]:
    """解析 YAML frontmatter，优先用 PyYAML，失败时回退到简单解析器"""
    fm_match = re.match(r'^---\n(.*?)\n---\n', content, re.DOTALL)
    if not fm_match:
        return {}, content

    fm_text = fm_match.group(1)
    body = content[fm_match.end():]

    if yaml is not None:
        try:
            fm = yaml.safe_load(fm_text) or {}
            if isinstance(fm, dict):
                return fm, body
        except Exception as e:
            print(f"[WARN] PyYAML 解析 frontmatter 失败，回退到简单解析: {e}",
                  file=__import__('sys').stderr)

    # 简单解析器（兜底）
    fm = {}
    for line in fm_text.split('\n'):
        if ':' in line:
            key, value = line.split(':', 1)
            fm[key.strip()] = value.strip().strip('"\'')

    return fm, body

def extract_wikilinks(content: str) -> List[str]:
    """提取 wiki 内链"""
    return re.findall(r'\[\[([^\]]+)\]\]', content)

def extract_headers(content: str) -> List[str]:
    """提取标题层级"""
    headers = []
    for line in content.split('\n'):
        m = re.match(r'^(#{1,6})\s+(.+)', line)
        if m:
            headers.append((len(m.group(1)), m.group(2)))
    return headers

def count_words(text: str) -> int:
    """统计中文字符数（粗略）"""
    return len(re.findall(r'[一-鿿]', text))


def page_identity(md_file: str, wiki_dir: str) -> str:
    """页面唯一标识：相对 wiki 目录、无 .md 后缀的路径"""
    rel = os.path.relpath(md_file, wiki_dir)
    return os.path.splitext(rel)[0].replace('\\', '/')


def normalize_link_target(link: str) -> str:
    """规范化 wikilink 目标：去 alias、去 anchor、去 .md 后缀"""
    return link.split('|')[0].split('#')[0].removesuffix('.md').strip()


def is_linked(identity: str, link_targets: set) -> bool:
    """判断页面 identity 是否被链接集合覆盖"""
    base = os.path.basename(identity)
    for t in link_targets:
        if t == identity:
            return True
        if os.path.basename(t) == base:
            return True
        if t == base:
            return True
    return False


def normalize_slug(name: str) -> str:
    """把文件名/标题归一化为可比较的 slug"""
    name = os.path.splitext(name)[0]
    return re.sub(r'[^\w一-鿿]', '', name).lower()


def check_wikilink_density(wiki_dir: str) -> Dict:
    """检查 Wikilink 密度"""
    stats = {
        "total_links": 0,
        "total_pages": 0,
        "avg_links_per_page": 0,
        "orphaned_pages": 0,
        "link_targets": set()
    }
    
    # 收集所有 wiki 页面
    md_files = glob.glob(f"{wiki_dir}/**/*.md", recursive=True)
    stats["total_pages"] = len(md_files)
    
    # 统计链接
    link_targets = set()
    for md_file in md_files:
        content = read_file(md_file)
        if content:
            links = extract_wikilinks(content)
            stats["total_links"] += len(links)
            for link in links:
                link_targets.add(normalize_link_target(link))
    
    stats["link_targets"] = link_targets
    stats["avg_links_per_page"] = stats["total_links"] / max(stats["total_pages"], 1)
    
    # 孤立页面：没有链接指向其 identity 或 basename
    identities = {page_identity(f, wiki_dir) for f in md_files}
    linked_identities = {ident for ident in identities if is_linked(ident, link_targets)}
    stats["orphaned_pages"] = len(identities - linked_identities)
    
    return stats

def check_schema_compliance(wiki_dir: str) -> Dict:
    """检查 schema 合规性"""
    stats = {
        "total_pages": 0,
        "compliant_pages": 0,
        "missing_fields": [],
        "compliance_rate": 0
    }
    
    md_files = glob.glob(f"{wiki_dir}/**/*.md", recursive=True)
    stats["total_pages"] = len(md_files)
    
    for md_file in md_files:
        content = read_file(md_file)
        if not content:
            continue
        
        fm, body = parse_frontmatter(content)
        
        # 检查必需字段
        has_required = all(k in fm for k in ['type', 'title'])
        if has_required:
            stats["compliant_pages"] += 1
        else:
            missing = [k for k in ['type', 'title'] if k not in fm]
            stats["missing_fields"].append({
                "file": os.path.relpath(md_file, wiki_dir),
                "missing": missing
            })
    
    stats["compliance_rate"] = stats["compliant_pages"] / max(stats["total_pages"], 1)
    
    return stats

def check_category_coverage(wiki_dir: str) -> Dict:
    """检查分类覆盖（sources/concepts/entities/scenarios）"""
    stats = {
        "sources": {"count": 0, "files": []},
        "concepts": {"count": 0, "files": []},
        "entities": {"count": 0, "files": []},
        "scenarios": {"count": 0, "files": []},
        "lessons": {"count": 0, "files": []}
    }
    
    for category in stats.keys():
        category_dir = os.path.join(wiki_dir, category)
        if os.path.exists(category_dir):
            md_files = glob.glob(f"{category_dir}/*.md")
            stats[category]["count"] = len(md_files)
            stats[category]["files"] = [os.path.basename(f) for f in md_files[:10]]
    
    # 检查 index.md 中的引用覆盖
    index_path = os.path.join(wiki_dir, "index.md")
    if index_path and os.path.exists(index_path):
        content = read_file(index_path)
        if content:
            stats["index_links"] = len(extract_wikilinks(content))
    else:
        stats["index_links"] = 0
    
    return stats

def compare_source_to_wiki(raw_dir: str, wiki_sources_dir: str) -> Dict:
    """对比原始材料与生成的 wiki 页面"""
    stats = {
        "total_raw_sources": 0,
        "total_wiki_pages": 0,
        "coverage_rate": 0,
        "missing_wiki_pages": [],
        "coverage_samples": []
    }
    
    # 统计原始材料
    raw_files = glob.glob(f"{raw_dir}/sources/*.md")
    stats["total_raw_sources"] = len(raw_files)
    
    # 统计 wiki 页面
    wiki_files = glob.glob(f"{wiki_sources_dir}/*.md")
    stats["total_wiki_pages"] = len(wiki_files)
    
    # 匹配检查（用归一化 slug 精确匹配，避免子串误匹配）
    if raw_files and wiki_files:
        raw_basenames = {os.path.basename(f) for f in raw_files}
        wiki_basenames = {os.path.basename(f) for f in wiki_files}
        wiki_slugs = {normalize_slug(b) for b in wiki_basenames}

        matched = 0
        missing = []
        for raw_base in raw_basenames:
            raw_slug = normalize_slug(raw_base)
            if raw_slug in wiki_slugs:
                matched += 1
            else:
                missing.append(raw_base)

        stats["coverage_rate"] = matched / max(len(raw_basenames), 1)
        stats["matched_sources"] = matched
        stats["missing_wiki_pages"] = missing[:20]
    else:
        stats["coverage_rate"] = 0

    return stats

def analyze_wiki_quality(wiki_dir: str) -> Dict:
    """分析 wiki 页面质量"""
    stats = {
        "avg_word_count": 0,
        "min_word_count": float('inf'),
        "max_word_count": 0,
        "empty_pages": 0,
        "quality_distribution": {
            "high": 0,    # > 500 字
            "medium": 0, # 200-500 字
            "low": 0     # < 200 字
        }
    }
    
    md_files = glob.glob(f"{wiki_dir}/**/*.md", recursive=True)
    word_counts = []
    
    for md_file in md_files:
        content = read_file(md_file)
        if not content:
            stats["empty_pages"] += 1
            continue
        
        _, body = parse_frontmatter(content)
        word_count = count_words(body)
        word_counts.append(word_count)
        
        if word_count > stats["max_word_count"]:
            stats["max_word_count"] = word_count
        if word_count < stats["min_word_count"]:
            stats["min_word_count"] = word_count
        
        if word_count > 500:
            stats["quality_distribution"]["high"] += 1
        elif word_count >= 200:
            stats["quality_distribution"]["medium"] += 1
        else:
            stats["quality_distribution"]["low"] += 1
    
    if word_counts:
        stats["avg_word_count"] = sum(word_counts) / len(word_counts)
    else:
        stats["min_word_count"] = 0
    
    return stats

def run_ingest_check(project_dir: str, verbose: bool = False) -> Dict:
    """运行 Ingest 质量检查"""
    wiki_dir = os.path.join(project_dir, "wiki")
    raw_dir = os.path.join(project_dir, "raw")
    wiki_sources_dir = os.path.join(wiki_dir, "sources")
    
    results = {
        "project": project_dir,
        "timestamp": str(__import__('datetime').datetime.now()),
        "sections": {}
    }
    
    print(f"\n{'='*60}")
    print(f"Ingest 质量检查: {project_dir}")
    print(f"{'='*60}\n")
    
    # 1. Schema 合规性
    print(">>> 检查 Schema 合规性...")
    schema_stats = check_schema_compliance(wiki_dir)
    results["sections"]["schema"] = schema_stats
    print(f"   合规率: {schema_stats['compliance_rate']*100:.1f}%")
    print(f"   合规页面: {schema_stats['compliant_pages']}/{schema_stats['total_pages']}")
    if verbose and schema_stats['missing_fields']:
        print(f"   缺失字段的页面: {len(schema_stats['missing_fields'])} 个")
    
    # 2. 分类覆盖
    print("\n>>> 检查分类覆盖...")
    category_stats = check_category_coverage(wiki_dir)
    results["sections"]["categories"] = category_stats
    for cat, data in category_stats.items():
        if cat != "index_links":
            print(f"   {cat}: {data['count']} 个")
    print(f"   index.md 内链数: {category_stats.get('index_links', 0)}")
    
    # 3. Wikilink 密度
    print("\n>>> 检查 Wikilink 密度...")
    link_stats = check_wikilink_density(wiki_dir)
    results["sections"]["wikilinks"] = link_stats
    print(f"   总链接数: {link_stats['total_links']}")
    print(f"   平均每页: {link_stats['avg_links_per_page']:.1f} 个")
    print(f"   孤立页面: {link_stats['orphaned_pages']} 个")
    
    # 4. 原始材料覆盖（如果 raw 目录存在）
    if os.path.exists(raw_dir):
        print("\n>>> 检查原始材料覆盖...")
        coverage_stats = compare_source_to_wiki(raw_dir, wiki_sources_dir)
        results["sections"]["coverage"] = coverage_stats
        print(f"   原始材料: {coverage_stats['total_raw_sources']} 个")
        print(f"   Wiki 页面: {coverage_stats['total_wiki_pages']} 个")
        print(f"   覆盖率: {coverage_stats['coverage_rate']*100:.1f}%")
    
    # 5. 页面质量
    print("\n>>> 分析页面质量...")
    quality_stats = analyze_wiki_quality(wiki_dir)
    results["sections"]["quality"] = quality_stats
    print(f"   平均字数: {quality_stats['avg_word_count']:.0f}")
    print(f"   字数范围: {quality_stats['min_word_count']} - {quality_stats['max_word_count']}")
    print(f"   高质量 (>500字): {quality_stats['quality_distribution']['high']} 个")
    print(f"   中等 (200-500): {quality_stats['quality_distribution']['medium']} 个")
    print(f"   低质量 (<200): {quality_stats['quality_distribution']['low']} 个")
    
    # 汇总评分
    print(f"\n{'='*60}")
    print("质量评分汇总")
    print(f"{'='*60}")
    
    schema_score = schema_stats['compliance_rate'] * 40
    link_score = min(link_stats['avg_links_per_page'] / 3, 1.0) * 30  # 假设 3 个/页为满分
    total_quality_pages = sum(quality_stats['quality_distribution'].values())
    quality_score = quality_stats['quality_distribution']['high'] / max(total_quality_pages, 1) * 30
    
    total_score = schema_score + link_score + quality_score
    results["overall_score"] = round(total_score, 1)
    
    print(f"\n综合得分: {total_score:.1f}/100")
    print(f"  - Schema 合规: {schema_score:.1f}/40")
    print(f"  - Wikilink 密度: {link_score:.1f}/30")
    print(f"  - 页面质量: {quality_score:.1f}/30")
    print("\n[NOTE] 此分数仅反映结构完整性（frontmatter、链接密度、字数），不代表内容事实准确性。")

    if total_score >= 80:
        print("\n✅ Ingest 结构质量良好")
    elif total_score >= 60:
        print("\n⚠️ Ingest 结构质量一般，建议优化")
    else:
        print("\n❌ Ingest 结构质量较差，需要改进")
    
    return results

# ============ 主函数 ============

def main():
    parser = argparse.ArgumentParser(description='Ingest 质量检查工具')
    parser.add_argument('--project', '-p', required=True, help='项目路径')
    parser.add_argument('--verbose', '-v', action='store_true', help='详细输出')
    parser.add_argument('--output', '-o', help='结果输出 JSON 路径')
    
    args = parser.parse_args()
    
    results = run_ingest_check(args.project, args.verbose)
    
    if args.output:
        with open(args.output, 'w', encoding='utf-8') as f:
            json.dump(results, f, ensure_ascii=False, indent=2)
        print(f"\n结果已保存: {args.output}")
    
    return results

if __name__ == "__main__":
    main()
