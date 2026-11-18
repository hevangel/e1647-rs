"""Artifact-oracle tests only; no simulator evidence is claimed here."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("qualify_hdl", Path(__file__).resolve().parents[1] / "scripts/qualify_hdl.py")
PROFILE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PROFILE)

# Independently specified artifact for the profile's three rising edges.
WAVE = """$timescale 1ps $end
$scope module profile $end
$var reg 1 a clk $end
$var reg 8 b count $end
$var reg 32 c word $end
$var reg 64 d wide $end
$var reg 4 e unknown_bits $end
$upscope $end
$enddefinitions $end
#0
0a
b0 b
b0 c
b0 d
bxz01 e
#1
b10000000000000000000000000000000 c
#2
b1111111111111111111111111111111 c
#3
1a
b1 b
#4
0a
#5
1a
b10 b
#6
0a
#7
1a
b11 b
#8
0a
#9
"""


class ArtifactOracleTest(unittest.TestCase):
    def test_owned_wave_mechanics(self):
        with tempfile.TemporaryDirectory() as work:
            path = Path(work) / "owned.vcd"
            path.write_text(WAVE)
            self.assertEqual(PROFILE.check_wave(path)["counter_values"], [0, 1, 2, 3])

    def test_wave_changes_fail_the_oracle(self):
        corruptions = [WAVE.replace("b11 b", "b100 b"), WAVE.replace("#7", "#6"),
                       WAVE.replace("#9", "#4\n#9"), WAVE.replace("reg 8 b", "reg 16 b"),
                       WAVE.replace("$var reg 1 a clk $end", ""), WAVE.replace("1ps", "1ns")]
        with tempfile.TemporaryDirectory() as work:
            path = Path(work) / "owned.vcd"
            for corrupted in corruptions:
                path.write_text(corrupted)
                with self.assertRaises((PROFILE.ProfileFailure, KeyError)):
                    PROFILE.check_wave(path)

    def test_coverage_changes_fail_the_oracle(self):
        coverage = {"sim_time": 9, "code_coverage": {"kinds": ["statement", "branch", "toggle"],
                    "statement": {"covered": 9, "total": 9}, "scopes": [{"scope": "profile",
                    "statement": {"covered": 9, "total": 9},
                    "statements": [{"kind": "always", "count": 3}],
                    "toggles": [{"signal": "clk", "rise": [3], "fall": [3]}]}]}}
        with tempfile.TemporaryDirectory() as work:
            path = Path(work) / "owned.json"
            path.write_text(json.dumps(coverage))
            self.assertEqual(PROFILE.check_coverage(path)["always_count"], 3)
            for location, key, wrong in [(coverage, "sim_time", 8),
                    (coverage["code_coverage"]["scopes"][0]["toggles"][0], "fall", [2]),
                    (coverage["code_coverage"]["statement"], "covered", 8)]:
                saved = location[key]
                location[key] = wrong
                path.write_text(json.dumps(coverage))
                with self.assertRaises(PROFILE.ProfileFailure):
                    PROFILE.check_coverage(path)
                location[key] = saved


if __name__ == "__main__":
    unittest.main()
