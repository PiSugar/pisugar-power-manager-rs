"""Verify CDN refresh propagation and byte comparisons without network access."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / 'scripts/verify-release-cdn.sh'


class ReleaseCdnTests(unittest.TestCase):
    def test_refresh_and_verification(self):
        for scenario in ('current', 'stale_once', 'stale_always', 'refresh_failure'):
            with self.subTest(scenario=scenario), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                tools = root / 'bin'
                tools.mkdir()
                (root / 'installer').write_text('new installer')
                for package in ('server', 'poweroff', 'programmer'):
                    (root / f'pisugar-{package}_2.4.0-1_arm64.deb').write_text(package)
                scripts = {
                    'qshell': '''echo "$*" > refresh-call
cp "$3" refresh-urls
[ "$SCENARIO" != refresh_failure ]
''',
                    'bin/sleep': 'exit 0\n',
                    'bin/curl': '''while [ "$#" -gt 0 ]; do
 case "$1" in https://*) url=$1;; -o) shift; output=$1;; esac
 shift
done
case "$url" in
 */pisugar-power-manager.sh)
  if [ "$SCENARIO" = stale_always ] || { [ "$SCENARIO" = stale_once ] && [ ! -f first-request ]; }; then
   touch first-request
   echo stale > "$output"
  else cp installer "$output"; fi;;
 *) cp "${url##*/}" "$output";;
esac
''',
                }
                for name, body in scripts.items():
                    path = root / name
                    path.write_text('#!/bin/sh\nset -e\n' + body)
                    path.chmod(0o755)
                env = dict(os.environ, PATH=str(tools) + ':' + os.environ['PATH'], SCENARIO=scenario)
                result = subprocess.run(['bash', str(SCRIPT), '2.4.0', 'installer'],
                                        cwd=root, env=env, text=True, capture_output=True, timeout=10)
                if scenario in ('current', 'stale_once'):
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertIn('verified through CDN', result.stdout)
                else:
                    self.assertNotEqual(result.returncode, 0)
                urls = (root / 'refresh-urls').read_text().splitlines()
                self.assertEqual(len(urls), 4)
                self.assertTrue(all(url.startswith('https://cdn.pisugar.com/release/') for url in urls))
                if scenario == 'stale_once':
                    self.assertIn('Waiting for CDN refresh', result.stdout)


if __name__ == '__main__':
    unittest.main()
