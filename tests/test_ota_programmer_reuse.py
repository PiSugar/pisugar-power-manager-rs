"""Exercise the local programmer check from the single-file OTA script."""
import hashlib
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "scripts/PiSugarUpdate.sh"
PROGRAMMER = b"#!/bin/sh\necho pisugar-programmer 2.4.3\n"
MATCHING_PACKAGE = "install ok installed\t2.4.3-1\tarm64"


class ProgrammerReuseTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        source, marker, tail = SCRIPT.read_text(encoding="utf8").rpartition('\nmain "$@"')
        assert marker and not tail.strip()
        cls.source = source + """
work=.
installed_tool=./installed
dpkg-query() {
    printf '%s\\n' "${!#}" >> queried
    printf '%s\\n' "$package_record"
}
stage_installed_programmer
"""

    def check_candidate(self, accepted, *, contents=PROGRAMMER, record=MATCHING_PACKAGE, arch="arm64"):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            if contents is not None:
                (root / "installed").write_bytes(contents)
                (root / "installed").chmod(0o755)
            env = dict(os.environ, package_version="2.4.3-1", arch=arch, package_record=record,
                       binary_sha=hashlib.sha256(PROGRAMMER).hexdigest())
            result = subprocess.run(["bash"], input=self.source.encode(), cwd=root, env=env,
                                    capture_output=True, timeout=10)
            self.assertEqual(result.returncode, 0 if accepted else 1, result.stderr.decode())
            if accepted:
                self.assertEqual((root / "local-programmer").read_bytes(), PROGRAMMER)
                if arch:
                    self.assertEqual((root / "queried").read_text().splitlines(), ["pisugar-programmer:" + arch])
                else:
                    self.assertFalse((root / "queried").exists())

    def test_matching_debian_and_standalone_programmers(self):
        for arch in ("arm64", ""):
            with self.subTest(arch=arch):
                self.check_candidate(True, arch=arch)

    def test_wrong_package_record(self):
        for record in ("install ok installed\t2.4.2-1\tarm64", "install ok installed\t2.4.3-1\tarmhf",
                       "install ok unpacked\t2.4.3-1\tarm64", ""):
            with self.subTest(record=record):
                self.check_candidate(False, record=record)

    def test_missing_or_modified_programmer(self):
        # Appending a comment changes the digest but keeps --version identical.
        for contents in (None, PROGRAMMER + b"# modified\n"):
            with self.subTest(contents=contents):
                self.check_candidate(False, contents=contents)
