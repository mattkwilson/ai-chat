use reqwest::Client;
use serde_json;
use futures::StreamExt;
use tauri::Emitter;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

struct CancelToken(Arc<AtomicBool>);

#[tauri::command]
async fn list_models() -> Result<Vec<String>, String> {
    let client = Client::new();
    let response = client
        .get("http://127.0.0.1:11434/api/tags")
        .send()
        .await
        .map_err(|e| format!("Request error: {}", e))?;

    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Parse error: {}", e))?;

    let models = json["models"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|m| m["name"].as_str().map(|s| s.to_string()))
        .collect();

    Ok(models)
}

#[tauri::command]
async fn cancel_chat(cancel: tauri::State<'_, CancelToken>) -> Result<(), String> {
    cancel.0.store(true, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
async fn ollama_chat(app: tauri::AppHandle, cancel: tauri::State<'_, CancelToken>, prompt: &str, model: &str) -> Result<(), String> {
    cancel.0.store(false, Ordering::Relaxed);

    let url = "http://127.0.0.1:11434/api/chat";

    let request_body = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": prompt}],
        "stream": true
    });

    let client = Client::new();
    let response = client
        .post(url)
        .json(&request_body)
        .send()
        .await
        .map_err(|e| format!("Request error: {}", e))?;

    let mut buffer = String::new();
    let mut full_content = String::new();
    let mut stream = response.bytes_stream();

    while let Some(chunk_result) = stream.next().await {
        if cancel.0.load(Ordering::Relaxed) {
            app.emit("ollama-done", ()).ok();
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

            let json: serde_json::Value = serde_json::from_str(&line).map_err(|e| format!("JSON parse error: {}", e))?;

            if let Some(done) = json.get("done").and_then(|d| d.as_bool()) {
                if done {
                    app.emit("ollama-done", ()).map_err(|e| format!("Emit error: {}", e))?;
                    return Ok(());
                }
            }

            if let Some(message) = json.get("message") {
                if let Some(content) = message.get("content").and_then(|c| c.as_str()) {
                    full_content.push_str(content);
                    app.emit("ollama-chunk", &full_content).map_err(|e| format!("Emit error: {}", e))?;
                }
            }
        }
    }

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(CancelToken(Arc::new(AtomicBool::new(false))))
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![ollama_chat, cancel_chat, list_models])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
