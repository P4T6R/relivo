# Relivo product naming

Approved by the owner on 2026-11-11.

| Surface | Display name |
| --- | --- |
| Product and source workspace | Relivo |
| Web administration | Relivo Console |
| Installed desktop/mobile application | Relivo Agent |
| Container images | `relivo/management`, `relivo/server`, `relivo/edge` |
| Current deployment archive | `relivo-2026-11-11.2.tar.gz` |

This is a display and release naming change. No account, device ID, permission,
network protocol, database schema or credentials are changed. Existing source
directories, Rust crate/library/executable names, native bundle IDs, service IDs,
configuration paths and `rustdesk://` links remain compatibility identifiers.
The Compose project remains `rustdesk-control` so named volumes are reused.
Original attribution and license notices remain in the source and documentation.
The old release archive remains available as a historical build.

## Regression surface

- Console HTML, React labels and npm metadata: titles, login, navigation, footer
  and connection guidance now identify Relivo Console and Relivo Agent.
- Native Flutter common display-name handling, application/window titles,
  mobile home and desktop tab title: presentation uses Relivo Agent; technical
  `appName` uses for installation, file storage and printers remain unchanged.
- Native Rust display-name helper, translated presentation text and Windows URI dispatch, together with the
  Windows runner: existing-window lookup must use the same title as Flutter.
- Linux desktop entries/window titles, Windows product-description resources,
  Android labels and Apple display-name metadata: installed display names change
  without renaming executable files or bundle identifiers.
- Compose image references, release preparation/packaging, acceptance checks and
  deployment documentation: use Relivo images and the new release tag, preserving
  the stable production volume namespace and private configuration paths.

Native files changed for these presentation paths:

| Files | Required naming change |
| --- | --- |
| `rustdesk/src/common.rs`, `rustdesk/src/lang.rs` | Separate display-name helper and translated product labels; keep attribution and substituted technical identifiers |
| `rustdesk/src/core_main.rs`, `rustdesk/flutter/windows/runner/main.cpp` | Match URI dispatch to the renamed native window title |
| `rustdesk/flutter/lib/common.dart`, `rustdesk/flutter/lib/main.dart` | Display-name getter and application/window titles |
| `rustdesk/flutter/lib/mobile/pages/home_page.dart`, `rustdesk/flutter/lib/desktop/widgets/tabbar_widget.dart`, `rustdesk/flutter/lib/desktop/pages/desktop_home_page.dart` | Home/tab headers and update announcement display name |
| `rustdesk/res/rustdesk.desktop`, `rustdesk/res/rustdesk-link.desktop`, `rustdesk/flutter/linux/my_application.cc` | Launcher labels and initial Linux window title |
| `rustdesk/flutter/windows/runner/Runner.rc` | Windows product name and file description |
| `rustdesk/flutter/android/app/src/main/AndroidManifest.xml`, `rustdesk/flutter/android/app/src/main/res/values/strings.xml` | Android application and accessibility display labels/descriptions |
| `rustdesk/flutter/ios/Runner/Info.plist`, `rustdesk/flutter/macos/Runner/Info.plist` | Apple display metadata, retaining bundle and executable identifiers |

## Verification

The web production build and full Docker protocol/persistence acceptance passed
for `2026-11-11.2`. The running local console's browser title and login rendering
were checked in the actual browser. Native XML/plists parse and the modified
Rust files pass syntax parsing; these checks do not replace a native build.

The upstream prebuilt GUI used for the earlier preview is unchanged. A full
native build and two-device acceptance remain required. This rename does not
claim trademark or domain availability, or deploy anything to the VPS.
