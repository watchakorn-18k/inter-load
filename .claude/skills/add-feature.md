# Add Feature

Plan and implement a new feature for Inter-Load. This is a guided workflow that helps break down a feature into Tauri commands and UI changes.

## Instructions

1. Ask the user what feature they want to add
2. Analyze the request and identify:
   - What new Tauri commands (Rust) are needed
   - What new state fields in `AppState` are needed
   - What UI changes in `+page.svelte` are needed
   - Whether any new dependencies are needed (Cargo.toml or package.json)
3. Present a plan with specific file changes before implementing
4. Implement the changes following the project patterns:
   - Rust commands: `#[tauri::command]`, `Result<T, String>`, register in `invoke_handler`
   - State: add to `AppState`, wrap in `Mutex`, initialize in `run()`
   - Frontend: runes (`$state`, `$effect`), `invoke()` calls, dark theme CSS variables
5. Run `bun run check` and `cd src-tauri && cargo check` to verify
