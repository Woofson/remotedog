# <img src="frontend/assets/Remotedogiconsmall.png" alt="RemoteDog Logo" height="36" style="vertical-align: -6px; margin-right: 8px;" /> RemoteDog — Roadmap & Architecture Plan

This document outlines the strategic roadmap and architectural milestones for **RemoteDog**, focusing on high-performance remote access, granular user-centric access control, and enterprise single sign-on integration.

---

## 🎯 Strategic Milestones

```mermaid
graph LR
  A["Phase 1: Core Gateway & UI (v0.1.0)"] --> B["Phase 2: Granular RBAC & Connections Manager (v0.1.1+)"]
  B --> C["Phase 3: Directory Sync & Password Recovery"]
  C --> D["Phase 4: Session Recording & Audit Compliance"]
  D --> E["Phase 5: High-Density Edge Clustering"]
```

---

## 📌 Phase 2: Granular Connections Manager & User Entitlements *(In Progress — v0.1.1)*

The Connections Manager is being unified directly with SQLite RBAC and user profile management. This enables administrators to define fine-grained security policies per connection, per user, and per group.

### 1. User & Group Permission Matrix

Each connection profile and user assignment supports a comprehensive capability matrix:

| Permission Category | Configuration Options | Description | Status |
| :--- | :--- | :--- | :--- |
| **Profile Photos** | `Direct SQLite Storage (160×160 WebP)` | 100% portable avatars without filesystem dependencies | ✅ **Completed (v0.1.1)** |
| **Account Renaming** | `Username & Display Name` | Edit username and nickname with uniqueness collision checks | ✅ **Completed (v0.1.1)** |
| **Email Accounts** | `Primary Email & Recovery` | User email for future credentials recovery and notifications | ✅ **Completed (v0.1.1)** |
| **Clipboard Sync** | `Bidirectional`, `Host-to-Remote Only`, `Remote-to-Host Only`, `Disabled` | Enforces clipboard data flow constraints to prevent data exfiltration | ⏳ Planned |
| **File Transfer & Dropboxes** | `Full (Upload & Download)`, `Download Only`, `Upload Only`, `Disabled` | Controls SFTP directory staging and browser-based file drag-and-drop | ⏳ Planned |
| **Interaction Mode** | `Full Interactive`, `View-Only (Observer)`, `Shadow / Collaborative` | Allows screen sharing or read-only monitoring without input transmission | ⏳ Planned |
| **Protocol Entitlements** | `SSH`, `VNC / RFB`, `RDP`, `Local PTY` | Per-user allowlist of accessible gateway protocol engines | ⏳ Planned |
| **Audio Redirection** | `Enabled`, `Disabled` | RDP and VNC audio channel forwarding | ⏳ Planned |

---

### 2. Global Shared vs. Personal Private Connections

RemoteDog distinguishes between shared organizational resources and personal private tunnels:

* **Global Organizational Connections**:
  * Managed centrally by **Admins**.
  * Assigned to user groups (e.g., `DevOps`, `QA`, `Tier-1 Support`) or individual users.
  * Shared credentials can be encrypted at rest using AES-256-GCM master keys so end-users never see raw passwords or private SSH keys.
* **Personal Private Connections**:
  * Users with `operator` privileges can create their own custom connection endpoints.
  * Private connections are isolated to the creator's workspace.
  * **Admin Governance**: Admins retain administrative oversight, session kill authority, and audit visibility over all personal connections.

---

### 3. Concurrency Limits & Time-Window Scheduling

* **Max Concurrent Sessions**: Enforce per-user and per-connection limits (e.g., max 2 simultaneous RDP sessions per operator).
* **Scheduled Access Windows**: Time-based access rules (e.g., connection active only Monday–Friday, 08:00–18:00 UTC).
* **Automatic Idle Timeout**: Auto-disconnect inactive sessions after configurable thresholds.

---

### 4. Full Audit Trail & Security Telemetry

* **Connection Lifecycle Logs**: Record connection start, duration, client IP, user agent, and clean/forced disconnect reasons.
* **Clipboard Integrity Logging**: Log timestamps, payload byte size, and SHA-256 hashes of clipboard transfers.
* **File Transfer Telemetry**: Full recording of staged uploads and downloads with file names, sizes, and remote destination paths.

---

## 📌 Phase 3: Identity Provider, Directory Sync & Password Recovery

* **Email-Based Password Recovery**:
  * Magic link / one-time reset token emailed to verified user addresses with Argon2id hash update.
* **Authentik / Keycloak / Okta OIDC Sync**:
  * Map Identity Provider group claims (e.g., `groups: ["infra-admins", "remote-viewers"]`) directly to RemoteDog user roles and connection pools.
* **Just-In-Time (JIT) User Provisioning**:
  * Automatically create and configure user profiles upon first successful OIDC login.
* **OAuth2 / WebAuthn / FIDO2 Passkeys**:
  * Hardware security key authentication for PAM and local SQLite accounts.

---

## 📌 Phase 4: Session Recording & Audit Compliance

* **Guacamole `.guac` & Asciinema Replacement**:
  * Terminal session streaming to lightweight `.cast` formats for replay.
  * VNC/RDP tile streaming capture with lossless zstd compression.
* **Built-in Session Player**:
  * Browser-based playback scrubber in the Admin Audit modal.

---

## 📌 Phase 5: High-Density Edge Gateway Clustering

* **Stateless Gateway Nodes**:
  * Scale WebSocket and protocol forwarding across multiple distributed edge nodes.
* **Central Database Sync**:
  * Support for external PostgreSQL and SQLite cluster replication for high-availability deployments.
