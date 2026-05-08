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

  interface ServerStatus { port: number; running: boolean; }
  interface MapEntry { source_key: string; target_key: string; enabled: boolean; }
  interface ForwardRule { id: string; name: string; target_url: string; method: string; mappings: MapEntry[]; headers: Record<string, string>; }
  interface ForwardResult { status: number; body: string; duration_ms: number; }

  let payloads = $state<PayloadEntry[]>([]);
  let serverStatus = $state<ServerStatus>({ port: 3030, running: false });
  let newPort = $state("3030");
  let testBody = $state('{\n  "message": "Hello from inter-load!",\n  "timestamp": "2024-01-01"\n}');
  let testContentType = $state("application/json");
  let autoRefresh = $state(true);
  let expandedIds = $state<Set<string>>(new Set());
  let copiedId = $state<string | null>(null);
  let viewModes = $state<Map<string, "pretty" | "raw" | "minified">>(new Map());
  let error = $state("");
  let pollInterval: ReturnType<typeof setInterval> | null = null;
  let webhookPath = $state("/webhook");
  let newWebhookPath = $state("/webhook");

  // Forward state
  let forwardPanelId = $state<string | null>(null);
  let forwardRules = $state<ForwardRule[]>([]);
  let fwdUrl = $state("");
  let fwdMethod = $state("POST");
  let fwdMappings = $state<MapEntry[]>([]);
  let fwdHeaders = $state<{ key: string; value: string }[]>([]);
  let fwdSending = $state(false);
  let fwdResult = $state<ForwardResult | null>(null);
  let fwdRuleName = $state("");

  async function fetchPayloads() {
    try {
      const result = await invoke<PayloadEntry[]>("get_payloads");
      payloads = result;
      error = "";
    } catch (e) { error = String(e); }
  }

  async function clearAll() {
    try { await invoke("clear_payloads"); payloads = []; } catch (e) { error = String(e); }
  }

  async function sendTest() {
    try {
      await invoke("send_test_payload", { body: testBody, contentType: testContentType });
      await fetchPayloads();
    } catch (e) { error = String(e); }
  }

  async function fetchStatus() {
    try {
      serverStatus = await invoke<ServerStatus>("get_server_status");
      newPort = String(serverStatus.port);
      webhookPath = await invoke<string>("get_webhook_path");
      newWebhookPath = webhookPath;
    } catch (e) { error = String(e); }
  }

  async function fetchForwardRules() {
    try { forwardRules = await invoke<ForwardRule[]>("get_forward_rules"); } catch { /* noop */ }
  }

  async function changeServerConfig() {
    const port = parseInt(newPort, 10);
    if (isNaN(port) || port < 1 || port > 65535) { error = "Port must be between 1-65535"; return; }
    let path = newWebhookPath.trim();
    if (!path.startsWith("/")) path = "/" + path;
    try {
      serverStatus = await invoke<ServerStatus>("restart_server", { port, webhookPath: path });
      webhookPath = path;
    } catch (e) { error = String(e); }
  }

  function toggleExpand(id: string) {
    const next = new Set(expandedIds);
    if (next.has(id)) { next.delete(id); if (forwardPanelId === id) forwardPanelId = null; }
    else { next.add(id); if (!viewModes.has(id)) { const m = new Map(viewModes); m.set(id, "pretty"); viewModes = m; } }
    expandedIds = next;
  }

  function expandAll() {
    expandedIds = new Set(payloads.map((p) => p.id));
    const m = new Map(viewModes);
    for (const p of payloads) { if (!m.has(p.id)) m.set(p.id, "pretty"); }
    viewModes = m;
  }

  function collapseAll() { expandedIds = new Set(); forwardPanelId = null; }

  function setViewMode(id: string, mode: "pretty" | "raw" | "minified") {
    const next = new Map(viewModes); next.set(id, mode); viewModes = next;
  }

  function getBodyDisplay(payload: PayloadEntry): { text: string; isJson: boolean } {
    const mode = viewModes.get(payload.id) ?? "pretty";
    try {
      const parsed = JSON.parse(payload.body);
      if (mode === "pretty") return { text: JSON.stringify(parsed, null, 2), isJson: true };
      if (mode === "minified") return { text: JSON.stringify(parsed), isJson: true };
      return { text: payload.body, isJson: true };
    } catch { return { text: payload.body, isJson: false }; }
  }

  // Forward helpers
  function getBodyKeys(payload: PayloadEntry): string[] {
    try { return Object.keys(JSON.parse(payload.body)); } catch { return []; }
  }

  function getBodyValue(payload: PayloadEntry, key: string): string {
    try {
      const val = JSON.parse(payload.body)[key];
      const s = typeof val === "string" ? val : JSON.stringify(val);
      return s.length > 40 ? s.slice(0, 40) + "..." : s;
    } catch { return ""; }
  }

  function openForwardPanel(payload: PayloadEntry) {
    forwardPanelId = payload.id;
    fwdUrl = ""; fwdMethod = "POST"; fwdResult = null; fwdRuleName = "";
    fwdHeaders = [{ key: "Content-Type", value: "application/json" }];
    const keys = getBodyKeys(payload);
    fwdMappings = keys.map((k) => ({ source_key: k, target_key: k, enabled: true }));
  }

  function addCustomMapping() {
    fwdMappings = [...fwdMappings, { source_key: "", target_key: "", enabled: true }];
  }

  function removeMapping(index: number) {
    fwdMappings = fwdMappings.filter((_, i) => i !== index);
  }

  function addCustomHeader() {
    fwdHeaders = [...fwdHeaders, { key: "", value: "" }];
  }

  function removeHeader(index: number) {
    fwdHeaders = fwdHeaders.filter((_, i) => i !== index);
  }

  function buildMappedBody(payload: PayloadEntry): string {
    try {
      const source = JSON.parse(payload.body);
      const output: Record<string, unknown> = {};
      for (const m of fwdMappings) {
        if (!m.enabled || !m.source_key || !m.target_key) continue;
        if (m.source_key in source) output[m.target_key] = source[m.source_key];
      }
      return JSON.stringify(output, null, 2);
    } catch { return "{}"; }
  }

  function applyRule(rule: ForwardRule) {
    fwdUrl = rule.target_url;
    fwdMethod = rule.method;
    fwdMappings = rule.mappings.map((m) => ({ ...m }));
    fwdHeaders = Object.entries(rule.headers).map(([key, value]) => ({ key, value }));
  }

  async function doForward(payload: PayloadEntry) {
    fwdSending = true; fwdResult = null;
    try {
      const body = buildMappedBody(payload);
      const headers: Record<string, string> = {};
      for (const h of fwdHeaders) { if (h.key) headers[h.key] = h.value; }
      fwdResult = await invoke<ForwardResult>("forward_mapped", {
        targetUrl: fwdUrl, method: fwdMethod, mappedBody: body, customHeaders: headers,
      });
    } catch (e) { error = String(e); }
    finally { fwdSending = false; }
  }

  async function saveRule() {
    try {
      const mappings = fwdMappings.map((m) => ({ ...m }));
      const headers: Record<string, string> = {};
      for (const h of fwdHeaders) { if (h.key) headers[h.key] = h.value; }
      await invoke("save_forward_rule", {
        name: fwdRuleName, targetUrl: fwdUrl, method: fwdMethod, mappings, headers,
      });
      await fetchForwardRules();
    } catch (e) { error = String(e); }
  }

  async function deleteRule(id: string) {
    try { await invoke("delete_forward_rule", { id }); await fetchForwardRules(); } catch (e) { error = String(e); }
  }

  async function copyText(text: string, id: string) {
    try { await navigator.clipboard.writeText(text); copiedId = id; setTimeout(() => { copiedId = null; }, 1500); } catch { /* noop */ }
  }

  function formatTime(iso: string): string {
    return new Date(iso).toLocaleString("th-TH", {
      year: "numeric", month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", second: "2-digit",
    });
  }

  function methodColor(method: string): string {
    switch (method.toUpperCase()) {
      case "GET": return "#5b9cf6"; case "POST": return "#4ade80"; case "PUT": return "#fb923c";
      case "DELETE": return "#f87171"; case "PATCH": return "#2dd4bf"; case "TEST": return "#c084fc";
      default: return "#94a3b8";
    }
  }

  $effect(() => { fetchPayloads(); fetchStatus(); fetchForwardRules(); });
  $effect(() => {
    if (pollInterval) clearInterval(pollInterval);
    if (autoRefresh) { pollInterval = setInterval(fetchPayloads, 2000); }
    return () => { if (pollInterval) clearInterval(pollInterval); };
  });
</script>

<main class="app">
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
        <label for="path-input">Path:</label>
        <input id="path-input" type="text" bind:value={newWebhookPath} />
        <button class="btn btn-small" onclick={changeServerConfig} disabled={newPort === String(serverStatus.port) && newWebhookPath === webhookPath}>Apply</button>
      </div>
    </div>
  </header>

  <div class="toolbar">
    <div class="toolbar-left">
      <span class="count">{payloads.length} payload{payloads.length !== 1 ? "s" : ""}</span>
      <span class="webhook-url">Webhook: <code>http://localhost:{serverStatus.port}{webhookPath}</code></span>
    </div>
    <div class="toolbar-right">
      <label class="toggle-label"><input type="checkbox" bind:checked={autoRefresh} /> Auto</label>
      <button class="btn btn-outline btn-small" onclick={fetchPayloads}>Refresh</button>
      <button class="btn btn-outline btn-small" onclick={expandAll}>Expand All</button>
      <button class="btn btn-outline btn-small" onclick={collapseAll}>Collapse</button>
      <button class="btn btn-danger btn-small" onclick={clearAll}>Clear</button>
    </div>
  </div>

  {#if error}<div class="error-bar">{error}</div>{/if}

  <div class="content">
    <div class="payload-list">
      {#if payloads.length === 0}
        <div class="empty-state">
          <div class="empty-icon">&#128230;</div>
          <h3>No payloads yet</h3>
          <p>Send a POST request to <code>http://localhost:{serverStatus.port}{webhookPath}</code></p>
          <p>or use the test form on the right</p>
        </div>
      {:else}
        {#each payloads as payload (payload.id)}
          {@const isExpanded = expandedIds.has(payload.id)}
          {@const { text: bodyText, isJson } = isExpanded ? getBodyDisplay(payload) : { text: "", isJson: false }}
          {@const viewMode = viewModes.get(payload.id) ?? "pretty"}
          {@const showForward = forwardPanelId === payload.id}
          <div class="payload-card" class:expanded={isExpanded}>
            <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
            <div class="payload-header" role="button" tabindex="0" onclick={() => toggleExpand(payload.id)} onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); toggleExpand(payload.id); } }}>
              <div class="payload-meta">
                <span class="method-badge" style="background: {methodColor(payload.method)}">{payload.method}</span>
                <span class="payload-path">{payload.path}</span>
                <span class="payload-source">from {payload.source_ip}</span>
              </div>
              <div class="payload-meta-right">
                <span class="payload-size">{payload.body.length} bytes</span>
                {#if isJson}<span class="json-tag">JSON</span>{/if}
                <span class="payload-time">{formatTime(payload.received_at)}</span>
                <span class="expand-icon">{isExpanded ? "&#9660;" : "&#9654;"}</span>
              </div>
            </div>

            {#if isExpanded}
              <div class="payload-detail">
                <div class="detail-section">
                  <div class="section-header"><h4>Headers ({Object.keys(payload.headers).length})</h4></div>
                  <table class="headers-table">
                    <tbody>
                    {#each Object.entries(payload.headers) as [key, value]}
                      <tr><td class="header-key">{key}</td><td class="header-value">{value}</td></tr>
                    {/each}
                    </tbody>
                  </table>
                </div>
                <div class="detail-section">
                  <div class="section-header">
                    <h4>Body</h4>
                    <div class="section-actions">
                      {#if isJson}
                        <div class="view-toggle">
                          <button class="view-btn" class:active={viewMode === "pretty"} onclick={() => setViewMode(payload.id, "pretty")}>Pretty</button>
                          <button class="view-btn" class:active={viewMode === "minified"} onclick={() => setViewMode(payload.id, "minified")}>Minified</button>
                          <button class="view-btn" class:active={viewMode === "raw"} onclick={() => setViewMode(payload.id, "raw")}>Raw</button>
                        </div>
                      {/if}
                      <button class="btn btn-small" onclick={() => copyText(bodyText, payload.id)}>
                        {copiedId === payload.id ? "Copied!" : "Copy"}
                      </button>
                      <button class="btn btn-primary btn-small" onclick={() => openForwardPanel(payload)}>Forward</button>
                    </div>
                  </div>
                  <pre class="body-content" class:json-content={isJson && viewMode === "pretty"}>{bodyText}</pre>
                </div>
              </div>

              <!-- Forward Panel -->
              {#if showForward}
                <div class="forward-panel">
                  <div class="fp-header">
                    <h4>Forward Payload</h4>
                    <button class="btn btn-small" onclick={() => forwardPanelId = null}>Close</button>
                  </div>

                  <!-- Saved rules -->
                  {#if forwardRules.length > 0}
                    <div class="fp-row">
                      <span class="fp-label">Saved Rules</span>
                      <div class="fp-rules">
                        {#each forwardRules as rule}
                          <button class="rule-chip" onclick={() => applyRule(rule)}>
                            {rule.name} ({rule.method} {rule.target_url.replace(/^https?:\/\/[^/]+/, "...")})
                            <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
                            <span class="rule-delete" role="button" tabindex="-1" onclick={(e) => { e.stopPropagation(); deleteRule(rule.id); }}>x</span>
                          </button>
                        {/each}
                      </div>
                    </div>
                  {/if}

                  <!-- Target config -->
                  <div class="fp-row">
                    <span class="fp-label">Target</span>
                    <div class="fp-target">
                      <select bind:value={fwdMethod}>
                        <option>POST</option><option>PUT</option><option>PATCH</option><option>DELETE</option>
                      </select>
                      <input type="text" placeholder="https://example.com/api/endpoint" bind:value={fwdUrl} />
                    </div>
                  </div>

                  <!-- Mapper table -->
                  <div class="fp-row">
                    <span class="fp-label">Key Mapping</span>
                    <table class="mapper-table">
                      <thead><tr><th></th><th>Source Key</th><th>Value</th><th></th><th>Target Key</th><th></th></tr></thead>
                      <tbody>
                        {#each fwdMappings as m, i}
                          <tr>
                            <td><input type="checkbox" bind:checked={m.enabled} /></td>
                            <td><input type="text" bind:value={m.source_key} placeholder="source.key" /></td>
                            <td class="value-preview">{m.source_key ? getBodyValue(payload, m.source_key) : "-"}</td>
                            <td class="arrow">&#8594;</td>
                            <td><input type="text" bind:value={m.target_key} placeholder="target.key" /></td>
                            <td><button class="btn-remove" onclick={() => removeMapping(i)}>x</button></td>
                          </tr>
                        {/each}
                      </tbody>
                    </table>
                    <button class="btn btn-outline btn-small" onclick={addCustomMapping}>+ Add Key</button>
                  </div>

                  <!-- Custom headers -->
                  <div class="fp-row">
                    <span class="fp-label">Headers</span>
                    {#each fwdHeaders as h, i}
                      <div class="header-row">
                        <input type="text" bind:value={h.key} placeholder="Header-Key" />
                        <input type="text" bind:value={h.value} placeholder="value" />
                        <button class="btn-remove" onclick={() => removeHeader(i)}>x</button>
                      </div>
                    {/each}
                    <button class="btn btn-outline btn-small" onclick={addCustomHeader}>+ Add Header</button>
                  </div>

                  <!-- Preview -->
                  <div class="fp-row">
                    <span class="fp-label">Preview</span>
                    <pre class="fp-preview">{buildMappedBody(payload)}</pre>
                  </div>

                  <!-- Actions -->
                  <div class="fp-actions">
                    <div class="save-rule">
                      <input type="text" placeholder="Rule name..." bind:value={fwdRuleName} />
                      <button class="btn btn-outline btn-small" onclick={saveRule} disabled={!fwdRuleName || !fwdUrl}>Save Rule</button>
                    </div>
                    <button class="btn btn-primary" onclick={() => doForward(payload)} disabled={!fwdUrl || fwdSending}>
                      {fwdSending ? "Sending..." : "Send"}
                    </button>
                  </div>

                  <!-- Result -->
                  {#if fwdResult}
                    <div class="fp-result" class:ok={fwdResult.status < 400} class:err={fwdResult.status >= 400}>
                      <span class="result-status">{fwdResult.status}</span>
                      <span class="result-time">{fwdResult.duration_ms}ms</span>
                      {#if fwdResult.body}
                        <pre class="result-body">{fwdResult.body.length > 500 ? fwdResult.body.slice(0, 500) + "..." : fwdResult.body}</pre>
                      {/if}
                    </div>
                  {/if}
                </div>
              {/if}
            {:else}
              <div class="payload-preview">
                <pre>{payload.body.length > 200 ? payload.body.slice(0, 200) + "..." : payload.body}</pre>
              </div>
            {/if}
          </div>
        {/each}
      {/if}
    </div>

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
        <pre class="curl-example">curl -X POST http://localhost:{serverStatus.port}{webhookPath} \
  -H "Content-Type: application/json" \
  -d '{"{"}hello": "world"{"}"}'</pre>
      </div>
    </aside>
  </div>
</main>

<style>
  :global(html, body) {
    margin: 0; padding: 0; background: #111318; color: #e1e4ea;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
    overflow: hidden; height: 100%;
  }
  :root {
    --bg: #111318; --bg-surface: #1a1d26; --bg-elevated: #222633; --bg-input: #161922;
    --border: #2a2f3e; --border-light: #353b4d; --text: #e1e4ea; --text-dim: #7c849a;
    --accent: #5b9cf6; --accent-soft: rgba(91,156,246,0.12); --accent-hover: #4a8be5;
    --green: #4ade80; --green-soft: rgba(74,222,128,0.12);
    --red: #f87171; --red-soft: rgba(248,113,113,0.12);
    --orange: #fb923c; --purple: #c084fc; --radius: 6px;
  }
  * { box-sizing: border-box; margin: 0; padding: 0; }
  .app { display: flex; flex-direction: column; height: 100vh; background: var(--bg); }

  .header { display: flex; justify-content: space-between; align-items: center; padding: 10px 20px; border-bottom: 1px solid var(--border); background: var(--bg-surface); flex-shrink: 0; }
  .header-left { display: flex; align-items: center; gap: 12px; }
  .header-left h1 { font-size: 16px; font-weight: 700; letter-spacing: -0.3px; color: var(--text); }
  .badge { font-size: 10px; padding: 2px 8px; border-radius: 10px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.5px; }
  .badge.running { background: var(--green-soft); color: var(--green); }
  .badge.stopped { background: var(--red-soft); color: var(--red); }
  .port-config { display: flex; align-items: center; gap: 8px; font-size: 12px; }
  .port-config label { color: var(--text-dim); }
  .port-config input { padding: 4px 8px; border-radius: var(--radius); border: 1px solid var(--border); background: var(--bg-input); color: var(--text); font-size: 12px; outline: none; }
  .port-config input[type="number"] { width: 64px; }
  .port-config input[type="text"] { width: 100px; }
  .port-config input:focus { border-color: var(--accent); }

  .toolbar { display: flex; justify-content: space-between; align-items: center; padding: 8px 20px; border-bottom: 1px solid var(--border); background: var(--bg-surface); flex-shrink: 0; }
  .toolbar-left, .toolbar-right { display: flex; align-items: center; gap: 8px; font-size: 12px; }
  .count { color: var(--text-dim); }
  .webhook-url { color: var(--text-dim); }
  .webhook-url code { background: var(--bg-input); padding: 2px 6px; border-radius: 4px; color: var(--accent); font-size: 11px; border: 1px solid var(--border); }
  .toggle-label { display: flex; align-items: center; gap: 4px; color: var(--text-dim); cursor: pointer; }

  .btn { padding: 5px 12px; border-radius: var(--radius); border: 1px solid var(--border); background: var(--bg-elevated); color: var(--text-dim); font-size: 12px; cursor: pointer; transition: all 0.15s; }
  .btn:hover { color: var(--text); background: var(--border); }
  .btn:disabled { opacity: 0.35; cursor: not-allowed; }
  .btn-small { padding: 3px 8px; font-size: 11px; }
  .btn-full { width: 100%; }
  .btn-primary { background: var(--accent); border-color: var(--accent); color: #fff; font-weight: 600; }
  .btn-primary:hover { background: var(--accent-hover); color: #fff; }
  .btn-outline { background: transparent; border-color: var(--border); }
  .btn-outline:hover { background: var(--bg-elevated); }
  .btn-danger { color: var(--red); border-color: rgba(248,113,113,0.25); }
  .btn-danger:hover { background: var(--red-soft); }

  .content { display: flex; flex: 1; overflow: hidden; background: var(--bg); }
  .payload-list { flex: 1; overflow-y: auto; padding: 12px 16px; background: var(--bg); }

  .empty-state { text-align: center; padding: 80px 20px; color: var(--text-dim); }
  .empty-icon { font-size: 42px; margin-bottom: 16px; opacity: 0.6; }
  .empty-state h3 { color: var(--text); margin-bottom: 8px; font-size: 15px; }
  .empty-state p { font-size: 13px; margin-bottom: 4px; }
  .empty-state code { background: var(--bg-surface); padding: 2px 6px; border-radius: 4px; color: var(--accent); font-size: 12px; border: 1px solid var(--border); }

  .payload-card { background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--radius); margin-bottom: 6px; overflow: hidden; transition: border-color 0.15s; }
  .payload-card:hover { border-color: var(--border-light); }
  .payload-card.expanded { border-color: var(--accent); }
  .payload-header { display: flex; justify-content: space-between; align-items: center; padding: 8px 14px; cursor: pointer; }
  .payload-header:hover { background: rgba(255,255,255,0.02); }
  .payload-meta { display: flex; align-items: center; gap: 8px; }
  .payload-meta-right { display: flex; align-items: center; gap: 10px; }
  .method-badge { font-size: 10px; font-weight: 700; padding: 2px 8px; border-radius: 4px; color: #fff; text-transform: uppercase; min-width: 44px; text-align: center; letter-spacing: 0.3px; }
  .payload-path { font-family: "SF Mono","Fira Code",Menlo,monospace; font-size: 12px; color: var(--text); }
  .payload-source { font-size: 11px; color: var(--text-dim); padding: 1px 6px; background: var(--bg-elevated); border-radius: 4px; }
  .json-tag { font-size: 10px; font-weight: 700; padding: 1px 6px; border-radius: 4px; background: rgba(251,191,36,0.12); color: #fbbf24; text-transform: uppercase; letter-spacing: 0.3px; }
  .payload-size { font-size: 11px; color: var(--text-dim); }
  .payload-time { font-size: 11px; color: var(--text-dim); white-space: nowrap; }
  .expand-icon { font-size: 8px; color: var(--text-dim); }
  .payload-preview { padding: 0 14px 8px 66px; }
  .payload-preview pre { font-size: 12px; color: var(--text-dim); white-space: pre-wrap; word-break: break-all; font-family: "SF Mono","Fira Code",Menlo,monospace; max-height: 48px; overflow: hidden; line-height: 1.5; }

  .payload-detail { padding: 0 14px 14px; border-top: 1px solid var(--border); background: var(--bg-surface); }
  .detail-section { margin-top: 10px; }
  .section-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 6px; }
  .section-header h4, .detail-section h4 { font-size: 11px; color: var(--text-dim); text-transform: uppercase; font-weight: 600; letter-spacing: 0.5px; }
  .section-actions { display: flex; align-items: center; gap: 6px; }
  .view-toggle { display: flex; border: 1px solid var(--border); border-radius: var(--radius); overflow: hidden; }
  .view-btn { padding: 2px 8px; font-size: 10px; font-weight: 600; background: transparent; border: none; color: var(--text-dim); cursor: pointer; transition: all 0.15s; }
  .view-btn:not(:last-child) { border-right: 1px solid var(--border); }
  .view-btn:hover { color: var(--text); background: var(--bg-elevated); }
  .view-btn.active { color: var(--accent); background: var(--accent-soft); }
  .headers-table { width: 100%; font-size: 12px; border-collapse: collapse; }
  .headers-table td { padding: 4px 8px; border-bottom: 1px solid var(--border); }
  .header-key { color: var(--accent); font-weight: 600; font-family: "SF Mono","Fira Code",Menlo,monospace; white-space: nowrap; width: 1%; }
  .header-value { color: var(--text-dim); font-family: "SF Mono","Fira Code",Menlo,monospace; word-break: break-all; }
  .body-content { background: var(--bg-input); border: 1px solid var(--border); border-radius: var(--radius); padding: 12px; font-size: 13px; font-family: "SF Mono","Fira Code",Menlo,monospace; white-space: pre-wrap; word-break: break-all; max-height: 500px; overflow-y: auto; color: var(--text-dim); line-height: 1.6; }
  .json-content { color: var(--text); }

  /* Forward Panel */
  .forward-panel {
    margin: 0 14px 14px; padding: 14px; background: var(--bg-input); border: 1px solid var(--border-light); border-radius: var(--radius);
  }
  .fp-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 12px; }
  .fp-header h4 { font-size: 13px; font-weight: 600; color: var(--accent); }
  .fp-row { margin-bottom: 12px; }
  .fp-row > .fp-label { display: block; font-size: 10px; color: var(--text-dim); text-transform: uppercase; font-weight: 600; letter-spacing: 0.5px; margin-bottom: 4px; }
  .fp-target { display: flex; gap: 6px; }
  .fp-target select { padding: 5px 8px; border-radius: var(--radius); border: 1px solid var(--border); background: var(--bg-surface); color: var(--text); font-size: 12px; font-weight: 600; outline: none; }
  .fp-target input { flex: 1; padding: 5px 10px; border-radius: var(--radius); border: 1px solid var(--border); background: var(--bg-surface); color: var(--text); font-size: 12px; font-family: "SF Mono","Fira Code",Menlo,monospace; outline: none; }
  .fp-target input:focus { border-color: var(--accent); }

  .fp-rules { display: flex; flex-wrap: wrap; gap: 4px; }
  .rule-chip { padding: 3px 10px; border-radius: 12px; border: 1px solid var(--border); background: var(--bg-surface); color: var(--text-dim); font-size: 11px; cursor: pointer; transition: all 0.15s; }
  .rule-chip:hover { border-color: var(--accent); color: var(--text); }
  .rule-delete { margin-left: 4px; color: var(--red); font-weight: 700; }

  .mapper-table { width: 100%; border-collapse: collapse; margin-bottom: 6px; font-size: 12px; }
  .mapper-table th { font-size: 10px; color: var(--text-dim); text-transform: uppercase; font-weight: 600; text-align: left; padding: 4px 6px; border-bottom: 1px solid var(--border); letter-spacing: 0.5px; }
  .mapper-table td { padding: 3px 4px; }
  .mapper-table input[type="text"] { width: 100%; padding: 3px 6px; border-radius: 4px; border: 1px solid var(--border); background: var(--bg-surface); color: var(--text); font-size: 11px; font-family: "SF Mono","Fira Code",Menlo,monospace; outline: none; }
  .mapper-table input[type="text"]:focus { border-color: var(--accent); }
  .mapper-table input[type="checkbox"] { accent-color: var(--accent); }
  .value-preview { color: var(--text-dim); font-size: 11px; font-family: "SF Mono","Fira Code",Menlo,monospace; max-width: 120px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .arrow { color: var(--accent); text-align: center; font-size: 14px; }
  .btn-remove { background: none; border: none; color: var(--text-dim); cursor: pointer; font-size: 12px; padding: 2px 4px; border-radius: 3px; }
  .btn-remove:hover { color: var(--red); background: var(--red-soft); }

  .header-row { display: flex; gap: 6px; margin-bottom: 4px; }
  .header-row input { flex: 1; padding: 3px 6px; border-radius: 4px; border: 1px solid var(--border); background: var(--bg-surface); color: var(--text); font-size: 11px; font-family: "SF Mono","Fira Code",Menlo,monospace; outline: none; }
  .header-row input:focus { border-color: var(--accent); }

  .fp-preview { background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--radius); padding: 8px; font-size: 12px; font-family: "SF Mono","Fira Code",Menlo,monospace; white-space: pre-wrap; word-break: break-all; color: var(--text); max-height: 150px; overflow-y: auto; line-height: 1.5; }

  .fp-actions { display: flex; justify-content: space-between; align-items: center; gap: 8px; }
  .save-rule { display: flex; gap: 6px; align-items: center; }
  .save-rule input { padding: 4px 8px; border-radius: var(--radius); border: 1px solid var(--border); background: var(--bg-surface); color: var(--text); font-size: 11px; outline: none; }
  .save-rule input:focus { border-color: var(--accent); }

  .fp-result { margin-top: 10px; padding: 10px; border-radius: var(--radius); border: 1px solid; }
  .fp-result.ok { background: var(--green-soft); border-color: rgba(74,222,128,0.3); }
  .fp-result.err { background: var(--red-soft); border-color: rgba(248,113,113,0.3); }
  .result-status { font-weight: 700; font-size: 14px; margin-right: 8px; }
  .fp-result.ok .result-status { color: var(--green); }
  .fp-result.err .result-status { color: var(--red); }
  .result-time { color: var(--text-dim); font-size: 12px; }
  .result-body { margin-top: 6px; font-size: 11px; font-family: "SF Mono","Fira Code",Menlo,monospace; white-space: pre-wrap; word-break: break-all; color: var(--text-dim); max-height: 200px; overflow-y: auto; background: rgba(0,0,0,0.2); border-radius: 4px; padding: 6px; }

  /* Sidebar */
  .sidebar { width: 300px; border-left: 1px solid var(--border); padding: 14px; overflow-y: auto; background: var(--bg-surface); flex-shrink: 0; }
  .sidebar h3 { font-size: 13px; margin-bottom: 12px; font-weight: 600; color: var(--text); }
  .form-group { margin-bottom: 10px; }
  .form-group label { display: block; font-size: 11px; color: var(--text-dim); margin-bottom: 4px; text-transform: uppercase; font-weight: 600; letter-spacing: 0.5px; }
  .form-group select, .form-group textarea { width: 100%; padding: 8px 10px; border-radius: var(--radius); border: 1px solid var(--border); background: var(--bg-input); color: var(--text); font-family: "SF Mono","Fira Code",Menlo,monospace; font-size: 12px; resize: vertical; outline: none; }
  .form-group select:focus, .form-group textarea:focus { border-color: var(--accent); box-shadow: 0 0 0 2px var(--accent-soft); }
  .sidebar-help { margin-top: 16px; padding-top: 12px; border-top: 1px solid var(--border); }
  .sidebar-help h4 { font-size: 11px; color: var(--text-dim); margin-bottom: 6px; text-transform: uppercase; font-weight: 600; letter-spacing: 0.5px; }
  .curl-example { background: var(--bg-input); border: 1px solid var(--border); border-radius: var(--radius); padding: 8px; font-size: 11px; font-family: "SF Mono","Fira Code",Menlo,monospace; white-space: pre-wrap; color: var(--text-dim); line-height: 1.5; }

  .error-bar { background: var(--red-soft); color: var(--red); padding: 6px 20px; font-size: 12px; border-bottom: 1px solid rgba(248,113,113,0.2); }

  ::-webkit-scrollbar { width: 6px; }
  ::-webkit-scrollbar-track { background: transparent; }
  ::-webkit-scrollbar-thumb { background: var(--border); border-radius: 3px; }
  ::-webkit-scrollbar-thumb:hover { background: var(--text-dim); }
</style>
