use crate::types::{AccountPlanTier, Message};
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

pub fn get_account_key(email: Option<&str>, access_token: &str) -> String {
    if access_token == "api_key_auth" {
        if let Some(key) = crate::auth::get_gemini_api_key() {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            use std::hash::{Hash, Hasher};
            key.hash(&mut hasher);
            return format!("api_{:x}", hasher.finish());
        }
        return "api_default".to_string();
    }

    if let Some(em) = email {
        let clean: String = em
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        if !clean.is_empty() {
            return format!("user_{}", clean);
        }
    }

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    use std::hash::{Hash, Hasher};
    access_token.hash(&mut hasher);
    format!("acc_{:x}", hasher.finish())
}

pub fn conversations_path_for_account(acc_key: &str) -> PathBuf {
    get_storage_dir().join(format!("conversations_{}.json", acc_key))
}

pub fn session_path_for_account(acc_key: &str) -> PathBuf {
    get_storage_dir().join(format!("session_{}.json", acc_key))
}

pub fn plan_path_for_account(acc_key: &str) -> PathBuf {
    get_storage_dir().join(format!("plan_{}.txt", acc_key))
}

fn conversations_path() -> PathBuf {
    get_storage_dir().join("conversations.json")
}

fn session_path() -> PathBuf {
    get_storage_dir().join("session_tabs.json")
}

pub fn load_theme_preference() -> bool {
    let p = get_storage_dir().join("theme.txt");
    if let Ok(content) = std::fs::read_to_string(&p) {
        let trimmed = content.trim().to_lowercase();
        if trimmed == "light" {
            return false;
        }
    }
    true // default: dark mode
}

pub fn save_theme_preference(is_dark: bool) {
    let p = get_storage_dir().join("theme.txt");
    let _ = std::fs::write(&p, if is_dark { "dark" } else { "light" });
}

pub fn load_account_plan_tier(acc_key: &str) -> AccountPlanTier {
    let p = plan_path_for_account(acc_key);
    if let Ok(content) = std::fs::read_to_string(&p) {
        return AccountPlanTier::from_str_name(&content);
    }
    // CRITICAL: Default to Free for all accounts unless explicitly detected or set!
    AccountPlanTier::Free
}

pub fn save_account_plan_tier(acc_key: &str, tier: AccountPlanTier) {
    let p = plan_path_for_account(acc_key);
    let _ = std::fs::write(&p, tier.to_storage_str());
}

pub fn load_conversations_for_account(acc_key: &str) -> Vec<StoredConversation> {
    let path = conversations_path_for_account(acc_key);
    if path.exists() {
        if let Ok(data) = std::fs::read_to_string(&path) {
            if let Ok(mut convs) = serde_json::from_str::<Vec<StoredConversation>>(&data) {
                convs.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
                return convs;
            }
        }
    } else {
        // Migration from legacy global conversations.json if it exists
        let legacy_path = conversations_path();
        if legacy_path.exists() {
            if let Ok(data) = std::fs::read_to_string(&legacy_path) {
                if let Ok(mut convs) = serde_json::from_str::<Vec<StoredConversation>>(&data) {
                    convs.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
                    save_conversations_for_account(acc_key, &convs);
                    return convs;
                }
            }
        }
    }
    Vec::new()
}

pub fn save_conversations_for_account(acc_key: &str, conversations: &[StoredConversation]) {
    let path = conversations_path_for_account(acc_key);
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

pub fn clear_all_conversations_for_account(acc_key: &str) {
    save_conversations_for_account(acc_key, &[]);
}

pub fn load_session_for_account(acc_key: &str) -> Option<SavedSessionState> {
    let path = session_path_for_account(acc_key);
    if path.exists() {
        if let Ok(data) = std::fs::read_to_string(&path) {
            if let Ok(session) = serde_json::from_str::<SavedSessionState>(&data) {
                if !session.tabs.is_empty() {
                    return Some(session);
                }
            }
        }
    } else {
        // Fallback to legacy session_tabs.json
        let legacy_path = session_path();
        if legacy_path.exists() {
            if let Ok(data) = std::fs::read_to_string(&legacy_path) {
                if let Ok(session) = serde_json::from_str::<SavedSessionState>(&data) {
                    if !session.tabs.is_empty() {
                        save_session_for_account(acc_key, &session);
                        return Some(session);
                    }
                }
            }
        }
    }
    None
}

pub fn save_session_for_account(acc_key: &str, session: &SavedSessionState) {
    let path = session_path_for_account(acc_key);
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

pub fn load_conversations() -> Vec<StoredConversation> {
    load_conversations_for_account("default")
}

pub fn save_conversations(conversations: &[StoredConversation]) {
    save_conversations_for_account("default", conversations);
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

#[allow(dead_code)]
pub fn clear_all_conversations() {
    clear_all_conversations_for_account("default");
}

#[allow(dead_code)]
pub fn save_session(session: &SavedSessionState) {
    save_session_for_account("default", session);
}

#[allow(dead_code)]
pub fn load_session() -> Option<SavedSessionState> {
    load_session_for_account("default")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_account_key_email() {
        let key = get_account_key(Some("user.test+1@example.com"), "bearer_token_123");
        assert_eq!(key, "user_user_test_1_example_com");
    }

    #[test]
    fn test_get_account_key_api_key_auth() {
        let key = get_account_key(Some("Gemini API Key"), "api_key_auth");
        assert!(key.starts_with("api_"));
    }

    #[test]
    fn test_account_plan_tier_parsing() {
        assert_eq!(AccountPlanTier::from_str_name("pro"), AccountPlanTier::Pro);
        assert_eq!(AccountPlanTier::from_str_name("✦ Google AI Pro"), AccountPlanTier::Pro);
        assert_eq!(AccountPlanTier::from_str_name("ultra"), AccountPlanTier::Ultra);
        assert_eq!(AccountPlanTier::from_str_name("free"), AccountPlanTier::Free);
        assert_eq!(AccountPlanTier::from_str_name("unknown"), AccountPlanTier::Free);
    }

    #[test]
    fn test_account_paths_are_distinct() {
        let p1 = conversations_path_for_account("user_a");
        let p2 = conversations_path_for_account("user_b");
        assert_ne!(p1, p2);
        assert!(p1.to_string_lossy().contains("conversations_user_a.json"));
        assert!(p2.to_string_lossy().contains("conversations_user_b.json"));
    }
}
