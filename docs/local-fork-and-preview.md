# Local source ownership and interface preview

## PRD

Make the cloned client and server ordinary source directories in the owner's
project, with no active Git relationship to the original repositories. Open the
desktop interface for an initial review.

## SRS

- Preserve every tracked source file, asset, license, and copyright notice.
- Preserve the original Git metadata in an offline recovery archive outside the
  project before changing repository structure.
- Remove submodule registration and nested Git worktrees from the active project.
- Keep one local root Git repository, with no remote configured.
- Keep the preview isolated from the network and from the user's normal RustDesk
  configuration. Do not install a system service or configure unattended access.
- Identify the preview as an official prebuilt application, not a local source
  build. Rust, Flutter, and GTK development packages are missing locally.

## CONTRACT

Inputs: the existing four Git worktrees (client, server, and their two common
libraries) and the upstream Linux x86_64 Flutter package for client tag 1.5.3.
This tag is marked as a prerelease; it is for interface review only.

Outputs: normal source files tracked by the parent project, recovery metadata,
and a repeatable offline desktop preview command. The downloaded package must
match the SHA-256 published in the GitHub release API.

The change does not yet replace branding, runtime server defaults, update URLs,
or Git-based build dependencies. Those require separately specified changes and
a source build; removing Git remotes alone does not change application traffic.

## ARCHITECTURE

The root repository owns both source trees directly. Original repository metadata
is kept outside the workspace for recovery and provenance. Runtime preview files
are local, ignored artifacts.

The preview uses the extracted official Debian package without installing it.
Bubblewrap creates an isolated network namespace, a private home and temporary
directory, and mounts the X11 socket to display the window. The original sources
remain the basis for future development.

## MODELING

Git: initialized submodules -> recovery snapshot -> remove Git links -> stage
the same source files as ordinary parent-repository files -> verify hashes.

Preview: download -> verify digest -> extract -> check shared libraries -> start
in network isolation -> verify that the desktop window is mapped.

Acceptance: no active remote or submodule configuration; no missing or changed
source assets except the removed submodule manifests; an observable desktop
window; no source-build or VPS-deployment claim.
