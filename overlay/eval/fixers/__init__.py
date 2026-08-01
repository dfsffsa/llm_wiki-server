# overlay/eval/fixers/__init__.py
"""Fixer 注册表 — 使修复循环可按 fix_strategy 路由到对应的修复函数。"""

import sys
from typing import Dict, List, Callable

# 确保 eval 目录在路径中以便导入 ingest_check
_eval_dir = __file__.rsplit("/", 2)[0]
if _eval_dir not in sys.path:
    sys.path.insert(0, _eval_dir)

from ingest_check import Finding

FixerFn = Callable[[str, Finding], Dict]  # (project_dir, finding) -> result

FIXER_REGISTRY: Dict[str, FixerFn] = {}


def register(strategy: str):
    """装饰器：注册一个修复函数到指定策略名"""
    def decorator(fn: FixerFn):
        FIXER_REGISTRY[strategy] = fn
        return fn
    return decorator


def get_fixer(strategy: str) -> FixerFn:
    """根据策略名获取修复函数，不存在时返回 None"""
    return FIXER_REGISTRY.get(strategy)


# 导入 fixer 模块触发 @register 装饰器
from . import frontmatter  # noqa: F401, E402
from . import wikilink  # noqa: F401, E402
