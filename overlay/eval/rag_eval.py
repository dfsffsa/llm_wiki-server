#!/usr/bin/env python3
"""
RAG + Chat 在线评测脚本

用法:
    python rag_eval.py --project <项目路径> --test-cases <测试集JSON>
    python rag_eval.py --project <项目路径> --mode retrieval  # 仅检索评测
    python rag_eval.py --project <项目路径> --mode chat      # 仅生成评测
"""

import argparse
import json
import os
import sys
import glob
import re
from pathlib import Path
from typing import List, Dict, Any, Optional

# ============ 配置 ============
LLM_WIKI_SERVER = "http://127.0.0.1:8080"
DEFAULT_TOKEN = "e2e-test-token"

# ============ 工具函数 ============

def load_test_cases(path: str) -> Dict:
    """加载测试集"""
    with open(path, 'r', encoding='utf-8') as f:
        return json.load(f)

def normalize_text(text: str) -> str:
    """文本规范化（去空格、标点、转为小写）"""
    if not text:
        return ""
    text = re.sub(r'[\s\n\r]+', ' ', text)
    text = re.sub(r'[^\w一-鿿]+', '', text)
    return text.lower()

def check_keyword_match(text: str, keywords: List[str]) -> float:
    """检查关键词匹配度"""
    if not text or not keywords:
        return 0.0
    text_lower = normalize_text(text)
    matches = sum(1 for kw in keywords if normalize_text(kw) in text_lower)
    return matches / len(keywords)

def check_source_coverage(retrieved: List[str], expected: List[str], project_dir: str) -> float:
    """检查来源覆盖度（支持 glob 模式）"""
    if not expected:
        return 1.0
    
    coverage = 0.0
    for pattern in expected:
        # 转换 glob pattern 为 regex
        pattern = pattern.replace('*', '.*')
        pattern = pattern.replace('.', r'\.')
        
        # 检查是否有文件匹配
        matched = False
        for wiki_file in retrieved:
            if re.search(pattern, wiki_file):
                matched = True
                break
        
        # 也检查项目目录中是否存在该模式对应的文件
        full_pattern = os.path.join(project_dir, pattern)
        if glob.glob(full_pattern):
            coverage += 1.0 if matched else 0.5
        elif matched:
            coverage += 0.8
    
    return min(coverage / len(expected), 1.0)

# ============ API 调用 ============

def call_api(endpoint: str, token: str = DEFAULT_TOKEN) -> Optional[Dict]:
    """调用 llm-wiki-server API"""
    import urllib.request
    import urllib.error
    
    url = f"{LLM_WIKI_SERVER}{endpoint}"
    req = urllib.request.Request(url)
    req.add_header('Authorization', f'Bearer {token}')
    
    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            data = resp.read().decode('utf-8')
            if resp.headers.get('Content-Type', '').startswith('application/json'):
                return json.loads(data)
            return None
    except Exception as e:
        print(f"  [WARN] API 调用失败: {endpoint} - {e}", file=sys.stderr)
        return None

def search_wiki(query: str, project_id: str, token: str) -> Dict:
    """搜索 wiki"""
    import urllib.request
    import urllib.parse
    
    url = f"{LLM_WIKI_SERVER}/api/v1/projects/{project_id}/search"
    data = json.dumps({"query": query, "topK": 10}).encode('utf-8')
    
    req = urllib.request.Request(url, data=data, method='POST')
    req.add_header('Authorization', f'Bearer {token}')
    req.add_header('Content-Type', 'application/json')
    
    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            return json.loads(resp.read().decode('utf-8'))
    except Exception as e:
        print(f"  [WARN] 搜索失败: {query[:30]}... - {e}", file=sys.stderr)
        return {"results": []}

def chat_ask(question: str, project_id: str, token: str) -> Dict:
    """发送 Chat 请求（SSE）"""
    import urllib.request
    import urllib.parse
    
    url = f"{LLM_WIKI_SERVER}/api/v1/projects/{project_id}/chat"
    data = json.dumps({
        "messages": [
            {"role": "user", "content": question}
        ]
    }).encode('utf-8')
    
    req = urllib.request.Request(url, data=data, method='POST')
    req.add_header('Authorization', f'Bearer {token}')
    req.add_header('Content-Type', 'application/json')
    req.add_header('Accept', 'text/event-stream')
    
    try:
        with urllib.request.urlopen(req, timeout=120) as resp:
            # 解析 SSE
            chunks = []
            for line in resp:
                line = line.decode('utf-8').strip()
                if line.startswith('data:'):
                    try:
                        event = json.loads(line[5:].strip())
                        if event.get('event') == 'token':
                            chunks.append(event.get('data', {}).get('content', ''))
                        elif event.get('event') == 'done':
                            break
                        elif event.get('event') == 'error':
                            print(f"  [WARN] Chat 错误: {event.get('data')}", file=sys.stderr)
                            break
                    except json.JSONDecodeError:
                        pass
            return {"answer": ''.join(chunks), "chunks": chunks}
    except Exception as e:
        print(f"  [WARN] Chat 失败: {question[:30]}... - {e}", file=sys.stderr)
        return {"answer": "", "chunks": []}

# ============ 评测逻辑 ============

def eval_retrieval(test_case: Dict, project_dir: str, project_id: str, token: str) -> Dict:
    """评测检索效果"""
    query = test_case['question']
    expected_sources = test_case.get('expected_sources', [])
    keywords = test_case.get('keywords', [])
    
    # 调用搜索 API
    result = search_wiki(query, project_id, token)
    retrieved_files = [r.get('file', r.get('path', '')) for r in result.get('results', [])]
    
    # 计算指标
    source_coverage = check_source_coverage(retrieved_files, expected_sources, project_dir)
    keyword_match = check_keyword_match(' '.join(retrieved_files), keywords)
    
    # Top-K 召回
    top_k = len(expected_sources)
    recall_at_k = 0.0
    if retrieved_files and expected_sources:
        for i, expected in enumerate(expected_sources[:top_k]):
            expected_pattern = expected.replace('*', '.*')
            for retrieved in retrieved_files[:top_k]:
                if re.search(expected_pattern, retrieved):
                    recall_at_k += 1.0 / (i + 1)
                    break
    
    return {
        "case_id": test_case['id'],
        "question": query,
        "retrieved_files": retrieved_files[:5],
        "source_coverage": round(source_coverage, 3),
        "keyword_match": round(keyword_match, 3),
        "recall_at_k": round(recall_at_k, 3),
        "retrieval_success": source_coverage >= 0.5
    }

def eval_chat(test_case: Dict, project_dir: str, project_id: str, token: str) -> Dict:
    """评测 Chat 生成效果"""
    query = test_case['question']
    expected_answers = test_case.get('expected_answers', [])
    keywords = test_case.get('keywords', [])
    
    # 调用 Chat API
    result = chat_ask(query, project_id, token)
    answer = result.get('answer', '')
    
    # 计算指标
    answer_match = check_keyword_match(answer, expected_answers)
    keyword_coverage = check_keyword_match(answer, keywords)
    
    return {
        "case_id": test_case['id'],
        "question": query,
        "answer": answer[:500] + ('...' if len(answer) > 500 else ''),
        "answer_match": round(answer_match, 3),
        "keyword_coverage": round(keyword_coverage, 3),
        "has_answer": len(answer) > 20,
        "chat_success": answer_match >= 0.5
    }

def run_evaluation(project: str, test_cases_path: str, mode: str = "all", output_dir: str = None):
    """运行评测"""
    # 加载测试集
    data = load_test_cases(test_cases_path)
    cases = data.get('cases', [])
    
    # 获取项目 ID
    projects_resp = call_api("/api/v1/projects")
    if not projects_resp:
        print("[ERROR] 无法获取项目列表，请确认 server 运行中", file=sys.stderr)
        return
    
    project_id = None
    for p in projects_resp.get('projects', []):
        if p.get('name') == project or p.get('id') == project:
            project_id = p.get('id')
            break
    
    if not project_id:
        project_id = projects_resp.get('projects', [{}])[0].get('id')
        print(f"[WARN] 未找到项目 {project}，使用: {project_id}", file=sys.stderr)
    
    # 确定项目目录
    project_dir = None
    for p in projects_resp.get('projects', []):
        if p.get('id') == project_id:
            project_dir = p.get('path')
            break
    
    # 准备输出
    if output_dir:
        os.makedirs(output_dir, exist_ok=True)
    
    results = {
        "project": project,
        "mode": mode,
        "total_cases": len(cases),
        "timestamp": str(__import__('datetime').datetime.now()),
        "retrieval_results": [],
        "chat_results": [],
        "summary": {}
    }
    
    print(f"\n{'='*60}")
    print(f"开始评测: {project} ({len(cases)} 个测试用例)")
    print(f"{'='*60}\n")
    
    # 检索评测
    if mode in ["retrieval", "all"]:
        print(">>> 检索评测...")
        for i, case in enumerate(cases):
            print(f"  [{i+1}/{len(cases)}] {case['id']}: {case['question'][:40]}...")
            result = eval_retrieval(case, project_dir or "", project_id, DEFAULT_TOKEN)
            results["retrieval_results"].append(result)
            status = "✓" if result["retrieval_success"] else "✗"
            print(f"       {status} recall@K={result['recall_at_k']:.3f}, coverage={result['source_coverage']:.3f}")
    
    # Chat 评测
    if mode in ["chat", "all"]:
        print("\n>>> Chat 生成评测...")
        for i, case in enumerate(cases):
            print(f"  [{i+1}/{len(cases)}] {case['id']}: {case['question'][:40]}...")
            result = eval_chat(case, project_dir or "", project_id, DEFAULT_TOKEN)
            results["chat_results"].append(result)
            status = "✓" if result["chat_success"] else "✗"
            print(f"       {status} answer_match={result['answer_match']:.3f}, has_answer={result['has_answer']}")
    
    # 汇总
    if results["retrieval_results"]:
        retrieval_success = sum(1 for r in results["retrieval_results"] if r["retrieval_success"])
        avg_recall = sum(r["recall_at_k"] for r in results["retrieval_results"]) / len(results["retrieval_results"])
        avg_coverage = sum(r["source_coverage"] for r in results["retrieval_results"]) / len(results["retrieval_results"])
        results["summary"]["retrieval"] = {
            "success_rate": f"{retrieval_success}/{len(cases)} ({retrieval_success/len(cases)*100:.1f}%)",
            "avg_recall_at_k": round(avg_recall, 3),
            "avg_source_coverage": round(avg_coverage, 3)
        }
    
    if results["chat_results"]:
        chat_success = sum(1 for r in results["chat_results"] if r["chat_success"])
        avg_match = sum(r["answer_match"] for r in results["chat_results"]) / len(results["chat_results"])
        results["summary"]["chat"] = {
            "success_rate": f"{chat_success}/{len(cases)} ({chat_success/len(cases)*100:.1f}%)",
            "avg_answer_match": round(avg_match, 3)
        }
    
    # 输出结果
    print(f"\n{'='*60}")
    print("评测结果汇总")
    print(f"{'='*60}")
    
    if "retrieval" in results["summary"]:
        r = results["summary"]["retrieval"]
        print(f"\n📊 检索效果:")
        print(f"   召回成功率: {r['success_rate']}")
        print(f"   平均 Recall@K: {r['avg_recall_at_k']:.3f}")
        print(f"   平均来源覆盖: {r['avg_source_coverage']:.3f}")
    
    if "chat" in results["summary"]:
        c = results["summary"]["chat"]
        print(f"\n📊 生成效果:")
        print(f"   回答准确率: {c['success_rate']}")
        print(f"   平均答案匹配: {c['avg_answer_match']:.3f}")
    
    # 保存结果
    if output_dir:
        output_path = os.path.join(output_dir, f"{project}_eval_results.json")
        with open(output_path, 'w', encoding='utf-8') as f:
            json.dump(results, f, ensure_ascii=False, indent=2)
        print(f"\n结果已保存: {output_path}")
    
    return results

# ============ 主函数 ============

def main():
    parser = argparse.ArgumentParser(description='RAG 评测工具')
    parser.add_argument('--project', '-p', required=True, help='项目路径')
    parser.add_argument('--test-cases', '-t', help='测试集 JSON 路径')
    parser.add_argument('--mode', '-m', choices=['retrieval', 'chat', 'all'], default='all', help='评测模式')
    parser.add_argument('--output', '-o', help='结果输出目录')
    parser.add_argument('--token', help='API Token')
    
    args = parser.parse_args()
    
    # 设置全局配置
    global DEFAULT_TOKEN
    if args.token:
        DEFAULT_TOKEN = args.token
    
    # 默认测试集路径
    if not args.test_cases:
        eval_dir = Path(__file__).parent
        test_cases_path = eval_dir / "test_cases" / "parenting_books.json"
        if not test_cases_path.exists():
            test_cases_path = eval_dir / "test_cases" / "template.json"
        args.test_cases = str(test_cases_path)
    
    # 运行评测
    run_evaluation(
        project=args.project,
        test_cases_path=args.test_cases,
        mode=args.mode,
        output_dir=args.output
    )

if __name__ == "__main__":
    main()
