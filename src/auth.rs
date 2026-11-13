use chrono::{DateTime, Duration, Utc};
use oauth2::{
    basic::BasicClient, reqwest::async_http_client, AuthUrl, AuthorizationCode, ClientId,
    ClientSecret, CsrfToken, PkceCodeChallenge, RedirectUrl, RefreshToken, Scope,
    TokenResponse, TokenUrl,
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use reqwest::Client;

// Loaded at runtime from GEMINI_CLIENT_ID / GEMINI_CLIENT_SECRET env vars,
// or falling back to a bundled default for personal builds.
// Never commit real credentials — set env vars or use a local .env loader.
fn client_id() -> String {
    std::env::var("GEMINI_CLIENT_ID").unwrap_or_else(|_| {
        // Replace with your own OAuth client ID for personal builds
        String::new()
    })
}

fn client_secret() -> String {
    std::env::var("GEMINI_CLIENT_SECRET").unwrap_or_else(|_| {
        // Replace with your own OAuth client secret for personal builds
        String::new()
    })
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

pub fn save_tokens(tokens: &TokenStore) {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER).unwrap();
    if let Ok(json) = serde_json::to_string(tokens) {
        let _ = entry.set_password(&json);
    }
}

pub fn load_tokens() -> Option<TokenStore> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER).ok()?;
    let json = entry.get_password().ok()?;
    serde_json::from_str(&json).ok()
}

pub fn clear_tokens() {
    if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER) {
        let _ = entry.delete_credential();
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
            "https://www.googleapis.com/auth/generative-language".to_string(),
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

pub async fn fetch_available_models(access_token: &str) -> Result<Vec<ModelInfo>, String> {
    let client = Client::new();
    let resp = client
        .get("https://generativelanguage.googleapis.com/v1beta/models")
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| format!("Request error: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("API error {}: {}", status, body));
    }

    let parsed: ModelsResponse = resp
        .json()
        .await
        .map_err(|e| format!("JSON parse error: {}", e))?;

    let models = parsed
        .models
        .unwrap_or_default()
        .into_iter()
        .filter(|m| m.supports_generation())
        .collect();

    Ok(models)
}
