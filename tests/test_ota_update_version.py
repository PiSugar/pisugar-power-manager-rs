"""Exercise the decision code shipped inside the single-file CDN updater."""
import contextlib
import io
from pathlib import Path
import unittest
from unittest.mock import patch
import sys

SCRIPT = Path(__file__).resolve().parents[1] / "scripts/PiSugarUpdate.sh"
FM = bytes.fromhex("505349440101000e")
CH32 = bytes.fromhex("505349440102010c")
LEGACY = bytes(8)


def registers(marker=FM, version=b"v1.4.2\0", tail=b"\x60\0\xff\xff"):
    result = {0: 3, 1: 15}
    for start, data in ((0x52, marker), (0x10, bytes(range(12)) + tail), (0xe2, version)):
        result.update((start + offset, val) for offset, val in enumerate(data))
    return result


class UpdateVersionTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        script = SCRIPT.read_text(encoding="utf8")
        probe = script.split("python3 - <<'PY'\n", 1)[1].split("\nPY\n", 1)[0]
        ns = {"__name__": "version_test"}
        # Only the real device entry point needs Linux's fcntl.
        with patch.dict(sys.modules, {"fcntl": object()}):
            exec(compile(probe, str(SCRIPT), "exec"), ns)
        cls.decide = staticmethod(ns["decide"])

    def test_supported_boards(self):
        cases = [
            (registers(LEGACY, b"v1.3.8\0"), "upgrade fm33lc0x"),
            (registers(LEGACY, b"v1.4.0\0"), "upgrade fm33lc0x"),
            (registers(FM), "current fm33lc0x"),
            (registers(CH32, b"v1.4.2\0"), "upgrade ch32l103"),
            (registers(CH32, b"v1.4.3\0"), "current ch32l103"),
        ]
        for values, result in cases:
            with self.subTest(result=result), contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(self.decide(values.__getitem__), result)

    def test_refuses_unsafe_or_unreadable_versions(self):
        cases = [
            registers(FM, b"v1.4.3\0"), registers(CH32, b"v1.4.10\0"),
            registers(LEGACY, b"v1.4.0\0", bytes(4)), registers(LEGACY, b"v1.4.2\0"),
            registers(b"PSID\x01\x02\x01\0"), registers(FM, b"v1.4.2test\0"),
            registers(FM, b"\0"), registers(FM, b"1" * 14),
            {**registers(), 1: 0xf0}, {**registers(), 1: 0xba}, {**registers(), 0: 2},
        ]
        for values in cases:
            with self.subTest(values=values), contextlib.redirect_stderr(io.StringIO()):
                with self.assertRaises(ValueError):
                    self.decide(values.__getitem__)

    def test_inconsistent_or_failed_reads(self):
        cases = {0x00: 2, 0x01: 0xf0, 0x57: 2, 0xe7: ord("1"), 0x1f: 0}
        for register, changed in cases.items():
            with self.subTest(register=register):
                values = registers(LEGACY, b"v1.3.8\0")
                seen = 0

                def read(index):
                    nonlocal seen
                    if index == register:
                        seen += 1
                        if seen == 2:
                            return changed
                    return values[index]

                with self.assertRaises(ValueError):
                    self.decide(read)
        for register in (0x52, 0xe2, 0x1f):
            def broken(index):
                if index == register:
                    raise OSError("NACK")
                return values[index]
            with self.assertRaises(OSError):
                self.decide(broken)
