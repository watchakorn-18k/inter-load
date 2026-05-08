# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**Inter-Load** is a desktop webhook inspector/forwarder built with Tauri v2 + SvelteKit 5 + Rust. It runs a local HTTP webhook server (Axum) that captures incoming requests, displays them in a UI, and can forward/transform payloads to other endpoints with key mapping and custom headers.

## Development Commands

```bash
# Install dependencies (uses bun)
bun install

# Frontend-only dev server (port 1420)
bun run dev

# Run the full Tauri desktop app (starts frontend + Rust backend)
bun run tauri dev

# Build for production
bun run tauri build

# Type-check Svelte/TypeScript
bun run check

# Watch mode type-checking
bun run check:watch
```

## Architecture

### Dual-process design
- **Frontend (SvelteKit SPA)**: Runs in a Tauri webview. Uses `adapter-static` with SPA fallback (`ssr = false`). Served by Vite on port 1420 during dev.
- **Backend (Rust/Tauri)**: Manages the app state and exposes Tauri commands. Also spawns a background HTTP server.

### Internal webhook server
On startup, a background Axum HTTP server starts on port 3030 (configurable) at path `/webhook` (configurable). It runs in a separate std::thread with its own Tokio runtime. All received payloads are stored in-memory (capped at 500) in `AppState.payloads`.

### Tauri commands (Rust → Frontend bridge)
All communication between the Svelte UI and Rust backend uses `invoke()` from `@tauri-apps/api/core`. Commands are defined with `#[tauri::command]` in `src-tauri/src/lib.rs`:

- `get_payloads` / `clear_payloads` / `send_test_payload` — payload CRUD
- `get_server_status` / `get_webhook_path` / `restart_server` — server config (port, path)
- `forward_mapped` — sends a transformed payload to an external URL via reqwest
- `save_forward_rule` / `get_forward_rules` / `delete_forward_rule` — forward rule persistence (in-memory)

### Shared state
`AppState` is wrapped in `Arc` and managed by Tauri. Fields use `std::sync::Mutex` for thread-safe access from both the Axum server thread and Tauri command handlers.

### Frontend structure
Single-page app with all UI in `src/routes/+page.svelte`. Uses Svelte 5 runes (`$state`, `$effect`). Layout config in `src/routes/+layout.ts` disables SSR.

## Key conventions

- Package manager: **bun** (configured in `tauri.conf.json` as `beforeDevCommand`/`beforeBuildCommand`)
- Svelte 5 runes syntax, not legacy stores
- Rust code uses Tauri v2 API (`tauri::State`, `tauri::generate_handler!`)
- The app identifier is `com.watchakornb.inter-load`
- Default dev server port: 1420 (frontend), 3030 (webhook listener)
