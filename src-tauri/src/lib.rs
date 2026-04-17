use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tauri::Emitter;

#[derive(Deserialize, Serialize)]
struct OllamaMessage {
    role: String,
    content: String,
    #[serde(skip_serializing_if = "option_vec_is_none_or_empty")]
    images: Option<Vec<String>>,
}

fn option_vec_is_none_or_empty(value: &Option<Vec<String>>) -> bool {
    value.as_ref().is_none_or(Vec::is_empty)
}

fn is_valid_role(role: &str) -> bool {
    matches!(role, "user" | "assistant" | "system")
}

struct CancelToken(Arc<AtomicBool>);

struct OllamaDoneGuard {
    app: tauri::AppHandle,
    emitted: bool,
}

impl OllamaDoneGuard {
    fn new(app: tauri::AppHandle) -> Self {
        Self {
            app,
            emitted: false,
        }
    }

    fn emit_done(&mut self) -> Result<(), String> {
        self.app
            .emit("ollama-done", ())
            .map_err(|e| format!("Emit error: {}", e))?;
        self.emitted = true;
        Ok(())
    }
}

impl Drop for OllamaDoneGuard {
    fn drop(&mut self) {
        if !self.emitted {
            self.app.emit("ollama-done", ()).ok();
        }
    }
}

#[tauri::command]
async fn list_models() -> Result<Vec<String>, String> {
    let client = Client::new();
    let response = client
        .get("http://127.0.0.1:11434/api/tags")
        .send()
        .await
        .map_err(|e| format!("Request error: {}", e))?
        .error_for_status()
        .map_err(|e| format!("HTTP error: {}", e))?;

    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Parse error: {}", e))?;

    let models = json["models"]
        .as_array()
        .map(|models| {
            models
                .iter()
                .filter_map(|m| m["name"].as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    Ok(models)
}

#[tauri::command]
async fn cancel_chat(cancel: tauri::State<'_, CancelToken>) -> Result<(), String> {
    cancel.0.store(true, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
async fn ollama_chat(
    app: tauri::AppHandle,
    cancel: tauri::State<'_, CancelToken>,
    model: &str,
    messages: Vec<OllamaMessage>,
) -> Result<(), String> {
    cancel.0.store(false, Ordering::Relaxed);
    let mut done_guard = OllamaDoneGuard::new(app.clone());

    let url = "http://127.0.0.1:11434/api/chat";

    if messages.is_empty() {
        return Err("At least one message is required.".to_string());
    }

    if let Some((index, invalid_role)) = messages
        .iter()
        .enumerate()
        .find_map(|(i, m)| (!is_valid_role(&m.role)).then_some((i, m.role.as_str())))
    {
        return Err(format!(
            "Invalid role at message index {}: '{}'. Allowed values are 'user', 'assistant', 'system'.",
            index, invalid_role
        ));
    }

    let request_body = serde_json::json!({
        "model": model,
        "messages": messages,
        "stream": true
    });

    let client = Client::new();
    let response = client
        .post(url)
        .json(&request_body)
        .send()
        .await
        .map_err(|e| format!("Request error: {}", e))?;

    if let Err(e) = response.error_for_status_ref() {
        let status = response.status();
        let body = response
            .text()
            .await
            .unwrap_or_else(|read_err| format!("Failed to read error body: {}", read_err));
        let error_message = if body.trim().is_empty() {
            format!("Ollama request failed with status {}: {}", status, e)
        } else {
            format!("Ollama request failed with status {}: {}", status, body)
        };

        app.emit("ollama-error", &error_message).ok();
        done_guard.emit_done().ok();
        return Err(error_message);
    }

    let mut buffer = String::new();
    let mut full_content = String::new();
    let mut stream = response.bytes_stream();

    while let Some(chunk_result) = stream.next().await {
        if cancel.0.load(Ordering::Relaxed) {
            return Ok(());
        }

        let chunk = chunk_result.map_err(|e| format!("Stream error: {}", e))?;
        let chunk_str = std::str::from_utf8(&chunk).map_err(|e| format!("UTF8 error: {}", e))?;
        buffer.push_str(chunk_str);

        while let Some(newline_pos) = buffer.find('\n') {
            let line = buffer[..newline_pos].to_string();
            buffer = buffer[newline_pos + 1..].to_string();

            if line.trim().is_empty() {
                continue;
            }

            let json: serde_json::Value =
                serde_json::from_str(&line).map_err(|e| format!("JSON parse error: {}", e))?;

            if let Some(done) = json.get("done").and_then(|d| d.as_bool()) {
                if done {
                    done_guard.emit_done()?;
                    return Ok(());
                }
            }

            if let Some(message) = json.get("message") {
                if let Some(content) = message.get("content").and_then(|c| c.as_str()) {
                    full_content.push_str(content);
                    app.emit("ollama-chunk", &full_content)
                        .map_err(|e| format!("Emit error: {}", e))?;
                }
            }
        }
    }

    Ok(())
}

#[tauri::command]
async fn extract_pdf_text(data: String) -> Result<String, String> {
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&data)
        .map_err(|e| format!("Base64 decode error: {}", e))?;
    let text = tokio::task::spawn_blocking(move || {
        pdf_extract::extract_text_from_mem(&bytes)
            .map_err(|e| format!("PDF extraction error: {}", e))
    })
    .await
    .map_err(|e| format!("Task error: {}", e))??;
    Ok(text)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(CancelToken(Arc::new(AtomicBool::new(false))))
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            ollama_chat,
            cancel_chat,
            list_models,
            extract_pdf_text
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
