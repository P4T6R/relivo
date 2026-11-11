# Relivo Agent for Linux amd64

This package contains the modified Relivo desktop client and public server
settings for https://relivo-desk.duckdns.org. It does not contain a machine
identity, enrollment credential, or administrator password.

## Install or run

On Debian 12 / Ubuntu 24.04 and compatible amd64 systems, install the `.deb`
using the graphical package installer or run the following command yourself:

```sh
sudo apt install ./relivo-agent_1.5.3+relivo.20261111.1_amd64.deb
relivo-agent
```

Alternatively extract the `.tar.gz`, enter its directory, then run
`./relivo-agent`. Keep the entire extracted directory together, including
`native/lib` and `native/data`. Python 3.11+ and the system libraries listed in
`dependencies.txt` are required. The binary requires glibc 2.34 or newer; that
ABI floor alone does not establish compatibility with every Linux distribution.

Always use `relivo-agent`, which configures the server before starting the
native application. The internal `native/rustdesk` executable retains its
technical upstream filename and must not be used as the entry point.

This is a per-user desktop application. Installation does not start a system
service or configure unattended access. The ordinary screen/input permission
and session-consent requirements still apply. The Debian package registers an
application entry and offers the rustdesk:// protocol handler; it does not
change your existing preferred handler automatically.

## Enroll each receiving machine

1. Start Relivo Agent and note its actual device ID.
2. Create that device in Relivo Console and save its unique credential in a
   private local text file. Never reuse one machine's credential on another.
3. Close all windows/processes of this packaged agent, including its tray item.
4. Run `relivo-agent --enrollment-file /absolute/path/device-credential` (or
   `./relivo-agent --enrollment-file ...` for the archive). The credential is
   read from the file, not passed as a command-line argument.
5. Restart the application, verify the reported identity in the console,
   approve the device, and assign its group and connect permissions.
6. Sign in with an authorized operator account to start a remote session.

Approval is mandatory. Installing the package or importing a credential does
not approve a device. Remove the temporary credential file after provisioning.

Profiles are stored at `~/.local/share/relivo-agent`, separately from an existing
RustDesk profile and from the developer workstation's test installation. For
isolated tests, set `RELIVO_PROFILE_DIR` to a separate absolute directory. Do
not copy profiles between machines. Profile validation fails if the managed
server settings were changed; it does not silently replace those settings.

## Version, source and validation

Package version: 1.5.3+relivo.20261111.1. The native version command still reports
1.5.3. The corresponding source archive is distributed beside these packages;
it includes the modified client, generated bridge, build scripts and lockfiles.
See LICENCE, native/share/licenses, and Flutter's bundled NOTICES.Z for notices.

Linux compilation, dependency loading, enrollment and console access checks
passed on the selected workstation. A real screen/control session is still an
acceptance requirement. Windows EXE and macOS DMG artifacts are separate native
builds; this Linux package cannot run on those operating systems.
