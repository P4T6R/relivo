# Relivo Agent delivery

## PRD

The owner requests native agents for Windows, Linux and macOS, connected to the
deployed Relivo Console. The first real installation target is this Pop!_OS
24.04 x86_64 workstation. Deliver a compiled modified client, per-device
enrollment, explicit approval and scoped group access. Preserve endpoint consent
and existing OS screen/input permissions. Do not install an upstream binary as
though it contained Relivo's management implementation.

## SRS

- Compile the Flutter client with the managed transport hooks from this fork.
- Package public server configuration separately from unique device credentials.
- Initialize the first Linux installation in the user's account before any
  system-service deployment; do not overwrite an unrelated RustDesk profile.
- Register the actual generated device ID, pin its reported UUID/public key,
  verify that identity on the endpoint, then approve it and assign its group.
- Test account login, reporting, allowed/denied access and an actual desktop
  session before declaring an agent ready for general use.
- Build Windows and macOS on appropriate native hosts. No Windows or macOS
  builder or test endpoint has been supplied yet. Unsigned outputs must be
  identified as such; no signing identity or notarization is assumed.

## Contract

Use the existing public origin https://relivo-desk.duckdns.org, ID server
relivo-desk.duckdns.org:21116, WSS relay /ws/relay, and the public identity
exported through the authenticated deployment connection. A controller uses a
workspace account; each receiving machine has its own enrollment credential.
No administrator password or shared enrollment token belongs in a package.
Keep technical RustDesk identifiers stable for native interoperability.

## Architecture

Use a dedicated local Docker image to provide Linux compiler/desktop libraries
and the pinned Flutter SDK, with one build job. This container builds artifacts;
the actual agent runs natively in the desktop session. Compile and validate
Linux first. Use the checked-in upstream build definitions as the versioned
reference for native Windows/macOS build requirements, preserving our source
changes and avoiding upstream publication jobs.

The checkout lacks src/bridge_generated.rs, src/bridge_generated.io.rs,
flutter/lib/generated_bridge.dart and generated native headers. The pinned
generator in .github/workflows/bridge.yml is flutter_rust_bridge_codegen 1.80.1,
with cargo-expand 1.0.95. rustdesk/AGENTS.md explicitly prohibits running that
generator locally. Prepare the exact generation command for review, but do not
execute it without the owner's explicit override or an existing compatible
generated artifact. The ordinary Flutter build SDK for Linux is 3.24.5.

## Modeling and acceptance

Source/config reviewed -> native dependencies ready -> bridge generation
authorized or matching artifact supplied -> Linux binary compiled -> native
launch verified -> real device enrolled -> identity verified -> approved ->
group/grant assigned -> operator login -> actual authorized desktop session.
Report each state accurately. No synthetic production device, approval, or
simulated session may stand in for a real installed agent.

Linux readiness does not establish Windows/macOS readiness. Complete native
build, installation, permissions and session checks for each supported target.

### Workstation preparation status (2026-11-11)

- The selected workstation uses an X11 desktop on Pop!_OS 24.04 x86_64.
- A fresh managed profile was created under
  `.local/relivo-agent/config/rustdesk/RustDesk2.toml`; the existing user profile
  was preserved. No enrollment credential is present in the fresh profile.
- The public configuration generator passed fresh-file, restrictive-permission,
  overwrite-refusal, origin-validation and credential-validation checks.
- The native-dependency and Flutter builder images completed successfully.
  Debian's libyuv package lacks pkg-config metadata; the builder supplies it
  using the installed header version. rustfmt is included for bridge generation.
- The exact pinned bridge-generation script is prepared in
  `agent/build/bridge-command.sh`. On 2026-11-11 the owner explicitly approved
  execution ("oui je valide vas y teste"), overriding the local generator
  prohibition for this build. Other repository instructions still apply.
- Bridge generation completed successfully with version 1.80.1. Native release
  compilation completed after the correction below; its log is
  `.local/agent-build/compile.log`.
- All 10 selected Flutter tests passed: API server warnings, device-ID
  normalization, and literal server-key input. The earlier test attempt before
  bridge generation could not load the API widget; rerunning after generation
  resolved that prerequisite failure.
- The modified native client is installed under
  `.local/relivo-agent/releases/linux-2026-11-11.1`, selected by the `current`
  symlink. The user desktop launcher and `rustdesk://` handler are installed.
- Its actual device ID is 472537557. The first native sysinfo report was
  observed before any manual heartbeat probe. Its public key matched the local
  profile; a subsequent authenticated probe validated the expected UUID against
  the server's existing pin, which the administrative API does not expose.
  The device is approved and belongs to `Linux workstations`.
- The new `support` operator belongs to `Support`, with a connect grant to that
  device group. Native API login, scoped listings, relay-ticket authorization,
  unauthenticated rejection and rejection of an unknown target passed. Removing
  the actual connect grant denied and hid this real device; restoring it allowed
  access again. The grant is restored and the installed release is running.
- The application reports version 1.5.3 and retains the enrolled ID after restart.
  All 16 executable/shared-library dependencies resolve on this workstation.
- The Linux desktop is locked. Visual acceptance, interactive account login and
  a real remote-desktop session remain pending until the user unlocks it. API
  permission tests and a native heartbeat do not prove screen/input operation.

### Native compilation correction

The first release build failed with Rust E0733 at `src/common.rs:1692`:
`post_request -> management::report -> post_request` formed an async type cycle.
The compiler identified the report dispatch, rather than the generated bridge,
as the cause. Managed audit dispatch now calls `report_managed`, which performs
the strict origin/credential/status checks without returning to the generic
HTTP dispatcher. The general report entry point retains its existing unmanaged
fallback. No boxing, nested runtime, or new dependency was introduced.

Regression surface: `rustdesk/src/common.rs` changes only the managed audit hook
added by this fork; `rustdesk/src/management.rs` separates its strict reporting
path. The existing HTTP/TCP behavior when management is disabled is preserved.
Native compilation and actual endpoint reporting are required to validate this
correction; the previously passed server/console tests alone do not cover it.

## Executable distribution (2026-11-11)

### PRD

The owner now requests executable packages for Linux, Windows and macOS. They
clarified that the recipient Windows/Mac computers are test endpoints, not
available build hosts, and authorized creation of a private P4T6R/relivo
repository for hosted builds. Deliver actual binaries, with an explicit
distinction between a compiled package and a validated remote-desktop session.

### SRS

Produce a Linux amd64 Debian package and a portable tar archive from the tested
Flutter bundle. Include runtime libraries, notices, checksums, and matching
source. Initialize an isolated per-user managed profile on first launch. A
separate enrollment-file option must provision one machine's credential after
its native ID is known. Never distribute workstation profiles or credentials.
Windows EXE and macOS DMG builds require the supplied native hosts; do not
substitute renamed upstream binaries or scripts for executable deliverables.

### Contract

Linux packages install the application under /opt/relivo-agent (Debian) or the
extracted archive directory (portable). Both use the same per-user profile at
~/.local/share/relivo-agent; RELIVO_PROFILE_DIR can select an explicit isolated
test profile. The launcher's --enrollment-file option applies a credential from
a private file while the application is stopped. Existing RustDesk settings and
the workspace's already-enrolled test installation remain separate.

### Architecture

Package the existing Linux binary without changing remote-access code. A small
Linux launcher initializes or validates the public settings, then runs the
native client with dedicated XDG directories. Hold a profile lock during native
execution so provisioning cannot race a running client. Debian installation
adds application files and a desktop entry, without starting a service. Supply
the corresponding fork sources and generated bridge in a separate archive.

### Modeling and acceptance

Compiled bundle -> dependency metadata -> isolated staging -> package files ->
archive inspection -> secret-exclusion checks -> isolated launch/version test ->
clean Debian installation/runtime dependency test -> checksums and delivery.
Windows/macOS: access supplied -> toolchain preflight -> native build -> package
inspection -> platform installation and permission tests. Signing identities
remain unspecified; identify unsigned output when it is produced.

### Distribution validation

- Linux `.deb` and portable `.tar.gz` packages were generated from the existing
  compiled bundle, together with matching source and SHA256SUMS.
- Three packaged-launcher tests passed: the real native version command with a
  fresh managed profile; credential import preserving identity and unrelated
  settings; rejection of active-profile updates and foreign server settings.
- Clean Ubuntu 24.04 installation passed. Its installed launcher reports 1.5.3
  as an unprivileged user with Docker networking disabled.
- All 1,367 inspected archive/package files excluded the known private device,
  administrator and support credentials. No .local or .git directories shipped.
- Existing native application paths are unchanged by this packaging work. New
  launcher behavior is confined to the distributable Linux package. The
  previously installed workspace test agent and its identity were not changed.

## Hosted executable builds

### PRD

The owner created P4T6R/relivo and explicitly requested sending this project to
that repository and continuing executable builds. The repository is initially
empty and public. The owner explicitly chose to keep it public before upload.
Produce Windows x64, macOS Intel and macOS Apple Silicon native packages.

### SRS

Use hosted native runners and versioned dependencies. Compile the modified
management-enabled client, not an upstream executable. The distributed native
applications must initialize the public Relivo settings themselves and provide
a credential-file import command. Do not embed a shared enrollment secret or
change device approval requirements. Preserve existing native technical paths.

### Contract

An opt-in `relivo-managed` Cargo feature enforces the public server profile and
disables upstream updates. `--relivo-enrollment-file <file>` imports one device
credential while the client is stopped and exits. The standard RustDesk native
profile filenames remain stable; do not run the packaged client concurrently
with another RustDesk client in the same user profile. Feature-off builds retain
their existing startup path. Imported profiles preserve unrelated settings.

### Architecture

Keep package initialization in a new management submodule, with a small gated
hook in core_main. Run dedicated manual GitHub Actions jobs for Windows x64,
macOS x64 and macOS arm64. Generate matching bridges in CI, use the existing
native build/portable packer implementation, and upload artifacts without
invoking the imported upstream publication or signing services. No developer
signing/notarization account is available; identify that status in delivery.

### Modeling and acceptance

Source/secret audit -> visibility confirmed -> repository initialized -> hosted
native jobs -> compile failures corrected from exact errors -> executable/DMG
and checksum inspection -> downloads -> recipient installation/session tests.
Validate credential import and public defaults locally before publication.

The first macOS arm64 job passed compilation, package tests, signing verification
and DMG verification, but its Actions artifact upload failed. Delivery therefore
uses a draft GitHub release, with separate checksums per platform. The draft is
reviewable by the owner before publication. CI may reuse bridge artifacts from a
previous run only after verifying the same repository/workflow and an identical
complete client source tree. Native executables are still built and tested on
their respective operating systems. Failed jobs retain dependency caches.

### Hosted-build validation record

- Initial public source commit: `d24834b` in `P4T6R/relivo`, branch `main`.
- First native build run: https://github.com/P4T6R/relivo/actions/runs/43091237911 .
  Its completion and platform artifacts remain to be verified.
- The `relivo-managed` Linux library compiled successfully in the pinned builder.
  A separate copied bundle passed actual CLI checks for public defaults, token
  import, preservation of unrelated settings, rejection with a nonzero exit
  status, no file change on rejection, and configuration mode `0600`.
- `cargo test --locked --release --lib --features
  flutter,linux-pkg-config,relivo-managed management::package::tests` passed:
  two tests, zero failures.
- The preservation check caught TOML table ordering in the older configuration
  writer. Serializing through `toml::Value::Table` preserves scalar-before-table
  ordering without replacing the existing atomic configuration writer.
- The invalid-token check caught an existing native-runner convention that can
  return success after a command error. The gated Relivo initialization now exits
  with failure directly. Other startup paths are unchanged.
- Existing shared files changed for native packaging: `rustdesk/Cargo.toml`
  declares the opt-in feature, and `rustdesk/src/core_main.rs` calls its gated
  initialization. Package logic stays in `rustdesk/src/management/package.rs`.
- The public-source audit excluded local databases, generated binaries, runtime
  profiles, and production credentials. The checked-in WebSocket test key is an
  existing test fixture, not a deployment key.

## Build references

- Checked-in rustdesk/.github/workflows/bridge.yml and flutter-build.yml.
- https://docs.flutter.dev/platform-integration/desktop
- https://rustdesk.com/docs/en/dev/build/
- Flutter 3.24.5 Linux archive checksum from the official releases_linux.json:
  a7c82f551a9eae018e078f6bb186171e5a77920d35a3d75a61d9a593d0a9e4ae.
