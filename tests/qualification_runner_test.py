"""Protocol tests use an owned fixture process, never a reference simulator."""
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("qualification_runner", ROOT / "scripts/qualify.py")
runner = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(runner)

FIXTURE_PROCESS = r"""
import hashlib, json, pathlib, sys, time
backend, mode = sys.argv[1:3]
if mode == "timeout": time.sleep(2)
if backend == "native":
    source = pathlib.Path(sys.argv[3])
    if mode == "mutate": source.write_text("changed source", encoding="utf-8")
    if mode == "delete": source.unlink()
    if mode == "large": print("x" * 1100000)
    value = "8" if mode == "wrong" else "7"
    print("PRIVATE_BACKEND_TEXT", file=sys.stderr)
    print("E1647_RESULT answer=" + value)
    if mode == "duplicate": print("E1647_RESULT answer=7")
    if mode != "missing_done": print("E1647_DONE sample")
else:
    request = json.load(sys.stdin)
    if mode == "malformed": print("PRIVATE_BACKEND_TEXT"); raise SystemExit(0)
    response = {key: request[key] for key in ("schema_version", "case_id", "source_sha256", "seed")}
    response.update(status="passed", values={"answer": "8" if mode == "wrong" else "7"},
                    tool={"name": "Owned protocol fixture", "version": "test-only", "profile": "protocol-only"})
    if hashlib.sha256(pathlib.Path(request["source"]).read_bytes()).hexdigest() != request["source_sha256"]:
        response["status"] = "failed"
    if mode == "wrong_hash": response["source_sha256"] = "0" * 64
    if mode == "wrong_seed": response["seed"] += 1
    if mode == "wrong_case": response["case_id"] = "different"
    if mode == "failed_status": response["status"] = "failed"
    if mode == "missing_profile": del response["tool"]["profile"]
    if mode == "extra": response["values"]["unexpected"] = "1"
    print("PRIVATE_BACKEND_TEXT", file=sys.stderr)
    print(json.dumps(response))
if mode == "nonzero": raise SystemExit(3)
"""


class QualificationRunnerTests(unittest.TestCase):
    def setUp(self):
        work = ROOT / "local/qualification-tests"
        work.mkdir(parents=True, exist_ok=True)
        self.temporary = tempfile.TemporaryDirectory(dir=work)
        self.root = Path(self.temporary.name).resolve()
        self.assertTrue(self.root.is_relative_to(work.resolve()))
        self.fixture = self.root / "fixture.py"
        self.fixture.write_text(FIXTURE_PROCESS, encoding="utf-8")
        self.source = self.root / "owned.e"
        self.source.write_text("independently owned protocol-test input\n", encoding="utf-8")
        self.case = {"id": "sample", "source": self.source.relative_to(ROOT).as_posix(),
                     "source_path": self.source, "seed": 1, "clauses": ["11.1"], "expected": {"answer": "7"}}

    def tearDown(self):
        self.assertTrue(self.root.is_relative_to((ROOT / "local/qualification-tests").resolve()))
        self.temporary.cleanup()

    def native(self, mode="good"):
        return [sys.executable, str(self.fixture), "native", mode, "{source}", "{seed}"]

    def reference(self, mode="good"):
        return [sys.executable, str(self.fixture), "reference", mode]

    def run_case(self, native="good", reference="good", timeout=5):
        return runner.run_case(self.case, self.native(native), None if reference is None else self.reference(reference),
                               timeout, self.root / "staging")

    def test_successful_adapter_comparison_is_scoped_to_case_values(self):
        result = self.run_case()
        self.assertEqual(result["comparison"], "matched")
        self.assertEqual(result["reference"]["tool"]["version"], "test-only")
        self.assertNotIn("PRIVATE_BACKEND_TEXT", json.dumps(result))

    def test_native_only_explicitly_records_reference_not_run(self):
        result = self.run_case(reference=None)
        self.assertEqual(result["native"]["status"], "ok")
        self.assertEqual(result["comparison"], "not_run")
        self.assertEqual(result["reference"]["status"], "not_requested")

    def test_equal_wrong_backends_do_not_defeat_the_owned_oracle(self):
        result = self.run_case(native="wrong", reference="wrong")
        self.assertEqual(result["native"]["status"], "value_mismatch")
        self.assertEqual(result["reference"]["status"], "value_mismatch")
        self.assertEqual(result["comparison"], "failed")

    def test_reference_input_and_success_identity_must_match(self):
        for mode in ("wrong_hash", "wrong_case", "wrong_seed", "missing_profile", "malformed"):
            with self.subTest(mode=mode):
                result = self.run_case(reference=mode)
                self.assertEqual(result["reference"]["status"], "invalid_envelope")
                self.assertEqual(result["comparison"], "failed")
                self.assertNotIn("PRIVATE_BACKEND_TEXT", json.dumps(result))

    def test_adapter_reported_failure_is_not_a_malformed_success(self):
        result = self.run_case(reference="failed_status")
        self.assertEqual(result["reference"]["status"], "adapter_failed")
        self.assertEqual(result["comparison"], "failed")

    def test_extra_reference_values_fail_the_exact_oracle(self):
        result = self.run_case(reference="extra")
        self.assertEqual(result["reference"]["status"], "value_mismatch")

    def test_nonzero_exit_wins_even_when_output_looks_successful(self):
        for backend in ("native", "reference"):
            with self.subTest(backend=backend):
                result = self.run_case(**{backend: "nonzero"})
                self.assertEqual(result[backend]["status"], "nonzero_exit")
                self.assertEqual(result["comparison"], "failed")

    def test_duplicate_or_incomplete_markers_cannot_pass(self):
        for mode in ("duplicate", "missing_done"):
            with self.subTest(mode=mode):
                result = self.run_case(native=mode)
                self.assertEqual(result["native"]["status"], "invalid_markers")

    def test_timeout_is_not_a_skipped_or_successful_reference(self):
        result = self.run_case(reference="timeout", timeout=0.2)
        self.assertEqual(result["reference"]["status"], "timeout")
        self.assertEqual(result["comparison"], "failed")

    def test_source_mutation_or_deletion_fails_and_restores_reference_input(self):
        for mode in ("mutate", "delete"):
            with self.subTest(mode=mode):
                result = self.run_case(native=mode)
                self.assertEqual(result["native"]["status"], "source_changed")
                self.assertEqual(result["reference"]["status"], "ok")
                self.assertEqual(result["comparison"], "failed")

    def test_excess_output_is_rejected_without_a_dump(self):
        result = self.run_case(native="large")
        self.assertEqual(result["native"]["status"], "output_limit")
        self.assertLess(len(json.dumps(result)), 2000)

    def test_manifest_rejects_duplicate_ids_and_escaping_sources(self):
        case = {key: value for key, value in self.case.items() if key != "source_path"}
        manifest = self.root / "manifest.json"
        manifest.write_text(json.dumps({"schema_version": 1, "cases": [case, case]}), encoding="utf-8")
        with self.assertRaises(runner.ConfigurationError): runner.load_cases(manifest)
        case["source"] = "../outside.e"
        manifest.write_text(json.dumps({"schema_version": 1, "cases": [case]}), encoding="utf-8")
        with self.assertRaises(runner.ConfigurationError): runner.load_cases(manifest)
        manifest.write_text('{"schema_version":1,"schema_version":1}', encoding="utf-8")
        with self.assertRaises(runner.ConfigurationError): runner.load_cases(manifest)

    def test_cli_report_has_no_raw_output_or_command_arguments(self):
        case = {key: value for key, value in self.case.items() if key != "source_path"}
        manifest = self.root / "manifest.json"
        report = self.root / "report.json"
        manifest.write_text(json.dumps({"schema_version": 1, "cases": [case]}), encoding="utf-8")
        with contextlib.redirect_stdout(io.StringIO()):
            code = runner.main(["--manifest", str(manifest), "--report", str(report), "--native-only",
                                "--native-command", json.dumps(self.native())])
        self.assertEqual(code, 0)
        result = json.loads(report.read_text(encoding="utf-8"))
        self.assertEqual(result["scope"], "native_expected_values_only")
        self.assertNotIn("PRIVATE_BACKEND_TEXT", json.dumps(result))
        self.assertNotIn(str(self.fixture), json.dumps(result))

    def test_cli_requires_an_explicit_reference_or_native_only_mode(self):
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit) as error:
            runner.main([])
        self.assertEqual(error.exception.code, 2)


if __name__ == "__main__":
    unittest.main()
