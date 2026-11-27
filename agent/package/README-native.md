# Relivo Agent for Windows and macOS

These builds connect to `https://relivo-desk.duckdns.org` and its configured ID
and relay servers. No shared password, account, or enrollment token is included.
The console manages access; remote screen and input use the native application.

## Install and register

Follow this order exactly. Signing in with an administrator (or any) account
on the remote computer does **not** enroll it: only the per-device enrollment
credential does.

1. On Windows x64, extract the ZIP for the first enrollment and start
   `rustdesk.exe` once; it creates the device identity and displays its ID.
   On Mac, choose the DMG for Intel (`x64`) or Apple Silicon (`arm64`), copy the
   application to Applications and start it once. Its display name is Relivo
   Agent; the technical bundle and service names remain RustDesk for
   compatibility. Do not install or enable the background service yet.
2. In Relivo Console, create the device with exactly the ID displayed by the
   agent, then create one enrollment credential for it. Save the token as a
   private text file on that computer (surrounding spaces and line breaks are
   ignored). Never reuse it on another device or send it with the installer.
3. Enroll. Close the application, including its tray icon, then run the import
   as the user who will run the agent (not as a different administrator).

   Windows PowerShell, from the extracted ZIP folder. Keep `| Out-Host`: it makes
   PowerShell wait for the command and display its result or error:

   ```powershell
   .\rustdesk.exe --relivo-enrollment-file "$env:USERPROFILE\Downloads\relivo-token.txt" | Out-Host
   ```

   macOS Terminal, after copying to Applications:

   ```sh
   /Applications/RustDesk.app/Contents/MacOS/RustDesk --relivo-enrollment-file "$HOME/Downloads/relivo-token.txt"
   ```

   The command prints a confirmation, or an error and a non-zero exit code.
   Started without a console (double-click), it shows a dialog instead.
   Alternatively, use the enrollment screen in the application
   (Settings → Relivo enrollment).
4. Verify. Start Relivo Agent again and check in Relivo Console that the device
   reports with the same ID.
5. Approve the device in Relivo Console and assign its access groups. Operators
   need the `connect` permission and must sign in on the controlling computer.
6. Install. Windows users can now run the `-install.exe` or install from the
   application; on Mac, enable the background service from the application if
   needed. The installers copy the enrolled user profile to the service account.
   Reopen the installed app and verify its ID and approved status again.
7. On macOS, grant the application Screen Recording and Accessibility access
   in System Settings. Use local consent or a device-specific unattended-access
   password. Verify a real screen/input session before relying on unattended use.

### Enrolling an already installed Windows agent

The installed service reads its own profile, not the user's. With the Relivo
Agent service running, run the installed executable; it sends the credential to
the service:

```powershell
& "C:\Program Files\RustDesk\rustdesk.exe" --relivo-enrollment-file "$env:USERPROFILE\Downloads\relivo-token.txt" | Out-Host
```

If the service is stopped, start it and retry, or use the enrollment screen of
the application (Settings → Relivo enrollment), which also works once installed.
After a reinstall the device identity can change: rotate the device credential
in Relivo Console and enroll again.

### Logs

- Windows, user application: `%APPDATA%\RustDesk\log\`
- Windows, installed service:
  `C:\Windows\ServiceProfiles\LocalService\AppData\Roaming\RustDesk\log\`

A heartbeat rejected with HTTP 401 means the credential is missing or invalid,
or the agent ID does not match the device created in the console. HTTP 403
means the device is disabled or its identity (UUID / public key) differs from
the one pinned at enrollment, typically after a reinstall.

These test builds have no Windows signing certificate or Apple Developer ID /
notarization. OS security prompts are expected: Windows SmartScreen may show
"Windows protected your PC" (choose More info, then Run anyway after checking
the SHA256SUMS), and Windows Defender may quarantine or block the unsigned
binaries. Installation remains a local decision by the computer owner. Do not
disable OS security globally; at most, restore or allow the specific verified
files.

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
