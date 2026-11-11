#!/usr/bin/env python3
"""Install the exact official Flutter SDK used by a native build."""
import hashlib
import os
from pathlib import Path
import platform
import subprocess
import sys
import urllib.request

SDK = {
    ("Windows", "x64"): ("3.24.5", "flutter_windows_3.24.5-stable.zip", "b8a7485acd3c6fb23a76b7ac09f89e8d93d62fbff7147c6f5f8c5686d949eeac"),
    ("Darwin", "x64"): ("3.24.5", "flutter_macos_3.24.5-stable.zip", "30f337aeb2f7a0d324cfae9a3718c1caa8a5abcb19eff933b337332465f9cfe8"),
    ("Darwin", "arm64"): ("3.44.9", "flutter_macos_arm64_3.44.9-stable.zip", "9b5849dec51d5f6f9ee55f5e8fb67d07236369e2f5ab4c834a9a81d96f826dc0"),
}
system = platform.system()
version, archive, expected = SDK[(system, sys.argv[1])]
destination = Path(os.environ["RUNNER_TEMP"]) / "relivo-flutter"
destination.mkdir(exist_ok=True)
download = destination / archive
family = "windows" if system == "Windows" else "macos"
url = f"https://storage.googleapis.com/flutter_infra_release/releases/stable/{family}/{archive}"
urllib.request.urlretrieve(url, download)
with download.open("rb") as source:
    actual = hashlib.file_digest(source, "sha256").hexdigest()
if actual != expected:
    raise SystemExit("Flutter SDK checksum mismatch")
if system == "Windows":
    subprocess.run(["7z", "x", "-y", str(download), f"-o{destination}"], check=True, stdout=subprocess.DEVNULL)
else:
    subprocess.run(["unzip", "-q", str(download), "-d", str(destination)], check=True)
download.unlink()
with open(os.environ["GITHUB_PATH"], "a") as output:
    output.write(str(destination / "flutter" / "bin") + "\n")
print(f"Verified and installed Flutter {version} for {system}/{sys.argv[1]}")
