# PRD: central administration V1

Status: approved for implementation by the owner on 2026-11-10; follows the
owner's explicit feature selection. See implementation-status.md for acceptance.

## Docker delivery addendum (2026-11-11)

The owner requires a Docker deployment targeting the Ubuntu VPS at
72.62.178.54. Deliver container images, Compose configuration, first-install,
backup/update/rollback instructions, and a tested local container deployment.
Native desktop builds and real endpoint acceptance remain separate prerequisites.
Production SSH and stateful production commands remain owner-run under MISHKAN.

## Problem and outcome

The current workspace contains RustDesk clients and OSS connection services but
no central interface for managing a support fleet. The owner needs one web
console to manage authorized computers, accounts, groups, and activity on their
own Ubuntu VPS.

## Users and scope

- Administrator: manage accounts, device enrollment, groups, grants, and audit.
- Operator: inspect assigned devices and initiate authorized remote sessions.
- Viewer: inspect assigned devices without permission to initiate sessions.

Deliver a dashboard, device inventory, user administration, group membership,
explicit access grants, and searchable activity records. Connect actions launch
the native RustDesk client. A web management console does not imply remote screen
control inside a browser.

## Success

An administrator can create an operator, approve two devices, assign only one
device to that operator's group, and demonstrate that the operator can access
the assigned device while a connection to the other is denied even when its ID
is entered directly. The decision is visible in the activity log. Removing the
grant prevents new connection attempts.

## Scope boundaries

SSO, LDAP, 2FA, billing, multi-tenant organizations, browser-based remote control,
recording session video, remote command execution, and installer generation are
not part of V1. Existing endpoint consent/password checks remain applicable.
No subscription purchase or replacement server is implicitly approved.

The first delivery is a locally verified integration. Production deployment
requires a separate environment-specific runbook executed by the owner.

## Approved product naming (2026-11-11)

The owner approved Relivo as the product name, Relivo Console for web administration, and Relivo Agent for the installed application. Apply these names to user-facing surfaces and release delivery. This naming change adds no product capabilities.

## Existing-proxy deployment refinement (2026-11-11)

Authenticated, owner-authorized SSH inspection confirmed Ubuntu 24.04.3,
Docker 29.2.0, Compose 5.0.2, about 3.5 GiB available RAM and 40 GiB free disk.
Host Nginx already serves other applications on 80/443. Prepare Relivo behind
that proxy without changing their virtual hosts. This is a backend/console pilot;
the native build and two-endpoint acceptance remain outstanding. Production
writes remain owner-run unless the owner explicitly overrides that restriction.
