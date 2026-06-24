#!/usr/bin/env python3
"""
LLM 辅助测试用例生成器

功能：
1. 读取原始材料 / wiki 页面
2. 使用 LLM 生成问题-答案对
3. 自动匹配 expected_sources
4. 质量验证与过滤

用法:
    python generate_test_cases.py --project <项目路径> --mode auto
    python generate_test_cases.py --project <项目路径> --mode hybrid  # 人工审核模式
"""

import argparse
import json
import os
import re
import glob
from pathlib import Path
from typing import List, Dict, Any, Optional
from datetime import datetime

try:
    import yaml
except Exception:  # pragma: no cover
    yaml = None  # type: ignore

# ============ 配置 ============

DEFAULT_CONFIG = """
你是一个育儿知识库的测试用例专家。
请根据提供的材料，生成高质量的问答测试用例。

要求：
1. 问题应该来自真实用户需求，覆盖不同类型（事实型、数值型、场景型、概念型）
2. 难度分布：简单40%、中等40%、困难20%
3. expected_answers 必须能在材料中找到原文支持
4. 每个问题必须有明确的检索关键词
"""

CATEGORY_PROMPTS = {
    "fact": "关于事实、定义的问答，如「什么是XX」「XX的原因是什么」",
    "number": "关于数值、剂量的问答，如「多少剂量」「多大月龄」",
    "scenario": "场景应对类，如「宝宝XX怎么办」「如何处理XX」",
    "concept": "概念理解类，如「XX的原理」「XX与YY的区别」"
}

# ============ 工具函数 ============

def read_file(path: str) -> Optional[str]:
    """读取文件内容"""
    try:
        with open(path, 'r', encoding='utf-8') as f:
            return f.read()
    except:
        return None

def parse_frontmatter(content: str) -> tuple:
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
            print(f"[WARN] PyYAML 解析 frontmatter 失败，回退到简单解析: {e}")

    fm = {}
    for line in fm_text.split('\n'):
        if ':' in line:
            key, value = line.split(':', 1)
            fm[key.strip()] = value.strip().strip('"\'')

    return fm, body

def extract_text_snippets(content: str, max_chars: int = 3000) -> List[str]:
    """提取文本片段（用于 LLM 输入）"""
    snippets = []
    
    # 提取标题段落
    paragraphs = content.split('\n\n')
    for p in paragraphs:
        p = p.strip()
        if len(p) > 50 and len(p) < 500:
            snippets.append(p)
    
    # 限制总长度
    total = '\n'.join(snippets[:20])  # 最多 20 段
    if len(total) > max_chars:
        total = total[:max_chars] + "..."
    
    return total

def normalize_text(text: str) -> str:
    """文本规范化"""
    text = re.sub(r'[\s\n\r]+', ' ', text)
    return text.strip()

# ============ LLM 调用 ============

def _build_anthropic_url(endpoint: str) -> str:
    trimmed = endpoint.rstrip('/')
    if re.search(r'/v\d+/messages$', trimmed):
        return trimmed
    if re.search(r'/v\d+$', trimmed):
        return trimmed + '/messages'
    return trimmed + '/v1/messages'


def _requires_bearer_auth(url: str) -> bool:
    u = url.lower().rstrip('/')
    return (
        u.startswith('https://api.minimax.io/anthropic')
        or u.startswith('https://api.minimaxi.com/anthropic')
        or u.startswith('https://coding.dashscope.aliyuncs.com/apps/anthropic')
        or u.startswith('https://api.kimi.com/coding')
        or u.startswith('https://api.moonshot.ai/anthropic')
        or u.startswith('https://api.moonshot.cn/anthropic')
    )


def call_llm(prompt: str, config: Dict) -> Optional[str]:
    """调用 LLM 生成内容，支持 anthropic_messages 与 chat_completions 两种 wire format"""
    import urllib.request
    import urllib.error

    api_key = config.get('apiKey') or os.environ.get('LLM_API_KEY', '')
    model = config.get('model', 'gpt-4o')
    endpoint = config.get('customEndpoint', 'https://api.openai.com/v1/chat/completions')
    api_mode = config.get('apiMode', 'chat_completions')
    temperature = config.get('temperature', 0.3)
    max_tokens = int(config.get('maxTokens', 4000))

    if not api_key:
        print("[WARN] 未设置 apiKey / LLM_API_KEY，跳过 LLM 调用")
        return None

    if api_mode == 'anthropic_messages':
        url = _build_anthropic_url(endpoint)
        headers = {
            'Content-Type': 'application/json',
            'anthropic-version': '2023-06-01',
        }
        if _requires_bearer_auth(url):
            headers['Authorization'] = f'Bearer {api_key}'
        else:
            headers['x-api-key'] = api_key
            headers['anthropic-dangerous-direct-browser-access'] = 'true'
        payload = {
            'model': model,
            'max_tokens': max_tokens,
            'temperature': temperature,
            'system': DEFAULT_CONFIG,
            'messages': [{'role': 'user', 'content': prompt}],
        }
        extract = lambda result: ''.join(
            b.get('text', '') for b in result.get('content', [])
            if b.get('type') == 'text'
        )
    else:
        url = endpoint.rstrip('/')
        if not re.search(r'/chat/completions$', url):
            url = url + '/chat/completions'
        headers = {
            'Content-Type': 'application/json',
            'Authorization': f'Bearer {api_key}',
        }
        payload = {
            'model': model,
            'temperature': temperature,
            'max_tokens': max_tokens,
            'messages': [
                {'role': 'system', 'content': DEFAULT_CONFIG},
                {'role': 'user', 'content': prompt},
            ],
        }
        extract = lambda result: result['choices'][0]['message']['content']

    data = json.dumps(payload, ensure_ascii=False).encode('utf-8')
    req = urllib.request.Request(url, data=data, method='POST')
    for k, v in headers.items():
        req.add_header(k, v)

    try:
        with urllib.request.urlopen(req, timeout=120) as resp:
            result = json.loads(resp.read().decode('utf-8'))
            return extract(result)
    except urllib.error.HTTPError as e:
        body = e.read().decode('utf-8', errors='replace')[:500]
        print(f"[WARN] LLM 调用失败 ({e.code}): {body}")
        return None
    except Exception as e:
        print(f"[WARN] LLM 调用失败: {e}")
        return None

# ============ 测试用例生成 ============

def generate_from_source(source_path: str, config: Dict, wiki_dir: str, project_dir: str = '') -> List[Dict]:
    """从单个源文件生成测试用例"""
    content = read_file(source_path)
    if not content:
        return []
    
    fm, body = parse_frontmatter(content)
    title = fm.get('title', os.path.basename(source_path))
    source_type = fm.get('type', 'unknown')
    
    # 准备 prompt
    snippets = extract_text_snippets(body)
    prompt = f"""
材料标题: {title}
材料类型: {source_type}

材料内容摘要:
{snippets}

请生成 3-5 个测试用例，格式如下（JSON 数组）:

[
  {{
    "id": "auto_001",
    "question": "用户问题",
    "category": "fact|number|scenario|concept",
    "difficulty": "easy|medium|hard",
    "expected_answers": ["期望回答中的关键表述1", "关键表述2"],
    "keywords": ["检索关键词1", "关键词2"],
    "note": "测试目的说明"
  }}
]

要求：
1. 问题要多样化，覆盖不同难度
2. expected_answers 必须能在材料原文中找到支持
3. keywords 用于检索匹配
4. category 选择最合适的类型
"""
    
    # 调用 LLM
    response = call_llm(prompt, config)
    if not response:
        return []
    
    # 解析 JSON
    try:
        # 提取 JSON 数组
        json_match = re.search(r'\[.*\]', response, re.DOTALL)
        if json_match:
            cases = json.loads(json_match.group(0))
            # 添加 source 信息（expected_sources 相对 project_dir，与 parenting_books.json 一致）
            for case in cases:
                base = project_dir or os.path.dirname(wiki_dir)
                rel_path = os.path.relpath(source_path, base).replace('\\', '/')
                case['expected_sources'] = [rel_path]
                case['source_file'] = os.path.basename(source_path)
            return cases
    except json.JSONDecodeError:
        print(f"[WARN] JSON 解析失败: {source_path}")
    
    return []

def generate_batch_test_cases(project_dir: str, config: Dict, max_sources: int = 0) -> List[Dict]:
    """批量生成测试用例。max_sources=0 表示处理全部。"""
    wiki_dir = os.path.join(project_dir, "wiki")

    # 收集 wiki 页面
    md_files = glob.glob(f"{wiki_dir}/**/*.md", recursive=True)

    all_cases = []
    case_id = 1

    limit = len(md_files) if max_sources <= 0 else min(max_sources, len(md_files))
    print(f"准备生成测试用例（共 {len(md_files)} 个文件，将处理 {limit} 个）...")

    for i, md_file in enumerate(md_files[:limit]):
        rel_path = os.path.relpath(md_file, wiki_dir)
        print(f"  [{i+1}/{limit}] {rel_path}")

        cases = generate_from_source(md_file, config, wiki_dir, project_dir)

        # 把源文件正文挂到 case 上，供 validate_test_case 使用
        source_body = ''
        raw = read_file(md_file)
        if raw:
            _, source_body = parse_frontmatter(raw)

        for case in cases:
            case['id'] = f"auto_{case_id:03d}"
            case['_source_body'] = source_body
            all_cases.append(case)
            case_id += 1

        if len(all_cases) >= 500:  # 总数硬上限，避免失控
            break

    return all_cases

def validate_test_case(case: Dict, wiki_dir: str) -> Dict:
    """验证测试用例质量"""
    validation = {
        "valid": True,
        "warnings": [],
        "suggestions": []
    }

    # 检查问题是否为空
    if not case.get('question'):
        validation["valid"] = False
        validation["warnings"].append("问题为空")

    # 检查 expected_answers
    if not case.get('expected_answers'):
        validation["valid"] = False
        validation["warnings"].append("期望答案为空")

    # 检查 expected_answers 是否真在源材料里出现
    source_body = case.get('_source_body', '') or ''
    if source_body and case.get('expected_answers'):
        missing = []
        for ans in case['expected_answers']:
            if ans.strip() and ans not in source_body:
                missing.append(ans)
        if missing:
            validation["warnings"].append(
                f"expected_answers 未在源材料中找到: {missing}"
            )

    # 检查 expected_sources 指向的文件是否存在
    if case.get('expected_sources'):
        for pattern in case['expected_sources']:
            pat = pattern.replace('*', '[^/]+')
            matches = glob.glob(f"{wiki_dir}/{pat}")
            if not matches:
                validation["suggestions"].append(f"expected_sources 可能需要更新: {pattern}")

    # 检查 category 和 difficulty
    if case.get('category') not in ['fact', 'number', 'scenario', 'concept']:
        validation["suggestions"].append(f"未知的 category: {case.get('category')}")

    return validation

def filter_and_rank_cases(cases: List[Dict], wiki_dir: str) -> List[Dict]:
    """过滤并排序测试用例"""
    validated_cases = []

    for case in cases:
        validation = validate_test_case(case, wiki_dir)
        if validation["valid"]:
            case['_validation'] = validation
            validated_cases.append(case)
    
    # 按 category 分布排序
    category_order = {'scenario': 0, 'fact': 1, 'number': 2, 'concept': 3}
    validated_cases.sort(key=lambda c: (
        category_order.get(c.get('category', ''), 99),
        c.get('difficulty', 'medium')
    ))
    
    return validated_cases

# ============ 质量保证 ============

def calculate_coverage(cases: List[Dict], wiki_dir: str) -> Dict:
    """计算测试用例覆盖率"""
    md_files = glob.glob(f"{wiki_dir}/**/*.md", recursive=True)
    
    # 统计被覆盖的文件
    covered_files = set()
    for case in cases:
        for pattern in case.get('expected_sources', []):
            pattern = pattern.replace('*', '.*')
            for md_file in md_files:
                if re.search(pattern, md_file):
                    covered_files.add(md_file)
    
    coverage = {
        "total_files": len(md_files),
        "covered_files": len(covered_files),
        "coverage_rate": len(covered_files) / max(len(md_files), 1),
        "category_distribution": {},
        "difficulty_distribution": {}
    }
    
    # 分类统计
    for case in cases:
        cat = case.get('category', 'unknown')
        diff = case.get('difficulty', 'unknown')
        coverage["category_distribution"][cat] = coverage["category_distribution"].get(cat, 0) + 1
        coverage["difficulty_distribution"][diff] = coverage["difficulty_distribution"].get(diff, 0) + 1
    
    return coverage

def generate_human_review_list(cases: List[Dict], sample_size: int = 10) -> List[Dict]:
    """生成人工审核列表（抽样）"""
    import random
    
    # 按 category 分层抽样
    samples = []
    by_category = {}
    
    for case in cases:
        cat = case.get('category', 'unknown')
        by_category.setdefault(cat, []).append(case)
    
    # 每个 category 抽 sample_size / 4 个
    per_category = max(sample_size // 4, 2)
    
    for cat, cat_cases in by_category.items():
        sampled = random.sample(cat_cases, min(per_category, len(cat_cases)))
        samples.extend(sampled)
    
    # 生成审核模板
    review_template = []
    for case in samples:
        review_template.append({
            "case_id": case['id'],
            "question": case['question'],
            "expected_answers": case.get('expected_answers', []),
            "expected_sources": case.get('expected_sources', []),
            "check_items": [
                "□ 问题是否清晰可回答？",
                "□ 期望答案是否能在材料中找到？",
                "□ 关键词是否合适？",
                "□ 难度标注是否准确？"
            ],
            "review_notes": "",
            "approved": None  # null=待审核, true=通过, false=不通过
        })
    
    return review_template

# ============ 主流程 ============

def generate_test_suite(project_dir: str, config: Dict, output_path: str,
                        mode: str = "auto", sample_for_review: int = 10,
                        max_sources: int = 0):
    """生成完整测试套件。max_sources=0 表示处理全部源文件。"""
    wiki_dir = os.path.join(project_dir, "wiki")

    print(f"\n{'='*60}")
    print(f"测试用例生成器")
    print(f"{'='*60}")
    print(f"项目: {project_dir}")
    print(f"模式: {mode}")
    print(f"{'='*60}\n")

    # Step 1: 生成测试用例
    print(">>> Step 1: LLM 生成测试用例...")
    cases = generate_batch_test_cases(project_dir, config, max_sources)
    print(f"    生成 {len(cases)} 个候选测试用例")
    
    # Step 2: 验证与过滤
    print("\n>>> Step 2: 质量验证与过滤...")
    validated_cases = filter_and_rank_cases(cases, wiki_dir)
    print(f"    通过验证 {len(validated_cases)} 个")
    
    # Step 3: 计算覆盖率
    print("\n>>> Step 3: 覆盖率分析...")
    coverage = calculate_coverage(validated_cases, wiki_dir)
    print(f"    覆盖 {coverage['covered_files']}/{coverage['total_files']} 个 wiki 页面")
    print(f"    覆盖率: {coverage['coverage_rate']*100:.1f}%")
    print(f"    分类分布: {coverage['category_distribution']}")
    print(f"    难度分布: {coverage['difficulty_distribution']}")
    
    # Step 4: 生成输出
    print("\n>>> Step 4: 生成输出文件...")

    # 清理临时字段
    for case in validated_cases:
        case.pop('_source_body', None)
        case.pop('_validation', None)

    output = {
        "project": project_dir,
        "version": "1.0.0-auto",
        "generated_at": datetime.now().isoformat(),
        "mode": mode,
        "coverage": coverage,
        "cases": validated_cases
    }
    
    # 保存测试用例
    with open(output_path, 'w', encoding='utf-8') as f:
        json.dump(output, f, ensure_ascii=False, indent=2)
    print(f"    测试用例已保存: {output_path}")
    
    # Step 5: 生成人工审核列表（hybrid 模式）
    if mode == "hybrid" and sample_for_review > 0:
        review_path = output_path.replace('.json', '_review.json')
        review_list = generate_human_review_list(validated_cases, sample_for_review)
        
        review_output = {
            "project": project_dir,
            "total_cases": len(validated_cases),
            "review_required": len(review_list),
            "review_list": review_list
        }
        
        with open(review_path, 'w', encoding='utf-8') as f:
            json.dump(review_output, f, ensure_ascii=False, indent=2)
        print(f"    人工审核列表已保存: {review_path}")
        print(f"    请审核 {len(review_list)} 个代表性用例后决定是否采用")
    
    print(f"\n{'='*60}")
    print("生成完成！")
    print(f"{'='*60}")
    print(f"测试用例: {output_path}")
    if mode == "hybrid":
        print(f"审核列表: {review_path}")
    
    return output

# ============ 主函数 ============

def main():
    parser = argparse.ArgumentParser(description='LLM 辅助测试用例生成器')
    parser.add_argument('--project', '-p', required=True, help='项目路径')
    parser.add_argument('--config', '-c', help='LLM 配置文件路径')
    parser.add_argument('--output', '-o', help='输出文件路径')
    parser.add_argument('--mode', '-m', choices=['auto', 'hybrid'], default='hybrid', 
                        help='auto=全自动, hybrid=人工审核')
    parser.add_argument('--review-size', '-r', type=int, default=10, 
                        help='人工审核抽样数量')
    parser.add_argument('--max-sources', '-n', type=int, default=0,
                        help='最多处理源文件数（0=全部）')
    parser.add_argument('--temperature', type=float, default=0.3,
                        help='LLM 采样温度（事实型 QA 建议 0.2-0.4）')
    parser.add_argument('--max-tokens', type=int, default=4000,
                        help='LLM 单次响应最大 token 数')

    args = parser.parse_args()

    # 加载配置
    config = {
        'apiKey': os.environ.get('LLM_API_KEY', ''),
        'model': os.environ.get('LLM_MODEL', 'gpt-4o'),
        'customEndpoint': os.environ.get('LLM_ENDPOINT', 'https://api.openai.com/v1/chat/completions'),
        'apiMode': os.environ.get('LLM_API_MODE', 'chat_completions'),
        'temperature': args.temperature,
        'maxTokens': args.max_tokens,
    }

    if args.config and os.path.exists(args.config):
        with open(args.config, 'r') as f:
            cfg = json.load(f)
            llm_cfg = cfg.get('llmConfig', {})
            config['apiKey'] = llm_cfg.get('apiKey', config['apiKey'])
            config['model'] = llm_cfg.get('model', config['model'])
            config['customEndpoint'] = llm_cfg.get('customEndpoint', config['customEndpoint'])
            config['apiMode'] = llm_cfg.get('apiMode', config['apiMode'])
    
    # 输出路径
    if not args.output:
        eval_dir = Path(__file__).parent
        project_name = os.path.basename(args.project)
        args.output = str(eval_dir / "test_cases" / f"{project_name}_auto_generated.json")
    
    # 生成
    generate_test_suite(
        project_dir=args.project,
        config=config,
        output_path=args.output,
        mode=args.mode,
        sample_for_review=args.review_size,
        max_sources=args.max_sources
    )

if __name__ == "__main__":
    main()
