use crate::types::Message;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredConversation {
    pub id: String,
    pub title: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub model: Option<String>,
    pub history: Vec<Message>,
}

impl StoredConversation {
    pub fn new(id: String, title: String, history: Vec<Message>, model: Option<String>) -> Self {
        let now = chrono::Local::now().timestamp();
        Self {
            id,
            title,
            created_at: now,
            updated_at: now,
            model,
            history,
        }
    }

    pub fn formatted_date(&self) -> String {
        let dt_opt = chrono::DateTime::from_timestamp(self.updated_at, 0);
        if let Some(dt) = dt_opt {
            let local: chrono::DateTime<chrono::Local> = chrono::DateTime::from(dt);
            let now = chrono::Local::now();
            let days_diff = (now.date_naive() - local.date_naive()).num_days();
            if days_diff == 0 {
                local.format("Today %H:%M").to_string()
            } else if days_diff == 1 {
                local.format("Yesterday %H:%M").to_string()
            } else if days_diff < 7 {
                local.format("%a %H:%M").to_string()
            } else {
                local.format("%b %d, %Y").to_string()
            }
        } else {
            "Recent".to_string()
        }
    }

    pub fn preview_text(&self) -> String {
        if let Some(last) = self.history.last() {
            let clean = last.content.replace(['\r', '\n'], " ");
            let trimmed = clean.trim();
            if trimmed.is_empty() {
                return "Empty message".to_string();
            }
            if trimmed.chars().count() > 55 {
                let preview: String = trimmed.chars().take(52).collect();
                format!("{}…", preview)
            } else {
                trimmed.to_string()
            }
        } else {
            "No messages yet".to_string()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedTabState {
    pub id: u64,
    pub conv_id: String,
    pub title: String,
    pub history: Vec<Message>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedSessionState {
    pub tabs: Vec<SavedTabState>,
    pub active_tab_id: u64,
    pub next_tab_id: u64,
    pub selected_model_idx: usize,
}

pub fn get_storage_dir() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        let p = PathBuf::from(appdata).join("gemini_client");
        let _ = std::fs::create_dir_all(&p);
        p
    } else {
        PathBuf::from(".")
    }
}

fn conversations_path() -> PathBuf {
    get_storage_dir().join("conversations.json")
}

fn session_path() -> PathBuf {
    get_storage_dir().join("session_tabs.json")
}

pub fn load_conversations() -> Vec<StoredConversation> {
    let path = conversations_path();
    if let Ok(data) = std::fs::read_to_string(&path) {
        if let Ok(mut convs) = serde_json::from_str::<Vec<StoredConversation>>(&data) {
            // Sort newest first
            convs.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
            return convs;
        }
    }
    Vec::new()
}

pub fn save_conversations(conversations: &[StoredConversation]) {
    let path = conversations_path();
    if let Ok(json) = serde_json::to_string_pretty(conversations) {
        let temp_path = path.with_extension("json.tmp");
        if std::fs::write(&temp_path, &json).is_ok() {
            if std::fs::rename(&temp_path, &path).is_err() {
                let _ = std::fs::write(&path, &json);
                let _ = std::fs::remove_file(&temp_path);
            }
        } else {
            let _ = std::fs::write(&path, &json);
        }
    }
}

#[allow(dead_code)]
pub fn upsert_conversation(conv: StoredConversation) {
    let mut convs = load_conversations();
    if let Some(pos) = convs.iter().position(|c| c.id == conv.id) {
        convs[pos] = conv;
    } else {
        convs.insert(0, conv);
    }
    convs.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    save_conversations(&convs);
}

#[allow(dead_code)]
pub fn delete_conversation(id: &str) {
    let mut convs = load_conversations();
    convs.retain(|c| c.id != id);
    save_conversations(&convs);
}

pub fn clear_all_conversations() {
    save_conversations(&[]);
}

pub fn save_session(session: &SavedSessionState) {
    let path = session_path();
    if let Ok(json) = serde_json::to_string_pretty(session) {
        let temp_path = path.with_extension("json.tmp");
        if std::fs::write(&temp_path, &json).is_ok() {
            if std::fs::rename(&temp_path, &path).is_err() {
                let _ = std::fs::write(&path, &json);
                let _ = std::fs::remove_file(&temp_path);
            }
        } else {
            let _ = std::fs::write(&path, &json);
        }
    }
}

pub fn load_session() -> Option<SavedSessionState> {
    let path = session_path();
    if let Ok(data) = std::fs::read_to_string(&path) {
        if let Ok(session) = serde_json::from_str::<SavedSessionState>(&data) {
            if !session.tabs.is_empty() {
                return Some(session);
            }
        }
    }
    None
}
