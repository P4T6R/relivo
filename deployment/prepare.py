#!/usr/bin/env python3
"""Prepare private Docker configuration. Never connects to or changes a server."""
import argparse
import ipaddress
import os
from pathlib import Path
import re
import secrets

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--domain", help="Public DNS name pointing to the VPS")
parser.add_argument("--tag", default="2026-11-11.2")
parser.add_argument("--output", type=Path, default=Path(__file__).parent / ".env")
parser.add_argument("--test", action="store_true", help="Isolated localhost TLS test ports")
args = parser.parse_args()
os.umask(0o077)
if not re.fullmatch(r"[A-Za-z0-9_][A-Za-z0-9_.-]{0,127}", args.tag) or args.tag == "latest":
    parser.error("Use an explicit, valid image release tag")
domain = "localhost" if args.test else (args.domain or "").lower()
if not args.test:
    try:
        ipaddress.ip_address(domain)
        parser.error("Use a public DNS name for this certificate configuration")
    except ValueError:
        pass
    labels = domain.split(".")
    if len(labels) < 2 or len(domain) > 253 or any(
        not re.fullmatch(r"[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?", label)
        for label in labels
    ) or domain.endswith((".invalid", ".localhost", ".local", "example.com", "example.org", "example.net")):
        parser.error("Provide your real public DNS name with --domain")
output = args.output.absolute()
if output.is_symlink():
    parser.error("Refusing a symlink configuration file")
output.parent.mkdir(parents=True, exist_ok=True)
runtime = output.parent / "runtime"
if runtime.is_symlink():
    parser.error("Refusing a symlink secret directory")
runtime.mkdir(mode=0o700, exist_ok=True)
runtime.chmod(0o700)

def create_private(path, value, mode=0o600):
    try:
        fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, mode)
    except FileExistsError:
        if path.is_symlink() or not path.is_file():
            parser.error(f"Refusing unexpected file type: {path}")
        return
    os.fchmod(fd, mode)
    with os.fdopen(fd, "w") as stream:
        stream.write(value + "\n")

# Compose file-backed secrets keep host ownership. The private parent directory
# prevents host-user access; 0444 permits the explicitly mounted non-root reader.
create_private(runtime / "internal-key", secrets.token_hex(32), 0o444)
create_private(runtime / "admin-password", secrets.token_urlsafe(24))
suffix = ":24443" if args.test else ""
values = {
    "DOMAIN": domain,
    "PUBLIC_ORIGIN": f"https://{domain}{suffix}",
    "RELAY_URL": f"wss://{domain}{suffix}/ws/relay",
    "IMAGE_TAG": args.tag,
    "RUNTIME_DIR": str(runtime),
    "PUBLIC_BIND": "127.0.0.1" if args.test else "0.0.0.0",
    "HTTP_PORT": "24080" if args.test else "80",
    "HTTPS_PORT": "24443" if args.test else "443",
    "NAT_PORT": "24115" if args.test else "21115",
    "ID_PORT": "24116" if args.test else "21116",
}
# Single-quoted Compose values preserve literal dollar signs and spaces in paths.
if any("'" in value or "\n" in value or "\r" in value for value in values.values()):
    parser.error("Configuration values must not contain quotes or newlines")
content = "\n".join(f"{key}='{value}'" for key, value in values.items())
if output.exists() and output.read_text().strip() != content:
    parser.error("Configuration already exists with different values; review it manually")
create_private(output, content)
print(f"Configuration: {output}")
print(f"Administrator: admin; password file: {runtime / 'admin-password'}")
print("Existing credentials were preserved. No services were started.")
