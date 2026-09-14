import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parents[1]))

spec = importlib.util.spec_from_file_location("service", Path(__file__).parents[1] / "service.py")
service = importlib.util.module_from_spec(spec)
spec.loader.exec_module(service)

class ServiceContractTests(unittest.TestCase):
    def test_language_mapping_and_invalid_languages(self):
        for source, expected in [("zh-CN", "chinese"), ("en-US", "english"), ("ja-JP", "japanese"), ("Auto", "auto")]:
            self.assertEqual(service.language_name(source), expected)
        with self.assertRaises(ValueError): service.language_name("unsupported")

    def test_rejects_unbounded_or_unknown_generation_parameters(self):
        good = {"text": "你好", "speaker": "serena", "language": "zh-CN", "timeoutSeconds": 180}
        self.assertEqual(service.validate_request(good, ["serena"]), ("你好", "serena", "chinese", 180))
        for patch in [{"text": ""}, {"text": "x" * 8001}, {"text": "x\0"}, {"speaker": "unknown"}, {"timeoutSeconds": 181}, {"timeoutSeconds": float("nan")}, {"timeoutSeconds": True}]:
            with self.subTest(patch=patch), self.assertRaises(ValueError):
                service.validate_request({**good, **patch}, ["serena"])

    def test_incomplete_local_model_fails_without_downloading(self):
        with tempfile.TemporaryDirectory() as folder:
            with self.assertRaisesRegex(RuntimeError, "missing config.json"):
                service.resolve_model(folder)

if __name__ == "__main__": unittest.main()
