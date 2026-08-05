"""Tests for robust JSON parsing in llm_client.parse_json_response."""

import json
import os
import sys
import unittest

EVAL_DIR = os.path.join(os.path.dirname(__file__), "..")
sys.path.insert(0, EVAL_DIR)


class TestParseJsonResponse(unittest.TestCase):
    def setUp(self):
        from judge.llm_client import parse_json_response
        self.parse = parse_json_response

    def test_plain_json(self):
        result = self.parse('{"ok": true, "severity": "ok", "issue": "无"}')
        self.assertEqual(result, {"ok": True, "severity": "ok", "issue": "无"})

    def test_fenced_json_block(self):
        result = self.parse(
            '```json\n{"ok": true, "severity": "ok", "issue": ""}\n```'
        )
        self.assertEqual(result, {"ok": True, "severity": "ok", "issue": ""})

    def test_prose_wrapped_json(self):
        result = self.parse(
            '前置文字 {"ok": true, "severity": "ok", "issue": "ok"} 后置'
        )
        self.assertEqual(result, {"ok": True, "severity": "ok", "issue": "ok"})

    def test_trailing_quote(self):
        result = self.parse(
            '{"ok": true, "severity": "ok", "issue": "无问题"}"'
        )
        self.assertEqual(result, {"ok": True, "severity": "ok", "issue": "无问题"})

    def test_inline_if_else(self):
        result = self.parse(
            '{"ok": true, "issue": "noise" if false else "文本"}'
        )
        self.assertEqual(result, {"ok": True, "issue": "文本"})

    def test_malformed_input_raises_valueerror(self):
        with self.assertRaises(ValueError):
            self.parse("this is not json at all")

    def test_brace_inside_string_value(self):
        # JSON string value legitimately contains a closing brace
        result = self.parse('{"a":"}"}')
        self.assertEqual(result, {"a": "}"})

    def test_fenced_with_trailing_garbage(self):
        result = self.parse(
            '```json\n{"ok": true, "severity": "ok", "issue": "ok"}\n``` extra'
        )
        self.assertEqual(result, {"ok": True, "severity": "ok", "issue": "ok"})

    def test_existing_valid_inputs_still_parse(self):
        # Patterns that the old parser handled directly
        cases = [
            '{"ok": false, "severity": "truncated", "issue": "结尾缺句号"}',
            '{"ok": true, "severity": "ok", "issue": ""}',
            '{"ok": false, "severity": "dangling", "issue": "悬空指代"}',
        ]
        for case in cases:
            result = self.parse(case)
            self.assertIn("ok", result)
            self.assertIn("severity", result)

    def test_fenced_with_no_json_tag(self):
        result = self.parse(
            '```\n{"ok": true, "severity": "ok", "issue": "ok"}\n```'
        )
        self.assertEqual(result, {"ok": True, "severity": "ok", "issue": "ok"})


if __name__ == "__main__":
    unittest.main()
