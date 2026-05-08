# Dev App

Start the Inter-Load development environment and verify it works.

## Instructions

1. Run `bun install` if `node_modules` is missing
2. Start the Tauri dev server with `bun run tauri dev` in the background
3. Wait for the app window to appear
4. Report the status — whether the frontend compiled, whether the Rust backend compiled, and any errors
5. If there are compilation errors, read the error output and suggest fixes

## Notes
- The frontend dev server runs on port 1420 (Vite)
- The internal webhook server starts on port 3030 by default
- Rust compilation can take a while on the first run — use a longer timeout
