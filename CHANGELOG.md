# 🐕 RemoteDog — Changelog

All notable changes to **RemoteDog** are documented in this file.
The project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html) and [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

---

## [0.1.1] - 2026-09-01

### 🎨 UI & Design Parity with CommanderDog
* **Header Toolbar & Action Buttons**:
  * Iconized top toolbar buttons into clean 32×32px square `.btn.btn-icon` stroke-based Lucide icon buttons (`New Connection`, `Connections`, `Clipboard`, `Dropboxes`).
  * Aligned multi-pane layout switchers (`layout-1`, `layout-2v`, `layout-2h`, `layout-3`, `layout-4`) to the right side of the header.
  * Added floating badge count over the `Dropboxes` icon button for active transfers.
  * Replaced footer status label with monospace `v0.1.1` version indicator.
  * Cleaned top-left branding header.

* **Modal Parity**:
  * **User Profile & Account Settings (`#user-profile-modal`)**: Synchronized 1:1 with CommanderDog's profile modal layout, featuring avatar camera badge button, authentication type label (`Local Database Account` vs `Authentik / OIDC SSO Account`), `ADMIN` role badge, upload/reset actions, and credentials recovery fields.
  * **Admin User Management (`#users-modal`)**: Updated user table to display avatars, usernames, display names, emails, roles, auth providers, and status.
  * **Add/Edit User Modal (`#user-edit-modal`)**: Matched CommanderDog user creation layout with username, display name, email, password, and role selector.
  * **About RemoteDog Modal (`#about-modal`)**: Added system architecture and runtime specifications card.

### ✨ Portable Profile Pictures
* **100% In-Database Portability**:
  * User profile photos are cropped to center-squares and resized to **160×160px** via client-side HTML5 canvas, then compressed as high-quality WebP/JPEG data URIs.
  * Stored directly inside the SQLite `users.avatar_data` column, keeping `remotedog.db` completely self-contained with zero external file dependencies or broken paths when moving across servers, Docker containers, or backup archives.
  * Dynamic avatar rendering with initial fallback in top navbar pill, profile dropdown menu header, profile modal, and admin user directory.

### 🛡️ Phase 2.1: Granular RBAC & Personal vs. Global Connection Isolation
* **Personal Private vs. Global Shared Connections**:
  * Added `is_global` database column and SQLite schema migrations.
  * **Global Shared Connections**: Admin-published organizational endpoints accessible to assigned users.
  * **Personal Private Connections**: Operators can create their own private endpoints and credentials that are isolated to their personal workspace.
  * **Admin Oversight**: Admins retain full visibility and administrative governance over all connections.
* **Security & Session Policy Enforcement**:
  * **Interaction Mode (`view_only`)**: Support for View-Only / Observer sessions (monitoring screen without keystroke/mouse input forwarding).
  * **Clipboard Policy (`allow_clipboard`)**: Granular policy control (`bidirectional`, `host_to_remote`, `remote_to_host`, `disabled`) enforced during active terminal and graphics sessions.
  * **File Transfer Policy (`allow_transfer`)**: File staging controls (`full`, `upload_only`, `download_only`, `disabled`) with automatic SFTP directory suppression.
* **Connection Directory & UI Upgrades**:
  * Upgraded Connections Directory (`#connections-modal`) and Target Edit (`#connection-edit-modal`) modals to CommanderDog standard overlay architecture.
  * Filter buttons for `All`, `🌐 Global Shared`, and `🔒 Personal Private` connections.
  * Live search filter across connection names, protocols, hosts, and tags.
  * Policy indicator badges for Mode, Clipboard, and File Staging.

---

## [0.1.0] - 2026-09-01

### 🚀 Initial Public Release
* **Ultra-Low-Resource Architecture**:
  * Single-binary Rust backend (Axum 0.7, Tokio, bundled C SQLite 3).
  * High-performance HTML5 Canvas & WebSockets gateway replacing Apache Guacamole.
* **Protocol Engines**:
  * **RDP**: Windows Remote Desktop gateway protocol handler.
  * **VNC / RFB**: Pure RFB 3.8 client and tile streaming engine.
  * **SSH & Local PTY**: Native terminal streaming with portable-pty and SSH2 backend.
* **Multi-Pane Viewport Grid**:
  * Dynamic 1-to-4 multi-pane layout switching with Alt+1 to Alt+4 keyboard shortcuts.
* **Security & Auth**:
  * Argon2id password hashing + JWT authentication.
  * SQLite RBAC and permission tables.
  * Authentik / Keycloak OpenID Connect (OIDC) Single Sign-On integration.
* **File Staging & Clipboard**:
  * Slide-out clipboard drawer with bidirectional clipboard auto-sync.
  * SFTP file transfer explorer and staging dropboxes.
