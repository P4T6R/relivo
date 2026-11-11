#!/usr/bin/env python3
"""Build and package Windows/macOS agents on their native CI runners."""
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "rustdesk"
OUTPUT = ROOT / "dist"
VERSION = "1.5.3-relivo.20261111.2"
ARCH = sys.argv[1]
WINDOWS = platform.system() == "Windows"


def run(*args, cwd=SOURCE, **kwargs):
    print("+", " ".join(map(str, args)), flush=True)
    return subprocess.run(list(map(str, args)), cwd=cwd, check=True, **kwargs)


if not WINDOWS and ARCH == "arm64":
    run("bash", ".github/patches/apply_flutter_3.44_source_patches.sh")
    run("flutter", "config", "--no-enable-swift-package-manager")
    replacements = {
        "flutter/macos/Podfile": (r"platform :osx, '[^']+'", "platform :osx, '12.3'"),
        "Cargo.toml": (r'osx_minimum_system_version = "[^"]+"', 'osx_minimum_system_version = "12.3"'),
        "flutter/macos/Runner.xcodeproj/project.pbxproj": (r"MACOSX_DEPLOYMENT_TARGET = [0-9.]+;", "MACOSX_DEPLOYMENT_TARGET = 12.3;"),
    }
    for name, (pattern, replacement) in replacements.items():
        path = SOURCE / name
        updated, count = re.subn(pattern, replacement, path.read_text())
        if not count:
            raise SystemExit(f"Missing macOS deployment target in {name}")
        path.write_text(updated)
if not WINDOWS:
    os.environ["MACOSX_DEPLOYMENT_TARGET"] = "12.3" if ARCH == "arm64" else "10.14"

features = "flutter,relivo-managed" + (",screencapturekit" if not WINDOWS and ARCH == "arm64" else "")
run("cargo", "build", "--locked", "--release", *(["--lib"] if WINDOWS else []), "--features", features)
run(sys.executable, "build.py", "--flutter", "--skip-cargo", *(["--portable", "--skip-portable-pack"] if WINDOWS else []))
run("cargo", "test", "--locked", "--release", "--lib", "--features", features, "management::package::tests")
OUTPUT.mkdir(exist_ok=True)
prefix = f"relivo-agent-{VERSION}-{'windows' if WINDOWS else 'macos'}-{ARCH}"
if WINDOWS:
    bundle = SOURCE / "flutter/build/windows/x64/runner/Release"
    # Keep an unpacked ZIP as well: the enrollment command runs directly there.
    executable = bundle / "rustdesk.exe"
else:
    bundle = SOURCE / "flutter/build/macos/Build/Products/Release/RustDesk.app"
    executable = bundle / "Contents/MacOS/RustDesk"
    if not executable.exists():
        executable = bundle / "Contents/MacOS/rustdesk"
result = run(executable, "--relivo-server-profile", cwd=executable.parent, capture_output=True, text=True, timeout=30)
profile = json.loads(result.stdout.strip())
if profile.get("api-server") != "https://relivo-desk.duckdns.org" or profile.get("management-required") != "Y" or "management-device-token" in profile:
    raise SystemExit("Packaged native server profile failed verification")

if WINDOWS:
    shutil.copy2(SOURCE / "LICENCE", bundle / "LICENSE.txt")
    shutil.copy2(ROOT / "agent/package/README-native.md", bundle / "README.md")
    shutil.make_archive(str(OUTPUT / prefix), "zip", bundle)
    run(sys.executable, "-m", "pip", "install", "-r", "requirements.txt", cwd=SOURCE / "libs/portable")
    run(sys.executable, "generate.py", "-f", bundle, "-o", ".", "-e", executable, "-l", "6", cwd=SOURCE / "libs/portable")
    shutil.copy2(SOURCE / "target/release/rustdesk-portable-packer.exe", OUTPUT / f"{prefix}-install.exe")
else:
    run("codesign", "--force", "--deep", "--sign", "-", bundle)
    run("codesign", "--verify", "--deep", "--strict", bundle)
    stage = Path(os.environ["RUNNER_TEMP"]) / "relivo-dmg"
    stage.mkdir()
    shutil.copytree(bundle, stage / bundle.name, symlinks=True)
    (stage / "Applications").symlink_to("/Applications")
    shutil.copy2(SOURCE / "LICENCE", stage / "LICENSE.txt")
    shutil.copy2(ROOT / "agent/package/README-native.md", stage / "README.md")
    run("hdiutil", "create", "-volname", "Relivo Agent", "-srcfolder", stage, "-ov", "-format", "UDZO", OUTPUT / f"{prefix}.dmg")
    run("hdiutil", "verify", OUTPUT / f"{prefix}.dmg")

commit = run("git", "rev-parse", "HEAD", capture_output=True, text=True).stdout.strip()
(OUTPUT / f"{prefix}.json").write_text(json.dumps({"version": VERSION, "source_commit": commit, "architecture": ARCH, "system": platform.system(), "server_profile": profile, "signing": "unsigned" if WINDOWS else "ad-hoc (not notarized)"}, indent=2) + "\n")
with (OUTPUT / f"{prefix}-SHA256SUMS").open("w") as checksums:
    for path in sorted(OUTPUT.iterdir()):
        if path.name.endswith("-SHA256SUMS"):
            continue
        with path.open("rb") as contents:
            checksums.write(f"{hashlib.file_digest(contents, 'sha256').hexdigest()}  {path.name}\n")
