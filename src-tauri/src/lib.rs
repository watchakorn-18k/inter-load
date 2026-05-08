use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use axum::extract::{Request, State};
use axum::response::Json;
use axum::Router;
use axum::routing::{any, get};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use uuid::Uuid;

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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppServerState {
    pub port: u16,
    pub running: bool,
}

pub struct AppState {
    pub payloads: Mutex<Vec<PayloadEntry>>,
    pub server_port: Mutex<u16>,
    pub webhook_path: Mutex<String>,
    pub shutdown_tx: Mutex<Option<tokio::sync::watch::Sender<bool>>>,
    pub forward_rules: Mutex<Vec<ForwardRule>>,
}

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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForwardResult {
    pub status: u16,
    pub body: String,
    pub duration_ms: u64,
}

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
    // Shutdown existing server
    {
        let mut tx = state.shutdown_tx.lock().map_err(|e| e.to_string())?;
        if let Some(sender) = tx.take() {
            let _ = sender.send(true);
        }
    }

    // Update port
    {
        let mut current_port = state.server_port.lock().map_err(|e| e.to_string())?;
        *current_port = port;
    }

    // Update webhook path
    if let Some(path) = webhook_path {
        let p = if path.starts_with('/') { path } else { format!("/{}", path) };
        let mut current_path = state.webhook_path.lock().map_err(|e| e.to_string())?;
        *current_path = p;
    }

    let wp = state.webhook_path.lock().map_err(|e| e.to_string())?.clone();

    // Start new server
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
) -> Result<ForwardRule, String> {
    let rule = ForwardRule {
        id: Uuid::new_v4().to_string(),
        name,
        target_url,
        method,
        mappings,
        headers,
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
        body,
        content_type,
        received_at: Utc::now(),
    };

    if let Ok(mut payloads) = shared.payloads.lock() {
        payloads.insert(0, entry);
        payloads.truncate(500);
    }

    Json(serde_json::json!({ "status": "ok", "message": "payload received" }))
}

async fn health_check() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok", "service": "inter-load-webhook" }))
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
                .route("/health", get(health_check))
                .with_state(app_state);

            let listener = match TcpListener::bind(format!("0.0.0.0:{}", port)).await {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("Failed to bind port {}: {}", port, e);
                    return;
                }
            };

            println!("Webhook server listening on port {} path {}", port, base);
            axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    let mut rx = rx;
                    let _ = rx.changed().await;
                })
                .await
                .unwrap_or_else(|e| eprintln!("Server error: {}", e));
        });
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let shared = Arc::new(AppState {
        payloads: Mutex::new(Vec::new()),
        server_port: Mutex::new(3030),
        webhook_path: Mutex::new("/webhook".to_string()),
        shutdown_tx: Mutex::new(None),
        forward_rules: Mutex::new(Vec::new()),
    });

    let shared_clone = shared.clone();
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(shared)
        .setup(move |_app| {
            start_server_bg(shared_clone, 3030, "/webhook".to_string());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_payloads,
            clear_payloads,
            send_test_payload,
            get_server_status,
            get_webhook_path,
            restart_server,
            forward_mapped,
            save_forward_rule,
            get_forward_rules,
            delete_forward_rule,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
