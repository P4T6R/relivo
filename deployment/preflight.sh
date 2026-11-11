#!/usr/bin/env bash
# Read-only preflight; run this yourself on the target VPS.
set -euo pipefail
uname -m
cat /etc/os-release
free -h
df -h / /var/lib/docker 2>/dev/null || df -h /
if command -v docker >/dev/null; then
  docker version --format 'Docker client {{.Client.Version}}; server {{.Server.Version}}'
  docker compose version
  docker ps --format 'table {{.Names}}\t{{.Ports}}'
else
  echo 'Docker is not installed. Install Docker Engine and the Compose plugin before deployment.'
fi
ss -lntu
