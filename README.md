# Relivo

Relivo Console centralizes devices, users, groups, access and activity.
Relivo Agent provides native remote support on the owner's computers, with
the connection infrastructure hosted on the owner's VPS.

## Local sources

Both source trees and their common libraries are ordinary files in this project's
Git repository, published at https://github.com/P4T6R/relivo. There are no nested
repositories or submodules. Original licenses and attribution remain in place.

| Directory | Source provenance | Purpose |
| --- | --- | --- |
| `rustdesk/` | https://github.com/rustdesk/rustdesk | Desktop and mobile client source |
| `rustdesk-server/` | https://github.com/rustdesk/rustdesk-server | Open-source ID and relay server source |

See [source provenance](docs/source-provenance.md) for the imported revisions.
The original Git metadata is archived outside this workspace; the local recovery
location is recorded in `.local/source-recovery.json`.

The product names are **Relivo**, **Relivo Console**, and **Relivo Agent**.
See [branding and compatibility](docs/branding.md) for display names and stable
technical identifiers. Source provenance, licenses and third-party dependencies
remain documented separately.

## Desktop interface preview

```sh
bash scripts/preview-ui.sh
```

The preview runs the official prebuilt Linux Flutter client for tag `1.5.3`,
marked as a prerelease by upstream. It is not compiled from this source tree.
The package is extracted locally rather than installed system-wide; downloads,
the private profile, and runtime files live in ignored `.local/preview/`.

Bubblewrap isolates the application's network and profile while showing its X11
window. Remote access and network-dependent actions are unavailable in this
preview. Closing the main window may leave a tray process; press Ctrl+C in the
launching terminal to stop it completely.

Local source builds still require Rust, Flutter, GTK development packages, and
the native dependencies used by the project's build process. See the scoped
[implementation specification](docs/local-fork-and-preview.md).

## Central administration

The first version adds a React web console and Rust management API for devices,
users, groups, access rights, and activity logs. Start the local console with:

```sh
bash scripts/start-management.sh
```

Open `http://127.0.0.1:21114`. The local account is `admin`, with its generated
password in `.local/management/admin-password`. See the
[management guide](management/README.md), [verification status](docs/enterprise/implementation-status.md),
and [research and ordered specifications](docs/enterprise/README.md).

## Deployment

- The VPS runs the management API, HTTPS proxy, and modified `hbbs`/`hbbr`.
  Managed sessions always use the relay so central authorization is enforced.
- Relivo Agent runs on the controlling and remote devices. They use the VPS
  address and its public server key.
- Managed access requires the modified native client and server builds. Official
  prebuilt clients remain suitable only for ordinary unmanaged self-hosting.
- Personal unattended access and attended support are configured on the devices.
  Unattended access requires a running, reachable device and suitable local
  permissions and authentication.

## Known VPS context

Authorized SSH inspection on 2026-11-11 confirmed Ubuntu 24.04.3 on x86_64,
7.8 GiB total RAM (about 3.5 GiB available), 40 GiB free disk, Docker 29.2.0
and Compose 5.0.2. Existing host Nginx serves other sites on ports 80/443.

This should accommodate personal RustDesk use if existing services leave enough
capacity. Relayed sessions also consume VPS bandwidth.

## Remaining endpoint and capacity checks

- Provider firewall and bandwidth allowance.
- Operating systems of the controlling and remote devices.

For managed mode, the planned public ports are HTTPS/WSS `443`, registration/NAT
TCP `21115`–`21116`, and UDP `21116`. Keep the management API and relay listeners
`21114`, `21117`–`21120` private behind the HTTPS proxy. These are recommendations,
not firewall changes. The browser console manages access; it does not display the
remote desktop.

The Docker release and [deployment guide](deployment/README.md) are prepared.
The selected domain is `relivo-desk.duckdns.org`, resolving to `72.62.178.54`.
The owner-authorized deployment is live at
[Relivo Console](https://relivo-desk.duckdns.org/). Bundle `2026-11-11.3` uses
images from `2026-11-11.2`; all four containers are healthy. Public HTTPS,
administrator login, WSS upgrades, device ports and certificate renewal were
verified. See the [deployment record](docs/enterprise/deployment-2026-11-11.md)
and [target-specific deployment guide](deployment/nginx/README.md).
The modified Linux Agent has been compiled and packaged as a Debian package and
portable archive. Installation, enrollment and access decisions have been tested;
real remote screen/input acceptance is still required on the intended endpoints.
See [Linux packages](agent/package/README-linux.md).

Windows x64, Mac Intel and Mac Apple Silicon builds use the manual
[Build Relivo Agent workflow](.github/workflows/agent-build.yml). The workflow
compiles the modified client on native runners, tests its public server settings,
and uploads installers with checksums and source commit manifests. See
[native installation and enrollment](agent/package/README-native.md). These test
builds are not signed with a Windows certificate or notarized with Apple.

## Official references

- [Server architecture and editions](https://rustdesk.com/docs/en/self-host/)
- [Server requirements](https://rustdesk.com/docs/en/self-host/rustdesk-server-oss/install/)
- [Docker deployment and networking](https://rustdesk.com/docs/en/self-host/rustdesk-server-oss/docker/)
- [Client configuration](https://rustdesk.com/docs/en/self-host/client-configuration/)
- [Hostinger VPS plan specifications](https://www.hostinger.com/vps-hosting)
