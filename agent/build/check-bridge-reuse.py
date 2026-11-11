#!/usr/bin/env python3
"""Allow CI bridge reuse only when its complete client source tree matches."""
import json
import os
import subprocess
import sys
import urllib.request

run_id = sys.argv[1]
if not run_id.isdecimal():
    raise SystemExit("A numeric workflow run ID is required")
repository = os.environ["GITHUB_REPOSITORY"]
request = urllib.request.Request(
    f"https://api.github.com/repos/{repository}/actions/runs/{run_id}",
    headers={"Authorization": f"Bearer {os.environ['GH_TOKEN']}",
             "Accept": "application/vnd.github+json", "User-Agent": "Relivo-build"},
)
with urllib.request.urlopen(request, timeout=30) as response:
    run = json.load(response)
if (run["head_repository"]["full_name"] != repository
        or run["path"] != ".github/workflows/agent-build.yml"):
    raise SystemExit("Bridge artifacts must come from this repository's agent workflow")
subprocess.run(["git", "diff", "--exit-code", run["head_sha"], "HEAD", "--", "rustdesk"], check=True)
print(f"Bridge source verified: {run['head_sha']}")
