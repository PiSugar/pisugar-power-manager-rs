"""Host-side regression tests; system tools are stubbed, jq is real."""
import json
import os
import pty
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class InstallerTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.base = Path(self.tmp.name)
        self.bin = self.base / 'bin'
        self.bin.mkdir()
        self.log = self.base / 'log'
        self.env = dict(os.environ, PATH=f'{self.bin}:' + os.environ['PATH'], LOG=str(self.log))
        self.stub('getopt', "exec python3 -c 'import getopt,shlex,sys; a=sys.argv[1:]; a=a[a.index(\"--\")+1:]; opts,rest=getopt.gnu_getopt(a,\"hv:c:t:m:\",[\"help\",\"version=\",\"channel=\",\"type=\",\"model=\",\"rpm-build=\",\"non-interactive\",\"interactive\"]); print(\" \".join(shlex.quote(x) for k,v in opts for x in ([k,v] if v else [k]))+\" -- \"+\" \".join(map(shlex.quote,rest)))' \"$@\"")
        self.stub('sudo', 'exec "$@"')
        self.stub('apt', 'exit 0')
        self.stub('apt-get', 'echo "apt:$DEBIAN_FRONTEND:$*" >> "$LOG"; case "$*" in *deb) exit "${APT_PACKAGE_FAIL:-0}";; *) exit "${APT_FAIL:-0}";; esac')
        self.stub('dpkg', 'echo "dpkg:$*" >> "$LOG"; echo arm64')
        self.stub('uname', 'echo aarch64')
        self.stub('hostname', 'echo 127.0.0.1')
        self.stub('raspi-config', 'exit 0')
        self.stub('wget', 'echo "wget:$*" >> "$LOG"; touch "$2"; exit "${DOWNLOAD_FAIL:-0}"')
        self.stub('debconf-set-selections', 'cat >> "$LOG"')
        self.stub('rpm', 'echo "rpm:$*" >> "$LOG"')

    def stub(self, name, body):
        path = self.bin / name
        path.write_text('#!/bin/sh\n' + body + '\n')
        path.chmod(0o755)

    def run_installer(self, *args, **env):
        return subprocess.run(['bash', str(ROOT / 'scripts/pisugar-power-manager.sh'), *args],
                              input='', text=True, capture_output=True, env=dict(self.env, **env))

    def test_headless_model_and_no_removal(self):
        result = self.run_installer('--model', 'PiSugar 2 Pro')
        self.assertEqual(result.returncode, 0, result.stderr)
        log = self.log.read_text()
        self.assertIn('pisugar-server/model select PiSugar 2 Pro', log)
        self.assertIn('pisugar-poweroff/model select PiSugar 2 Pro', log)
        self.assertIn('apt:noninteractive:install -y /tmp/', log)
        self.assertNotIn('dpkg:-r', log)

    def test_no_model_preserves_answers(self):
        result = self.run_installer('--non-interactive')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn('/model select', self.log.read_text())

    def test_interactive_terminal(self):
        master, slave = pty.openpty()
        try:
            process = subprocess.Popen(['bash', str(ROOT / 'scripts/pisugar-power-manager.sh'),
                                        '--interactive'], stdin=slave, stdout=slave,
                                       stderr=subprocess.PIPE, env=self.env)
            _, stderr = process.communicate(timeout=30)
            self.assertEqual(process.returncode, 0, stderr)
            self.assertIn('apt:dialog:', self.log.read_text())
        finally:
            os.close(slave)
            os.close(master)

    def test_type_and_rpm_build(self):
        result = self.run_installer('-t', 'rpm', '--rpm-build', '7')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('2.4.0-7.aarch64.rpm', self.log.read_text())

    def test_version_suffix_after_override(self):
        result = self.run_installer('-v', '1.7.5')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('pisugar-server_1.7.5_arm64.deb', self.log.read_text())

    def test_reject_invalid_options_before_changes(self):
        for args in [('--model', 'unknown'), ('--type', 'zip'), ('--channel', 'bad'),
                     ('--rpm-build', 'no'), ('--unknown',), ('--version',), ('extra',), ('--interactive',)]:
            result = self.run_installer(*args)
            self.assertNotEqual(result.returncode, 0, args)
        self.assertFalse(self.log.exists())

    def test_failures_propagate(self):
        result = self.run_installer(DOWNLOAD_FAIL='1')
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn('apt:', self.log.read_text())
        result = self.run_installer(APT_FAIL='1')
        self.assertNotEqual(result.returncode, 0)
        result = self.run_installer(APT_PACKAGE_FAIL='1')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('sudo dpkg --configure', result.stderr)

    def postinst(self, password, broken=False):
        config = self.base / 'config.json'
        config.write_text('broken' if broken else '{"auth_user":"old","auth_password":"old","keep":42}')
        confmodule = self.base / 'confmodule'
        confmodule.write_text('''db_get() {
 case "$1" in
 */model) RET="PiSugar 3";; */address) RET=127.0.0.1;;
 */auth-username) RET=$TEST_USER;; */auth-password) RET=$TEST_PASSWORD;; *) RET=true;;
 esac
}
db_set() { echo "set:$*" >> "$LOG"; }
db_input() { echo unexpected-dialog >&2; return 30; }
db_go() { return 30; }
''')
        self.stub('deb-systemd-helper', 'exit 0')
        self.stub('deb-systemd-invoke', 'exit 0')
        # macOS lacks GNU --reference; preserve production commands in Linux tests.
        if os.uname().sysname == 'Darwin':
            self.stub('chmod', '[ "${1#--reference=}" != "$1" ] || /bin/chmod "$@"')
            self.stub('chown', 'exit 0')
        source = (ROOT / 'pisugar-server/debian/postinst').read_text()
        source = source.replace('/usr/share/debconf/confmodule', str(confmodule))
        source = source.replace('/etc/default/pisugar-server', str(self.base / 'default'))
        source = source.replace('/etc/pisugar-server/config.json', str(config))
        script = self.base / 'postinst'
        script.write_text(source)
        result = subprocess.run(['sh', str(script), 'configure'], capture_output=True, text=True,
                                env=dict(self.env, TEST_USER='u"\\ser', TEST_PASSWORD=password))
        return result, config

    def test_auth_escaping(self):
        password = 'secret"\\\n$() unicode 密码'
        result, config = self.postinst(password)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(config.read_text()),
                         {'auth_user': 'u"\\ser', 'auth_password': password, 'keep': 42})
        self.assertNotIn('unexpected-dialog', result.stderr)
        self.assertIn('set:pisugar-server/auth-password ', self.log.read_text())

    def test_blank_password_preserves_auth(self):
        result, config = self.postinst('')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(config.read_text())['auth_password'], 'old')

    def test_bad_json_is_not_truncated(self):
        result, config = self.postinst('secret', broken=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(config.read_text(), 'broken')
        self.assertFalse(self.log.exists())
        self.assertEqual(list(self.base.glob('config.json.*')), [])

    def test_cancelled_config_continues_with_defaults(self):
        confmodule = self.base / 'confmodule'
        confmodule.write_text('db_input() { return 30; }; db_go() { return 30; }\n')
        for package in ('pisugar-server', 'pisugar-poweroff'):
            source = (ROOT / package / 'debian/config').read_text()
            source = source.replace('/usr/share/debconf/confmodule', str(confmodule))
            script = self.base / 'config'
            script.write_text(source)
            result = subprocess.run(['sh', '-e', str(script)], capture_output=True)
            self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == '__main__':
    unittest.main()
