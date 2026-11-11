# Relivo Agent for Windows and macOS

These builds connect to `https://relivo-desk.duckdns.org` and its configured ID
and relay servers. No shared password, account, or enrollment token is included.
The console manages access; remote screen and input use the native application.

## Install and register

1. On Windows x64, extract the ZIP for the first enrollment. Start `rustdesk.exe`
   once to initialize the device identity, then close it including its tray icon.
   On Mac, choose the DMG for Intel (`x64`) or Apple Silicon (`arm64`) and copy the
   application to Applications. Its display name is Relivo Agent; the technical
   bundle and service names remain RustDesk for compatibility. Start it once,
   then close it. Do not enable the background service before enrollment.
2. In Relivo Console, create one enrollment credential for this computer and
   save the token as a private text file on that computer. Never reuse it on
   another device or send it with the installer.
3. Close the application, including its tray icon and any installed service,
   before importing the credential. Run the command as the user who will run
   the agent (not as a different administrator account).

Windows PowerShell, from the extracted ZIP folder:

```powershell
.\rustdesk.exe --relivo-enrollment-file "$env:USERPROFILE\Downloads\relivo-token.txt"
```

macOS Terminal, after copying to Applications:

```sh
/Applications/RustDesk.app/Contents/MacOS/RustDesk --relivo-enrollment-file "$HOME/Downloads/relivo-token.txt"
```

4. Start Relivo Agent, then check and approve the newly registered device in
   Relivo Console. Record the ID displayed by the agent and assign access groups.
   Sign in as an authorized operator on the controlling device to start support.
5. On macOS, grant the application Screen Recording and Accessibility access
   in System Settings. Use local consent or a device-specific unattended-access
   password. Verify a real screen/input session before relying on unattended use.

After initial enrollment and approval, Windows users can run the `-install.exe`
or install from the application. On Mac, enable the background service from the
application if needed. The existing installers copy the enrolled user profile
to the service account; enrolling first prevents a service starting without its
device credential. Reopen the installed app and verify its ID and approved
status again. Do not provision a pre-existing service with this first-test flow.

These test builds have no Windows signing certificate or Apple Developer ID /
notarization. OS security prompts are expected; installation remains a local
decision by the computer owner. Do not disable OS security globally.

Existing native RustDesk profiles and service identifiers are retained. Do not
run another RustDesk installation concurrently with this build. For a first
test, use a fresh device/profile without an existing RustDesk service.

## Build and integrity

The root GitHub Actions workflow builds on Windows, Mac Intel and Mac Apple
Silicon. Each package has a platform-specific SHA256SUMS file and a manifest
recording its source commit. Completed packages are stored in a draft GitHub
release for the repository owner to review and download before publication.
Sources and license are in https://github.com/P4T6R/relivo .

Software codecs are included. This initial build does not bundle optional
virtual display or printer drivers. Actual remote desktop acceptance must be
performed on the intended endpoints; a successful build is not that test.
