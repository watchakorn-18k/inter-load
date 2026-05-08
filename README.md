# Inter-Load

![alt text](screens/image.png)

**Intercept. Inspect. Forward.**

Ever struggled with webhook integrations where you have no idea what the incoming payload looks like? No way to debug because it's server-to-server? Setting up dummy endpoints just to see the data feels like overkill?

**Inter-Load** was born from that exact frustration.

The name **Inter-Load** comes from **Intercept** + **Payload** — a tool that **intercepts incoming payloads and puts them on full display**.

---

## Why Inter-Load?

One day, I was integrating with a payment gateway that sent webhooks via POST, but I had no idea what fields were in the body, what headers were included, or where the signature lived. `console.log` was useless since it was server-to-server, not going through a browser.

Setting up an ngrok tunnel to point to localhost took forever. Creating a RequestBin meant I'd forget the URL every time.

That's when it hit me: **"Why isn't there a desktop app that just listens for everything and shows it instantly?"**

And that's how Inter-Load came to be.

---

## Features

### 1. Receive Webhooks on Any Method
The app runs a background HTTP server at `http://localhost:3030/webhook` that accepts every HTTP method — GET, POST, PUT, PATCH, DELETE. Send anything, it receives everything. No questions asked.

### 2. Full Payload Visibility
Each incoming payload is displayed as a card with:
- **HTTP Method** (POST, GET, PUT...) color-coded by type
- **Path** the request was sent to (`/webhook`, `/webhook/github`...)
- **Source IP** of the sender
- **All Headers** laid out clearly
- **Body** with Pretty / Minified / Raw toggle
- **JSON auto-format** — if it's JSON, it gets beautifully formatted
- **Timestamp** of when it was received

### 3. Visual Mapper + Forward
This is what makes Inter-Load more than just a viewer — it's a real tool.

Once you see the payload, you can forward it elsewhere with **remapped keys**:

```
Incoming payload:
{ "name": "John", "age": 30 }

Remapped to:
{ "username": "John", "user_age": 30 }

Forwarded to:
POST https://my-api.com/users
```

All done through the UI:
- Pick a key from the source → set the target key name
- Toggle individual keys on/off (checkbox)
- Add your own custom keys
- Choose HTTP method (POST, PUT, PATCH, DELETE)
- Add custom headers
- Preview the mapped payload before sending
- Save as a **Saved Rule** for reuse

### 4. Send Test Payloads
A built-in form lets you type a body and send a test payload directly — no need to open a terminal and fire curl commands.

### 5. Configurable Port & Path
- Change the port from 3030 to anything you want
- Change the webhook path from `/webhook` to `/api/callback`, `/hooks/github`, or whatever suits your setup
- Hit Apply and the server restarts instantly

---

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Desktop | Tauri v2 |
| Frontend | SvelteKit + Svelte 5 |
| Backend | Rust (Axum) |
| HTTP Client | Reqwest |
| Language | TypeScript + Rust |

---

## Getting Started

```bash
# Install dependencies
bun install

# Run in dev mode
bun run tauri dev

# Build for production
bun run tauri build
```

## Testing

```bash
# Send a webhook
curl -X POST http://localhost:3030/webhook \
  -H "Content-Type: application/json" \
  -d '{"event": "payment.success", "amount": 999, "currency": "THB"}'

# Try a GET
curl http://localhost:3030/webhook

# Try a sub-path
curl -X POST http://localhost:3030/webhook/github \
  -H "Content-Type: application/json" \
  -d '{"action": "push", "ref": "refs/heads/main"}'
```

---

## Roadmap

### Done
- [x] Auto-forward with saved rules (no manual trigger needed)
- [x] Filter / Search payloads
- [x] Export payloads as JSON / CSV
- [x] Dark / Light theme toggle
- [x] WebSocket support
- [x] HTTP/HTTPS MITM Proxy (capture all traffic)
- [x] CA cert persist across sessions
- [x] System proxy auto-set/unset
- [x] Pause/Resume traffic capture
- [x] Curl examples & testing guide in-app

### Planned

**API Tester** — Postman ในตัว
สร้าง request เอง (GET/POST/PUT/PATCH/DELETE) ตั้ง headers, body, query params ดู response พร้อม status code, timing บันทึกเป็น collection ใช้ซ้ำได้

**Request Replay** — เอา traffic ที่ดักได้ยิงซ้ำ
เลือก request จาก Webhook/Proxy tab กด Replay ยิงอีกครั้ง หรือแก้ body/headers ก่อนยิง เปรียบเทียบ response รอบเก่า vs รอบใหม่ (diff view)

**Mock Server** — สร้าง fake API response
กำหนด path, method, response body, status code ใช้แทน backend ตอน dev frontend รองรับ dynamic response (จาก template) บันทึกเป็น mock collection

**SSL/TLS Inspector** — ตรวจ cert ของเว็บ
ใส่ URL แล้วดู certificate chain, วันหมดอายุ, issuer, SAN เช็ค security headers (HSTS, CSP, etc.) เหมาะสำหรับ debug HTTPS ไม่ผ่าน

**Latency Monitor** — จับเวลา API ตามช่วงเวลา
Track response time ของแต่ละ endpoint แสดงเป็นกราฟเส้น (p50, p95, p99) แจ้งเตือนเมื่อ latency เกิน threshold Export เป็น report

**Diff & Compare** — เทียบ request/response 2 อัน
เลือก 2 entries จาก traffic มาเทียบ side-by-side เน้นส่วนที่ต่างกัน (headers, body diff) เหมาะสำหรับ debug ว่า "request นี้ต่างจากอันนั้นตรงไหน"

**Proxy Ad Blocker** — ดักและบล็อก ads/trackers
Block ads, tracking scripts, malware domains ผ่าน proxy โดยอัตโนมัติ ใช้ filter list แบบ uBlock Origin แสดงสถิติว่าบล็อกอะไรไปบ้าง ปรับเปิด/ปิดได้ตาม domain

**Protocol Fingerprinter** — จับและ auto-decode traffic
จับ traffic เข้ามาแล้วบอกได้เลยว่านี่คืออะไร (REST, GraphQL, gRPC, protobuf, msgpack, WebSocket binary frame) พร้อม auto-decode format นั้นให้เป็น readable format ทันที

**Token/Secret Hunter** — ขุดทุกอย่างที่ซ่อนอยู่ใน payload
Scan payload ทุก field อัตโนมัติว่ามีอะไรซ่อนอยู่ (JWT, base64 encoded payload, API key patterns, OAuth tokens, private keys) แล้ว decode ทุกชั้นออกมาให้ แม้ base64 ซ้อน base64 ก็ขุดให้หมด

**API Behavior Diff** — เทียบ response ระหว่าง 2 environment
ยิง request เดียวกันไป 2 environment (prod vs staging / version เก่า vs ใหม่) แล้ว diff response ทุก field บอกว่า field ไหนเปลี่ยน type, หาย, เพิ่ม, ค่าต่าง — ใช้จับ breaking changes และ undocumented API changes

**Traffic Timeline Replay** — เล่นย้อน traffic แบบ video player
อัด traffic ทั้งหมดเป็น timeline แล้ว "เล่นย้อน" ได้แบบ video player — scrub ไปมาดูว่าช่วงเวลานั้นมี request อะไรบ้าง ลำดับยังไง ใช้ reproduce bug ที่เกิดจาก sequence ของ request

---

## Download & Install

Go to the [Releases page](https://github.com/watchakorn-18k/inter-load/releases) and download the latest version for your platform.

### macOS

1. Download `inter-load_aarch64-apple-darwin.dmg` (Apple Silicon M1/M2/M3/M4) or `inter-load_x86_64-apple-darwin.dmg` (Intel Mac)
2. Open the `.dmg` file and drag **Inter-Load** into the **Applications** folder
3. On first launch, if you see **"Inter-Load is damaged and can't be opened"**, run this in Terminal:
   ```bash
   xattr -cr /Applications/inter-load.app
   ```
4. Open the app normally after that

### Windows

1. Download `inter-load_x86_64-pc-windows-msvc.msi` or `inter-load_x86_64-pc-windows-msvc.exe`
2. Run the installer and follow the setup wizard
3. If Windows shows **"Windows protected your PC"**, click **More info** → **Run anyway**

### Linux

1. Download `inter-load_x86_64-unknown-linux-gnu.AppImage` (universal) or `inter-load_x86_64-unknown-linux-gnu.deb` (Debian/Ubuntu)
2. **AppImage**: Make it executable and run:
   ```bash
   chmod +x inter-load_*.AppImage
   ./inter-load_*.AppImage
   ```
3. **.deb**: Install with:
   ```bash
   sudo dpkg -i inter-load_*.deb
   ```

---

Built with Tauri + Svelte + Rust

> Intercept the load. Understand the flow.
