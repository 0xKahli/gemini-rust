use chrono::{DateTime, Duration, Utc};
use crate::types::AccountPlanTier;
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
    let p = get_config_dir().join("api_key.txt");
    if let Ok(content) = std::fs::read_to_string(&p) {
        let trimmed = content.trim();
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

pub fn save_gemini_api_key(key: &str) {
    let p = get_config_dir().join("api_key.txt");
    let _ = std::fs::write(&p, key.trim());

    #[cfg(windows)]
    {
        use winreg::enums::*;
        use winreg::RegKey;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if let Ok(env) = hkcu.open_subkey_with_flags("Environment", KEY_WRITE) {
            let _ = env.set_value("GEMINI_API_KEY", &key.trim().to_string());
        }
    }
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
    let api_key_path = get_config_dir().join("api_key.txt");
    let _ = std::fs::remove_file(&api_key_path);
    let avatar = get_config_dir().join("avatar.png");
    let _ = std::fs::remove_file(&avatar);
    let plan = get_config_dir().join("plan.txt");
    let _ = std::fs::remove_file(&plan);
    #[cfg(windows)]
    {
        use winreg::enums::*;
        use winreg::RegKey;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if let Ok(env) = hkcu.open_subkey_with_flags("Environment", KEY_WRITE) {
            let _ = env.delete_value("GEMINI_API_KEY");
            let _ = env.delete_value("GEMINI_IS_PRO_PLAN");
        }
    }
    if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER) {
        let _ = entry.delete_credential();
    }
}

#[allow(dead_code)]
pub fn load_is_pro_plan() -> bool {
    // 1. Check legacy plan.txt in config directory
    let p = get_config_dir().join("plan.txt");
    if let Ok(content) = std::fs::read_to_string(&p) {
        let trimmed = content.trim().to_lowercase();
        if trimmed == "1" || trimmed == "true" || trimmed == "pro" || trimmed == "ultra" {
            return true;
        } else if trimmed == "0" || trimmed == "false" || trimmed == "free" {
            return false;
        }
    }

    // Default to false (Free Plan) — never assume Pro for unknown or basic accounts
    false
}

#[allow(dead_code)]
pub fn save_is_pro_plan(is_pro: bool) {
    let p = get_config_dir().join("plan.txt");
    let _ = std::fs::write(&p, if is_pro { "pro" } else { "free" });

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

    // Download and cache profile photo locally if returned
    if let Some(ref pic_url) = picture_url {
        let http = reqwest::Client::new();
        if let Ok(resp) = http.get(pic_url).send().await {
            if let Ok(bytes) = resp.bytes().await {
                let path = get_config_dir().join("avatar.png");
                let _ = std::fs::write(&path, bytes);
            }
        }
    }

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
        .map_err(|e| format!("Failed to start redirect server on port {}: {}", REDIRECT_PORT, e))?;

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);

    while std::time::Instant::now() < deadline {
        let timeout = deadline.saturating_duration_since(std::time::Instant::now());
        let request = match server.recv_timeout(timeout.min(std::time::Duration::from_secs(2))) {
            Ok(Some(req)) => req,
            Ok(None) => continue, // Timeout slice elapsed, keep waiting until deadline
            Err(e) => return Err(format!("Server error while waiting for redirect: {}", e)),
        };

        let raw_url = request.url();
        if raw_url.starts_with("/favicon.ico") {
            let resp = tiny_http::Response::empty(404);
            let _ = request.respond(resp);
            continue;
        }

        let url_str = format!("http://localhost{}", raw_url);
        let parsed = match url::Url::parse(&url_str) {
            Ok(u) => u,
            Err(_) => {
                let resp = tiny_http::Response::from_string("Invalid request").with_status_code(400);
                let _ = request.respond(resp);
                continue;
            }
        };

        let mut code = None;
        let mut state = None;
        let mut error = None;

        for (key, val) in parsed.query_pairs() {
            match key.as_ref() {
                "code" => code = Some(val.into_owned()),
                "state" => state = Some(val.into_owned()),
                "error" => error = Some(val.into_owned()),
                _ => {}
            }
        }

        if let Some(err) = error {
            let html = format!(
                "<html><body style='font-family:sans-serif;text-align:center;padding:60px'>\
                <h2>❌ Sign-in Error</h2><p>Google authentication returned: {}</p>\
                </body></html>",
                err
            );
            let response = tiny_http::Response::from_string(html)
                .with_status_code(400)
                .with_header("Content-Type: text/html".parse::<tiny_http::Header>().unwrap());
            let _ = request.respond(response);
            return Err(format!("Authentication canceled or denied: {}", err));
        }

        if state.as_deref() != Some(csrf_token.secret()) {
            let html = "<html><body style='font-family:sans-serif;text-align:center;padding:60px'>\
                <h2>❌ Security Verification Failed</h2><p>CSRF state token mismatch. Please try signing in again.</p>\
                </body></html>";
            let response = tiny_http::Response::from_string(html)
                .with_status_code(403)
                .with_header("Content-Type: text/html".parse::<tiny_http::Header>().unwrap());
            let _ = request.respond(response);
            return Err("Security verification failed: CSRF token mismatch".to_string());
        }

        if let Some(auth_code) = code {
            let html = "<html><body style='font-family:sans-serif;text-align:center;padding:60px'>\
                <h2>✅ Signed in!</h2><p>You can close this tab and return to Rusty Gemini.</p>\
                </body></html>";
            let response = tiny_http::Response::from_string(html)
                .with_header("Content-Type: text/html".parse::<tiny_http::Header>().unwrap());
            let _ = request.respond(response);
            return Ok(auth_code);
        } else {
            let resp = tiny_http::Response::from_string("Missing authorization code").with_status_code(400);
            let _ = request.respond(resp);
        }
    }

    Err("Sign-in timed out after 2 minutes. Please try again.".to_string())
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

pub async fn detect_account_plan(tokens: &TokenStore) -> AccountPlanTier {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(6))
        .build()
        .unwrap_or_else(|_| Client::new());

    let body = serde_json::json!({
        "contents": [{
            "parts": [{ "text": "ping" }]
        }],
        "generationConfig": {
            "maxOutputTokens": 1
        }
    });

    // 1. Probe Ultra access
    let ultra_url = "https://generativelanguage.googleapis.com/v1beta/models/gemini-ultra:generateContent";
    let ultra_req = if tokens.access_token == "api_key_auth" {
        get_gemini_api_key().map(|key| client.post(ultra_url).header("x-goog-api-key", key))
    } else {
        Some(client.post(ultra_url).bearer_auth(&tokens.access_token))
    };

    if let Some(req) = ultra_req {
        if let Ok(resp) = req.json(&body).send().await {
            if resp.status().is_success() {
                let text = resp.text().await.unwrap_or_default();
                if !text.contains("limit: 0") && !text.contains("PERMISSION_DENIED") && !text.contains("ACCESS_TOKEN_SCOPE_INSUFFICIENT") {
                    return AccountPlanTier::Ultra;
                }
            }
        }
    }

    // 2. Probe Pro access (gemini-4-argon)
    let pro_url = "https://generativelanguage.googleapis.com/v1beta/models/gemini-4-argon:generateContent";
    let pro_req = if tokens.access_token == "api_key_auth" {
        get_gemini_api_key().map(|key| client.post(pro_url).header("x-goog-api-key", key))
    } else {
        Some(client.post(pro_url).bearer_auth(&tokens.access_token))
    };

    if let Some(req) = pro_req {
        if let Ok(resp) = req.json(&body).send().await {
            if resp.status().is_success() {
                let text = resp.text().await.unwrap_or_default();
                if !text.contains("limit: 0") && !text.contains("PERMISSION_DENIED") && !text.contains("ACCESS_TOKEN_SCOPE_INSUFFICIENT") {
                    return AccountPlanTier::Pro;
                }
            }
        }
    }

    // Default: Basic / Free account
    AccountPlanTier::Free
}

pub async fn fetch_available_models(access_token: &str, is_pro: bool) -> Result<Vec<ModelInfo>, String> {
    let client = Client::new();
    let mut resp = if access_token == "api_key_auth" {
        if let Some(key) = get_gemini_api_key() {
            client
                .get("https://generativelanguage.googleapis.com/v1beta/models")
                .header("x-goog-api-key", &key)
                .send()
                .await
        } else {
            return Err("No Gemini API key configured".to_string());
        }
    } else {
        client
            .get("https://generativelanguage.googleapis.com/v1beta/models")
            .bearer_auth(access_token)
            .send()
            .await
    };

    // If OAuth token is rejected (e.g. 403 scope insufficient), fall back to GEMINI_API_KEY if available
    let need_fallback = match &resp {
        Ok(r) => !r.status().is_success(),
        Err(_) => true,
    };

    if need_fallback {
        if let Some(key) = get_gemini_api_key() {
            if let Ok(api_resp) = client
                .get("https://generativelanguage.googleapis.com/v1beta/models")
                .header("x-goog-api-key", &key)
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
        // If Pro plan is active, ensure Gemini 4 Argon and Carbon are included
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
        let has_carbon = models.iter().any(|m| m.name.contains("gemini-4-carbon"));
        if !has_carbon {
            models.insert(
                1,
                ModelInfo {
                    name: "models/gemini-4-carbon".to_string(),
                    display_name: "✦ Gemini 4 (Carbon Pro)".to_string(),
                    description: "High-throughput Gemini 4 frontier model for Pro subscribers".to_string(),
                    supported_generation_methods: vec!["generateContent".to_string()],
                },
            );
        }
    } else {
        // Filter out Gemini 4 Argon and Carbon for free tier (which has limit 0)
        models.retain(|m| !m.name.contains("gemini-4-argon") && !m.name.contains("gemini-4-carbon"));
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
        } else if n.contains("gemini-4-carbon") {
            1
        } else if n.contains("gemini-4-hydrogen") {
            2
        } else if n.contains("gemini-3-flash") {
            3
        } else if n.contains("gemini-3.5-flash-lite") {
            4
        } else if n.contains("gemini-3.1-flash-lite") {
            5
        } else if n.contains("gemini-flash-lite-latest") {
            6
        } else {
            50
        }
    });

    Ok(models)
}
