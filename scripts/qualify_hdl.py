#!/usr/bin/env python3
"""Qualify one explicit Linux/xezim VPI profile; never a Specman comparison."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "tests/fixtures/hdl-profile"


class ProfileFailure(Exception):
    pass


def require(condition: bool, category: str) -> None:
    if not condition:
        raise ProfileFailure(category)


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(argv: list[str], env: dict[str, str], cwd: Path, stdin: str | None = None):
    # Every launch owns a new process group. Timeout cleanup includes the
    # native interpreter and its xezim child, not unrelated host processes.
    process = subprocess.Popen(argv, cwd=cwd, env=env, text=True,
                               stdin=subprocess.PIPE if stdin is not None else subprocess.DEVNULL,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                               start_new_session=True)
    try:
        stdout, stderr = process.communicate(stdin, timeout=15)
    except subprocess.TimeoutExpired:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        process.communicate()
        raise ProfileFailure("process_timeout") from None
    require(len(stdout) + len(stderr) <= 2 * 1024 * 1024, "process_output_limit")
    return process.returncode, stdout, stderr


def check_wave(path: Path) -> dict:
    scope: list[str] = []
    names: dict[str, str] = {}
    values: dict[str, list] = {}
    widths: dict[str, int] = {}
    tick = 0
    definitions = True
    text = path.read_text()
    require(re.search(r"\$timescale\s+1ps\s+\$end", text) is not None, "wave_timescale")
    for line in text.splitlines():
        fields = line.split()
        if definitions:
            if fields[:1] == ["$scope"]:
                scope.append(fields[2])
            elif fields[:1] == ["$upscope"]:
                scope.pop()
            elif fields[:1] == ["$var"]:
                require(fields[3] not in names, "wave_duplicate_symbol")
                names[fields[3]] = ".".join(scope + [fields[4]])
                widths[names[fields[3]]] = int(fields[2])
                values[names[fields[3]]] = []
            elif fields[:1] == ["$enddefinitions"]:
                definitions = False
        elif line.startswith("#"):
            next_tick = int(line[1:])
            require(next_tick >= tick, "wave_time_reversed")
            tick = next_tick
        elif line.startswith("b"):
            bits, symbol = line[1:].split()
            value = int(bits, 2) if set(bits) <= {"0", "1"} else bits.lower()
            values[names[symbol]].append([tick, value])
        elif line[:1] in ("0", "1", "x", "z"):
            value = int(line[0]) if line[0] in "01" else line[0]
            values[names[line[1:]]].append([tick, value])
    expected = {
        "profile.clk": [[0, 0], [3, 1], [4, 0], [5, 1], [6, 0], [7, 1], [8, 0]],
        "profile.count": [[0, 0], [3, 1], [5, 2], [7, 3]],
        "profile.word": [[0, 0], [1, 2147483648], [2, 2147483647]],
        "profile.wide": [[0, 0]], "profile.unknown_bits": [[0, "xz01"]],
    }
    require(widths == {"profile.clk": 1, "profile.count": 8, "profile.word": 32,
                       "profile.wide": 64, "profile.unknown_bits": 4}, "wave_widths")
    require(values == expected and tick == 9, "wave_values_or_times")
    return {"final_tick": tick, "rising_edges": 3, "counter_values": [0, 1, 2, 3]}


def check_coverage(path: Path) -> dict:
    data = json.loads(path.read_text())
    require(data["sim_time"] == 9, "hdl_coverage_time")
    code = data["code_coverage"]
    require(set(code["kinds"]) == {"statement", "branch", "toggle"}, "hdl_coverage_kinds")
    require(code["statement"]["covered"] == code["statement"]["total"] == 9,
            "hdl_global_statement_coverage")
    scopes = code["scopes"]
    require(len(scopes) == 1 and scopes[0]["scope"] == "profile", "hdl_coverage_scope")
    scope = scopes[0]
    require(scope["statement"]["covered"] == scope["statement"]["total"] == 9,
            "hdl_statement_coverage")
    always = [site for site in scope["statements"] if site["kind"] == "always"]
    require(len(always) == 1 and always[0]["count"] == 3, "hdl_always_count")
    clock = [item for item in scope["toggles"] if item["signal"] == "clk"]
    require(len(clock) == 1 and clock[0]["rise"] == [3] and clock[0]["fall"] == [3],
            "hdl_clock_coverage")
    return {"covered_statements": 9, "always_count": 3, "clock_rise": 3, "clock_fall": 3}


def qualify(native: Path, report_path: Path) -> dict:
    env = {key: value for key, value in os.environ.items() if not key.startswith("XEZIM_")}
    simulator = Path(shutil.which("xezim", path=env.get("PATH")) or "")
    bridge = Path(env.get("E1647_XEZIM_BRIDGE", "/opt/xezim/bridge.so"))
    require(native.is_file() and simulator.is_file() and bridge.is_file(), "missing_profile_binary")
    binaries = {"native": native, "simulator": simulator, "bridge": bridge}
    fingerprints = {name: digest(path) for name, path in binaries.items()}
    implementations = ["src/hdl.rs", "src/interp.rs", "c/xezim_bridge.c", "scripts/qualify_hdl.py"]
    input_paths = {name: ROOT / name for name in implementations}
    input_paths.update({"tests/fixtures/hdl-profile/" + p.name: p for p in FIXTURES.iterdir() if p.is_file()})
    inputs = {name: digest(path) for name, path in input_paths.items()}
    report_path.parent.mkdir(parents=True, exist_ok=True)
    work = Path(tempfile.mkdtemp(prefix="e1647-hdl-", dir=report_path.parent)).resolve()
    # The source contains no imports and all fixture paths are independently
    # authored. Keep staged inputs and generated wave/HDL coverage for review.
    require(re.fullmatch(r"[A-Za-z0-9/_.-]+", str(work)) is not None, "work_path_not_bridge_compatible")
    code, version, _ = run([str(simulator), "-V"], env, work)
    require(code == 0 and version.splitlines()[:2] == ["xezim version 0.11.3",
            "git 058f2bba (2026-10-23T17:05:55-07:00)"], "unqualified_simulator_version")
    env.update(E1647_XEZIM_TIMEOUT_MS="3000", E1647_XEZIM_WAVE="1")
    wave, coverage = work / "profile.vcd", work / "hdl-coverage.json"
    design = work / "profile.sv"
    design.write_text((FIXTURES / "profile.sv").read_text().replace("PROFILE_WAVE", str(wave)))
    env.update(XEZIM_CODE_COVERAGE="all", XEZIM_COV_DB=str(coverage))
    cases = []
    artifacts = {}
    for source in sorted(FIXTURES.glob("*.e")):
        staged = work / source.name
        staged.write_text(source.read_text().replace("PROFILE_DESIGN", str(design)))
        code, stdout, stderr = run([str(native), "run", "--seed", "1", str(staged)], env, work)
        expected = source.with_suffix(".run.expected").read_text()
        prefix = str(staged) + ":"
        diagnostics = "".join(line[len(prefix):] + "\n" for line in stderr.splitlines()
                              if line.startswith(prefix))
        require(stdout == source.with_suffix(".out").read_text(), source.stem + ":output")
        require(diagnostics == expected, source.stem + ":diagnostics")
        require(code == (1 if expected else 0), source.stem + ":exit")
        cases.append({"case": source.stem, "status": "passed", "exit_code": code})
        if source.stem == "positive":
            artifacts = {"wave": {"path": str(wave), "sha256": digest(wave), "checks": check_wave(wave)},
                         "hdl_coverage": {"path": str(coverage), "sha256": digest(coverage),
                                          "checks": check_coverage(coverage)}}
            # Later negative sessions must not overwrite the evidence.
            design.write_text(design.read_text().replace(str(wave), str(work / "negative.vcd")))
            env["XEZIM_COV_DB"] = str(work / "negative-coverage.json")
    for name, ending in [("mixed_counter", "mixed testcase complete"),
                         ("mixed_port", "port testcase complete")]:
        source = ROOT / "examples" / (name + ".e")
        staged = work / source.name
        staged.write_text(source.read_text().replace("examples/mixed/counter.sv",
                          str(ROOT / "examples/mixed/counter.sv")))
        code, stdout, stderr = run([str(native), "run", "--seed", "1", str(staged)], env, work)
        expected = "".join(f"cycle {i}: count={i + 1} OK\n" for i in range(5)) + ending + "\n"
        require(code == 0 and stdout == expected and "error[E" not in stderr, name + ":example")
        cases.append({"case": name, "status": "passed", "exit_code": code})
    # The bridge itself rejects bad raw commands without narrowing/partial
    # application. This is real VPI evidence, distinct from Rust transport doubles.
    commands = ("RUN 1\nSET profile.word 2147483648\nGET profile.word\n"
                "GET profile.wide\nGET profile.unknown_bits\nRUN -1\nRUN 12junk\n"
                "SET profile.word 1 extra\nGET profile.word extra\n" + "x" * 1100 + "\nFINISH\n")
    code, stdout, _ = run([str(simulator), "--simulate", "--vpi-lib", str(bridge),
                           "-s", "profile", str(design)], env, work, commands)
    tokens = [line for line in stdout.splitlines() if line.split(" ")[0] in
              {"READY", "OK", "ERR", "VAL", "BYE"}]
    require(code == 0 and tokens == ["READY", "OK", "READY",
            "ERR SET requires a signed 32-bit integer", "VAL 0",
            "ERR signal width must be from 1 to 32 bits", "ERR GET does not support X/Z signal values",
            "ERR RUN requires an unsigned 64-bit delay", "ERR RUN requires an unsigned 64-bit delay",
            "ERR usage: SET <path> <value>", "ERR usage: GET <path>", "ERR command exceeds input buffer", "BYE"],
            "raw_bridge_commands")
    require(all(digest(path) == fingerprints[name] for name, path in binaries.items()), "binary_changed")
    require(all(digest(path) == inputs[name] for name, path in input_paths.items()), "input_changed")
    require(digest(wave) == artifacts["wave"]["sha256"] and
            digest(coverage) == artifacts["hdl_coverage"]["sha256"], "artifact_changed")
    return {"schema_version": 1, "status": "passed", "scope": "explicit_linux_xezim_vpi_profile",
            "simulator_version": "0.11.3", "simulator_revision": "058f2bba",
            "binary_sha256": fingerprints,
            "implementation_sha256": {name: inputs[name] for name in implementations},
            "fixture_sha256": {p.name: inputs["tests/fixtures/hdl-profile/" + p.name]
                               for p in sorted(FIXTURES.iterdir()) if p.is_file()},
            "cases": cases, "raw_bridge_commands": "passed", "artifacts": artifacts,
            "reference_specman_comparison": "not_run", "general_hdl_event_ordering": "not_qualified"}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--native", required=True, type=Path)
    parser.add_argument("--report", required=True, type=Path)
    args = parser.parse_args()
    if os.name != "posix":
        parser.error("this concrete qualification profile requires Linux (use the documented container)")
    try:
        report = qualify(args.native.resolve(), args.report.resolve())
    except (ProfileFailure, OSError, ValueError, KeyError, IndexError) as error:
        # No raw simulator stdout/stderr is persisted in the report.
        report = {"schema_version": 1, "status": "failed", "category": str(error)
                  if isinstance(error, ProfileFailure) else type(error).__name__}
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print("HDL profile: " + report["status"])
    if report["status"] == "failed":
        print("check: " + report["category"])
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
