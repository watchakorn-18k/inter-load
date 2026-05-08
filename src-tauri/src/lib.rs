use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use axum::extract::{Request, State};
use axum::response::Json;
use axum::Router;
use axum::routing::{any, get};
use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use hudsucker::{
    certificate_authority::RcgenAuthority,
    Body, HttpContext, HttpHandler, Proxy, RequestOrResponse,
};
use http_body_util::{BodyExt, Full};
use hyper::{Request as HyperRequest, Response as HyperResponse};
use rcgen::{BasicConstraints, CertificateParams, IsCa, KeyPair};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use uuid::Uuid;

// ── Webhook structs ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayloadEntry {
    pub id: String,
    pub method: String,
    pub path: String,
    pub source_ip: String,
    pub headers: HashMap<String, String>,
    pub body: String,
    pub content_type: Option<String>,
    pub received_at: DateTime<Utc>,
    pub source_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppServerState {
    pub port: u16,
    pub running: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoForwardLog {
    pub payload_id: String,
    pub rule_id: String,
    pub rule_name: String,
    pub status: u16,
    pub body: String,
    pub duration_ms: u64,
    pub forwarded_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsStatus {
    pub active_connections: u32,
    pub total_messages: u64,
}

// ── Proxy structs ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyTrafficEntry {
    pub id: String,
    pub method: String,
    pub url: String,
    pub host: String,
    pub scheme: String,
    pub path: String,
    pub request_headers: HashMap<String, String>,
    pub request_body: String,
    pub response_status: Option<u16>,
    pub response_headers: Option<HashMap<String, String>>,
    pub response_body: Option<String>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyStatus {
    pub running: bool,
    pub port: u16,
}

// ── Shared state ──

pub struct AppState {
    pub payloads: Mutex<Vec<PayloadEntry>>,
    pub server_port: Mutex<u16>,
    pub webhook_path: Mutex<String>,
    pub shutdown_tx: Mutex<Option<tokio::sync::watch::Sender<bool>>>,
    pub forward_rules: Mutex<Vec<ForwardRule>>,
    pub auto_forward_logs: Mutex<Vec<AutoForwardLog>>,
    pub ws_connections: Mutex<u32>,
    pub ws_total_messages: Mutex<u64>,
    pub paused: Mutex<bool>,
    // Proxy
    pub proxy_traffic: Mutex<Vec<ProxyTrafficEntry>>,
    pub proxy_running: Mutex<bool>,
    pub proxy_port: Mutex<u16>,
    pub proxy_shutdown: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
    pub ca_cert_pem: Mutex<Option<String>>,
    pub ca_key_pem: Mutex<Option<String>>,
}

// ── Forward rule structs ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapEntry {
    pub source_key: String,
    pub target_key: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForwardRule {
    pub id: String,
    pub name: String,
    pub target_url: String,
    pub method: String,
    pub mappings: Vec<MapEntry>,
    pub headers: HashMap<String, String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForwardResult {
    pub status: u16,
    pub body: String,
    pub duration_ms: u64,
}

// ── Webhook Tauri commands ──

#[tauri::command]
fn get_payloads(state: tauri::State<'_, Arc<AppState>>) -> Result<Vec<PayloadEntry>, String> {
    let payloads = state.payloads.lock().map_err(|e| e.to_string())?;
    Ok(payloads.clone())
}

#[tauri::command]
fn clear_payloads(state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    let mut payloads = state.payloads.lock().map_err(|e| e.to_string())?;
    payloads.clear();
    Ok(())
}

#[tauri::command]
fn send_test_payload(
    state: tauri::State<'_, Arc<AppState>>,
    body: String,
    content_type: Option<String>,
) -> Result<PayloadEntry, String> {
    let mut headers = HashMap::new();
    headers.insert(
        "content-type".to_string(),
        content_type.clone().unwrap_or_else(|| "application/json".to_string()),
    );
    headers.insert("source".to_string(), "internal-test".to_string());

    let entry = PayloadEntry {
        id: Uuid::new_v4().to_string(),
        method: "TEST".to_string(),
        path: "/test".to_string(),
        source_ip: "localhost".to_string(),
        headers,
        body,
        content_type,
        received_at: Utc::now(),
        source_type: "http".to_string(),
    };

    let mut payloads = state.payloads.lock().map_err(|e| e.to_string())?;
    payloads.insert(0, entry.clone());
    Ok(entry)
}

#[tauri::command]
fn get_server_status(state: tauri::State<'_, Arc<AppState>>) -> Result<AppServerState, String> {
    let port = *state.server_port.lock().map_err(|e| e.to_string())?;
    let running = state.shutdown_tx.lock().map_err(|e| e.to_string())?.is_some();
    Ok(AppServerState { port, running })
}

#[tauri::command]
fn get_webhook_path(state: tauri::State<'_, Arc<AppState>>) -> Result<String, String> {
    let path = state.webhook_path.lock().map_err(|e| e.to_string())?;
    Ok(path.clone())
}

#[tauri::command]
fn restart_server(
    state: tauri::State<'_, Arc<AppState>>,
    port: u16,
    webhook_path: Option<String>,
) -> Result<AppServerState, String> {
    {
        let mut tx = state.shutdown_tx.lock().map_err(|e| e.to_string())?;
        if let Some(sender) = tx.take() {
            let _ = sender.send(true);
        }
    }
    {
        let mut current_port = state.server_port.lock().map_err(|e| e.to_string())?;
        *current_port = port;
    }
    if let Some(path) = webhook_path {
        let p = if path.starts_with('/') { path } else { format!("/{}", path) };
        let mut current_path = state.webhook_path.lock().map_err(|e| e.to_string())?;
        *current_path = p;
    }
    let wp = state.webhook_path.lock().map_err(|e| e.to_string())?.clone();
    let shared = state.inner().clone();
    start_server_bg(shared, port, wp);
    Ok(AppServerState { port, running: true })
}

#[tauri::command]
async fn forward_mapped(
    target_url: String,
    method: String,
    mapped_body: String,
    custom_headers: Option<HashMap<String, String>>,
) -> Result<ForwardResult, String> {
    let client = reqwest::Client::new();
    let start = std::time::Instant::now();
    let parsed_body: serde_json::Value = serde_json::from_str(&mapped_body)
        .map_err(|e| format!("Invalid JSON body: {}", e))?;
    let mut req = match method.to_uppercase().as_str() {
        "POST" => client.post(&target_url),
        "PUT" => client.put(&target_url),
        "PATCH" => client.patch(&target_url),
        "DELETE" => client.delete(&target_url),
        _ => client.post(&target_url),
    };
    req = req.json(&parsed_body);
    if let Some(headers) = custom_headers {
        for (key, value) in headers {
            req = req.header(&key, &value);
        }
    }
    let resp = req.send().await.map_err(|e| format!("Request failed: {}", e))?;
    let status = resp.status().as_u16();
    let resp_body = resp.text().await.unwrap_or_default();
    let duration_ms = start.elapsed().as_millis() as u64;
    Ok(ForwardResult { status, body: resp_body, duration_ms })
}

#[tauri::command]
fn save_forward_rule(
    state: tauri::State<'_, Arc<AppState>>,
    name: String,
    target_url: String,
    method: String,
    mappings: Vec<MapEntry>,
    headers: HashMap<String, String>,
    enabled: Option<bool>,
) -> Result<ForwardRule, String> {
    let rule = ForwardRule {
        id: Uuid::new_v4().to_string(),
        name,
        target_url,
        method,
        mappings,
        headers,
        enabled: enabled.unwrap_or(true),
    };
    let mut rules = state.forward_rules.lock().map_err(|e| e.to_string())?;
    rules.push(rule.clone());
    Ok(rule)
}

#[tauri::command]
fn get_forward_rules(state: tauri::State<'_, Arc<AppState>>) -> Result<Vec<ForwardRule>, String> {
    let rules = state.forward_rules.lock().map_err(|e| e.to_string())?;
    Ok(rules.clone())
}

#[tauri::command]
fn delete_forward_rule(
    state: tauri::State<'_, Arc<AppState>>,
    id: String,
) -> Result<(), String> {
    let mut rules = state.forward_rules.lock().map_err(|e| e.to_string())?;
    rules.retain(|r| r.id != id);
    Ok(())
}

#[tauri::command]
fn toggle_forward_rule(
    state: tauri::State<'_, Arc<AppState>>,
    id: String,
) -> Result<(), String> {
    let mut rules = state.forward_rules.lock().map_err(|e| e.to_string())?;
    if let Some(rule) = rules.iter_mut().find(|r| r.id == id) {
        rule.enabled = !rule.enabled;
    }
    Ok(())
}

#[tauri::command]
fn get_auto_forward_logs(state: tauri::State<'_, Arc<AppState>>) -> Result<Vec<AutoForwardLog>, String> {
    let logs = state.auto_forward_logs.lock().map_err(|e| e.to_string())?;
    Ok(logs.clone())
}

#[tauri::command]
fn get_ws_status(state: tauri::State<'_, Arc<AppState>>) -> Result<WsStatus, String> {
    let connections = *state.ws_connections.lock().map_err(|e| e.to_string())?;
    let total = *state.ws_total_messages.lock().map_err(|e| e.to_string())?;
    Ok(WsStatus { active_connections: connections, total_messages: total })
}

#[tauri::command]
fn export_payloads(
    state: tauri::State<'_, Arc<AppState>>,
    format: String,
    payload_ids: Option<Vec<String>>,
) -> Result<String, String> {
    let payloads = state.payloads.lock().map_err(|e| e.to_string())?;
    let selected: Vec<&PayloadEntry> = match payload_ids {
        Some(ids) => payloads.iter().filter(|p| ids.contains(&p.id)).collect(),
        None => payloads.iter().collect(),
    };
    match format.as_str() {
        "json" => {
            serde_json::to_string_pretty(&selected)
                .map_err(|e| format!("JSON export error: {}", e))
        }
        "csv" => {
            let mut wtr = csv::Writer::from_writer(Vec::new());
            wtr.write_record(["id", "method", "path", "source_ip", "body", "content_type", "received_at", "source_type"])
                .map_err(|e| format!("CSV header error: {}", e))?;
            for p in &selected {
                wtr.write_record([
                    &p.id, &p.method, &p.path, &p.source_ip, &p.body,
                    p.content_type.as_deref().unwrap_or(""), &p.received_at.to_rfc3339(), &p.source_type,
                ]).map_err(|e| format!("CSV row error: {}", e))?;
            }
            let bytes = wtr.into_inner().map_err(|e| format!("CSV flush: {}", e))?;
            String::from_utf8(bytes).map_err(|e| format!("CSV utf8: {}", e))
        }
        _ => Err("Unsupported format. Use 'json' or 'csv'.".into()),
    }
}

#[tauri::command]
fn write_export_file(path: String, content: String) -> Result<(), String> {
    std::fs::write(&path, &content)
        .map_err(|e| format!("Failed to write file: {}", e))
}

// ── Proxy Tauri commands ──

#[tauri::command]
fn get_proxy_status(state: tauri::State<'_, Arc<AppState>>) -> Result<ProxyStatus, String> {
    let running = *state.proxy_running.lock().map_err(|e| e.to_string())?;
    let port = *state.proxy_port.lock().map_err(|e| e.to_string())?;
    Ok(ProxyStatus { running, port })
}

#[tauri::command]
fn get_proxy_traffic(state: tauri::State<'_, Arc<AppState>>) -> Result<Vec<ProxyTrafficEntry>, String> {
    let traffic = state.proxy_traffic.lock().map_err(|e| e.to_string())?;
    Ok(traffic.clone())
}

#[tauri::command]
fn clear_proxy_traffic(state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    let mut traffic = state.proxy_traffic.lock().map_err(|e| e.to_string())?;
    traffic.clear();
    Ok(())
}

#[tauri::command]
fn get_ca_cert_pem(state: tauri::State<'_, Arc<AppState>>) -> Result<String, String> {
    let cert = state.ca_cert_pem.lock().map_err(|e| e.to_string())?;
    cert.clone().ok_or_else(|| "CA certificate not generated yet".to_string())
}

#[tauri::command]
fn check_ca_cert_installed(state: tauri::State<'_, Arc<AppState>>) -> Result<bool, String> {
    let cert_pem = state.ca_cert_pem.lock().map_err(|e| e.to_string())?
        .clone()
        .ok_or_else(|| "CA certificate not generated yet. Start the proxy first.".to_string())?;

    // Write cert to temp for verification
    let cert_path = std::env::temp_dir().join("inter-load-ca.pem");
    std::fs::write(&cert_path, &cert_pem)
        .map_err(|e| format!("Failed to write temp cert: {}", e))?;

    // Check if cert is already in System keychain and trusted
    let output = std::process::Command::new("security")
        .args(["find-certificate", "-a", "-c", "Inter-Load CA", "-p", "/Library/Keychains/System.keychain"])
        .output()
        .map_err(|e| format!("Failed to check cert: {}", e))?;

    if output.status.success() && !output.stdout.is_empty() {
        // Verify it's the same cert by comparing
        let installed_pem = String::from_utf8_lossy(&output.stdout);
        if installed_pem.contains("BEGIN CERTIFICATE") {
            return Ok(true);
        }
    }
    Ok(false)
}

#[tauri::command]
fn install_ca_cert(state: tauri::State<'_, Arc<AppState>>) -> Result<String, String> {
    let cert_pem = state.ca_cert_pem.lock().map_err(|e| e.to_string())?
        .clone()
        .ok_or_else(|| "CA certificate not generated yet. Start the proxy first.".to_string())?;

    // Write cert to a fixed, user-accessible location
    let cert_path = std::env::temp_dir().join("inter-load-ca.pem");
    std::fs::write(&cert_path, &cert_pem)
        .map_err(|e| format!("Failed to write temp cert: {}", e))?;

    let cert_path_str = cert_path.to_string_lossy().to_string();

    // macOS won't allow admin privilege escalation from Tauri's process context.
    // We write a small helper shell script and open Terminal to run it with sudo.
    let helper_script = format!(
        "#!/bin/bash\n\
echo \"Installing Inter-Load CA certificate...\"\n\
echo \"Your password is needed to add the cert to the System Keychain.\"\n\
echo \"\"\n\
echo \"Removing old Inter-Load CA certificates...\"\n\
while sudo security delete-certificate -c \"Inter-Load CA\" /Library/Keychains/System.keychain 2>/dev/null; do\n\
    :\n\
done\n\
echo \"Adding new certificate...\"\n\
sudo security add-trusted-cert -d -r trustRoot -k /Library/Keychains/System.keychain \"{cert_path_str}\"\n\
if [ $? -eq 0 ]; then\n\
    echo \"\"\n\
    echo \"Success! CA certificate installed and trusted.\"\n\
else\n\
    echo \"\"\n\
    echo \"Failed to install. Try manually:\"\n\
    echo \"  1. Open Keychain Access\"\n\
    echo \"  2. Drag the cert file into System keychain\"\n\
    echo \"  3. Double-click the cert, set Trust to Always Trust\"\n\
fi\n\
echo \"\"\n\
echo \"You can close this Terminal window.\"\n\
read -n 1 -s"
    );

    let script_path = std::env::temp_dir().join("inter-load-install-cert.sh");
    std::fs::write(&script_path, &helper_script)
        .map_err(|e| format!("Failed to write helper script: {}", e))?;

    // Make script executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("Failed to set permissions: {}", e))?;
    }

    // Open Terminal.app and run the script — Terminal handles the sudo password prompt natively
    std::process::Command::new("open")
        .arg("-a")
        .arg("Terminal")
        .arg(&script_path)
        .spawn()
        .map_err(|e| format!("Failed to open Terminal: {}", e))?;

    Ok("CA cert installation script opened in Terminal. Enter your password there to complete.".to_string())
}

#[tauri::command]
fn remove_ca_cert() -> Result<String, String> {
    let helper_script = r#"#!/bin/bash
echo "Removing Inter-Load CA certificates..."
echo "Your password is needed to remove from System Keychain."
echo ""
while sudo security delete-certificate -c "Inter-Load CA" /Library/Keychains/System.keychain 2>/dev/null; do
    :
done
echo ""
echo "All Inter-Load CA certificates removed."
echo ""
echo "You can close this Terminal window."
read -n 1 -s"#;

    let script_path = std::env::temp_dir().join("inter-load-remove-cert.sh");
    std::fs::write(&script_path, helper_script)
        .map_err(|e| format!("Failed to write script: {}", e))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("Failed to set permissions: {}", e))?;
    }

    std::process::Command::new("open")
        .arg("-a")
        .arg("Terminal")
        .arg(&script_path)
        .spawn()
        .map_err(|e| format!("Failed to open Terminal: {}", e))?;

    Ok("Terminal opened to remove CA cert. Enter your password there.".to_string())
}

#[tauri::command]
fn open_system_proxy_settings() -> Result<String, String> {
    std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.Network-Settings.extension")
        .spawn()
        .map_err(|e| format!("Failed to open System Settings: {}", e))?;
    Ok("Opened Network settings. Go to Proxies tab to configure.".to_string())
}

#[tauri::command]
fn set_system_proxy(port: u16) -> Result<String, String> {
    // Try to set proxy automatically using networksetup
    // Get the primary network service (usually "Wi-Fi")
    let services_output = std::process::Command::new("networksetup")
        .args(["-listallnetworkservices"])
        .output()
        .map_err(|e| format!("Failed to list network services: {}", e))?;

    let services_str = String::from_utf8_lossy(&services_output.stdout);
    let mut services: Vec<String> = services_str.lines()
        .skip(1) // Skip header line
        .filter(|s| !s.is_empty() && !s.contains("*"))
        .map(|s| s.trim().to_string())
        .collect();

    // Prefer Wi-Fi, then Ethernet
    services.sort_by_key(|s| {
        if s.to_lowercase().contains("wi-fi") { 0 }
        else if s.to_lowercase().contains("ethernet") { 1 }
        else { 2 }
    });

    let service = services.first()
        .ok_or_else(|| "No network service found".to_string())?
        .clone();

    let port_str = port.to_string();

    // Set HTTP proxy
    let http_out = std::process::Command::new("networksetup")
        .args(["-setwebproxy", &service, "127.0.0.1", &port_str])
        .output()
        .map_err(|e| format!("Failed to set HTTP proxy: {}", e))?;

    if !http_out.status.success() {
        let err = String::from_utf8_lossy(&http_out.stderr);
        return Err(format!("Failed to set HTTP proxy: {}. Try setting manually in System Settings.", err));
    }

    // Enable HTTP proxy
    let _ = std::process::Command::new("networksetup")
        .args(["-setwebproxystate", &service, "on"])
        .output();

    // Set HTTPS proxy
    let https_out = std::process::Command::new("networksetup")
        .args(["-setsecurewebproxy", &service, "127.0.0.1", &port_str])
        .output()
        .map_err(|e| format!("Failed to set HTTPS proxy: {}", e))?;

    if !https_out.status.success() {
        let err = String::from_utf8_lossy(&https_out.stderr);
        return Err(format!("Failed to set HTTPS proxy: {}. Try setting manually in System Settings.", err));
    }

    // Enable HTTPS proxy
    let _ = std::process::Command::new("networksetup")
        .args(["-setsecurewebproxystate", &service, "on"])
        .output();

    Ok(format!("System proxy set to 127.0.0.1:{} on \"{}\". Remember to disable when done!", port, service))
}

#[tauri::command]
fn disable_system_proxy() -> Result<String, String> {
    let services_output = std::process::Command::new("networksetup")
        .args(["-listallnetworkservices"])
        .output()
        .map_err(|e| format!("Failed to list network services: {}", e))?;

    let services_str = String::from_utf8_lossy(&services_output.stdout);
    let services: Vec<String> = services_str.lines()
        .skip(1)
        .filter(|s| !s.is_empty() && !s.contains("*"))
        .map(|s| s.trim().to_string())
        .collect();

    for service in &services {
        let _ = std::process::Command::new("networksetup")
            .args(["-setwebproxystate", service, "off"])
            .output();
        let _ = std::process::Command::new("networksetup")
            .args(["-setsecurewebproxystate", service, "off"])
            .output();
    }

    Ok("System proxy disabled.".to_string())
}

#[tauri::command]
fn start_proxy_cmd(
    state: tauri::State<'_, Arc<AppState>>,
    port: u16,
) -> Result<ProxyStatus, String> {
    let already_running = *state.proxy_running.lock().map_err(|e| e.to_string())?;
    if already_running {
        return Err("Proxy already running".to_string());
    }

    // Load or generate CA cert (persisted to disk)
    {
        let has_cert = state.ca_cert_pem.lock().map_err(|e| e.to_string())?.is_some();
        if !has_cert {
            let config_dir = dirs::data_local_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join("inter-load");
            std::fs::create_dir_all(&config_dir).ok();
            let cert_path = config_dir.join("ca-cert.pem");
            let key_path = config_dir.join("ca-key.pem");

            let (cert_pem, key_pem) = if cert_path.exists() && key_path.exists() {
                // Load existing cert from disk
                let c = std::fs::read_to_string(&cert_path)
                    .map_err(|e| format!("Failed to read saved cert: {}", e))?;
                let k = std::fs::read_to_string(&key_path)
                    .map_err(|e| format!("Failed to read saved key: {}", e))?;
                (c, k)
            } else {
                // Generate new cert and save to disk
                let (c, k) = generate_ca_cert()?;
                std::fs::write(&cert_path, &c).ok();
                std::fs::write(&key_path, &k).ok();
                (c, k)
            };

            *state.ca_cert_pem.lock().map_err(|e| e.to_string())? = Some(cert_pem);
            *state.ca_key_pem.lock().map_err(|e| e.to_string())? = Some(key_pem);
        }
    }

    let cert_pem = state.ca_cert_pem.lock().map_err(|e| e.to_string())?.clone().unwrap();
    let key_pem = state.ca_key_pem.lock().map_err(|e| e.to_string())?.clone().unwrap();

    *state.proxy_port.lock().map_err(|e| e.to_string())? = port;

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    *state.proxy_shutdown.lock().map_err(|e| e.to_string())? = Some(shutdown_tx);

    let shared = state.inner().clone();
    start_proxy_bg(shared, port, cert_pem, key_pem, shutdown_rx);

    *state.proxy_running.lock().map_err(|e| e.to_string())? = true;
    Ok(ProxyStatus { running: true, port })
}

#[tauri::command]
fn stop_proxy_cmd(state: tauri::State<'_, Arc<AppState>>) -> Result<ProxyStatus, String> {
    let running = *state.proxy_running.lock().map_err(|e| e.to_string())?;
    if !running {
        return Ok(ProxyStatus { running: false, port: 0 });
    }

    if let Some(tx) = state.proxy_shutdown.lock().map_err(|e| e.to_string())?.take() {
        let _ = tx.send(());
    }
    *state.proxy_running.lock().map_err(|e| e.to_string())? = false;

    let port = *state.proxy_port.lock().map_err(|e| e.to_string())?;
    Ok(ProxyStatus { running: false, port })
}

// ── CA cert generation ──

fn generate_ca_cert() -> Result<(String, String), String> {
    use rcgen::{RsaKeySize, PKCS_RSA_SHA256};
    let key_pair = KeyPair::generate_rsa_for(&PKCS_RSA_SHA256, RsaKeySize::_2048)
        .map_err(|e| format!("RSA key generation failed: {}", e))?;
    let mut params = CertificateParams::default();
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.distinguished_name.push(rcgen::DnType::CommonName, "Inter-Load CA");
    params.distinguished_name.push(rcgen::DnType::OrganizationName, "Inter-Load");

    let cert = params.self_signed(&key_pair)
        .map_err(|e| format!("Cert signing failed: {}", e))?;

    let cert_pem = cert.pem();
    let key_pem = key_pair.serialize_pem();
    Ok((cert_pem, key_pem))
}

// ── Proxy HttpHandler ──

#[derive(Clone)]
struct ProxyHandler {
    state: Arc<AppState>,
    pending: Arc<Mutex<HashMap<String, String>>>, // client_addr:port -> entry_id
}

impl HttpHandler for ProxyHandler {
    async fn handle_request(
        &mut self,
        ctx: &HttpContext,
        req: HyperRequest<Body>,
    ) -> RequestOrResponse {
        let method = req.method().clone();
        let uri = req.uri().clone();
        let url = uri.to_string();
        let host = uri.host().unwrap_or("unknown").to_string();
        let scheme = if method == hyper::Method::CONNECT {
            "https".to_string()
        } else {
            uri.scheme_str().unwrap_or("http").to_string()
        };
        let path = uri.path().to_string();

        let mut req_headers = HashMap::new();
        for (k, v) in req.headers() {
            if let Ok(s) = v.to_str() {
                req_headers.insert(k.to_string(), s.to_string());
            }
        }

        let client_key = ctx.client_addr.to_string();
        let id = Uuid::new_v4().to_string();

        // CONNECT requests MUST be returned as-is to preserve the upgrade state.
        // hudsucker handles the TLS MITM internally after we return.
        if method == hyper::Method::CONNECT {
            let entry = ProxyTrafficEntry {
                id: id.clone(),
                method: method.to_string(),
                url,
                host,
                scheme,
                path,
                request_headers: req_headers,
                request_body: String::new(),
                response_status: None,
                response_headers: None,
                response_body: None,
                started_at: Utc::now(),
                completed_at: None,
                duration_ms: None,
            };

            if let Ok(mut traffic) = self.state.proxy_traffic.lock() {
                traffic.insert(0, entry);
                traffic.truncate(2000);
            }
            if let Ok(mut pending) = self.pending.lock() {
                pending.insert(client_key, id);
            }

            return req.into();
        }

        // For non-CONNECT: consume body, record, rebuild and return
        let (parts, body) = req.into_parts();
        let body_bytes = body.collect().await
            .map(|b| b.to_bytes())
            .unwrap_or_default();
        let request_body = String::from_utf8_lossy(&body_bytes).to_string();

        let entry = ProxyTrafficEntry {
            id: id.clone(),
            method: method.to_string(),
            url,
            host,
            scheme,
            path,
            request_headers: req_headers,
            request_body,
            response_status: None,
            response_headers: None,
            response_body: None,
            started_at: Utc::now(),
            completed_at: None,
            duration_ms: None,
        };

        if let Ok(mut traffic) = self.state.proxy_traffic.lock() {
            traffic.insert(0, entry);
            traffic.truncate(2000);
        }
        if let Ok(mut pending) = self.pending.lock() {
            pending.insert(client_key, id);
        }

        let mut new_req = HyperRequest::builder()
            .method(method)
            .uri(uri);
        for (k, v) in &parts.headers {
            new_req = new_req.header(k, v);
        }
        new_req.body(Body::from(Full::new(body_bytes)))
            .expect("Failed to rebuild request")
            .into()
    }

    async fn handle_response(
        &mut self,
        ctx: &HttpContext,
        res: HyperResponse<Body>,
    ) -> HyperResponse<Body> {
        let client_key = ctx.client_addr.to_string();
        let status = res.status().as_u16();

        let mut resp_headers = HashMap::new();
        for (k, v) in res.headers() {
            if let Ok(s) = v.to_str() {
                resp_headers.insert(k.to_string(), s.to_string());
            }
        }

        // Collect body by consuming the response
        let (parts, body) = res.into_parts();
        let body_bytes = body.collect().await
            .map(|b| b.to_bytes())
            .unwrap_or_default();
        let body_vec = body_bytes.to_vec();
        let response_body = truncate_body(&String::from_utf8_lossy(&body_vec), 50000);

        let now = Utc::now();

        if let Ok(mut pending) = self.pending.lock() {
            if let Some(entry_id) = pending.remove(&client_key) {
                if let Ok(mut traffic) = self.state.proxy_traffic.lock() {
                    if let Some(entry) = traffic.iter_mut().find(|e| e.id == entry_id) {
                        entry.response_status = Some(status);
                        entry.response_headers = Some(resp_headers);
                        entry.response_body = Some(response_body);
                        entry.completed_at = Some(now);
                        entry.duration_ms = Some(
                            now.signed_duration_since(entry.started_at)
                                .num_milliseconds().max(0) as u64
                        );
                    }
                }
            }
        }

        // Rebuild response with the collected body
        let mut builder = HyperResponse::builder().status(parts.status);
        for (k, v) in &parts.headers {
            builder = builder.header(k, v);
        }
        builder.body(Body::from(Full::new(body_bytes))).expect("Failed to rebuild response")
    }
}

fn truncate_body(s: &str, max_len: usize) -> String {
    if s.len() > max_len {
        format!("{}... (truncated, {} bytes total)", &s[..max_len.min(s.len())], s.len())
    } else {
        s.to_string()
    }
}

// ── Proxy server background thread ──

fn start_proxy_bg(
    state: Arc<AppState>,
    port: u16,
    cert_pem: String,
    key_pem: String,
    mut shutdown_rx: tokio::sync::oneshot::Receiver<()>,
) {
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("Failed to create proxy Tokio runtime");
        rt.block_on(async move {
            let key_pair = match hudsucker::rcgen::KeyPair::from_pem(&key_pem) {
                Ok(kp) => kp,
                Err(e) => { eprintln!("Failed to parse CA key: {}", e); return; }
            };

            // Parse CA cert PEM back into rcgen Certificate
            let ca_cert = match rcgen::CertificateParams::from_ca_cert_pem(&cert_pem) {
                Ok(params) => params.self_signed(&key_pair).expect("Failed to sign CA cert"),
                Err(e) => {
                    // Fallback: generate fresh cert from params
                    eprintln!("Failed to parse CA cert PEM: {}, generating fresh", e);
                    let mut params = rcgen::CertificateParams::default();
                    params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
                    params.distinguished_name.push(rcgen::DnType::CommonName, "Inter-Load CA");
                    params.self_signed(&key_pair).expect("Failed to sign CA cert")
                }
            };

            let ca = RcgenAuthority::new(key_pair, ca_cert, 1000);

            let handler = ProxyHandler {
                state: state.clone(),
                pending: Arc::new(Mutex::new(HashMap::new())),
            };

            let proxy = Proxy::builder()
                .with_addr(SocketAddr::from(([127, 0, 0, 1], port)))
                .with_rustls_client()
                .with_ca(ca)
                .with_http_handler(handler)
                .build();

            println!("MITM proxy listening on port {}", port);

            tokio::select! {
                result = proxy.start() => {
                    if let Err(e) = result {
                        eprintln!("Proxy error: {}", e);
                    }
                }
                _ = &mut shutdown_rx => {
                    println!("Proxy shutting down");
                }
            }

            *state.proxy_running.lock().unwrap() = false;
        });
    });
}

// ── Webhook server ──

async fn handle_webhook(
    State(shared): State<Arc<AppState>>,
    req: Request,
) -> Json<serde_json::Value> {
    let method = req.method().to_string();
    let path = req.uri().path().to_string();
    let source_ip = req
        .headers()
        .get("x-forwarded-for")
        .or_else(|| req.headers().get("x-real-ip"))
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();

    let mut headers = HashMap::new();
    for (name, value) in req.headers() {
        if let Ok(v) = value.to_str() {
            headers.insert(name.to_string(), v.to_string());
        }
    }

    let content_type = headers.get("content-type").cloned();

    let body_bytes = axum::body::to_bytes(req.into_body(), 10 * 1024 * 1024)
        .await
        .unwrap_or_default();
    let body = String::from_utf8_lossy(&body_bytes).to_string();

    let entry = PayloadEntry {
        id: Uuid::new_v4().to_string(),
        method,
        path,
        source_ip,
        headers,
        body: body.clone(),
        content_type,
        received_at: Utc::now(),
        source_type: "http".to_string(),
    };

    let payload_id = entry.id.clone();

    if let Ok(mut payloads) = shared.payloads.lock() {
        payloads.insert(0, entry);
        payloads.truncate(500);
    }

    let rules = shared.forward_rules.lock().unwrap();
    let enabled_rules: Vec<_> = rules.iter().filter(|r| r.enabled).cloned().collect();
    drop(rules);

    for rule in enabled_rules {
        let mapped_body = build_auto_mapped_body(&body, &rule.mappings);
        let shared_clone = shared.clone();
        let payload_id = payload_id.clone();
        let rule_id = rule.id.clone();
        let rule_name = rule.name.clone();
        tokio::spawn(async move {
            let result = auto_forward_request(
                rule.target_url, rule.method, mapped_body, rule.headers,
            ).await;
            if let Ok(fwd_result) = result {
                let log = AutoForwardLog {
                    payload_id,
                    rule_id,
                    rule_name,
                    status: fwd_result.status,
                    body: fwd_result.body,
                    duration_ms: fwd_result.duration_ms,
                    forwarded_at: Utc::now(),
                };
                if let Ok(mut logs) = shared_clone.auto_forward_logs.lock() {
                    logs.insert(0, log);
                    logs.truncate(500);
                }
            }
        });
    }

    Json(serde_json::json!({ "status": "ok", "message": "payload received" }))
}

fn build_auto_mapped_body(source_body: &str, mappings: &[MapEntry]) -> String {
    let source: serde_json::Value = match serde_json::from_str(source_body) {
        Ok(v) => v,
        Err(_) => return "{}".to_string(),
    };
    let source_obj = match source.as_object() {
        Some(o) => o,
        None => return "{}".to_string(),
    };
    let mut output = serde_json::Map::new();
    for m in mappings {
        if !m.enabled || m.source_key.is_empty() || m.target_key.is_empty() { continue; }
        if let Some(val) = source_obj.get(&m.source_key) {
            output.insert(m.target_key.clone(), val.clone());
        }
    }
    serde_json::Value::Object(output).to_string()
}

async fn auto_forward_request(
    target_url: String, method: String, body: String, headers: HashMap<String, String>,
) -> Result<ForwardResult, String> {
    let client = reqwest::Client::new();
    let start = std::time::Instant::now();
    let parsed: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
    let mut req = match method.to_uppercase().as_str() {
        "POST" => client.post(&target_url), "PUT" => client.put(&target_url),
        "PATCH" => client.patch(&target_url), "DELETE" => client.delete(&target_url),
        _ => client.post(&target_url),
    };
    req = req.json(&parsed);
    for (k, v) in &headers { req = req.header(k.as_str(), v.as_str()); }
    let resp = req.send().await.map_err(|e| e.to_string())?;
    let status = resp.status().as_u16();
    let resp_body = resp.text().await.unwrap_or_default();
    Ok(ForwardResult { status, body: resp_body, duration_ms: start.elapsed().as_millis() as u64 })
}

async fn health_check() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok", "service": "inter-load-webhook" }))
}

async fn handle_websocket(
    State(shared): State<Arc<AppState>>,
    ws: axum::extract::WebSocketUpgrade,
) -> axum::response::Response {
    ws.on_upgrade(move |socket| handle_ws_socket(socket, shared))
}

async fn handle_ws_socket(socket: axum::extract::ws::WebSocket, shared: Arc<AppState>) {
    {
        let mut count = shared.ws_connections.lock().unwrap();
        *count += 1;
    }
    let mut socket = socket;
    while let Some(msg) = socket.next().await {
        match msg {
            Ok(axum::extract::ws::Message::Text(text)) => {
                let entry = PayloadEntry {
                    id: Uuid::new_v4().to_string(), method: "WS".to_string(),
                    path: "/ws".to_string(), source_ip: "websocket".to_string(),
                    headers: HashMap::new(), body: text.to_string(),
                    content_type: Some("text/plain".to_string()), received_at: Utc::now(),
                    source_type: "websocket".to_string(),
                };
                if let Ok(mut payloads) = shared.payloads.lock() { payloads.insert(0, entry); payloads.truncate(500); }
                if let Ok(mut total) = shared.ws_total_messages.lock() { *total += 1; }
            }
            Ok(axum::extract::ws::Message::Binary(data)) => {
                let entry = PayloadEntry {
                    id: Uuid::new_v4().to_string(), method: "WS".to_string(),
                    path: "/ws".to_string(), source_ip: "websocket".to_string(),
                    headers: HashMap::new(), body: format!("[Binary {} bytes]", data.len()),
                    content_type: Some("application/octet-stream".to_string()), received_at: Utc::now(),
                    source_type: "websocket".to_string(),
                };
                if let Ok(mut payloads) = shared.payloads.lock() { payloads.insert(0, entry); payloads.truncate(500); }
                if let Ok(mut total) = shared.ws_total_messages.lock() { *total += 1; }
            }
            Ok(axum::extract::ws::Message::Close(_)) => break,
            Err(_) => break,
            _ => {}
        }
    }
    {
        let mut count = shared.ws_connections.lock().unwrap();
        *count = count.saturating_sub(1);
    }
}

fn start_server_bg(shared: Arc<AppState>, port: u16, webhook_path: String) {
    let (tx, rx) = tokio::sync::watch::channel(false);
    {
        let mut shutdown_tx = shared.shutdown_tx.lock().unwrap();
        *shutdown_tx = Some(tx);
    }
    let shared_clone = shared.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
        rt.block_on(async move {
            let app_state = shared_clone.clone();
            let base = webhook_path.trim_end_matches('/');
            let sub = format!("{}/{{*path}}", base);
            let router = Router::new()
                .route(base, any(handle_webhook))
                .route(&sub, any(handle_webhook))
                .route("/ws", get(handle_websocket))
                .route("/health", get(health_check))
                .with_state(app_state);
            let listener = match TcpListener::bind(format!("0.0.0.0:{}", port)).await {
                Ok(l) => l, Err(e) => { eprintln!("Failed to bind port {}: {}", port, e); return; }
            };
            println!("Webhook server listening on port {} path {}", port, base);
            axum::serve(listener, router)
                .with_graceful_shutdown(async move { let mut rx = rx; let _ = rx.changed().await; })
                .await
                .unwrap_or_else(|e| eprintln!("Server error: {}", e));
        });
    });
}

// ── App entry ──

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let shared = Arc::new(AppState {
        payloads: Mutex::new(Vec::new()),
        server_port: Mutex::new(3030),
        webhook_path: Mutex::new("/webhook".to_string()),
        shutdown_tx: Mutex::new(None),
        forward_rules: Mutex::new(Vec::new()),
        auto_forward_logs: Mutex::new(Vec::new()),
        ws_connections: Mutex::new(0),
        ws_total_messages: Mutex::new(0),
        proxy_traffic: Mutex::new(Vec::new()),
        proxy_running: Mutex::new(false),
        proxy_port: Mutex::new(8080),
        proxy_shutdown: Mutex::new(None),
        ca_cert_pem: Mutex::new(None),
        ca_key_pem: Mutex::new(None),
    });

    tracing_subscriber::fmt()
        .with_env_filter("hudsucker=error")
        .init();

    let shared_clone = shared.clone();
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(shared)
        .setup(move |_app| {
            start_server_bg(shared_clone, 3030, "/webhook".to_string());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_payloads, clear_payloads, send_test_payload,
            get_server_status, get_webhook_path, restart_server,
            forward_mapped, save_forward_rule, get_forward_rules, delete_forward_rule,
            toggle_forward_rule, get_auto_forward_logs, get_ws_status,
            export_payloads, write_export_file,
            get_proxy_status, get_proxy_traffic, clear_proxy_traffic,
            get_ca_cert_pem, install_ca_cert, check_ca_cert_installed, remove_ca_cert, start_proxy_cmd, stop_proxy_cmd,
            open_system_proxy_settings, set_system_proxy, disable_system_proxy,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
