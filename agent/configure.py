#!/usr/bin/env python3
"""Create a fresh Relivo native profile; never enroll or approve a device."""
import argparse
import base64
import json
import os
from pathlib import Path
import re


def options_from_profile(profile, credential=None):
    if profile.get("schema") != 1 or profile.get("managed_required") is not True:
        raise ValueError("Expected a version 1 managed server profile")
    if profile.get("api_origin") != "https://relivo-desk.duckdns.org":
        raise ValueError("Unexpected Relivo API origin")
    if profile.get("id_server") != "relivo-desk.duckdns.org:21116":
        raise ValueError("Unexpected Relivo ID server")
    if profile.get("relay_url") != "wss://relivo-desk.duckdns.org/ws/relay":
        raise ValueError("Unexpected Relivo relay URL")
    key = profile.get("server_public_key", "")
    if len(base64.b64decode(key, validate=True)) != 32:
        raise ValueError("Invalid server public key")
    options = {
        "management-required": "Y",
        "api-server": profile["api_origin"],
        "custom-rendezvous-server": profile["id_server"],
        "relay-server": profile["relay_url"],
        "key": key,
        "allow-websocket": "N",
        "direct-server": "N",
        "allow-auto-update": "N",
    }
    if credential is not None:
        if not re.fullmatch(r"[a-f0-9]{64}", credential):
            raise ValueError("Expected the unique 64-character device enrollment credential")
        options["management-device-token"] = credential
    return options


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--server-profile", type=Path, default=Path(__file__).with_name("server-profile.json"))
    parser.add_argument("--config-file", type=Path, required=True)
    parser.add_argument("--credential-file", type=Path)
    args = parser.parse_args()
    os.umask(0o077)
    try:
        profile = json.loads(args.server_profile.read_text())
        credential = args.credential_file.read_text().strip() if args.credential_file else None
        options = options_from_profile(profile, credential)
        destination = args.config_file.absolute()
        if any(parent.is_symlink() for parent in [destination, *destination.parents]):
            raise ValueError("Refusing a symlink configuration path")
        content = "[options]\n" + "".join(f"{key} = {json.dumps(value)}\n" for key, value in options.items())
        destination.parent.mkdir(parents=True, mode=0o700, exist_ok=True)
        with destination.open("x", encoding="utf-8") as stream:
            stream.write(content)
        print(f"Fresh native configuration created: {destination}")
        print("No existing profile was overwritten; no device was registered or approved.")
    except (OSError, ValueError, KeyError, TypeError) as error:
        parser.exit(1, f"Configuration was not applied: {error}\n")


if __name__ == "__main__":
    main()
