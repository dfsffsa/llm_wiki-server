#!/bin/bash
# 评测执行脚本
# 用法: ./run_eval.sh <project_name> [--mode retrieval|chat|all]

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
EVAL_DIR="$(dirname "$SCRIPT_DIR")"
RESULTS_DIR="$EVAL_DIR/results"

PROJECT_NAME="${1:-ParentingBooks}"
MODE="${2:-all}"

# 项目路径
case "$PROJECT_NAME" in
    "ParentingBooks")
        PROJECT_PATH="$HOME/overseas-github/llm_wiki_projects/ParentingBooks"
        ;;
    "CivilCareer")
        PROJECT_PATH="$HOME/overseas-github/llm_wiki_projects/CivilCareer"
        ;;
    *)
        PROJECT_PATH="$PROJECT_NAME"
        ;;
esac

# 测试集路径
TEST_CASES="$EVAL_DIR/test_cases/parenting_books.json"

mkdir -p "$RESULTS_DIR"

echo "============================================================"
echo "RAG 评测系统"
echo "============================================================"
echo "项目: $PROJECT_NAME"
echo "模式: $MODE"
echo "项目路径: $PROJECT_PATH"
echo "============================================================"

# 检查 server 是否运行
echo ""
echo "[1/3] 检查 llm-wiki-server 状态..."
if curl -s --max-time 5 "http://127.0.0.1:8080/api/v1/health" > /dev/null 2>&1; then
    echo "✅ Server 运行正常"
else
    echo "❌ Server 未运行，请先启动:"
    echo "   ./overlay/server/target/release/llm-wiki-server"
    exit 1
fi

# Ingest 质量检查
echo ""
echo "[2/3] Ingest 质量检查..."
python3 "$EVAL_DIR/ingest_check.py" \
    --project "$PROJECT_PATH" \
    --output "$RESULTS_DIR/${PROJECT_NAME}_ingest_check.json"

# RAG + Chat 评测
echo ""
echo "[3/3] RAG + Chat 在线评测..."
if [ "$MODE" = "retrieval" ] || [ "$MODE" = "all" ]; then
    echo "--- 检索评测 ---"
    python3 "$EVAL_DIR/rag_eval.py" \
        --project "$PROJECT_NAME" \
        --test-cases "$TEST_CASES" \
        --mode retrieval \
        --output "$RESULTS_DIR"
fi

if [ "$MODE" = "chat" ] || [ "$MODE" = "all" ]; then
    echo "--- Chat 生成评测 ---"
    python3 "$EVAL_DIR/rag_eval.py" \
        --project "$PROJECT_NAME" \
        --test-cases "$TEST_CASES" \
        --mode chat \
        --output "$RESULTS_DIR"
fi

echo ""
echo "============================================================"
echo "评测完成！结果保存在: $RESULTS_DIR/"
echo "============================================================"
