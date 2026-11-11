# Relivo Agent preparation

Status: the modified Linux client is compiled and installed on the selected
workstation. Device 472537557 is enrolled, identity-verified and approved.
Native account/permission API checks pass; visual and remote-desktop session
acceptance is pending because the workstation session is locked. Windows and
macOS installers are not built yet. The upstream preview binary does not contain
Relivo's transport authorization and is not a Relivo Agent release.

The owner selected the current Linux workstation for the first installation.
The ordered delivery specification is in ../docs/agent-delivery.md.

## Downloadable Linux packages

`artifacts/agents/1.5.3+relivo.20261111.1/` contains the Linux amd64 `.deb`,
portable `.tar.gz`, matching source archive, release manifest and SHA256SUMS.
The package launcher prepares a separate per-user profile and accepts
`--enrollment-file` for that machine's credential. See
`package/README-linux.md` for installation and enrollment instructions.

Build the packages from the existing compiled bundle with
`python3 agent/package/build-linux.py`. The output directory must be fresh.
`python3 agent/package/test-linux.py` verifies the real staged launcher, native
version command, profile initialization, credential import and profile locking.
The three packaging tests pass. Installation and the native version command
also pass in a clean Ubuntu 24.04 container without production network access
during execution. These checks do not validate screen capture or input control.

The owner clarified that recipient machines are test endpoints, not available
build hosts, and authorized creation of a private `P4T6R/relivo` repository for
hosted Windows/macOS builds. Browser GitHub authentication is currently required
to create it. Do not report Windows/macOS binaries until those builds complete.

## Public configuration

server-profile.json contains the verified public domain, ID/relay endpoints and
server public key. It contains no administrator password or enrollment token.
configure.py creates a new RustDesk2.toml without overwriting any existing file:

```sh
python3 agent/configure.py \
  --config-file .local/relivo-agent/config/rustdesk/RustDesk2.toml
```

The Linux launcher must set XDG_CONFIG_HOME to the absolute
.local/relivo-agent/config directory for this first isolated installation. Do
not launch an ordinary upstream binary with this profile. Do not copy the
existing ~/.config/rustdesk identity into it. After the modified client runs,
use its actual device ID to create a pending device in the console. Its unique
credential belongs only to that receiving machine. Verify the reported public
key/UUID before approval and group assignment. Creating a configuration file
alone does not enroll a device; the first workstation has completed enrollment.

## Installed Linux workstation

Start **Relivo Agent** from the desktop application menu, or run
`bash agent/run-linux.sh` from this workspace. The launcher uses the stable
release under `.local/relivo-agent/current`, independently of the compiler's
build directory. The `rustdesk://` protocol is registered to this launcher.

The `support` account has the operator role and membership in the `Support`
group, with a connect grant to `Linux workstations`. The first machine belongs
to that device group. Its unique device credential and the support password are
stored separately in private files under `.local/relivo-agent`; neither belongs
in a distributable installer or the public server profile.

## Linux build preparation

```sh
docker build --target native-deps -t relivo/agent-linux-deps:2026-11-11 \
  -f agent/build/linux.Dockerfile agent/build
docker build --target builder -t relivo/agent-linux-builder:2026-11-11 \
  -f agent/build/linux.Dockerfile agent/build
```

The builder uses pinned Rust and Flutter archives, one compiler job, and Debian
12 native development libraries. The runtime compatibility floor must be measured
on the resulting binary. It does not install packages on the workstation.
The first build uses software codecs and ordinary desktop permission paths;
no unattended Wayland/DRM consent-bypass build is selected.

build/bridge-command.sh uses pinned generator versions. The owner explicitly
authorized generation on 2026-11-11, overriding rustdesk/AGENTS.md for this build.
Its environment guard is not itself permission for other builds. Generated
artifacts must match this fork's FFI source and Flutter dependency set.

After the builder image is ready, execute the authorized generation and then
the native build separately:

```sh
RELIVO_ALLOW_BRIDGE_GENERATION=owner-authorized bash agent/build/run-linux.sh bridge
bash agent/build/run-linux.sh compile
```

The runner mounts only the client source, build scripts and compiler caches.
It runs as UID 1000 inside the builder. The Linux bundle is produced under
`rustdesk/flutter/build/linux/x64/release/bundle`; successful compilation does
not itself establish successful enrollment or remote-desktop operation.
The runtime bundling step includes the Debian libvpx, libyuv and JPEG ABI
libraries absent on this workstation, together with their copyright notices.
All 16 executable/shared-library dependencies were checked on Pop!_OS 24.04.

The first isolated profile disables upstream update checks in
`RustDesk_local.toml` and automatic updates in `RustDesk2.toml`. Relivo updates
must deliver this modified client, preserving its management hooks.

## Other native hosts

| Target | Build environment still needed | Versioned source reference |
| --- | --- | --- |
| Windows x64 | Windows with MSVC, Windows SDK, LLVM, Flutter 3.24.5, Rust and vcpkg | rustdesk/.github/workflows/flutter-build.yml, build-for-windows-flutter |
| Windows ARM64 | Windows ARM64 with the corresponding Flutter 3.44.9 source patches and bridge | Same job's ARM64 matrix |
| macOS Intel | Mac with Xcode, CocoaPods, Flutter 3.24.5, Rust and vcpkg | rustdesk/.github/workflows/flutter-build.yml, macOS job |
| macOS Apple Silicon | Apple Silicon Mac, Xcode, CocoaPods, Flutter 3.44.9 patches and matching bridge | Same job's ARM64 matrix |

The pinned vcpkg baseline is 9e593bb18ea69cc5095e012465dcd675a822ed0d.
The imported CI files describe upstream builds and publishing steps; they are
references, not workflows to execute unchanged against an upstream repository.
No Windows/Mac builder, signing certificate or notarization account is currently
connected. Native builds and real installation/session tests are required before
those packages can be offered as ready. See the official
[Flutter desktop guidance](https://docs.flutter.dev/platform-integration/desktop)
and [RustDesk build guides](https://rustdesk.com/docs/en/dev/build/).
