#!/usr/bin/env python3
"""Package the compiled Relivo Linux bundle and its matching fork sources."""
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tarfile


ROOT = Path(__file__).resolve().parents[2]
VERSION = "1.5.3+relivo.20261111.1"
NAME = f"relivo-agent-{VERSION}-linux-amd64"
OUT = ROOT / "artifacts/agents" / VERSION
STAGE = ROOT / ".local/agent-package" / VERSION
BUILDER = "relivo/agent-linux-builder:2026-11-11"


def digest(file):
    with file.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def archive(directory, destination):
    with tarfile.open(destination, "w:gz") as tar:
        tar.add(directory, arcname=directory.name)


def main():
    bundle = ROOT / "rustdesk/flutter/build/linux/x64/release/bundle"
    for required in ("rustdesk", "lib/librustdesk.so", "lib/libapp.so", "lib/libvpx.so.7", "lib/libyuv.so.0"):
        if not (bundle / required).is_file():
            raise SystemExit(f"Missing compiled runtime: {required}")
    if STAGE.exists() or OUT.exists():
        raise SystemExit("Output already exists. Select a new release version or remove only the previous packaging output.")
    OUT.mkdir(parents=True)
    portable = STAGE / NAME
    shutil.copytree(bundle, portable / "native")
    for source, target in (
        ("agent/package/linux/relivo-agent", "relivo-agent"),
        ("agent/configure.py", "configure.py"),
        ("agent/server-profile.json", "server-profile.json"),
        ("agent/package/README-linux.md", "README.md"),
        ("rustdesk/LICENCE", "LICENCE"),
        ("rustdesk/res/scalable.svg", "relivo-agent.svg"),
    ):
        shutil.copyfile(ROOT / source, portable / target)
    (portable / "relivo-agent").chmod(0o755)

    dependency_script = '''
set -eu
mkdir -p /tmp/relivo-shlibs/debian
cd /tmp/relivo-shlibs
printf 'Source: relivo-agent\nSection: net\nPriority: optional\nMaintainer: Relivo\n\nPackage: relivo-agent\nArchitecture: amd64\nDescription: Relivo Agent\n' > debian/control
dpkg-shlibdeps -O --ignore-missing-info -xlibvpx7 -xlibyuv0 -xlibjpeg62-turbo -l/bundle/lib /bundle/rustdesk /bundle/lib/*.so /bundle/lib/libvpx.so.7 /bundle/lib/libyuv.so.0 /bundle/lib/libjpeg.so.62
'''
    result = subprocess.run([
        "docker", "run", "--rm", "--network", "none", "--user", "1000:1000",
        "--mount", f"type=bind,src={bundle},dst=/bundle,readonly",
        BUILDER, "bash", "-c", dependency_script,
    ], capture_output=True, text=True, check=True)
    (STAGE / "dependency-analysis.log").write_text(result.stderr)
    dependencies = result.stdout.strip().removeprefix("shlibs:Depends=")
    if "libc6 (" not in dependencies or "\n" in dependencies:
        raise SystemExit("Unexpected dependency analysis output")
    dependencies += (", python3 (>= 3.11), libxdo3 | libxdo4, libxcb-shape0, libxcb-xfixes0, "
                     "libasound2t64 | libasound2, libsystemd0, libayatana-appindicator3-1")
    (portable / "dependencies.txt").write_text(dependencies + "\n")
    (portable / "source.txt").write_text(f"Matching source: relivo-agent-{VERSION}-source.tar.gz\n")
    archive(portable, OUT / f"{NAME}.tar.gz")

    debroot = STAGE / "debian"
    shutil.copytree(portable, debroot / "opt/relivo-agent")
    for directory in ("DEBIAN", "usr/bin", "usr/share/applications", "usr/share/icons/hicolor/scalable/apps", "usr/share/doc/relivo-agent"):
        (debroot / directory).mkdir(parents=True, exist_ok=True)
    (debroot / "usr/bin/relivo-agent").symlink_to("/opt/relivo-agent/relivo-agent")
    shutil.copyfile(portable / "relivo-agent.svg", debroot / "usr/share/icons/hicolor/scalable/apps/relivo-agent.svg")
    shutil.copyfile(portable / "README.md", debroot / "usr/share/doc/relivo-agent/README.md")
    shutil.copyfile(portable / "LICENCE", debroot / "usr/share/doc/relivo-agent/copyright")
    desktop = debroot / "usr/share/applications/relivo-agent.desktop"
    desktop.write_text("""[Desktop Entry]
Type=Application
Name=Relivo Agent
Comment=Remote support through Relivo Console
Exec=/usr/bin/relivo-agent %u
Icon=relivo-agent
Terminal=false
Categories=Network;RemoteAccess;
StartupWMClass=rustdesk
MimeType=x-scheme-handler/rustdesk;
""")
    subprocess.run(["desktop-file-validate", str(desktop)], check=True)
    installed_size = sum(file.stat().st_size for file in debroot.rglob("*") if file.is_file() and not file.is_symlink()) // 1024
    (debroot / "DEBIAN/control").write_text(
        f"Package: relivo-agent\nVersion: {VERSION}\nArchitecture: amd64\n"
        "Section: net\nPriority: optional\nMaintainer: Relivo Project <relivo@localhost>\n"
        f"Installed-Size: {installed_size}\nDepends: {dependencies}\n"
        "Recommends: gstreamer1.0-pipewire\nHomepage: https://relivo-desk.duckdns.org\n"
        "Description: Relivo managed remote support agent\n"
        " Native desktop client with per-device enrollment and console authorization.\n")
    deb = OUT / f"relivo-agent_{VERSION}_amd64.deb"
    subprocess.run(["dpkg-deb", "--build", "--root-owner-group", "-Zgzip", str(debroot), str(deb)], check=True)

    names = subprocess.check_output([
        "git", "ls-files", "-z", "--cached", "--others", "--exclude-standard", "--",
        "rustdesk", "agent", "docs/agent-delivery.md", "docs/branding.md",
    ], cwd=ROOT).decode().split("\0")
    names += ["rustdesk/" + generated for generated in (
        "src/bridge_generated.rs", "src/bridge_generated.io.rs",
        "flutter/lib/generated_bridge.dart", "flutter/lib/generated_bridge.freezed.dart",
        "flutter/macos/Runner/bridge_generated.h", "flutter/ios/Runner/bridge_generated.h",
    )]
    source_manifest = {}
    source = OUT / f"relivo-agent-{VERSION}-source.tar.gz"
    with tarfile.open(source, "w:gz") as tar:
        for name in sorted(set(filter(None, names))):
            file = ROOT / name
            if not file.is_file():
                continue
            if any(part in (".git", ".local", "target", "build", "__pycache__", ".dart_tool", "node_modules") for part in file.relative_to(ROOT).parts):
                # agent/build contains the required build instructions, not generated output.
                if not name.startswith("agent/build/"):
                    continue
            if file.is_symlink():
                raise SystemExit(f"Review source symlink before packaging: {name}")
            source_manifest[name] = digest(file)
            tar.add(file, arcname=f"relivo-agent-source/{name}", recursive=False)
    (OUT / "source-manifest.json").write_text(json.dumps(source_manifest, indent=2) + "\n")
    (OUT / "release.json").write_text(json.dumps({
        "product": "Relivo Agent", "package_version": VERSION, "native_version": "1.5.3",
        "linux_amd64": {"status": "packaged", "builder": BUILDER,
            "native_sha256": {item: digest(bundle / item) for item in ("rustdesk", "lib/librustdesk.so", "lib/libapp.so")}},
        "windows": {"status": "pending_native_builder"},
        "macos": {"status": "pending_native_builder"},
        "remote_desktop_session_validated": False,
        "contains_device_credentials": False,
    }, indent=2) + "\n")
    (OUT / "SHA256SUMS").write_text("".join(
        f"{digest(file)}  {file.name}\n" for file in sorted(OUT.iterdir()) if file.is_file()))
    print(f"Linux packages and source written to {OUT}")


if __name__ == "__main__":
    main()
