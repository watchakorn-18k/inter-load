# Add Tauri Command

Add a new Tauri command with both Rust backend and Svelte frontend integration.

## Instructions

1. Ask the user for the command name and what it should do (parameters, return type, logic)
2. In `src-tauri/src/lib.rs`:
   - Define the Rust structs for input/output if needed (derive `Serialize`, `Deserialize`)
   - Add a `#[tauri::command]` function with the logic
   - Register it in the `invoke_handler` macro in the `run()` function
3. In `src/routes/+page.svelte`:
   - Add the TypeScript interface for the return type
   - Add a state variable (`$state`) if the result needs to be displayed
   - Add an async function that calls `invoke<ReturnType>("command_name", { params })`
   - Add UI elements to trigger the command and display the result, following the existing dark theme CSS variables
4. Run `bun run check` to verify TypeScript types are correct
5. Show the user a summary of what was added and where

## Style Rules
- Follow the existing patterns in `lib.rs` — use `tauri::State<'_, Arc<AppState>>` for state access, return `Result<T, String>`, use `Mutex::lock().map_err(|e| e.to_string())?`
- For new shared state, add fields to `AppState` struct and initialize in the `run()` function
- Svelte: use runes (`$state`, `$effect`), follow the existing dark theme CSS variables, keep components in the single-page structure
