#!/usr/bin/env python3
"""Export already-tested local images and their corresponding backend sources."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import tarfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--tag", default="2026-11-11.2")
parser.add_argument("--release", help="Bundle version, when only deployment files changed")
args = parser.parse_args()
release = args.release or args.tag
for tag in (args.tag, release):
    if not re.fullmatch(r"[A-Za-z0-9_][A-Za-z0-9_.-]{0,127}", tag) or tag == "latest":
        parser.error("Use an explicit release tag")
root = Path(__file__).resolve().parent.parent
images = [f"relivo/{service}:{args.tag}" for service in ("management", "server", "edge")]
metadata = json.loads(subprocess.check_output(["docker", "image", "inspect", *images]))
destination = root / "artifacts" / f"relivo-{release}"
destination.mkdir(parents=True, exist_ok=False)
ignored = shutil.ignore_patterns(".git", "target", "node_modules", "dist", "__pycache__", ".env", "runtime", "backups", "data", "*.log", "*.tsbuildinfo", "*.sqlite3", "*.sqlite3-wal", "*.sqlite3-shm", "id_ed25519*")
for name in ("management", "rustdesk-server", "deployment", "docs", "scripts"):
    shutil.copytree(root / name, destination / name, ignore=ignored)
shutil.copy2(root / ".dockerignore", destination / ".dockerignore")
shutil.copy2(root / "deployment/README.md", destination / "README.md")
subprocess.run(["docker", "image", "save", "--output", str(destination / "images.tar"), *images], check=True)
manifest = {
    "release": release,
    "image_release": args.tag,
    "created_at": datetime.now(timezone.utc).isoformat(),
    "target": "Ubuntu VPS 72.62.178.54; relivo-desk.duckdns.org behind existing host Nginx; see deployment/nginx/README.md",
    "images": [{"tags": i["RepoTags"], "id": i["Id"], "os": i["Os"], "architecture": i["Architecture"]} for i in metadata],
    "scope": "Backend, console, proxy and corresponding sources; native client build is not included",
}
(destination / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
checksums = []
for path in sorted(destination.rglob("*")):
    if path.is_file():
        with path.open("rb") as stream:
            digest = hashlib.file_digest(stream, "sha256").hexdigest()
        checksums.append(f"{digest}  {path.relative_to(destination)}")
(destination / "SHA256SUMS").write_text("\n".join(checksums) + "\n")
archive = destination.parent / (destination.name + ".tar.gz")
with tarfile.open(archive, "w:gz", compresslevel=3) as tar:
    tar.add(destination, arcname=destination.name)
with archive.open("rb") as stream:
    digest = hashlib.file_digest(stream, "sha256").hexdigest()
Path(str(archive) + ".sha256").write_text(f"{digest}  {archive.name}\n")
print(f"Release archive: {archive}")
print("Images and backend sources only. No runtime secrets or databases included.")
