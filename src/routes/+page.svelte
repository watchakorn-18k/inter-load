<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { save } from "@tauri-apps/plugin-dialog";

  interface PayloadEntry {
    id: string;
    method: string;
    path: string;
    source_ip: string;
    headers: Record<string, string>;
    body: string;
    content_type: string | null;
    received_at: string;
    source_type: string;
  }

  interface ServerStatus { port: number; running: boolean; }
  interface MapEntry { source_key: string; target_key: string; enabled: boolean; }
  interface ForwardRule { id: string; name: string; target_url: string; method: string; mappings: MapEntry[]; headers: Record<string, string>; enabled: boolean; }
  interface ForwardResult { status: number; body: string; duration_ms: number; }
  interface AutoForwardLog { payload_id: string; rule_id: string; rule_name: string; status: number; body: string; duration_ms: number; forwarded_at: string; }
  interface WsStatus { active_connections: number; total_messages: number; }

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

  // Theme
  let theme = $state<"dark" | "light">("dark");

  // Filter
  let filterText = $state("");
  let filterMethod = $state("ALL");

  // Export
  let exportFormat = $state<"json" | "csv">("json");

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

  // Auto-forward logs
  let autoForwardLogs = $state<AutoForwardLog[]>([]);

  // WebSocket
  let wsStatus = $state<WsStatus>({ active_connections: 0, total_messages: 0 });

  // Proxy
  interface ProxyTrafficEntry {
    id: string; method: string; url: string; host: string; scheme: string; path: string;
    request_headers: Record<string, string>; request_body: string;
    response_status: number | null; response_headers: Record<string, string> | null; response_body: string | null;
    started_at: string; completed_at: string | null; duration_ms: number | null;
  }
  interface ProxyStatus { running: boolean; port: number; }
  let activeTab = $state<"webhook" | "proxy">("webhook");
  let proxyStatus = $state<ProxyStatus>({ running: false, port: 8080 });
  let proxyPort = $state("8080");
  let proxyTraffic = $state<ProxyTrafficEntry[]>([]);
  let proxyExpandedIds = $state<Set<string>>(new Set());
  let proxyFilter = $state("");
  let showCurlExamples = $state(false);
  let caCertPem = $state("");
  let showCaCert = $state(false);

  let filteredProxyTraffic = $derived(() => {
    if (!proxyFilter.trim()) return proxyTraffic;
    const q = proxyFilter.toLowerCase().trim();
    return proxyTraffic.filter(t =>
      t.url.toLowerCase().includes(q) ||
      t.host.toLowerCase().includes(q) ||
      t.request_body.toLowerCase().includes(q) ||
      t.method.toLowerCase().includes(q)
    );
  });

  let filteredPayloads = $derived(() => {
    let result = payloads;
    if (filterMethod !== "ALL") {
      result = result.filter(p => p.method === filterMethod);
    }
    if (filterText.trim()) {
      const q = filterText.toLowerCase().trim();
      result = result.filter(p =>
        p.path.toLowerCase().includes(q) ||
        p.body.toLowerCase().includes(q) ||
        p.source_ip.toLowerCase().includes(q)
      );
    }
    return result;
  });

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

  async function fetchAutoForwardLogs() {
    try { autoForwardLogs = await invoke<AutoForwardLog[]>("get_auto_forward_logs"); } catch { /* noop */ }
  }

  async function fetchWsStatus() {
    try { wsStatus = await invoke<WsStatus>("get_ws_status"); } catch { /* noop */ }
  }

  async function fetchProxyStatus() {
    try { proxyStatus = await invoke<ProxyStatus>("get_proxy_status"); } catch { /* noop */ }
  }

  async function fetchProxyTraffic() {
    try { proxyTraffic = await invoke<ProxyTrafficEntry[]>("get_proxy_traffic"); } catch { /* noop */ }
  }

  async function startProxy() {
    const port = parseInt(proxyPort, 10);
    if (isNaN(port) || port < 1 || port > 65535) { error = "Proxy port must be 1-65535"; return; }
    try {
      proxyStatus = await invoke<ProxyStatus>("start_proxy_cmd", { port });
      checkCaCertStatus();
    } catch (e) { error = String(e); }
  }

  async function stopProxy() {
    try { proxyStatus = await invoke<ProxyStatus>("stop_proxy_cmd"); } catch (e) { error = String(e); }
  }

  async function clearProxyTraffic() {
    try { await invoke("clear_proxy_traffic"); proxyTraffic = []; } catch (e) { error = String(e); }
  }

  async function setSystemProxy() {
    try {
      const msg = await invoke<string>("set_system_proxy", { port: parseInt(proxyPort, 10) });
      error = msg;
    } catch (e) { error = String(e); }
  }

  async function disableSystemProxy() {
    try {
      const msg = await invoke<string>("disable_system_proxy");
      error = msg;
    } catch (e) { error = String(e); }
  }

  async function fetchCaCert() {
    try {
      caCertPem = await invoke<string>("get_ca_cert_pem");
      showCaCert = true;
    } catch (e) { error = String(e); }
  }

  async function exportCaCert() {
    try {
      const pem = await invoke<string>("get_ca_cert_pem");
      const filePath = await save({
        defaultPath: "inter-load-ca.pem",
        filters: [{ name: "PEM", extensions: ["pem", "crt", "cer"] }],
      });
      if (filePath) {
        await invoke("write_export_file", { path: filePath, content: pem });
      }
    } catch (e) { error = String(e); }
  }

  let caInstallMsg = $state("");
  let caInstalling = $state(false);
  let caCertInstalled = $state(false);

  async function checkCaCertStatus() {
    try {
      caCertInstalled = await invoke<boolean>("check_ca_cert_installed");
      if (caCertInstalled) caInstallMsg = "CA certificate is already installed and trusted.";
    } catch { /* proxy not started yet */ }
  }

  async function installCaCert() {
    caInstalling = true; caInstallMsg = "";
    try {
      caInstallMsg = await invoke<string>("install_ca_cert");
      // Poll for cert status since Terminal install is async
      let attempts = 0;
      const poll = setInterval(async () => {
        attempts++;
        await checkCaCertStatus();
        if (caCertInstalled) {
          clearInterval(poll);
          setTimeout(() => { caInstallMsg = ""; }, 3000);
        }
        if (attempts > 30) clearInterval(poll);
      }, 2000);
      // Auto-hide message after 5s if no poll match
      setTimeout(() => { caInstallMsg = ""; }, 5000);
    } catch (e) { caInstallMsg = String(e); setTimeout(() => { caInstallMsg = ""; }, 5000); }
    finally { caInstalling = false; }
  }

  function toggleProxyExpand(id: string) {
    const next = new Set(proxyExpandedIds);
    if (next.has(id)) next.delete(id); else next.add(id);
    proxyExpandedIds = next;
  }

  function proxyStatusColor(status: number | null): string {
    if (!status) return "var(--text-dim)";
    if (status < 300) return "var(--green)";
    if (status < 400) return "var(--orange)";
    return "var(--red)";
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

  async function toggleAutoForward(ruleId: string) {
    try { await invoke("toggle_forward_rule", { id: ruleId }); await fetchForwardRules(); } catch (e) { error = String(e); }
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
        name: fwdRuleName, targetUrl: fwdUrl, method: fwdMethod, mappings, headers, enabled: true,
      });
      await fetchForwardRules();
    } catch (e) { error = String(e); }
  }

  async function deleteRule(id: string) {
    try { await invoke("delete_forward_rule", { id }); await fetchForwardRules(); } catch (e) { error = String(e); }
  }

  async function exportPayloads() {
    try {
      const content = await invoke<string>("export_payloads", { format: exportFormat });
      const ext = exportFormat;
      const filePath = await save({
        defaultPath: `payloads.${ext}`,
        filters: [{ name: ext.toUpperCase(), extensions: [ext] }],
      });
      if (filePath) {
        await invoke("write_export_file", { path: filePath, content });
      }
    } catch (e) { error = String(e); }
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
      case "WS": return "#e879f9";
      default: return "#94a3b8";
    }
  }

  $effect(() => {
    document.documentElement.setAttribute("data-theme", theme);
    localStorage.setItem("inter-load-theme", theme);
  });

  $effect(() => {
    const saved = localStorage.getItem("inter-load-theme");
    if (saved === "light" || saved === "dark") theme = saved;

    fetchPayloads(); fetchStatus(); fetchForwardRules(); fetchAutoForwardLogs(); fetchWsStatus(); fetchProxyStatus(); fetchProxyTraffic();
  });

  $effect(() => {
    if (pollInterval) clearInterval(pollInterval);
    if (autoRefresh) {
      pollInterval = setInterval(() => {
        fetchPayloads(); fetchAutoForwardLogs(); fetchWsStatus();
        if (activeTab === "proxy") fetchProxyTraffic();
      }, 2000);
    }
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
      <div class="tab-switch">
        <button class="tab-btn" class:active={activeTab === "webhook"} onclick={() => activeTab = "webhook"}>Webhook</button>
        <button class="tab-btn" class:active={activeTab === "proxy"} onclick={() => { activeTab = "proxy"; fetchProxyTraffic(); }}>
          Proxy
          {#if proxyStatus.running}<span class="tab-dot"></span>{/if}
        </button>
      </div>
    </div>
    <div class="header-right">
      <button class="btn btn-outline btn-small theme-toggle" onclick={() => theme = theme === "dark" ? "light" : "dark"}>
        {theme === "dark" ? "&#9728;" : "&#9790;"}
      </button>
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
      {#if wsStatus.active_connections > 0}
        <span class="ws-indicator active">WS: {wsStatus.active_connections} connected ({wsStatus.total_messages} msgs)</span>
      {:else}
        <span class="ws-indicator">WS: ws://localhost:{serverStatus.port}/ws</span>
      {/if}
    </div>
    <div class="toolbar-right">
      <label class="toggle-label"><input type="checkbox" bind:checked={autoRefresh} /> Auto</label>
      <button class="btn btn-outline btn-small" onclick={fetchPayloads}>Refresh</button>
      <button class="btn btn-outline btn-small" onclick={expandAll}>Expand All</button>
      <button class="btn btn-outline btn-small" onclick={collapseAll}>Collapse</button>
      <select bind:value={exportFormat} class="export-select">
        <option value="json">JSON</option>
        <option value="csv">CSV</option>
      </select>
      <button class="btn btn-outline btn-small" onclick={exportPayloads} disabled={payloads.length === 0}>Export</button>
      <button class="btn btn-danger btn-small" onclick={clearAll}>Clear</button>
    </div>
  </div>

  <div class="filter-bar">
    <input type="text" placeholder="Search path, body, IP..." bind:value={filterText} class="filter-input" />
    <select bind:value={filterMethod} class="filter-select">
      <option value="ALL">All Methods</option>
      <option>GET</option><option>POST</option><option>PUT</option>
      <option>DELETE</option><option>PATCH</option><option>TEST</option><option>WS</option>
    </select>
    {#if filterText || filterMethod !== "ALL"}
      <button class="btn btn-outline btn-small" onclick={() => { filterText = ''; filterMethod = 'ALL'; }}>Clear</button>
    {/if}
    <span class="filter-count">{filteredPayloads().length} of {payloads.length}</span>
  </div>

  {#if error}<div class="error-bar">{error}</div>{/if}

  {#if activeTab === "webhook"}
  <div class="toolbar">
    <div class="payload-list">
      {#if payloads.length === 0}
        <div class="empty-state">
          <div class="empty-icon">&#128230;</div>
          <h3>No payloads yet</h3>
          <p>Send a POST request to <code>http://localhost:{serverStatus.port}{webhookPath}</code></p>
          <p>or connect via <code>ws://localhost:{serverStatus.port}/ws</code></p>
          <p>or use the test form on the right</p>
        </div>
      {:else if filteredPayloads().length === 0}
        <div class="empty-state">
          <div class="empty-icon">&#128269;</div>
          <h3>No matches</h3>
          <p>No payloads match your filter. <button class="btn btn-outline btn-small" onclick={() => { filterText = ''; filterMethod = 'ALL'; }}>Clear filters</button></p>
        </div>
      {:else}
        {#each filteredPayloads() as payload (payload.id)}
          {@const isExpanded = expandedIds.has(payload.id)}
          {@const { text: bodyText, isJson } = isExpanded ? getBodyDisplay(payload) : { text: "", isJson: false }}
          {@const viewMode = viewModes.get(payload.id) ?? "pretty"}
          {@const showForward = forwardPanelId === payload.id}
          <div class="payload-card" class:expanded={isExpanded}>
            <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
            <div class="payload-header" role="button" tabindex="0" onclick={() => toggleExpand(payload.id)} onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); toggleExpand(payload.id); } }}>
              <div class="payload-meta">
                <span class="method-badge" style="background: {methodColor(payload.method)}">{payload.method}</span>
                {#if payload.source_type === "websocket"}
                  <span class="ws-tag">WS</span>
                {/if}
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
                          <div class="rule-chip-wrapper">
                            <button class="rule-chip" class:disabled={!rule.enabled} onclick={() => applyRule(rule)}>
                              {rule.name} ({rule.method} {rule.target_url.replace(/^https?:\/\/[^/]+/, "...")})
                              <span class="rule-delete" role="button" tabindex="-1" onclick={(e) => { e.stopPropagation(); deleteRule(rule.id); }}>x</span>
                            </button>
                            <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
                            <span class="rule-toggle" class:active={rule.enabled} role="button" tabindex="-1" onclick={(e) => { e.stopPropagation(); toggleAutoForward(rule.id); }}>
                              {rule.enabled ? "ON" : "OFF"}
                            </span>
                          </div>
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
        <textarea id="test-body" rows="8" bind:value={testBody}></textarea>
      </div>
      <button class="btn btn-primary btn-full" onclick={sendTest}>Send</button>

      <div class="sidebar-help">
        <h4>Quick Test (curl)</h4>
        <pre class="curl-example">curl -X POST http://localhost:{serverStatus.port}{webhookPath} \
  -H "Content-Type: application/json" \
  -d '{"{"}hello": "world"{"}"}'</pre>
      </div>

      <div class="sidebar-help">
        <h4>WebSocket Test</h4>
        <pre class="curl-example">ws://localhost:{serverStatus.port}/ws</pre>
      </div>

      {#if autoForwardLogs.length > 0}
        <div class="sidebar-help">
          <h4>Auto-Forward Log ({autoForwardLogs.length})</h4>
          <div class="af-logs">
            {#each autoForwardLogs.slice(0, 20) as log}
              <div class="af-log-entry" class:ok={log.status < 400} class:err={log.status >= 400}>
                <span class="af-rule-name">{log.rule_name}</span>
                <span class="af-status">{log.status}</span>
                <span class="af-time">{log.duration_ms}ms</span>
              </div>
            {/each}
          </div>
        </div>
      {/if}
    </aside>
  </div>
  {:else}
  <!-- Proxy Tab -->
  <div class="proxy-toolbar">
    <div class="toolbar-left">
      <span class="proxy-badge" class:active={proxyStatus.running}>
        {proxyStatus.running ? "Proxy Active" : "Proxy Off"}
      </span>
      <label class="port-label">Port:</label>
      <input type="number" bind:value={proxyPort} min="1" max="65535" class="proxy-port-input" disabled={proxyStatus.running} />
      {#if proxyStatus.running}
        <button class="btn btn-danger btn-small" onclick={stopProxy}>Stop Proxy</button>
      {:else}
        <button class="btn btn-primary btn-small" onclick={startProxy}>Start Proxy</button>
      {/if}
    </div>
    <div class="toolbar-right">
      <button class="btn btn-outline btn-small" onclick={setSystemProxy} disabled={!proxyStatus.running}>Set System Proxy</button>
      <button class="btn btn-outline btn-small" onclick={disableSystemProxy}>Disable System Proxy</button>
      <button class="btn btn-outline btn-small" onclick={fetchProxyTraffic}>Refresh</button>
      <button class="btn btn-outline btn-small" onclick={exportCaCert}>Export CA Cert</button>
      <button class="btn btn-primary btn-small" onclick={installCaCert} disabled={caInstalling || caCertInstalled}>
        {#if caCertInstalled}
          CA Cert Installed
        {:else if caInstalling}
          Installing...
        {:else}
          Install CA Cert
        {/if}
      </button>
      <button class="btn btn-outline btn-small" onclick={fetchCaCert}>Show CA Cert</button>
      <button class="btn btn-danger btn-small" onclick={clearProxyTraffic} disabled={proxyTraffic.length === 0}>Clear</button>
    </div>
  </div>

  {#if showCaCert}
    <div class="ca-cert-bar">
      <pre class="ca-cert-pre">{caCertPem}</pre>
      <button class="btn btn-small" onclick={() => showCaCert = false}>Close</button>
    </div>
  {/if}
  {#if caInstallMsg}
    <div class="ca-install-msg" class:ok={caInstallMsg.includes("success")} class:err={caInstallMsg.includes("cancel") || caInstallMsg.includes("Failed")}>
      {caInstallMsg}
    </div>
  {/if}

  <div class="proxy-info">
    <span class="proxy-hint">Set system proxy to <code>127.0.0.1:{proxyPort}</code> to capture traffic. Install the CA cert to decrypt HTTPS.</span>
    <input type="text" placeholder="Filter URL, host, body..." bind:value={proxyFilter} class="filter-input proxy-filter" />
    <span class="filter-count">{filteredProxyTraffic().length} of {proxyTraffic.length}</span>
  </div>

  <div class="proxy-content">
    {#if proxyTraffic.length === 0}
      <div class="empty-state">
        <div class="empty-icon">&#127760;</div>
        <h3>No proxy traffic</h3>
        <p>Start the proxy and set your system proxy to <code>127.0.0.1:{proxyPort}</code></p>
        <p>Install the CA certificate to decrypt HTTPS traffic</p>
        <button class="curl-toggle-btn" onclick={() => showCurlExamples = !showCurlExamples}>
          {showCurlExamples ? "▲ Hide" : "▼ Show"} curl examples
        </button>
        {#if showCurlExamples}
          <div class="curl-examples">
            <div class="curl-section">
              <h4>HTTP (works without CA cert)</h4>
              <pre class="curl-code">curl -x http://127.0.0.1:{proxyPort} http://httpbin.org/get</pre>
            </div>
            <div class="curl-section">
              <h4>HTTPS (requires CA cert installed)</h4>
              <pre class="curl-code">curl -x http://127.0.0.1:{proxyPort} https://httpbin.org/get</pre>
            </div>
            <div class="curl-section">
              <h4>POST with JSON body</h4>
              <pre class="curl-code">curl -x http://127.0.0.1:{proxyPort} -X POST https://httpbin.org/post -H "Content-Type: application/json" -d '&#123;"event": "test", "amount": 999&#125;'</pre>
            </div>
            <div class="curl-section">
              <h4>Node.js app</h4>
              <pre class="curl-code">HTTP_PROXY=http://127.0.0.1:{proxyPort} \
HTTPS_PROXY=http://127.0.0.1:{proxyPort} \
node your-app.js</pre>
            </div>
            <div class="curl-section">
              <h4>Python requests</h4>
              <pre class="curl-code">HTTP_PROXY=http://127.0.0.1:{proxyPort} \
HTTPS_PROXY=http://127.0.0.1:{proxyPort} \
python3 -c "import requests; print(requests.get('https://httpbin.org/get').json())"</pre>
            </div>
            <div class="curl-section">
              <h4>Browser (Chrome)</h4>
              <p class="curl-note">Set system proxy to <code>127.0.0.1:{proxyPort}</code> then browse normally.</p>
              <p class="curl-note" style="margin-top: 4px;"><strong>macOS:</strong> System Settings → Network → Wi-Fi → Details → Proxies → turn on Web Proxy (HTTP) & Secure Web Proxy (HTTPS) → Server: <code>127.0.0.1</code> Port: <code>{proxyPort}</code></p>
              <p class="curl-note"><strong>Windows:</strong> Settings → Network & Internet → Proxy → Manual proxy → Address: <code>127.0.0.1</code> Port: <code>{proxyPort}</code></p>
              <p class="curl-note" style="color: var(--orange); margin-top: 4px;">&#9888; YouTube/Google use QUIC (HTTP/3 over UDP) which bypasses proxy. Disable it:<br/>Open <code>chrome://flags/#enable-quic</code> → set to <strong>Disabled</strong> → Relaunch</p>
            </div>
            <div class="curl-section">
              <h4>Browser (Safari)</h4>
              <p class="curl-note">Set system proxy, then disable QUIC:<br/><code>defaults write com.apple.Safari IncludeInternalDebugMenu 1</code></p>
            </div>
            <div class="curl-section">
              <h4>Mobile phone</h4>
              <p class="curl-note">Connect phone & computer to same WiFi. Set phone WiFi proxy to <code>computer-ip:{proxyPort}</code>. Export CA cert and install on phone.</p>
            </div>
          </div>
        {/if}
      </div>
    {:else}
      <table class="proxy-table">
        <thead>
          <tr>
            <th>Method</th>
            <th>URL</th>
            <th>Status</th>
            <th>Time</th>
            <th>Scheme</th>
          </tr>
        </thead>
        <tbody>
          {#each filteredProxyTraffic() as entry (entry.id)}
            <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
            <tr class="proxy-row" class:expanded={proxyExpandedIds.has(entry.id)} onclick={() => toggleProxyExpand(entry.id)}>
              <td><span class="method-badge" style="background: {methodColor(entry.method)}">{entry.method}</span></td>
              <td class="proxy-url">{entry.url.length > 80 ? entry.url.slice(0, 80) + "..." : entry.url}</td>
              <td style="color: {proxyStatusColor(entry.response_status)}; font-weight: 700;">
                {entry.response_status ?? "..."}
              </td>
              <td class="proxy-time">{entry.duration_ms != null ? entry.duration_ms + "ms" : "..."}</td>
              <td><span class="scheme-tag" class:https={entry.scheme === "https"}>{entry.scheme.toUpperCase()}</span></td>
            </tr>
            {#if proxyExpandedIds.has(entry.id)}
              <tr class="proxy-detail-row"><td colspan="5">
                <div class="proxy-detail">
                  <div class="pd-section">
                    <h4>Request</h4>
                    <div class="pd-headers">
                      {#each Object.entries(entry.request_headers) as [k, v]}
                        <div><span class="header-key">{k}:</span> <span class="header-value">{v}</span></div>
                      {/each}
                    </div>
                    {#if entry.request_body}
                      <pre class="pd-body">{entry.request_body.length > 3000 ? entry.request_body.slice(0, 3000) + "..." : entry.request_body}</pre>
                    {/if}
                  </div>
                  {#if entry.response_status}
                    <div class="pd-section">
                      <h4>Response ({entry.response_status}) - {entry.duration_ms}ms</h4>
                      {#if entry.response_headers}
                        <div class="pd-headers">
                          {#each Object.entries(entry.response_headers) as [k, v]}
                            <div><span class="header-key">{k}:</span> <span class="header-value">{v}</span></div>
                          {/each}
                        </div>
                      {/if}
                      {#if entry.response_body}
                        <pre class="pd-body">{entry.response_body.length > 3000 ? entry.response_body.slice(0, 3000) + "..." : entry.response_body}</pre>
                      {/if}
                    </div>
                  {/if}
                </div>
              </td></tr>
            {/if}
          {/each}
        </tbody>
      </table>
    {/if}
  </div>
  {/if}
</main>

<style>
  :global(html, body) {
    margin: 0; padding: 0; background: var(--bg); color: var(--text);
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
    --scrollbar-thumb: #2a2f3e; --scrollbar-thumb-hover: #7c849a;
  }
  [data-theme="light"] {
    --bg: #f5f6fa; --bg-surface: #ffffff; --bg-elevated: #eef0f5; --bg-input: #f0f1f5;
    --border: #d8dbe5; --border-light: #c5c9d6; --text: #1a1d26; --text-dim: #5c6378;
    --accent: #3b7dd8; --accent-soft: rgba(59,125,216,0.08); --accent-hover: #2d6bc4;
    --green-soft: rgba(34,197,94,0.1); --red-soft: rgba(239,68,68,0.08);
    --scrollbar-thumb: #c5c9d6; --scrollbar-thumb-hover: #94a3b8;
  }
  * { box-sizing: border-box; margin: 0; padding: 0; }
  .app { display: flex; flex-direction: column; height: 100vh; background: var(--bg); }

  .header { display: flex; justify-content: space-between; align-items: center; padding: 10px 20px; border-bottom: 1px solid var(--border); background: var(--bg-surface); flex-shrink: 0; }
  .header-left { display: flex; align-items: center; gap: 12px; }
  .header-left h1 { font-size: 16px; font-weight: 700; letter-spacing: -0.3px; color: var(--text); }
  .header-right { display: flex; align-items: center; gap: 8px; }
  .badge { font-size: 10px; padding: 2px 8px; border-radius: 10px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.5px; }
  .badge.running { background: var(--green-soft); color: var(--green); }
  .badge.stopped { background: var(--red-soft); color: var(--red); }
  .theme-toggle { font-size: 14px; min-width: 34px; text-align: center; }
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

  .ws-indicator { font-size: 11px; color: var(--text-dim); padding: 1px 6px; background: var(--bg-elevated); border-radius: 4px; }
  .ws-indicator.active { color: #e879f9; background: rgba(232,121,249,0.12); }
  .ws-tag { font-size: 10px; font-weight: 700; padding: 1px 6px; border-radius: 4px; background: rgba(232,121,249,0.12); color: #e879f9; text-transform: uppercase; letter-spacing: 0.3px; }

  .export-select { padding: 3px 6px; border-radius: var(--radius); border: 1px solid var(--border); background: var(--bg-input); color: var(--text); font-size: 11px; }

  .filter-bar { display: flex; align-items: center; gap: 8px; padding: 6px 20px; border-bottom: 1px solid var(--border); background: var(--bg-surface); flex-shrink: 0; }
  .filter-input { flex: 1; padding: 5px 10px; border-radius: var(--radius); border: 1px solid var(--border); background: var(--bg-input); color: var(--text); font-size: 12px; outline: none; }
  .filter-input:focus { border-color: var(--accent); }
  .filter-select { padding: 5px 8px; border-radius: var(--radius); border: 1px solid var(--border); background: var(--bg-input); color: var(--text); font-size: 12px; }
  .filter-count { font-size: 11px; color: var(--text-dim); white-space: nowrap; }

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

  .empty-state { text-align: center; padding: 40px 20px 40px; color: var(--text-dim); }
  .empty-icon { font-size: 42px; margin-bottom: 16px; opacity: 0.6; }
  .empty-state h3 { color: var(--text); margin-bottom: 8px; font-size: 15px; }
  .empty-state p { font-size: 13px; margin-bottom: 4px; }
  .empty-state code { background: var(--bg-surface); padding: 2px 6px; border-radius: 4px; color: var(--accent); font-size: 12px; border: 1px solid var(--border); }
  .curl-toggle-btn { margin-top: 16px; padding: 6px 16px; font-size: 12px; background: var(--bg-surface); color: var(--accent); border: 1px solid var(--border); border-radius: 6px; cursor: pointer; transition: all 0.15s; }
  .curl-toggle-btn:hover { background: var(--accent); color: #fff; }
  .curl-examples { margin-top: 16px; text-align: left; max-width: 620px; margin-inline: auto; }
  .curl-section { margin-bottom: 12px; }
  .curl-section h4 { font-size: 11px; color: var(--text-dim); text-transform: uppercase; letter-spacing: 0.5px; margin-bottom: 4px; }
  .curl-code { background: var(--bg-input); border: 1px solid var(--border); border-radius: 6px; padding: 10px 14px; font-size: 12px; color: var(--green); font-family: "SF Mono", "Fira Code", monospace; overflow-x: auto; white-space: pre; margin: 0; }
  .curl-note { font-size: 12px; color: var(--text-dim); line-height: 1.5; }
  .curl-note code { background: var(--bg-surface); padding: 1px 5px; border-radius: 3px; font-size: 11px; color: var(--accent); border: 1px solid var(--border); }

  .payload-card { background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--radius); margin-bottom: 6px; overflow: hidden; transition: border-color 0.15s; }
  .payload-card:hover { border-color: var(--border-light); }
  .payload-card.expanded { border-color: var(--accent); }
  .payload-header { display: flex; justify-content: space-between; align-items: center; padding: 8px 14px; cursor: pointer; }
  .payload-header:hover { background: rgba(128,128,128,0.04); }
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
  .rule-chip-wrapper { display: flex; align-items: center; gap: 2px; }
  .rule-chip { padding: 3px 10px; border-radius: 12px; border: 1px solid var(--border); background: var(--bg-surface); color: var(--text-dim); font-size: 11px; cursor: pointer; transition: all 0.15s; }
  .rule-chip:hover { border-color: var(--accent); color: var(--text); }
  .rule-chip.disabled { opacity: 0.5; }
  .rule-delete { margin-left: 4px; color: var(--red); font-weight: 700; }
  .rule-toggle { font-size: 9px; font-weight: 700; padding: 1px 4px; border-radius: 3px; color: var(--red); background: var(--red-soft); cursor: pointer; user-select: none; }
  .rule-toggle.active { color: var(--green); background: var(--green-soft); }

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
  .result-body { margin-top: 6px; font-size: 11px; font-family: "SF Mono","Fira Code",Menlo,monospace; white-space: pre-wrap; word-break: break-all; color: var(--text-dim); max-height: 200px; overflow-y: auto; background: rgba(0,0,0,0.15); border-radius: 4px; padding: 6px; }

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

  .af-logs { display: flex; flex-direction: column; gap: 4px; max-height: 200px; overflow-y: auto; }
  .af-log-entry { display: flex; align-items: center; gap: 6px; padding: 3px 6px; border-radius: 4px; font-size: 11px; background: var(--bg-input); border: 1px solid var(--border); }
  .af-rule-name { flex: 1; color: var(--text); font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .af-status { font-weight: 700; }
  .af-time { color: var(--text-dim); }
  .af-log-entry.ok .af-status { color: var(--green); }
  .af-log-entry.err .af-status { color: var(--red); }

  .error-bar { background: var(--red-soft); color: var(--red); padding: 6px 20px; font-size: 12px; border-bottom: 1px solid rgba(248,113,113,0.2); }

  ::-webkit-scrollbar { width: 6px; }
  ::-webkit-scrollbar-track { background: transparent; }
  ::-webkit-scrollbar-thumb { background: var(--scrollbar-thumb); border-radius: 3px; }
  ::-webkit-scrollbar-thumb:hover { background: var(--scrollbar-thumb-hover); }

  /* Tab switch */
  .tab-switch { display: flex; gap: 2px; background: var(--bg-input); border-radius: 6px; padding: 2px; }
  .tab-btn { padding: 4px 12px; border: none; border-radius: 4px; background: transparent; color: var(--text-dim); font-size: 12px; font-weight: 600; cursor: pointer; transition: all 0.15s; display: flex; align-items: center; gap: 4px; }
  .tab-btn:hover { color: var(--text); }
  .tab-btn.active { background: var(--accent); color: #fff; }
  .tab-dot { width: 6px; height: 6px; border-radius: 50%; background: var(--green); display: inline-block; }

  /* Proxy */
  .proxy-toolbar { display: flex; justify-content: space-between; align-items: center; padding: 10px 20px; border-bottom: 1px solid var(--border); background: var(--bg-surface); flex-shrink: 0; }
  .proxy-badge { font-size: 11px; font-weight: 600; padding: 3px 10px; border-radius: 10px; background: var(--bg-elevated); color: var(--text-dim); }
  .proxy-badge.active { background: var(--green-soft); color: var(--green); }
  .port-label { font-size: 12px; color: var(--text-dim); }
  .proxy-port-input { padding: 4px 8px; border-radius: var(--radius); border: 1px solid var(--border); background: var(--bg-input); color: var(--text); font-size: 12px; width: 64px; outline: none; }
  .proxy-port-input:focus { border-color: var(--accent); }

  .proxy-info { display: flex; align-items: center; gap: 10px; padding: 6px 20px; border-bottom: 1px solid var(--border); background: var(--bg-surface); flex-shrink: 0; }
  .proxy-hint { font-size: 11px; color: var(--text-dim); white-space: nowrap; }
  .proxy-hint code { background: var(--bg-input); padding: 1px 5px; border-radius: 3px; font-size: 11px; color: var(--accent); border: 1px solid var(--border); }
  .proxy-filter { flex: 1; }

  .ca-cert-bar { display: flex; align-items: flex-start; gap: 10px; padding: 8px 20px; background: var(--bg-elevated); border-bottom: 1px solid var(--border); }
  .ca-cert-pre { flex: 1; font-size: 10px; font-family: "SF Mono","Fira Code",Menlo,monospace; white-space: pre-wrap; word-break: break-all; color: var(--text-dim); background: var(--bg-input); border: 1px solid var(--border); border-radius: var(--radius); padding: 8px; max-height: 120px; overflow-y: auto; margin: 0; }

  .ca-install-msg { padding: 6px 20px; font-size: 12px; border-bottom: 1px solid var(--border); background: var(--bg-surface); }
  .ca-install-msg.ok { color: var(--green); background: var(--green-soft); }
  .ca-install-msg.err { color: var(--red); background: var(--red-soft); }

  .proxy-content { flex: 1; overflow-y: auto; padding: 0; background: var(--bg); }

  .proxy-table { width: 100%; border-collapse: collapse; font-size: 12px; }
  .proxy-table th { position: sticky; top: 0; background: var(--bg-surface); font-size: 10px; color: var(--text-dim); text-transform: uppercase; font-weight: 600; text-align: left; padding: 6px 12px; border-bottom: 1px solid var(--border); letter-spacing: 0.5px; z-index: 1; }
  .proxy-row { cursor: pointer; border-bottom: 1px solid var(--border); transition: background 0.1s; }
  .proxy-row:hover { background: rgba(128,128,128,0.04); }
  .proxy-row.expanded { background: var(--accent-soft); }
  .proxy-row td { padding: 6px 12px; }
  .proxy-url { font-family: "SF Mono","Fira Code",Menlo,monospace; font-size: 11px; color: var(--text); max-width: 500px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .proxy-time { font-size: 11px; color: var(--text-dim); white-space: nowrap; }
  .scheme-tag { font-size: 9px; font-weight: 700; padding: 1px 5px; border-radius: 3px; background: var(--bg-elevated); color: var(--text-dim); text-transform: uppercase; }
  .scheme-tag.https { background: rgba(91,156,246,0.12); color: var(--accent); }

  .proxy-detail-row td { padding: 0 12px 12px; background: var(--bg-surface); }
  .proxy-detail { padding: 10px; border: 1px solid var(--border-light); border-radius: var(--radius); background: var(--bg-input); }
  .pd-section { margin-bottom: 10px; }
  .pd-section:last-child { margin-bottom: 0; }
  .pd-section h4 { font-size: 11px; color: var(--accent); margin-bottom: 6px; text-transform: uppercase; font-weight: 600; letter-spacing: 0.5px; }
  .pd-headers { font-size: 11px; margin-bottom: 6px; }
  .pd-headers div { padding: 2px 0; border-bottom: 1px solid var(--border); }
  .pd-body { background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--radius); padding: 8px; font-size: 11px; font-family: "SF Mono","Fira Code",Menlo,monospace; white-space: pre-wrap; word-break: break-all; color: var(--text-dim); max-height: 200px; overflow-y: auto; line-height: 1.5; margin: 0; }
</style>
