"""Exercise the real installers against local release archives; never contact GitHub."""
import hashlib
import io
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
TARGETS = ["x86_64-unknown-linux-gnu", "x86_64-apple-darwin", "aarch64-apple-darwin"]


@unittest.skipIf(os.name == "nt", "Unix installer requires Bash and Unix filesystem semantics")
class UnixInstaller(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="timer install ")
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.home = self.base / "home with spaces"
        self.home.mkdir()
        self.tools = self.base / "tools"
        self.tools.mkdir()
        self.assets = self.base / "assets"
        self.assets.mkdir()
        self.env = dict(os.environ, HOME=str(self.home), PATH=f"{self.tools}:{os.environ['PATH']}",
                        XDG_DATA_HOME=str(self.home / "data"), FIXTURE_ROOT=str(self.assets),
                        REAL_CURL=shutil.which("curl"), TEST_OS="Linux", TEST_ARCH="x86_64", TEST_ROSETTA="0")
        self.tool("uname", '#!/bin/sh\nif [ "$1" = -s ]; then echo "$TEST_OS"; else echo "$TEST_ARCH"; fi\n')
        self.tool("sysctl", '#!/bin/sh\necho "$TEST_ROSETTA"\n')
        self.tool("getconf", '#!/bin/sh\necho "${TEST_LIBC:-glibc 2.39}"\n')
        self.tool("curl", f'''#!{shutil.which("python3")}
import os, pathlib, subprocess, sys
args = sys.argv[1:]
url = next(a for a in args if a.startswith("https://"))
if url.endswith("/releases/latest"):
    print("https://github.com/MSpiechowicz/useful-timer/releases/tag/v1.2.3", end="")
    sys.exit(0)
source = pathlib.Path(os.environ["FIXTURE_ROOT"]) / url.rsplit("/", 1)[1]
args[args.index(url)] = source.as_uri()
args[args.index("=https")] = "=file"
sys.exit(subprocess.call([os.environ["REAL_CURL"], *args]))
''')
        for target in TARGETS:
            self.archive(target)

    def tool(self, name, content):
        path = self.tools / name
        path.write_text(content)
        path.chmod(0o755)

    def archive(self, target, version="1.2.3", label="first", omit=None):
        path = self.assets / f"useful-timer-{version}-{target}.tar.gz"
        files = {"useful-timer": f"#!/bin/sh\nprintf '%s\\n' '{label} {target}'\n".encode(),
                 "LICENSE": b"test license", "README.md": b"test instructions", "useful-timer.png": b"test icon"}
        with tarfile.open(path, "w:gz") as archive:
            for name, data in files.items():
                if name == omit:
                    continue
                info = tarfile.TarInfo(name)
                info.size = len(data)
                info.mode = 0o755 if name == "useful-timer" else 0o644
                archive.addfile(info, io.BytesIO(data))
        path.with_name(path.name + ".sha256").write_text(f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.name}\n")
        return path

    def install(self, *args, success=True):
        # Pipe form exercises the documented curl | bash entry point.
        result = subprocess.run(["bash", "-s", "--", *args], input=(ROOT / "install.sh").read_text(),
                                env=self.env, text=True, capture_output=True)
        if success:
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        else:
            self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        return result

    def test_install_launch_and_upgrade_all_unix_targets(self):
        for system, arch, rosetta, target in [("Linux", "x86_64", "0", TARGETS[0]),
                                            ("Darwin", "x86_64", "0", TARGETS[1]),
                                            ("Darwin", "arm64", "0", TARGETS[2]),
                                            ("Darwin", "x86_64", "1", TARGETS[2])]:
            with self.subTest(system=system, arch=arch, rosetta=rosetta):
                self.env.update(TEST_OS=system, TEST_ARCH=arch, TEST_ROSETTA=rosetta)
                self.archive(target)
                self.install()
                binary = self.home / ".local/bin/useful-timer"
                self.assertEqual(subprocess.check_output([binary], text=True).strip(), f"first {target}")
                if system == "Darwin":
                    bundle = self.home / "Applications/Useful Timer.app/Contents"
                    metadata = plistlib.loads((bundle / "Info.plist").read_bytes())
                    self.assertEqual(metadata["CFBundleShortVersionString"], "1.2.3")
                    self.assertEqual((bundle / "MacOS" / metadata["CFBundleExecutable"]).resolve(), binary.resolve())
                else:
                    launcher = (self.home / "data/applications/useful-timer.desktop").read_text()
                    self.assertIn(f'Exec="{binary}"', launcher)
                    self.assertTrue((self.home / "data/useful-timer/useful-timer.png").is_file())
                self.archive(target, "2.0.0", label="upgraded")
                self.install("--version", "v2.0.0")
                self.assertEqual(subprocess.check_output([binary], text=True).strip(), f"upgraded {target}")
                # Remove only this fixture's symlink so the next Linux case cannot overwrite its target.
                if binary.is_symlink():
                    binary.unlink()

    def test_failed_download_or_verification_preserves_existing_installation(self):
        self.install()
        binary = self.home / ".local/bin/useful-timer"
        before = binary.read_bytes()
        archive = self.archive(TARGETS[0], "2.0.0")
        checksum = archive.with_name(archive.name + ".sha256")
        for failure in ["corruption", "wrong-name", "missing-checksum", "missing-archive", "missing-executable"]:
            with self.subTest(failure=failure):
                archive = self.archive(TARGETS[0], "2.0.0")
                if failure == "corruption":
                    archive.write_bytes(b"corrupted")
                elif failure == "wrong-name":
                    checksum.write_text("0" * 64 + "  another-archive.tar.gz\n")
                elif failure == "missing-checksum":
                    checksum.unlink()
                elif failure == "missing-archive":
                    archive.unlink()
                else:
                    self.archive(TARGETS[0], "2.0.0", omit="useful-timer")
                self.install("--version", "2.0.0", success=False)
                self.assertEqual(binary.read_bytes(), before)
                self.assertEqual(subprocess.check_output([binary], text=True).strip(), f"first {TARGETS[0]}")

    def test_custom_bin_directory_and_relative_xdg_fallback(self):
        self.env["XDG_DATA_HOME"] = "relative-is-not-valid"
        directory = self.home / "custom bin"
        self.install("--bin-dir", str(directory))
        self.assertEqual(subprocess.check_output([directory / "useful-timer"], text=True).strip(), f"first {TARGETS[0]}")
        self.assertTrue((self.home / ".local/share/applications/useful-timer.desktop").is_file())

    def test_unsupported_libc_does_not_install(self):
        for libc in ["glibc 2.38", "musl", ""]:
            with self.subTest(libc=libc):
                self.env["TEST_LIBC"] = libc if libc else "unknown"
                self.install(success=False)
                self.assertFalse((self.home / ".local/bin/useful-timer").exists())

    def test_unsupported_platforms_and_invalid_options_do_not_install(self):
        for system, arch in [("Linux", "aarch64"), ("MINGW64_NT", "x86_64")]:
            self.env.update(TEST_OS=system, TEST_ARCH=arch)
            self.install(success=False)
        self.env.update(TEST_OS="Linux", TEST_ARCH="x86_64")
        for args in [("--version", "../evil"), ("--version",), ("--bin-dir", "relative"), ("--unknown",)]:
            self.install(*args, success=False)
        self.assertFalse((self.home / ".local/bin/useful-timer").exists())


if __name__ == "__main__":
    unittest.main(verbosity=2)
