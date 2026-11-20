"""Exercise the decision code shipped inside the single-file CDN updater."""
import contextlib
import errno
import io
from pathlib import Path
import unittest
from unittest.mock import patch
import sys

SCRIPT = Path(__file__).resolve().parents[1] / "scripts/PiSugarUpdate.sh"
FM = bytes.fromhex("505349440101000e")
CH32 = bytes.fromhex("505349440102010c")
LEGACY = bytes(8)
LINUX_EREMOTEIO = 121


def registers(marker=FM, version=b"v1.4.2\0", tail=b"\x60\0\xff\xff"):
    result = {0: 3, 1: 15}
    for start, data in ((0x52, marker), (0x10, bytes(range(12)) + tail), (0xe2, version)):
        result.update((start + offset, val) for offset, val in enumerate(data))
    return result


class UpdateVersionTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        script = SCRIPT.read_text(encoding="utf8")
        probe = script.split(" <<'PY'\n", 1)[1].split("\nPY\n", 1)[0]
        ns = {"__name__": "version_test"}
        # Only the real device entry point needs Linux's fcntl.
        with patch.dict(sys.modules, {"fcntl": object()}):
            exec(compile(probe, str(SCRIPT), "exec"), ns)
        cls.decide = staticmethod(ns["decide"])
        cls.wait_for_board = staticmethod(ns["wait_for_board"])

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

    def test_newer_version_requires_downgrade_decision(self):
        for marker, version, expected in [
            (FM, b"v1.4.3\0", "downgrade fm33lc0x v1.4.3"),
            (CH32, b"v1.4.10\0", "downgrade ch32l103 v1.4.10"),
        ]:
            with self.subTest(expected=expected), contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(self.decide(registers(marker, version).__getitem__), expected)

    def test_known_ch32_boot_modes_without_application_registers(self):
        for mode, name in [(0xf0, "bootloader"), (0xba, "bootapp")]:
            values = {0: 3, 1: mode, **dict(enumerate(CH32, 0x52))}
            with self.subTest(mode=mode), contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(self.decide(values.__getitem__), "recover ch32l103 " + name)

    def test_boot_modes_without_trusted_identity(self):
        for marker in (LEGACY, FM, CH32[:-1] + b"\0"):
            for mode in (0xf0, 0xba):
                # A read outside these boot registers fails this test with KeyError.
                values = {0: 3, 1: mode, **dict(enumerate(marker, 0x52))}
                with self.subTest(marker=marker, mode=mode), self.assertRaises(ValueError):
                    self.decide(values.__getitem__)

    def test_marked_application_with_persistently_unreadable_version(self):
        for marker, platform in ((FM, "fm33lc0x"), (CH32, "ch32l103")):
            for raw in (6, LINUX_EREMOTEIO, b"\0", b"1" * 14):
                with self.subTest(marker=marker, raw=raw), contextlib.redirect_stderr(io.StringIO()):
                    values = registers(marker, b"" if isinstance(raw, int) else raw)
                    requests = []

                    def read(index):
                        requests.append(index)
                        if index == 0xe2 and isinstance(raw, int):
                            raise OSError(raw, "NACK")
                        return values[index]

                    self.assertEqual(self.decide(read), "recover " + platform + " unreadable")
                    self.assertGreaterEqual(requests.count(0xe2), 2)

    def test_non_nack_io_errors_are_not_application_recovery(self):
        values = registers(CH32)
        for error in (5, 13, 16, 19, 110):
            with self.subTest(error=error):
                def read(index):
                    if index == 0xe2:
                        raise OSError(error, "I2C read failed")
                    return values[index]

                with self.assertRaises((OSError, ValueError)):
                    self.decide(read)

    def test_intermittent_version_failure_is_not_recovery(self):
        values = registers(CH32, b"v1.4.3\0")
        for failure_occurrence in (1, 2):
            with self.subTest(failure_occurrence=failure_occurrence):
                seen = 0

                def read(index):
                    nonlocal seen
                    if index == 0xe4:
                        seen += 1
                        if seen == failure_occurrence:
                            raise OSError(LINUX_EREMOTEIO, "NACK")
                    return values[index]

                with self.assertRaises((ValueError, OSError)):
                    self.decide(read)

    def test_refuses_unsafe_or_unrecognized_versions(self):
        cases = [
            registers(LEGACY, b"v1.4.0\0", bytes(4)), registers(LEGACY, b"v1.4.2\0"),
            registers(LEGACY, b"v1.4.3\0"),
            registers(b"PSID\x01\x02\x01\0"), registers(FM, b"v1.4.2test\0"),
            registers(CH32, b"custom\0"), registers(LEGACY, b"\0"), registers(LEGACY, b"1" * 14),
            {**registers(), 1: 0xff}, {**registers(), 0: 2},
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
            with self.assertRaises((OSError, ValueError)):
                self.decide(broken)

    def test_boot_identity_must_be_stable_and_readable(self):
        values = {0: 3, 1: 0xf0, **dict(enumerate(CH32, 0x52))}
        for register, changed in ((0, 2), (1, 0xba), (0x57, 1), (0x59, None)):
            with self.subTest(register=register):
                seen = 0

                def read(index):
                    nonlocal seen
                    if index == register:
                        seen += 1
                        if seen == 2:
                            if changed is None:
                                raise OSError(LINUX_EREMOTEIO, "NACK")
                            return changed
                    return values[index]

                with self.assertRaises((ValueError, OSError)):
                    self.decide(read)

    def test_default_wait_reads_board_when_it_appears(self):
        marker = dict(enumerate(CH32, 0x52))
        boot = {0: 3, 1: 0xf0, **marker}
        initializing = {0: 0, 1: 0}
        for stages, result in [
            ([6, LINUX_EREMOTEIO, initializing, boot], "recover ch32l103 bootloader"),
            ([errno.ETIMEDOUT, registers(FM)], "current fm33lc0x"),
            ([registers(LEGACY, b"v1.3.8\0")], "upgrade fm33lc0x"),
        ]:
            with self.subTest(stages=stages):
                position = 0
                requests = []

                def read(index):
                    sample = stages[position]
                    requests.append((position, index))
                    if isinstance(sample, int):
                        raise OSError(sample, "NACK")
                    return sample[index]

                def sleep(seconds):
                    nonlocal position
                    self.assertEqual(seconds, 0.1)
                    self.assertLess(len(requests), 2000, "missed stable bootloader")
                    position = min(position + 1, len(stages) - 1)

                with contextlib.redirect_stderr(io.StringIO()):
                    self.assertEqual(self.wait_for_board(read, sleep), result)
                self.assertEqual(position, len(stages) - 1)
                self.assertGreaterEqual(requests.count((position, 0x59)), 2)

    def test_reset_wait_refuses_unidentified_or_wrong_boot_target(self):
        for product, marker in [(3, LEGACY), (3, CH32[:-1] + b"\0"), (2, CH32)]:
            with self.subTest(product=product, marker=marker):
                values = {0: product, 1: 0xf0, **dict(enumerate(marker, 0x52))}
                calls = 0

                def read(index):
                    nonlocal calls
                    calls += 1
                    self.assertLess(calls, 2000, "did not reject the unidentified target")
                    return values[index]

                with self.assertRaises((OSError, ValueError)):
                    self.wait_for_board(read, lambda _seconds: self.fail("unexpected retry"))
