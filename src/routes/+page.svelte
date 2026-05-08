<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  interface PayloadEntry {
    id: string;
    method: string;
    path: string;
    source_ip: string;
    headers: Record<string, string>;
    body: string;
    content_type: string | null;
    received_at: string;
  }

  interface ServerStatus {
    port: number;
    running: boolean;
  }

  let payloads = $state<PayloadEntry[]>([]);
  let serverStatus = $state<ServerStatus>({ port: 3030, running: false });
  let newPort = $state("3030");
  let testBody = $state('{\n  "message": "Hello from inter-load!",\n  "timestamp": "2024-01-01"\n}');
  let testContentType = $state("application/json");
  let autoRefresh = $state(true);
  let expandedId = $state<string | null>(null);
  let copiedId = $state<string | null>(null);
  let error = $state("");
  let pollInterval: ReturnType<typeof setInterval> | null = null;

  async function fetchPayloads() {
    try {
      const result = await invoke<PayloadEntry[]>("get_payloads");
      payloads = result;
      error = "";
    } catch (e) {
      error = String(e);
    }
  }

  async function clearAll() {
    try {
      await invoke("clear_payloads");
      payloads = [];
    } catch (e) {
      error = String(e);
    }
  }

  async function sendTest() {
    try {
      await invoke("send_test_payload", {
        body: testBody,
        contentType: testContentType,
      });
      await fetchPayloads();
    } catch (e) {
      error = String(e);
    }
  }

  async function fetchStatus() {
    try {
      serverStatus = await invoke<ServerStatus>("get_server_status");
      newPort = String(serverStatus.port);
    } catch (e) {
      error = String(e);
    }
  }

  async function changePort() {
    const port = parseInt(newPort, 10);
    if (isNaN(port) || port < 1 || port > 65535) {
      error = "Port must be between 1-65535";
      return;
    }
    try {
      serverStatus = await invoke<ServerStatus>("restart_server", { port });
    } catch (e) {
      error = String(e);
    }
  }

  function toggleExpand(id: string) {
    expandedId = expandedId === id ? null : id;
  }

  async function copyBody(body: string, id: string) {
    try {
      await navigator.clipboard.writeText(body);
      copiedId = id;
      setTimeout(() => { copiedId = null; }, 1500);
    } catch { /* noop */ }
  }

  function formatTime(iso: string): string {
    const d = new Date(iso);
    return d.toLocaleString("th-TH", {
      year: "numeric", month: "2-digit", day: "2-digit",
      hour: "2-digit", minute: "2-digit", second: "2-digit",
    });
  }

  function tryFormatJson(body: string): { formatted: string; isJson: boolean } {
    try {
      const parsed = JSON.parse(body);
      return { formatted: JSON.stringify(parsed, null, 2), isJson: true };
    } catch {
      return { formatted: body, isJson: false };
    }
  }

  function methodColor(method: string): string {
    switch (method.toUpperCase()) {
      case "GET": return "#61affe";
      case "POST": return "#49cc90";
      case "PUT": return "#fca130";
      case "DELETE": return "#f93e3e";
      case "PATCH": return "#50e3c2";
      case "TEST": return "#a855f7";
      default: return "#999";
    }
  }

  $effect(() => {
    fetchPayloads();
    fetchStatus();
  });

  $effect(() => {
    if (pollInterval) clearInterval(pollInterval);
    if (autoRefresh) {
      pollInterval = setInterval(fetchPayloads, 2000);
    }
    return () => { if (pollInterval) clearInterval(pollInterval); };
  });
</script>

<main class="app">
  <!-- Header -->
  <header class="header">
    <div class="header-left">
      <h1>Inter-Load</h1>
      <span class="badge" class:running={serverStatus.running} class:stopped={!serverStatus.running}>
        {serverStatus.running ? "Running" : "Stopped"}
      </span>
    </div>
    <div class="header-right">
      <div class="port-config">
        <label for="port-input">Port:</label>
        <input id="port-input" type="number" bind:value={newPort} min="1" max="65535" />
        <button class="btn btn-small" onclick={changePort}>Apply</button>
      </div>
    </div>
  </header>

  <!-- Toolbar -->
  <div class="toolbar">
    <div class="toolbar-left">
      <span class="count">{payloads.length} payload{payloads.length !== 1 ? "s" : ""}</span>
      <span class="webhook-url">Webhook URL: <code>http://localhost:{serverStatus.port}/webhook</code></span>
    </div>
    <div class="toolbar-right">
      <label class="toggle-label">
        <input type="checkbox" bind:checked={autoRefresh} />
        Auto-refresh
      </label>
      <button class="btn btn-outline" onclick={fetchPayloads}>Refresh</button>
      <button class="btn btn-danger" onclick={clearAll}>Clear All</button>
    </div>
  </div>

  {#if error}
    <div class="error-bar">{error}</div>
  {/if}

  <div class="content">
    <!-- Payload List -->
    <div class="payload-list">
      {#if payloads.length === 0}
        <div class="empty-state">
          <div class="empty-icon">&#128230;</div>
          <h3>No payloads yet</h3>
          <p>Send a POST request to <code>http://localhost:{serverStatus.port}/webhook</code></p>
          <p>or use the test form on the right</p>
        </div>
      {:else}
        {#each payloads as payload (payload.id)}
          {@const { formatted, isJson } = tryFormatJson(payload.body)}
          <div class="payload-card" class:expanded={expandedId === payload.id}>
            <div class="payload-header" onclick={() => toggleExpand(payload.id)}>
              <div class="payload-meta">
                <span class="method-badge" style="background: {methodColor(payload.method)}">
                  {payload.method}
                </span>
                <span class="payload-path">{payload.path}</span>
                <span class="payload-source">{payload.source_ip}</span>
              </div>
              <span class="payload-time">{formatTime(payload.received_at)}</span>
            </div>

            {#if expandedId === payload.id}
              <div class="payload-detail">
                <div class="detail-section">
                  <h4>Headers</h4>
                  <table class="headers-table">
                    {#each Object.entries(payload.headers) as [key, value]}
                      <tr>
                        <td class="header-key">{key}</td>
                        <td class="header-value">{value}</td>
                      </tr>
                    {/each}
                  </table>
                </div>
                <div class="detail-section">
                  <div class="body-header">
                    <h4>Body</h4>
                    <button class="btn btn-small" onclick={() => copyBody(formatted, payload.id)}>
                      {copiedId === payload.id ? "Copied!" : "Copy"}
                    </button>
                  </div>
                  <pre class="body-content" class:json-content={isJson}>{formatted}</pre>
                </div>
              </div>
            {:else}
              <div class="payload-preview">
                <pre>{payload.body.length > 150 ? payload.body.slice(0, 150) + "..." : payload.body}</pre>
              </div>
            {/if}
          </div>
        {/each}
      {/if}
    </div>

    <!-- Test Form Sidebar -->
    <aside class="sidebar">
      <h3>Send Test Payload</h3>
      <div class="form-group">
        <label for="content-type">Content-Type</label>
        <select id="content-type" bind:value={testContentType}>
          <option value="application/json">application/json</option>
          <option value="text/plain">text/plain</option>
          <option value="application/x-www-form-urlencoded">application/x-www-form-urlencoded</option>
          <option value="application/xml">application/xml</option>
        </select>
      </div>
      <div class="form-group">
        <label for="test-body">Body</label>
        <textarea id="test-body" rows="12" bind:value={testBody}></textarea>
      </div>
      <button class="btn btn-primary btn-full" onclick={sendTest}>Send</button>

      <div class="sidebar-help">
        <h4>Quick Test (curl)</h4>
        <pre class="curl-example">curl -X POST http://localhost:{serverStatus.port}/webhook \
  -H "Content-Type: application/json" \
  -d '{"{"}hello": "world"{"}"}'</pre>
      </div>
    </aside>
  </div>
</main>

<style>
  :root {
    --bg: #0f1117;
    --bg-card: #1a1d27;
    --bg-hover: #222633;
    --bg-input: #12141c;
    --border: #2a2e3a;
    --text: #e4e6eb;
    --text-dim: #8b8fa3;
    --accent: #6c8cff;
    --accent-hover: #5a7aee;
    --danger: #f93e3e;
    --success: #49cc90;
    --purple: #a855f7;
    --radius: 8px;
  }

  * { box-sizing: border-box; margin: 0; padding: 0; }

  body {
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
    background: var(--bg);
    color: var(--text);
    overflow: hidden;
  }

  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }

  /* Header */
  .header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 12px 20px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-card);
  }

  .header-left {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .header-left h1 {
    font-size: 18px;
    font-weight: 700;
    letter-spacing: -0.5px;
  }

  .badge {
    font-size: 11px;
    padding: 2px 8px;
    border-radius: 10px;
    font-weight: 600;
    text-transform: uppercase;
  }

  .badge.running { background: rgba(73, 204, 144, 0.15); color: var(--success); }
  .badge.stopped { background: rgba(249, 62, 62, 0.15); color: var(--danger); }

  .port-config {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
  }

  .port-config label { color: var(--text-dim); }

  .port-config input {
    width: 70px;
    padding: 4px 8px;
    border-radius: var(--radius);
    border: 1px solid var(--border);
    background: var(--bg-input);
    color: var(--text);
    font-size: 13px;
  }

  /* Toolbar */
  .toolbar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 8px 20px;
    border-bottom: 1px solid var(--border);
    background: var(--bg);
    font-size: 13px;
  }

  .toolbar-left, .toolbar-right {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .count { color: var(--text-dim); }

  .webhook-url { color: var(--text-dim); }
  .webhook-url code {
    background: var(--bg-card);
    padding: 2px 6px;
    border-radius: 4px;
    color: var(--accent);
    font-size: 12px;
  }

  .toggle-label {
    display: flex;
    align-items: center;
    gap: 4px;
    color: var(--text-dim);
    cursor: pointer;
  }

  /* Buttons */
  .btn {
    padding: 6px 14px;
    border-radius: var(--radius);
    border: 1px solid var(--border);
    background: var(--bg-card);
    color: var(--text);
    font-size: 13px;
    cursor: pointer;
    transition: all 0.15s;
  }

  .btn:hover { background: var(--bg-hover); }
  .btn-small { padding: 3px 10px; font-size: 12px; }
  .btn-full { width: 100%; }
  .btn-primary { background: var(--accent); border-color: var(--accent); color: #fff; font-weight: 600; }
  .btn-primary:hover { background: var(--accent-hover); }
  .btn-outline { background: transparent; }
  .btn-danger { color: var(--danger); border-color: rgba(249, 62, 62, 0.3); }
  .btn-danger:hover { background: rgba(249, 62, 62, 0.1); }

  /* Content */
  .content {
    display: flex;
    flex: 1;
    overflow: hidden;
  }

  .payload-list {
    flex: 1;
    overflow-y: auto;
    padding: 16px 20px;
  }

  /* Empty state */
  .empty-state {
    text-align: center;
    padding: 60px 20px;
    color: var(--text-dim);
  }

  .empty-icon { font-size: 48px; margin-bottom: 16px; }
  .empty-state h3 { color: var(--text); margin-bottom: 8px; }
  .empty-state code {
    background: var(--bg-card);
    padding: 2px 6px;
    border-radius: 4px;
    color: var(--accent);
  }

  /* Payload cards */
  .payload-card {
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    margin-bottom: 8px;
    overflow: hidden;
    transition: border-color 0.15s;
  }

  .payload-card:hover { border-color: var(--accent); }
  .payload-card.expanded { border-color: var(--accent); }

  .payload-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 10px 14px;
    cursor: pointer;
  }

  .payload-meta { display: flex; align-items: center; gap: 10px; }

  .method-badge {
    font-size: 11px;
    font-weight: 700;
    padding: 2px 8px;
    border-radius: 4px;
    color: #fff;
    text-transform: uppercase;
    min-width: 52px;
    text-align: center;
  }

  .payload-path {
    font-family: monospace;
    font-size: 13px;
    color: var(--text);
  }

  .payload-source {
    font-size: 12px;
    color: var(--text-dim);
  }

  .payload-time {
    font-size: 12px;
    color: var(--text-dim);
    white-space: nowrap;
  }

  .payload-preview {
    padding: 0 14px 10px;
  }

  .payload-preview pre {
    font-size: 12px;
    color: var(--text-dim);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    font-family: monospace;
  }

  /* Payload detail */
  .payload-detail {
    padding: 0 14px 14px;
    border-top: 1px solid var(--border);
  }

  .detail-section { margin-top: 10px; }
  .detail-section h4 {
    font-size: 12px;
    color: var(--text-dim);
    text-transform: uppercase;
    margin-bottom: 6px;
    font-weight: 600;
  }

  .headers-table {
    width: 100%;
    font-size: 12px;
    border-collapse: collapse;
  }

  .headers-table td {
    padding: 3px 8px;
    border-bottom: 1px solid var(--border);
  }

  .header-key {
    color: var(--accent);
    font-weight: 600;
    font-family: monospace;
    white-space: nowrap;
    width: 1%;
  }

  .header-value {
    color: var(--text-dim);
    font-family: monospace;
    word-break: break-all;
  }

  .body-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .body-content {
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 10px;
    font-size: 12px;
    font-family: monospace;
    white-space: pre-wrap;
    word-break: break-all;
    max-height: 300px;
    overflow-y: auto;
    color: var(--text-dim);
  }

  .json-content { color: var(--text); }

  /* Sidebar */
  .sidebar {
    width: 320px;
    border-left: 1px solid var(--border);
    padding: 16px;
    overflow-y: auto;
    background: var(--bg-card);
    flex-shrink: 0;
  }

  .sidebar h3 {
    font-size: 14px;
    margin-bottom: 14px;
    font-weight: 600;
  }

  .form-group { margin-bottom: 12px; }
  .form-group label {
    display: block;
    font-size: 12px;
    color: var(--text-dim);
    margin-bottom: 4px;
    text-transform: uppercase;
    font-weight: 600;
  }

  .form-group select,
  .form-group textarea {
    width: 100%;
    padding: 8px 10px;
    border-radius: var(--radius);
    border: 1px solid var(--border);
    background: var(--bg-input);
    color: var(--text);
    font-family: monospace;
    font-size: 13px;
    resize: vertical;
  }

  .form-group select:focus,
  .form-group textarea:focus,
  .port-config input:focus {
    outline: none;
    border-color: var(--accent);
  }

  .sidebar-help {
    margin-top: 20px;
    padding-top: 16px;
    border-top: 1px solid var(--border);
  }

  .sidebar-help h4 {
    font-size: 12px;
    color: var(--text-dim);
    margin-bottom: 8px;
    text-transform: uppercase;
    font-weight: 600;
  }

  .curl-example {
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 10px;
    font-size: 11px;
    font-family: monospace;
    white-space: pre-wrap;
    color: var(--text-dim);
    line-height: 1.5;
  }

  /* Error bar */
  .error-bar {
    background: rgba(249, 62, 62, 0.1);
    color: var(--danger);
    padding: 8px 20px;
    font-size: 13px;
    border-bottom: 1px solid rgba(249, 62, 62, 0.2);
  }

  /* Scrollbar */
  ::-webkit-scrollbar { width: 6px; }
  ::-webkit-scrollbar-track { background: transparent; }
  ::-webkit-scrollbar-thumb { background: var(--border); border-radius: 3px; }
  ::-webkit-scrollbar-thumb:hover { background: var(--text-dim); }
</style>
