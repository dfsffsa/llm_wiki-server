"""Tests for judge models and LLM client."""

import json
import os
import sys
import tempfile
import unittest
from unittest.mock import patch, MagicMock

EVAL_DIR = os.path.join(os.path.dirname(__file__), "..")
sys.path.insert(0, EVAL_DIR)


class TestModels(unittest.TestCase):
    def test_report_to_dict(self):
        from judge.models import JudgeReportItem, CoverageClaim

        r = JudgeReportItem(
            source_file="a.md",
            wiki_page="wiki/a.md",
            coverage_claims=[
                CoverageClaim(
                    claim="C1", source_location="L1", wiki_coverage="full"
                )
            ],
            scores={"coverage": 8},
        )
        d = r.to_dict()
        self.assertEqual(d["scores"]["coverage"], 8)
        self.assertEqual(d["coverage_claims"][0]["wiki_coverage"], "full")


class TestLlmClient(unittest.TestCase):
    @patch("judge.llm_client.requests.post")
    def test_call_llm(self, mock_post):
        from judge.llm_client import call_llm

        mock_resp = MagicMock()
        mock_resp.json.return_value = {
            "choices": [{"message": {"content": "ok"}}]
        }
        mock_post.return_value = mock_resp
        mock_post.return_value.raise_for_status = lambda: None

        result = call_llm("hello", {"model": "test", "endpoint": "http://x"})
        self.assertEqual(result, "ok")

    def test_env_var_expansion(self):
        from judge.llm_client import load_llm_config

        os.environ["TEST_KEY"] = "sk-test"
        with tempfile.NamedTemporaryFile(mode="w", suffix=".json", delete=False) as f:
            json.dump({"llmConfig": {"apiKey": "${TEST_KEY}"}}, f)
        cfg = load_llm_config(f.name)
        self.assertEqual(cfg["apiKey"], "sk-test")
        os.unlink(f.name)

    def test_parse_json_response_direct(self):
        from judge.llm_client import parse_json_response

        result = parse_json_response('{"key": "value"}')
        self.assertEqual(result, {"key": "value"})

    def test_parse_json_response_code_block(self):
        from judge.llm_client import parse_json_response

        text = "Here is the result:\n```json\n{\"key\": \"value\"}\n```\n"
        result = parse_json_response(text)
        self.assertEqual(result, {"key": "value"})

    def test_parse_json_response_code_block_no_lang(self):
        from judge.llm_client import parse_json_response

        text = "```\n{\"key\": \"value\"}\n```"
        result = parse_json_response(text)
        self.assertEqual(result, {"key": "value"})

    def test_parse_json_response_raises_on_invalid(self):
        from judge.llm_client import parse_json_response

        with self.assertRaises(ValueError):
            parse_json_response("not json at all")


class TestExtractor(unittest.TestCase):
    @patch("judge.extractor.call_llm")
    def test_extract_claims(self, mock_call):
        mock_call.return_value = json.dumps([
            {"claim": "维D从出生后15天开始补", "location": "第3段"},
            {"claim": "每天400国际单位", "location": "第4段"}
        ])
        from judge.extractor import extract_claims, parse_extracted_claims
        resp = extract_claims("source text", "test.md", {})
        claims = parse_extracted_claims(resp)
        self.assertEqual(len(claims), 2)
        self.assertEqual(claims[0]["claim"], "维D从出生后15天开始补")

    def test_parse_fallback(self):
        from judge.extractor import parse_extracted_claims
        resp = "```json\n[{\"claim\": \"test\", \"location\": \"L1\"}]\n```"
        claims = parse_extracted_claims(resp)
        self.assertEqual(len(claims), 1)


class TestEvaluator(unittest.TestCase):
    @patch("judge.evaluator.call_llm")
    def test_evaluate_full_coverage(self, mock_call):
        mock_call.return_value = json.dumps({
            "coverage_claims": [
                {"claim": "C1", "source_location": "L1",
                 "wiki_coverage": "full", "wiki_excerpt": "..."}
            ],
            "hallucinations": [],
            "scores": {"coverage": 10, "consistency": 10}})
        from judge.evaluator import evaluate_wiki
        report = evaluate_wiki("[C1]", "wiki content", {})
        self.assertEqual(len(report.coverage_claims), 1)
        self.assertEqual(report.coverage_claims[0].wiki_coverage, "full")

    def test_parse_code_block(self):
        from judge.evaluator import parse_eval_response
        resp = '```json\n{"coverage_claims": [], "hallucinations": [], "scores": {}}\n```'
        report = parse_eval_response(resp, "s.md", "w.md")
        self.assertEqual(report.source_file, "s.md")


if __name__ == "__main__":
    unittest.main()
