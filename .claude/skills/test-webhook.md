# Test Webhook

Send test requests to the Inter-Load webhook server and verify payloads appear in the UI.

## Instructions

1. First check if the dev server is running: `curl -s http://localhost:3030/health`
2. If not running, start it with `bun run tauri dev` in the background and wait for it to be ready
3. Send test payloads using curl:
   - A basic JSON POST: `curl -X POST http://localhost:3030/webhook -H "Content-Type: application/json" -d '{"test": "hello"}'`
   - A nested JSON: `curl -X POST http://localhost:3030/webhook -H "Content-Type: application/json" -d '{"user": {"name": "test"}, "action": "login"}'`
   - A form payload: `curl -X POST http://localhost:3030/webhook -H "Content-Type: application/x-www-form-urlencoded" -d 'field1=value1&field2=value2'`
4. Verify each response returns `{"status":"ok","message":"payload received"}`
5. Report results — how many payloads were captured, any errors encountered
