from dataclasses import dataclass, field, asdict
from typing import List, Optional


@dataclass
class CoverageClaim:
    claim: str  # "生后15天开始补充维生素D"
    source_location: str  # "第3段" 或行号
    wiki_coverage: str  # "full" | "partial" | "missing"
    wiki_excerpt: str = ""  # wiki 中对应的原文片段


@dataclass
class Hallucination:
    claim: str  # wiki 页中的陈述
    wiki_location: str  # wiki 中的位置
    severity: str  # "major" | "minor"
    judge_reasoning: str  # 为什么判定为幻觉


@dataclass
class JudgeReportItem:
    source_file: str  # raw/sources/xxx.md
    wiki_page: str  # wiki/sources/xxx.md
    coverage_claims: List[CoverageClaim] = field(default_factory=list)
    hallucinations: List[Hallucination] = field(default_factory=list)
    scores: dict = field(default_factory=dict)
    # scores: {"coverage": 7, "consistency": 8}

    def to_dict(self) -> dict:
        return asdict(self)

    @classmethod
    def from_dict(cls, d: dict) -> "JudgeReportItem":
        """从 to_dict() 的输出重建对象"""
        claims = [CoverageClaim(**c) for c in d.get("coverage_claims", [])]
        halls = [Hallucination(**h) for h in d.get("hallucinations", [])]
        return cls(
            source_file=d.get("source_file", ""),
            wiki_page=d.get("wiki_page", ""),
            coverage_claims=claims,
            hallucinations=halls,
            scores=d.get("scores", {}),
        )
