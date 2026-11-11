#!/usr/bin/env python3
"""Exercise the real packaged launcher without a desktop session or network."""
import fcntl
import os
from pathlib import Path
import subprocess
import tempfile
import tomllib
import unittest


ROOT = Path(__file__).resolve().parents[2]
PACKAGE = ROOT / ".local/agent-package/1.5.3+relivo.20261111.1/relivo-agent-1.5.3+relivo.20261111.1-linux-amd64"


class PackagedLauncherTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="relivo-package-test-")
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        self.profile = self.directory / "profile"
        self.environment = {**os.environ, "RELIVO_PROFILE_DIR": str(self.profile), "PYTHONDONTWRITEBYTECODE": "1"}

    def run_agent(self, *args):
        return subprocess.run([str(PACKAGE / "relivo-agent"), *args],
                              env=self.environment, capture_output=True, text=True, timeout=20)

    def test_native_version_with_fresh_managed_profile(self):
        result = self.run_agent("--version")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("1.5.3", result.stdout)
        config = self.profile / "config/rustdesk/RustDesk2.toml"
        options = tomllib.loads(config.read_text())["options"]
        self.assertEqual(options["management-required"], "Y")
        self.assertEqual(options["api-server"], "https://relivo-desk.duckdns.org")
        self.assertNotIn("management-device-token", options)
        self.assertEqual(config.stat().st_mode & 0o777, 0o600)

    def test_provisioning_preserves_identity_and_other_settings(self):
        self.assertEqual(self.run_agent("--prepare-only").returncode, 0)
        identity = self.profile / "config/rustdesk/RustDesk.toml"
        identity.write_text('id = "local-test-identity"\n')
        config = self.profile / "config/rustdesk/RustDesk2.toml"
        with config.open("a") as stream:
            stream.write('unrelated-option = "preserved"\n\n[test_table]\nvalue = 42\n')
        credential = self.directory / "credential"
        credential.write_text("a" * 64)
        result = self.run_agent("--enrollment-file", str(credential))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn("a" * 64, result.stdout + result.stderr)
        actual = tomllib.loads(config.read_text())
        self.assertEqual(actual["options"]["management-device-token"], "a" * 64)
        self.assertEqual(actual["options"]["unrelated-option"], "preserved")
        self.assertEqual(actual["test_table"]["value"], 42)
        self.assertEqual(identity.read_text(), 'id = "local-test-identity"\n')
        before = config.read_bytes()
        credential.write_text("invalid")
        self.assertNotEqual(self.run_agent("--enrollment-file", str(credential)).returncode, 0)
        self.assertEqual(config.read_bytes(), before)

    def test_active_profile_and_foreign_server_are_rejected(self):
        self.assertEqual(self.run_agent("--prepare-only").returncode, 0)
        with (self.profile / "profile.lock").open("a") as lock:
            fcntl.flock(lock, fcntl.LOCK_SH)
            result = self.run_agent("--prepare-only")
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("Close all instances", result.stderr)
        config = self.profile / "config/rustdesk/RustDesk2.toml"
        config.write_text(config.read_text().replace("https://relivo-desk.duckdns.org", "https://example.invalid"))
        before = config.read_bytes()
        result = self.run_agent("--version")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(config.read_bytes(), before)


if __name__ == "__main__":
    unittest.main()
