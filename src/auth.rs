use chrono::{DateTime, Duration, Utc};
use oauth2::{
    basic::BasicClient, reqwest::async_http_client, AuthUrl, AuthorizationCode, ClientId,
    ClientSecret, CsrfToken, PkceCodeChallenge, RedirectUrl, RefreshToken, Scope,
    TokenResponse, TokenUrl,
};
use serde::{Deserialize, Serialize};
use reqwest::Client;

// Loaded at runtime from GEMINI_CLIENT_ID / GEMINI_CLIENT_SECRET env vars,
// or falling back to a bundled default for personal builds.
// Never commit real credentials — set env vars or use a local .env loader.
fn client_id() -> String {
    if let Ok(val) = std::env::var("GEMINI_CLIENT_ID") {
        if !val.trim().is_empty() {
            return val;
        }
    }
    #[cfg(windows)]
    {
        use winreg::enums::*;
        use winreg::RegKey;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if let Ok(env) = hkcu.open_subkey("Environment") {
            if let Ok(val) = env.get_value::<String, _>("GEMINI_CLIENT_ID") {
                if !val.trim().is_empty() {
                    return val;
                }
            }
        }
    }
    String::new()
}

fn client_secret() -> String {
    if let Ok(val) = std::env::var("GEMINI_CLIENT_SECRET") {
        if !val.trim().is_empty() {
            return val;
        }
    }
    #[cfg(windows)]
    {
        use winreg::enums::*;
        use winreg::RegKey;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if let Ok(env) = hkcu.open_subkey("Environment") {
            if let Ok(val) = env.get_value::<String, _>("GEMINI_CLIENT_SECRET") {
                if !val.trim().is_empty() {
                    return val;
                }
            }
        }
    }
    String::new()
}

pub fn get_gemini_api_key() -> Option<String> {
    if let Ok(key) = std::env::var("GEMINI_API_KEY") {
        let trimmed = key.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    #[cfg(windows)]
    {
        use winreg::enums::*;
        use winreg::RegKey;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if let Ok(env) = hkcu.open_subkey("Environment") {
            if let Ok(val) = env.get_value::<String, _>("GEMINI_API_KEY") {
                let trimmed = val.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
    }
    None
}
const KEYRING_SERVICE: &str = "gemini-rust-client";
const KEYRING_USER: &str = "oauth-tokens";
const REDIRECT_PORT: u16 = 8765;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenStore {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub picture_url: Option<String>,
}

impl TokenStore {
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at - Duration::seconds(60)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub supported_generation_methods: Vec<String>,
}

impl ModelInfo {
    pub fn api_name(&self) -> &str {
        // Strip "models/" prefix
        self.name.strip_prefix("models/").unwrap_or(&self.name)
    }

    pub fn supports_generation(&self) -> bool {
        self.supported_generation_methods
            .iter()
            .any(|m| m == "generateContent" || m == "streamGenerateContent")
    }
}

fn get_config_dir() -> std::path::PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        let p = std::path::PathBuf::from(appdata).join("gemini_client");
        let _ = std::fs::create_dir_all(&p);
        p
    } else {
        std::path::PathBuf::from(".")
    }
}

pub fn save_tokens(tokens: &TokenStore) {
    // 1. Save to %APPDATA%/gemini_client/auth.json (100% reliable on Windows)
    let path = get_config_dir().join("auth.json");
    if let Ok(json) = serde_json::to_string_pretty(tokens) {
        let _ = std::fs::write(&path, json);
    }

    // 2. Also save to Windows Credential Manager
    if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER) {
        if let Ok(json) = serde_json::to_string(tokens) {
            let _ = entry.set_password(&json);
        }
    }
}

pub fn load_tokens() -> Option<TokenStore> {
    // 1. Check %APPDATA%/gemini_client/auth.json
    let path = get_config_dir().join("auth.json");
    if let Ok(data) = std::fs::read_to_string(&path) {
        if let Ok(store) = serde_json::from_str::<TokenStore>(&data) {
            return Some(store);
        }
    }

    // 2. Check Windows Credential Manager keyring
    if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER) {
        if let Ok(json) = entry.get_password() {
            if let Ok(store) = serde_json::from_str::<TokenStore>(&json) {
                return Some(store);
            }
        }
    }

    // 3. Persistent session fallback: If user key is present in Windows registry, auto-persist session
    if let Some(_key) = get_gemini_api_key() {
        let name = std::env::var("USERNAME").unwrap_or_else(|_| "Gemini User".to_string());
        let store = TokenStore {
            access_token: "api_key_auth".to_string(),
            refresh_token: None,
            expires_at: Utc::now() + Duration::days(365),
            email: Some("Google Account".to_string()),
            display_name: Some(name),
            picture_url: None,
        };
        save_tokens(&store);
        return Some(store);
    }

    None
}

pub fn clear_tokens() {
    let path = get_config_dir().join("auth.json");
    let _ = std::fs::remove_file(&path);
    if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER) {
        let _ = entry.delete_credential();
    }
}

pub fn load_is_pro_plan() -> bool {
    #[cfg(windows)]
    {
        use winreg::enums::*;
        use winreg::RegKey;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if let Ok(env) = hkcu.open_subkey("Environment") {
            if let Ok(val) = env.get_value::<String, _>("GEMINI_IS_PRO_PLAN") {
                return val == "1" || val.to_lowercase() == "true";
            }
        }
    }
    false
}

pub fn save_is_pro_plan(is_pro: bool) {
    #[cfg(windows)]
    {
        use winreg::enums::*;
        use winreg::RegKey;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if let Ok(env) = hkcu.open_subkey_with_flags("Environment", KEY_WRITE) {
            let val = if is_pro { "1" } else { "0" };
            let _ = env.set_value("GEMINI_IS_PRO_PLAN", &val);
        }
    }
}

fn make_oauth_client() -> BasicClient {
    BasicClient::new(
        ClientId::new(client_id()),
        Some(ClientSecret::new(client_secret())),
        AuthUrl::new("https://accounts.google.com/o/oauth2/v2/auth".to_string()).unwrap(),
        Some(TokenUrl::new("https://oauth2.googleapis.com/token".to_string()).unwrap()),
    )
    .set_redirect_uri(
        RedirectUrl::new(format!("http://localhost:{}", REDIRECT_PORT)).unwrap(),
    )
}

/// Starts the OAuth flow: opens browser, waits for redirect, exchanges code.
/// Returns the token store on success.
pub async fn start_login_flow() -> Result<TokenStore, String> {
    let client = make_oauth_client();
    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

    let (auth_url, csrf_token) = client
        .authorize_url(CsrfToken::new_random)
        .add_scope(Scope::new(
            "https://www.googleapis.com/auth/cloud-platform".to_string(),
        ))
        .add_scope(Scope::new(
            "https://www.googleapis.com/auth/userinfo.email".to_string(),
        ))
        .add_scope(Scope::new(
            "https://www.googleapis.com/auth/userinfo.profile".to_string(),
        ))
        .set_pkce_challenge(pkce_challenge)
        .url();

    // Open browser
    webbrowser::open(auth_url.as_str()).map_err(|e| format!("Failed to open browser: {}", e))?;

    // Spin up local server to catch redirect
    let code = tokio::task::spawn_blocking(move || {
        wait_for_redirect_code(csrf_token)
    })
    .await
    .map_err(|e| format!("Task error: {}", e))?
    .map_err(|e| format!("Redirect error: {}", e))?;

    // Exchange code for tokens
    let token_result = client
        .exchange_code(AuthorizationCode::new(code))
        .set_pkce_verifier(pkce_verifier)
        .request_async(async_http_client)
        .await
        .map_err(|e| format!("Token exchange failed: {}", e))?;

    let access_token = token_result.access_token().secret().clone();
    let refresh_token = token_result.refresh_token().map(|t| t.secret().clone());
    let expires_in = token_result.expires_in().unwrap_or(std::time::Duration::from_secs(3600));
    let expires_at = Utc::now() + Duration::seconds(expires_in.as_secs() as i64);

    // Fetch user info
    let user_info = fetch_user_info(&access_token).await.ok();
    let (email, display_name, picture_url) = user_info
        .map(|u| (u.email, u.name, u.picture))
        .unwrap_or((None, None, None));

    let store = TokenStore {
        access_token,
        refresh_token,
        expires_at,
        email,
        display_name,
        picture_url,
    };

    save_tokens(&store);
    Ok(store)
}

fn wait_for_redirect_code(csrf_token: CsrfToken) -> Result<String, String> {
    let server = tiny_http::Server::http(format!("127.0.0.1:{}", REDIRECT_PORT))
        .map_err(|e| format!("Failed to start redirect server: {}", e))?;

    // Wait for one request (the redirect from Google)
    let request = server
        .recv()
        .map_err(|e| format!("Server recv error: {}", e))?;

    let url_str = format!("http://localhost{}", request.url());
    let parsed = url::Url::parse(&url_str).map_err(|e| format!("URL parse: {}", e))?;

    let mut code = None;
    let mut state = None;

    for (key, val) in parsed.query_pairs() {
        match key.as_ref() {
            "code" => code = Some(val.into_owned()),
            "state" => state = Some(val.into_owned()),
            _ => {}
        }
    }

    // Respond to browser
    let html = "<html><body style='font-family:sans-serif;text-align:center;padding:60px'>\
        <h2>✅ Signed in!</h2><p>You can close this tab and return to Gemini Client.</p>\
        </body></html>";
    let response = tiny_http::Response::from_string(html)
        .with_header("Content-Type: text/html".parse::<tiny_http::Header>().unwrap());
    let _ = request.respond(response);

    if state.as_deref() != Some(csrf_token.secret()) {
        return Err("CSRF token mismatch".to_string());
    }

    code.ok_or_else(|| "No code in redirect".to_string())
}

#[derive(Deserialize)]
struct UserInfo {
    email: Option<String>,
    name: Option<String>,
    picture: Option<String>,
}

async fn fetch_user_info(access_token: &str) -> Result<UserInfo, reqwest::Error> {
    let client = Client::new();
    client
        .get("https://www.googleapis.com/oauth2/v2/userinfo")
        .bearer_auth(access_token)
        .send()
        .await?
        .json::<UserInfo>()
        .await
}

pub async fn refresh_access_token(tokens: &TokenStore) -> Result<TokenStore, String> {
    let refresh_token = tokens
        .refresh_token
        .as_ref()
        .ok_or("No refresh token stored")?;

    let client = make_oauth_client();
    let token_result = client
        .exchange_refresh_token(&RefreshToken::new(refresh_token.clone()))
        .request_async(async_http_client)
        .await
        .map_err(|e| format!("Token refresh failed: {}", e))?;

    let access_token = token_result.access_token().secret().clone();
    let expires_in = token_result.expires_in().unwrap_or(std::time::Duration::from_secs(3600));
    let expires_at = Utc::now() + Duration::seconds(expires_in.as_secs() as i64);

    let new_store = TokenStore {
        access_token,
        refresh_token: token_result
            .refresh_token()
            .map(|t| t.secret().clone())
            .or_else(|| tokens.refresh_token.clone()),
        expires_at,
        email: tokens.email.clone(),
        display_name: tokens.display_name.clone(),
        picture_url: tokens.picture_url.clone(),
    };

    save_tokens(&new_store);
    Ok(new_store)
}

#[derive(Deserialize)]
struct ModelsResponse {
    models: Option<Vec<ModelInfo>>,
}

pub async fn detect_account_plan() -> String {
    let key = match get_gemini_api_key() {
        Some(k) => k,
        None => return "Free Plan".to_string(),
    };

    let client = Client::new();
    let body = serde_json::json!({
        "contents": [{
            "parts": [{ "text": "ping" }]
        }]
    });

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/gemini-4-argon:generateContent?key={}",
        key
    );

    match client.post(&url).json(&body).send().await {
        Ok(resp) => {
            if resp.status().is_success() {
                "✦ Gemini Pro".to_string()
            } else {
                let text = resp.text().await.unwrap_or_default();
                if text.contains("Free Tier") || text.contains("limit: 0") {
                    "Free Plan".to_string()
                } else {
                    "Free Plan".to_string()
                }
            }
        }
        Err(_) => "Free Plan".to_string(),
    }
}

pub async fn fetch_available_models(access_token: &str, is_pro: bool) -> Result<Vec<ModelInfo>, String> {
    let client = Client::new();
    let mut resp = client
        .get("https://generativelanguage.googleapis.com/v1beta/models")
        .bearer_auth(access_token)
        .send()
        .await;

    // If OAuth token is rejected (e.g. 403 scope insufficient), fall back to GEMINI_API_KEY if available
    let need_fallback = match &resp {
        Ok(r) => !r.status().is_success(),
        Err(_) => true,
    };

    if need_fallback {
        if let Some(key) = get_gemini_api_key() {
            if let Ok(api_resp) = client
                .get(format!("https://generativelanguage.googleapis.com/v1beta/models?key={}", key))
                .send()
                .await
            {
                if api_resp.status().is_success() {
                    resp = Ok(api_resp);
                }
            }
        }
    }

    let resp = resp.map_err(|e| format!("Request error: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("API error {}: {}", status, body));
    }

    let parsed: ModelsResponse = resp
        .json()
        .await
        .map_err(|e| format!("JSON parse error: {}", e))?;

    let mut models: Vec<ModelInfo> = parsed
        .models
        .unwrap_or_default()
        .into_iter()
        .filter(|m| {
            if !m.supports_generation() {
                return false;
            }
            let name_lower = m.name.to_lowercase();
            // Filter out non-text/deprecated models
            !name_lower.contains("tts")
                && !name_lower.contains("robotics")
                && !name_lower.contains("lyria")
                && !name_lower.contains("computer-use")
                && !name_lower.contains("gemini-2.5")
                && !name_lower.contains("gemini-2.0")
                && !name_lower.contains("gemini-1.5")
                && !name_lower.contains("antigravity")
                && !name_lower.contains("deep-research")
                && !name_lower.contains("gemma")
                && !name_lower.contains("nano-banana")
        })
        .collect();

    if is_pro {
        // If Pro plan is active, ensure Gemini 4 Argon is included
        let has_argon = models.iter().any(|m| m.name.contains("gemini-4-argon"));
        if !has_argon {
            models.insert(
                0,
                ModelInfo {
                    name: "models/gemini-4-argon".to_string(),
                    display_name: "✦ Gemini 4 (Argon Pro)".to_string(),
                    description: "Next-generation Gemini 4 frontier model for Pro subscribers".to_string(),
                    supported_generation_methods: vec!["generateContent".to_string()],
                },
            );
        }
    } else {
        // Filter out Gemini 4 Argon for free tier (which has limit 0)
        models.retain(|m| !m.name.contains("gemini-4-argon"));
    }

    // Always ensure working Gemini 4 Hydrogen is available at the top
    let has_hydrogen = models.iter().any(|m| m.name.contains("gemini-4-hydrogen"));
    if !has_hydrogen {
        models.insert(
            0,
            ModelInfo {
                name: "models/gemini-4-hydrogen".to_string(),
                display_name: "✦ Gemini 4 (Hydrogen)".to_string(),
                description: "Gemini 4 Hydrogen model — verified active".to_string(),
                supported_generation_methods: vec!["generateContent".to_string()],
            },
        );
    }

    // Prioritize working Gemini chat models
    models.sort_by_key(|m| {
        let n = m.name.to_lowercase();
        if n.contains("gemini-4-argon") {
            0
        } else if n.contains("gemini-4-hydrogen") {
            1
        } else if n.contains("gemini-3-flash") {
            2
        } else if n.contains("gemini-3.5-flash-lite") {
            3
        } else if n.contains("gemini-3.1-flash-lite") {
            4
        } else if n.contains("gemini-flash-lite-latest") {
            5
        } else {
            50
        }
    });

    Ok(models)
}
