#!/usr/bin/env python3
"""Compare owned e case values with an explicitly configured reference adapter.

Raw subprocess output and command arguments never enter the report. This is
case-level comparison infrastructure, not a claim of IEEE conformance.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
IDENTIFIER = re.compile(r"[a-zA-Z0-9_.-]+\Z")
RESULT = re.compile(r"E1647_RESULT ([a-zA-Z0-9_.-]+)=(.*)\Z")
OUTPUT_LIMIT = 1_048_576


class ConfigurationError(ValueError):
    pass


class AdapterFailure(ValueError):
    pass


def digest(data):
    return hashlib.sha256(data).hexdigest()


def command_fingerprint(command):
    return digest(json.dumps(command, ensure_ascii=True).encode("utf-8"))


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate JSON key")
        result[key] = value
    return result


def launcher_fingerprint(command):
    executable = shutil.which(command[0])
    if executable is None:
        return None
    try:
        return digest(Path(executable).read_bytes())
    except OSError:
        return None


def command_json(value):
    try:
        command = json.loads(value, object_pairs_hook=unique_object)
    except (ValueError, TypeError) as error:
        raise ConfigurationError("command must be a JSON argument array") from error
    if not isinstance(command, list) or not command or not command[0] or any(not isinstance(arg, str) for arg in command):
        raise ConfigurationError("command must be a nonempty JSON string array")
    return command


def values_valid(values):
    return isinstance(values, dict) and bool(values) and all(
        isinstance(key, str) and IDENTIFIER.fullmatch(key) and
        isinstance(value, str) and len(value) <= 256 and "\n" not in value and "\r" not in value
        for key, value in values.items()
    )


def load_cases(path, root=ROOT):
    try:
        document = json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=unique_object)
    except (OSError, UnicodeError, ValueError) as error:
        raise ConfigurationError("manifest is not readable JSON") from error
    if not isinstance(document, dict) or type(document.get("schema_version")) is not int or document["schema_version"] != 1:
        raise ConfigurationError("unsupported manifest schema")
    cases = document.get("cases")
    if not isinstance(cases, list) or not cases:
        raise ConfigurationError("manifest needs at least one case")
    seen = set()
    for case in cases:
        if not isinstance(case, dict):
            raise ConfigurationError("case must be an object")
        name = case.get("id")
        if not isinstance(name, str) or not IDENTIFIER.fullmatch(name) or name in seen:
            raise ConfigurationError("case IDs must be unique identifiers")
        seen.add(name)
        if type(case.get("seed")) is not int or not 0 <= case["seed"] <= 2**64 - 1:
            raise ConfigurationError("case seed must be an unsigned 64-bit integer")
        if not values_valid(case.get("expected")):
            raise ConfigurationError("case expected values must be nonempty string markers")
        if not isinstance(case.get("source"), str):
            raise ConfigurationError("case needs a source path")
        source = (root / case["source"]).resolve()
        if not source.is_relative_to(root.resolve()) or source.suffix != ".e" or not source.is_file():
            raise ConfigurationError("case source must be an e file inside the repository")
        if not isinstance(case.get("clauses"), list) or not case["clauses"] or any(not isinstance(clause, str) or not re.fullmatch(r"[0-9]+(?:\.[0-9]+)*", clause) for clause in case["clauses"]):
            raise ConfigurationError("case needs numeric clause references")
        case["source_path"] = source
    return cases


def execute(command, timeout, request=None):
    """Timeout kills the adapter itself; simulator descendant cleanup is its contract."""
    try:
        completed = subprocess.run(command, input=None if request is None else json.dumps(request).encode("utf-8"),
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=timeout, cwd=ROOT)
    except subprocess.TimeoutExpired:
        return {"status": "timeout"}, None
    except OSError:
        return {"status": "spawn_error"}, None
    metadata = {"status": "ok" if completed.returncode == 0 else "nonzero_exit", "exit_code": completed.returncode}
    if completed.returncode != 0:
        return metadata, None
    if len(completed.stdout) + len(completed.stderr) > OUTPUT_LIMIT:
        return {"status": "output_limit"}, None
    try:
        output = completed.stdout.decode("utf-8")
    except UnicodeError:
        return {"status": "invalid_output"}, None
    return metadata, output


def native_values(output, case_id):
    values = {}
    completed = False
    for line in output.splitlines():
        if line.startswith("E1647_RESULT"):
            match = RESULT.fullmatch(line)
            if not match or completed or match[1] in values:
                raise ValueError("invalid result marker")
            values[match[1]] = match[2]
        elif line.startswith("E1647_DONE"):
            if line != f"E1647_DONE {case_id}" or completed:
                raise ValueError("invalid completion marker")
            completed = True
    if not completed or not values_valid(values):
        raise ValueError("missing owned result markers")
    return values


def reference_values(output, request):
    response = json.loads(output, object_pairs_hook=unique_object)
    if not isinstance(response, dict) or type(response.get("schema_version")) is not int or response["schema_version"] != 1:
        raise ValueError("invalid adapter schema")
    for key in ("case_id", "source_sha256", "seed"):
        if type(response.get(key)) is not type(request[key]) or response[key] != request[key]:
            raise ValueError("adapter input identity mismatch")
    if response.get("status") == "failed":
        raise AdapterFailure("adapter reported a failed case")
    if response.get("status") != "passed" or not values_valid(response.get("values")):
        raise ValueError("adapter did not report a successful case")
    tool = response.get("tool")
    if not isinstance(tool, dict) or any(not isinstance(tool.get(key), str) or not tool[key].strip() or len(tool[key]) > 256 for key in ("name", "version", "profile")):
        raise ValueError("adapter must identify its tool and version")
    return response["values"], {key: tool[key] for key in ("name", "version", "profile")}


def run_case(case, native_command, reference_command, timeout, work_root=None):
    source_bytes = case["source_path"].read_bytes()
    checksum = digest(source_bytes)
    result = {"id": case["id"], "source": case["source"], "source_sha256": checksum,
              "seed": case["seed"], "clauses": case["clauses"], "expected": case["expected"], "comparison": "not_run"}
    # The corpus is self-contained. Both backends receive one immutable copy;
    # the checksum is checked again after each process to catch mutation.
    work_root = ROOT / "local/qualification-work" if work_root is None else Path(work_root)
    work_root.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="case-", dir=work_root, ignore_cleanup_errors=True) as directory:
        work = Path(directory).resolve()
        if not work.is_relative_to(work_root.resolve()):
            raise ConfigurationError("staging directory escaped its work root")
        source = work / f"{case['id']}.e"
        source.write_bytes(source_bytes)
        arguments = [arg.replace("{source}", str(source)).replace("{seed}", str(case["seed"])) for arg in native_command]
        native, output = execute(arguments, timeout)
        result["native"] = native
        if not source.exists() or source.read_bytes() != source_bytes:
            native["status"] = "source_changed"
        if native["status"] == "ok":
            try:
                native["values"] = native_values(output, case["id"])
                if native["values"] != case["expected"]:
                    native["status"] = "value_mismatch"
            except ValueError:
                native["status"] = "invalid_markers"
        if reference_command is None:
            result["reference"] = {"status": "not_requested"}
            return result
        source.write_bytes(source_bytes)
        request = {"schema_version": 1, "case_id": case["id"], "source": str(source),
                   "source_sha256": checksum, "seed": case["seed"], "work_dir": str(work)}
        reference, output = execute(reference_command, timeout, request)
        result["reference"] = reference
        if not source.exists() or source.read_bytes() != source_bytes:
            reference["status"] = "source_changed"
        if reference["status"] == "ok":
            try:
                reference["values"], reference["tool"] = reference_values(output, request)
                if reference["values"] != case["expected"]:
                    reference["status"] = "value_mismatch"
            except AdapterFailure:
                reference["status"] = "adapter_failed"
            except (ValueError, TypeError):
                reference["status"] = "invalid_envelope"
        if native["status"] == "ok" and reference["status"] == "ok":
            result["comparison"] = "matched"
        else:
            result["comparison"] = "failed"
    return result


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, default=ROOT / "qualification/cases.json")
    parser.add_argument("--report", type=Path, default=ROOT / "local/qualification-report.json")
    parser.add_argument("--native", type=Path, default=ROOT / ("target/debug/e1647.exe" if sys.platform == "win32" else "target/debug/e1647"))
    parser.add_argument("--native-command", help='JSON argv array with {source} and {seed} placeholders; overrides --native')
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--native-only", action="store_true")
    mode.add_argument("--reference-command", help="JSON argv array for an explicitly authorized local adapter")
    parser.add_argument("--timeout", type=float, default=60)
    args = parser.parse_args(argv)
    try:
        if not math.isfinite(args.timeout) or args.timeout <= 0:
            raise ConfigurationError("timeout must be finite and positive")
        cases = load_cases(args.manifest)
        native = command_json(args.native_command) if args.native_command else [str(args.native.resolve()), "run", "{source}", "--seed", "{seed}"]
        if not all(any(placeholder in arg for arg in native) for placeholder in ("{source}", "{seed}")):
            raise ConfigurationError("native command must pass source and seed")
        reference = None if args.native_only else command_json(args.reference_command)
        results = [run_case(case, native, reference, args.timeout) for case in cases]
        report = {"schema_version": 1, "scope": "native_expected_values_only" if reference is None else "configured_adapter_comparison_only",
                  "native_command_sha256": command_fingerprint(native),
                  "native_launcher_sha256": launcher_fingerprint(native),
                  "reference_command_sha256": None if reference is None else command_fingerprint(reference),
                  "reference_identity": "not_requested" if reference is None else "adapter_reported",
                  "cases": results}
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        failed = any(case["native"]["status"] != "ok" or (reference is not None and case["comparison"] != "matched") for case in results)
        print(f"{len(results)} case(s): {'failed' if failed else 'passed'}; scope={report['scope']}")
        return 1 if failed else 0
    except ConfigurationError as error:
        print(f"qualification configuration failed: {error}", file=sys.stderr)
        return 2
    except OSError as error:
        # Never echo subprocess output, argv, paths or malformed input text.
        print(f"qualification file access failed: {type(error).__name__}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
