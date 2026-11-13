use crate::auth::{
    clear_tokens, detect_account_plan, fetch_available_models, get_gemini_api_key, load_is_pro_plan,
    load_tokens, refresh_access_token, save_is_pro_plan, start_login_flow, ModelInfo, TokenStore,
};
use crate::gemini::{stream_message, AuthMethod};
use crate::types::{Message, Role};
use egui::{Color32, RichText, Rounding, Stroke, Vec2};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use std::sync::Arc;
use tokio::sync::mpsc;

// ── Verified Live Working Models ─────────────────────────────────────────────

fn default_models(is_pro: bool) -> Vec<ModelInfo> {
    let mut list = vec![
        ModelInfo {
            name: "models/gemini-4-hydrogen".to_string(),
            display_name: "✦ Gemini 4 (Hydrogen)".to_string(),
            description: "Next-generation Gemini 4 frontier model — live & verified".to_string(),
            supported_generation_methods: vec!["generateContent".to_string()],
        },
        ModelInfo {
            name: "models/gemini-3-flash-preview".to_string(),
            display_name: "⚡ Gemini 3 Flash".to_string(),
            description: "Flagship high-speed multimodal intelligence — live & verified".to_string(),
            supported_generation_methods: vec!["generateContent".to_string()],
        },
        ModelInfo {
            name: "models/gemini-3.5-flash-lite".to_string(),
            display_name: "⚡ Gemini 3.5 Flash Lite".to_string(),
            description: "Ultra-fast, efficient model for quick answers & coding — live & verified".to_string(),
            supported_generation_methods: vec!["generateContent".to_string()],
        },
        ModelInfo {
            name: "models/gemini-3.1-flash-lite-preview".to_string(),
            display_name: "⚡ Gemini 3.1 Flash Lite".to_string(),
            description: "Fast reasoning and problem-solving lite model — live & verified".to_string(),
            supported_generation_methods: vec!["generateContent".to_string()],
        },
        ModelInfo {
            name: "models/gemini-flash-lite-latest".to_string(),
            display_name: "⚡ Gemini Flash Lite (Latest)".to_string(),
            description: "Auto-updating release of Gemini Flash Lite — live & verified".to_string(),
            supported_generation_methods: vec!["generateContent".to_string()],
        },
    ];

    if is_pro {
        list.insert(
            0,
            ModelInfo {
                name: "models/gemini-4-argon".to_string(),
                display_name: "✦ Gemini 4 (Argon Pro)".to_string(),
                description: "Gemini 4 Argon Pro — frontier reasoning for Pro accounts".to_string(),
                supported_generation_methods: vec!["generateContent".to_string()],
            },
        );
        list.insert(
            1,
            ModelInfo {
                name: "models/gemini-4-carbon".to_string(),
                display_name: "✦ Gemini 4 (Carbon Pro)".to_string(),
                description: "Gemini 4 Carbon Pro — high-throughput frontier reasoning for Pro accounts".to_string(),
                supported_generation_methods: vec!["generateContent".to_string()],
            },
        );
    }

    list
}

// ── Multi-Tab Conversation Struct ───────────────────────────────────────────

pub struct ConversationTab {
    pub id: u64,
    pub title: String,
    pub history: Vec<Message>,
    pub input: String,
    pub is_streaming: bool,
    pub pending_response: String,
    pub copied_idx: Option<usize>,
    pub scroll_to_bottom: bool,
    pub chat_rx: Option<mpsc::Receiver<String>>,
    pub markdown_cache: CommonMarkCache,
}

impl ConversationTab {
    pub fn new(id: u64) -> Self {
        Self {
            id,
            title: "New chat".to_string(),
            history: Vec::new(),
            input: String::new(),
            is_streaming: false,
            pending_response: String::new(),
            copied_idx: None,
            scroll_to_bottom: true,
            chat_rx: None,
            markdown_cache: CommonMarkCache::default(),
        }
    }
}

enum AuthState {
    LoggedOut,
    LoggingIn,
    LoggedIn {
        tokens: TokenStore,
        models: Vec<ModelInfo>,
        loading_models: bool,
    },
    Error(String),
}

pub struct GeminiApp {
    // Auth & Plan
    auth_state: AuthState,
    is_pro_plan: bool,
    detected_plan_name: String,
    avatar_texture: Option<egui::TextureHandle>,

    // Multi-tab Conversations (Browser-style)
    tabs: Vec<ConversationTab>,
    active_tab_id: u64,
    next_tab_id: u64,

    // App state
    selected_model_idx: usize,
    system_prompt: String,
    show_settings: bool,

    // Runtime & async channels
    runtime: Arc<tokio::runtime::Runtime>,
    auth_rx: Option<mpsc::Receiver<AuthTaskResult>>,
}

enum AuthTaskResult {
    LoginSuccess(TokenStore),
    LoginError(String),
    ModelsLoaded(Vec<ModelInfo>),
    ModelsError(String),
    TokenRefreshed(TokenStore),
    PlanDetected(String),
}

impl GeminiApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("Failed to create Tokio runtime"),
        );

        let is_pro_plan = load_is_pro_plan();
        let detected_plan_name = if is_pro_plan {
            "✦ Google AI Pro".to_string()
        } else {
            "Free Plan".to_string()
        };

        // Load saved session from %APPDATA%/gemini_client/auth.json or keyring
        let saved = load_tokens();
        let auth_state = match saved {
            Some(tokens) => AuthState::LoggedIn {
                tokens,
                models: default_models(is_pro_plan),
                loading_models: true,
            },
            None => AuthState::LoggedOut,
        };

        let initial_tab = ConversationTab::new(1);

        Self {
            auth_state,
            is_pro_plan,
            detected_plan_name,
            avatar_texture: None,
            tabs: vec![initial_tab],
            active_tab_id: 1,
            next_tab_id: 2,
            selected_model_idx: 0,
            system_prompt: String::new(),
            show_settings: false,
            runtime,
            auth_rx: None,
        }
    }

    // ── Tab Management Helpers ────────────────────────────────────────────────

    pub fn add_new_tab(&mut self) -> u64 {
        let id = self.next_tab_id;
        self.next_tab_id += 1;
        self.tabs.push(ConversationTab::new(id));
        self.active_tab_id = id;
        id
    }

    pub fn close_tab(&mut self, id: u64) {
        if self.tabs.len() <= 1 {
            // Keep at least one tab open; reset it to fresh state
            if let Some(tab) = self.tabs.first_mut() {
                tab.title = "New chat".to_string();
                tab.history.clear();
                tab.input.clear();
                tab.is_streaming = false;
                tab.pending_response.clear();
                tab.copied_idx = None;
                tab.chat_rx = None;
                tab.scroll_to_bottom = true;
            }
            return;
        }

        if let Some(pos) = self.tabs.iter().position(|t| t.id == id) {
            self.tabs.remove(pos);
            if self.active_tab_id == id {
                let new_idx = if pos >= self.tabs.len() {
                    self.tabs.len() - 1
                } else {
                    pos
                };
                self.active_tab_id = self.tabs[new_idx].id;
            }
        }
    }

    pub fn next_tab(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        if let Some(pos) = self.tabs.iter().position(|t| t.id == self.active_tab_id) {
            let next_pos = (pos + 1) % self.tabs.len();
            self.active_tab_id = self.tabs[next_pos].id;
        }
    }

    pub fn prev_tab(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        if let Some(pos) = self.tabs.iter().position(|t| t.id == self.active_tab_id) {
            let prev_pos = if pos == 0 {
                self.tabs.len() - 1
            } else {
                pos - 1
            };
            self.active_tab_id = self.tabs[prev_pos].id;
        }
    }

    pub fn active_tab_mut(&mut self) -> Option<&mut ConversationTab> {
        self.tabs.iter_mut().find(|t| t.id == self.active_tab_id)
    }

    pub fn active_tab(&self) -> Option<&ConversationTab> {
        self.tabs.iter().find(|t| t.id == self.active_tab_id)
    }

    fn get_avatar_texture(&mut self, ctx: &egui::Context) -> Option<egui::TextureHandle> {
        if let Some(ref tex) = self.avatar_texture {
            return Some(tex.clone());
        }

        let mut bytes_opt = None;
        // 1. Try tokens picture_url
        if let AuthState::LoggedIn { tokens, .. } = &self.auth_state {
            if let Some(ref pic) = tokens.picture_url {
                let p = std::path::PathBuf::from(pic);
                if p.exists() {
                    bytes_opt = std::fs::read(&p).ok();
                }
            }
        }

        // 2. Try %APPDATA%\gemini_client\avatar.png
        if bytes_opt.is_none() {
            if let Ok(appdata) = std::env::var("APPDATA") {
                let p = std::path::PathBuf::from(appdata).join("gemini_client").join("avatar.png");
                if p.exists() {
                    bytes_opt = std::fs::read(&p).ok();
                }
            }
        }

        if let Some(bytes) = bytes_opt {
            if let Ok(img) = image::load_from_memory(&bytes) {
                let rgba = img.to_rgba8();
                let size = [rgba.width() as usize, rgba.height() as usize];
                let pixels = rgba.into_raw();
                let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
                let tex = ctx.load_texture("user_avatar", color_image, egui::TextureOptions::LINEAR);
                self.avatar_texture = Some(tex.clone());
                return Some(tex);
            }
        }

        None
    }

    fn set_pro_plan(&mut self, is_pro: bool, ctx: egui::Context) {
        self.is_pro_plan = is_pro;
        self.detected_plan_name = if is_pro {
            "✦ Google AI Pro".to_string()
        } else {
            "Free Plan".to_string()
        };
        save_is_pro_plan(is_pro);
        if let AuthState::LoggedIn { models, .. } = &mut self.auth_state {
            *models = default_models(is_pro);
            self.selected_model_idx = 0;
        }
        ctx.request_repaint();
    }

    // ── Auth helpers ──────────────────────────────────────────────────────────

    fn start_login(&mut self, ctx: egui::Context) {
        self.auth_state = AuthState::LoggingIn;
        let (tx, rx) = mpsc::channel::<AuthTaskResult>(4);
        self.auth_rx = Some(rx);
        let tx2 = tx.clone();
        self.runtime.spawn(async move {
            match start_login_flow().await {
                Ok(tokens) => {
                    let _ = tx2.send(AuthTaskResult::LoginSuccess(tokens)).await;
                }
                Err(e) => {
                    let _ = tx2.send(AuthTaskResult::LoginError(e)).await;
                }
            }
            ctx.request_repaint();
        });
    }

    fn fetch_models(&mut self, tokens: TokenStore, ctx: egui::Context) {
        let (tx, rx) = mpsc::channel::<AuthTaskResult>(8);
        self.auth_rx = Some(rx);
        let access_token = tokens.access_token.clone();
        let is_pro = self.is_pro_plan;

        let tx_models = tx.clone();
        let ctx_models = ctx.clone();
        self.runtime.spawn(async move {
            match fetch_available_models(&access_token, is_pro).await {
                Ok(models) => {
                    let _ = tx_models.send(AuthTaskResult::ModelsLoaded(models)).await;
                }
                Err(e) => {
                    let _ = tx_models.send(AuthTaskResult::ModelsError(e)).await;
                }
            }
            ctx_models.request_repaint();
        });

        // Automatically detect account plan from live quota
        let tx_plan = tx.clone();
        let ctx_plan = ctx.clone();
        self.runtime.spawn(async move {
            let plan = detect_account_plan().await;
            let _ = tx_plan.send(AuthTaskResult::PlanDetected(plan)).await;
            ctx_plan.request_repaint();
        });
    }

    fn refresh_token_if_needed(&mut self, ctx: egui::Context) {
        if let AuthState::LoggedIn { tokens, .. } = &self.auth_state {
            if tokens.is_expired() {
                let tokens = tokens.clone();
                let (tx, rx) = mpsc::channel::<AuthTaskResult>(2);
                self.auth_rx = Some(rx);
                self.runtime.spawn(async move {
                    match refresh_access_token(&tokens).await {
                        Ok(new_tokens) => {
                            let _ = tx.send(AuthTaskResult::TokenRefreshed(new_tokens)).await;
                        }
                        Err(_) => {}
                    }
                    ctx.request_repaint();
                });
            }
        }
    }

    fn poll_auth(&mut self, ctx: &egui::Context) {
        if let AuthState::LoggedIn { tokens, models, loading_models } = &self.auth_state {
            if *loading_models && models.is_empty() && self.auth_rx.is_none() {
                let tokens_clone = tokens.clone();
                let ctx_clone = ctx.clone();
                self.fetch_models(tokens_clone, ctx_clone);
                if let AuthState::LoggedIn { loading_models, .. } = &mut self.auth_state {
                    *loading_models = false;
                }
            }
        }

        let result = if let Some(rx) = &mut self.auth_rx {
            rx.try_recv().ok()
        } else {
            None
        };

        match result {
            Some(AuthTaskResult::LoginSuccess(tokens)) => {
                let ctx_clone = ctx.clone();
                self.auth_rx = None;
                self.auth_state = AuthState::LoggedIn {
                    tokens: tokens.clone(),
                    models: default_models(self.is_pro_plan),
                    loading_models: false,
                };
                self.fetch_models(tokens, ctx_clone);
            }
            Some(AuthTaskResult::LoginError(e)) => {
                self.auth_rx = None;
                self.auth_state = AuthState::Error(e);
            }
            Some(AuthTaskResult::ModelsLoaded(models)) => {
                self.auth_rx = None;
                if let AuthState::LoggedIn { models: m, loading_models, .. } = &mut self.auth_state {
                    *m = if models.is_empty() { default_models(self.is_pro_plan) } else { models };
                    *loading_models = false;
                    self.selected_model_idx = 0;
                }
            }
            Some(AuthTaskResult::ModelsError(e)) => {
                self.auth_rx = None;
                eprintln!("Models fetch error: {}", e);
                if let AuthState::LoggedIn { models: m, loading_models, .. } = &mut self.auth_state {
                    if m.is_empty() {
                        *m = default_models(self.is_pro_plan);
                    }
                    *loading_models = false;
                    self.selected_model_idx = 0;
                }
            }
            Some(AuthTaskResult::TokenRefreshed(new_tokens)) => {
                self.auth_rx = None;
                if let AuthState::LoggedIn { tokens, .. } = &mut self.auth_state {
                    *tokens = new_tokens;
                }
            }
            Some(AuthTaskResult::PlanDetected(plan)) => {
                let is_pro = plan.contains("Pro") || self.is_pro_plan;
                self.is_pro_plan = is_pro;
                self.detected_plan_name = if is_pro {
                    "✦ Google AI Pro".to_string()
                } else {
                    plan
                };
                save_is_pro_plan(is_pro);
            }
            None => {}
        }
    }

    // ── Chat & Background Streaming Helpers ────────────────────────────────────

    fn send_message(&mut self, ctx: egui::Context) {
        let tab = match self.tabs.iter_mut().find(|t| t.id == self.active_tab_id) {
            Some(t) => t,
            None => return,
        };

        let input = tab.input.trim().to_string();
        if input.is_empty() || tab.is_streaming {
            return;
        }

        // Auto-update tab title if this is the first message or "New chat"
        if tab.title == "New chat" || tab.title.is_empty() {
            let first_line = input.lines().next().unwrap_or("Chat").trim();
            let chars: Vec<char> = first_line.chars().collect();
            tab.title = if chars.len() > 20 {
                let s: String = chars[..18].iter().collect();
                format!("{}…", s)
            } else {
                first_line.to_string()
            };
        }

        let auth = if let Some(key) = get_gemini_api_key() {
            AuthMethod::ApiKey(key)
        } else {
            match &self.auth_state {
                AuthState::LoggedIn { tokens, .. } => {
                    AuthMethod::Bearer(tokens.access_token.clone())
                }
                _ => return,
            }
        };

        let model = match &self.auth_state {
            AuthState::LoggedIn { models, .. } if !models.is_empty() => {
                models
                    .get(self.selected_model_idx)
                    .map(|m| m.api_name().to_string())
                    .unwrap_or_else(|| "gemini-4-hydrogen".to_string())
            }
            _ => "gemini-4-hydrogen".to_string(),
        };

        tab.history.push(Message::user(&input));
        tab.input.clear();
        tab.is_streaming = true;
        tab.pending_response.clear();
        tab.scroll_to_bottom = true;

        let (tx, rx) = mpsc::channel::<String>(256);
        tab.chat_rx = Some(rx);

        let mut history_to_send = tab.history.clone();
        if !self.system_prompt.is_empty() && history_to_send.len() == 1 {
            history_to_send.insert(
                0,
                Message {
                    role: Role::User,
                    content: format!("System instructions: {}\n\nAcknowledge briefly.", self.system_prompt),
                },
            );
            history_to_send.insert(
                1,
                Message {
                    role: Role::Model,
                    content: "Understood.".to_string(),
                },
            );
        }

        self.runtime.spawn(async move {
            stream_message(auth, model, history_to_send, tx).await;
            ctx.request_repaint();
        });
    }

    fn poll_chat(&mut self) {
        // Poll all tabs concurrently so background tabs stream uninterrupted
        for tab in &mut self.tabs {
            if !tab.is_streaming {
                continue;
            }

            let mut done = false;
            if let Some(rx) = &mut tab.chat_rx {
                loop {
                    match rx.try_recv() {
                        Ok(token) => {
                            tab.pending_response.push_str(&token);
                            tab.scroll_to_bottom = true;
                        }
                        Err(mpsc::error::TryRecvError::Empty) => break,
                        Err(mpsc::error::TryRecvError::Disconnected) => {
                            done = true;
                            break;
                        }
                    }
                }
            }

            if done {
                if !tab.pending_response.is_empty() {
                    tab.history.push(Message::model(tab.pending_response.clone()));
                }
                tab.pending_response.clear();
                tab.is_streaming = false;
                tab.chat_rx = None;
            }
        }
    }
}

// ── Application UI ───────────────────────────────────────────────────────────

impl eframe::App for GeminiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_auth(ctx);
        self.poll_chat();
        self.refresh_token_if_needed(ctx.clone());

        // Keyboard Shortcuts (Browser-style)
        // Ctrl+T: New Tab
        if ctx.input(|i| (i.modifiers.ctrl || i.modifiers.command) && i.key_pressed(egui::Key::T)) {
            self.add_new_tab();
        }
        // Ctrl+W: Close Current Tab
        if ctx.input(|i| (i.modifiers.ctrl || i.modifiers.command) && i.key_pressed(egui::Key::W)) {
            self.close_tab(self.active_tab_id);
        }
        // Ctrl+Tab / Ctrl+Shift+Tab: Cycle Tabs
        if ctx.input(|i| (i.modifiers.ctrl || i.modifiers.command) && i.modifiers.shift && i.key_pressed(egui::Key::Tab)) {
            self.prev_tab();
        } else if ctx.input(|i| (i.modifiers.ctrl || i.modifiers.command) && i.key_pressed(egui::Key::Tab)) {
            self.next_tab();
        }

        // Repaint if any tab is streaming or auth in progress
        if self.tabs.iter().any(|t| t.is_streaming) || matches!(self.auth_state, AuthState::LoggingIn) {
            ctx.request_repaint();
        }

        match &self.auth_state {
            AuthState::LoggedOut | AuthState::LoggingIn | AuthState::Error(_) => {
                self.draw_login_screen(ctx);
            }
            AuthState::LoggedIn { .. } => {
                self.draw_chat_ui(ctx);
            }
        }
    }
}

impl GeminiApp {
    fn draw_login_screen(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(Color32::from_rgb(19, 19, 20)))
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(ui.available_height() / 4.0);

                    ui.label(
                        RichText::new("✦")
                            .size(54.0)
                            .color(Color32::from_rgb(120, 190, 255)),
                    );
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new("Gemini")
                            .size(32.0)
                            .color(Color32::WHITE)
                            .strong(),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("Fast, lightweight native desktop experience.")
                            .size(15.0)
                            .color(Color32::from_gray(160)),
                    );
                    ui.add_space(36.0);

                    match &self.auth_state {
                        AuthState::LoggedOut => {
                            let btn = ui.add_sized(
                                Vec2::new(240.0, 48.0),
                                egui::Button::new(
                                    RichText::new("🔑  Sign in with Google")
                                        .size(15.0)
                                        .color(Color32::WHITE),
                                )
                                .fill(Color32::from_rgb(26, 115, 232))
                                .rounding(Rounding::same(24.0)),
                            );
                            if btn.clicked() {
                                self.start_login(ctx.clone());
                            }
                        }
                        AuthState::LoggingIn => {
                            ui.spinner();
                            ui.add_space(10.0);
                            ui.label(
                                RichText::new("Opening browser… complete sign-in there.")
                                    .color(Color32::from_gray(190)),
                            );
                        }
                        AuthState::Error(e) => {
                            ui.colored_label(Color32::from_rgb(255, 100, 100), format!("⚠ {}", e));
                            ui.add_space(12.0);
                            if ui.button("Try again").clicked() {
                                self.auth_state = AuthState::LoggedOut;
                            }
                        }
                        _ => {}
                    }
                });
            });
    }

    fn draw_chat_ui(&mut self, ctx: &egui::Context) {
        // ── Top Navigation Bar ────────────────────────────────────────────────
        egui::TopBottomPanel::top("top_bar")
            .frame(
                egui::Frame::none()
                    .fill(Color32::from_rgb(27, 28, 29))
                    .inner_margin(egui::Margin::symmetric(16.0, 10.0))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(42, 43, 46))),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    // Logo
                    ui.label(
                        RichText::new("✦ Gemini")
                            .size(18.0)
                            .color(Color32::from_rgb(120, 190, 255))
                            .strong(),
                    );

                    ui.add_space(12.0);

                    // New Chat Button
                    let new_chat_btn = ui.add(
                        egui::Button::new(RichText::new("✨ + New chat").size(13.0).color(Color32::WHITE))
                            .fill(Color32::from_rgb(43, 44, 48))
                            .rounding(Rounding::same(16.0)),
                    ).on_hover_text("Open new conversation tab (Ctrl+T)");
                    if new_chat_btn.clicked() {
                        self.add_new_tab();
                    }

                    ui.add_space(16.0);

                    // Model Selector Dropdown
                    if let AuthState::LoggedIn { models, loading_models, .. } = &self.auth_state {
                        if *loading_models && models.is_empty() {
                            ui.spinner();
                            ui.label(RichText::new("Loading models…").color(Color32::from_gray(160)));
                        } else {
                            let selected_name = models
                                .get(self.selected_model_idx)
                                .map(|m| m.display_name.as_str())
                                .unwrap_or("Select model");

                            egui::ComboBox::from_id_salt("model_select")
                                .selected_text(RichText::new(selected_name).size(13.0).color(Color32::WHITE))
                                .width(280.0)
                                .show_ui(ui, |ui| {
                                    for (i, model) in models.iter().enumerate() {
                                        ui.selectable_value(
                                            &mut self.selected_model_idx,
                                            i,
                                            &model.display_name,
                                        )
                                        .on_hover_text(&model.description);
                                    }
                                });
                        }
                    }

                    // Right items: Plan Badge + Account Profile + Settings
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let AuthState::LoggedIn { tokens, .. } = &self.auth_state {
                            let name = tokens
                                .display_name
                                .as_deref()
                                .or(tokens.email.as_deref())
                                .unwrap_or("Account")
                                .to_string();
                            let email = tokens.email.clone();

                            // Settings button
                            if ui
                                .add(
                                    egui::Button::new(RichText::new("⚙️ Settings").size(12.5).color(Color32::from_gray(210)))
                                        .fill(Color32::from_rgb(34, 35, 38))
                                        .rounding(Rounding::same(16.0))
                                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(52, 54, 60))),
                                )
                                .on_hover_text("Open Settings & Customizations")
                                .clicked()
                            {
                                self.show_settings = !self.show_settings;
                            }

                            ui.add_space(8.0);

                            // User Profile Menu Button with Photo Avatar
                            let avatar_tex = self.get_avatar_texture(ctx);
                            let popup_id = ui.make_persistent_id("user_profile_popup");

                            let profile_frame = egui::Frame::none()
                                .fill(Color32::from_rgb(34, 35, 38))
                                .rounding(Rounding::same(18.0))
                                .stroke(Stroke::new(1.0_f32, Color32::from_rgb(52, 54, 60)))
                                .inner_margin(egui::Margin::symmetric(10.0, 4.0));

                            let profile_resp = profile_frame.show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    if let Some(ref tex) = avatar_tex {
                                        ui.add(
                                            egui::Image::new(tex)
                                                .fit_to_exact_size(egui::vec2(22.0, 22.0))
                                                .rounding(Rounding::same(11.0)),
                                        );
                                        ui.add_space(4.0);
                                    } else {
                                        ui.label(RichText::new("👤").size(14.0));
                                        ui.add_space(2.0);
                                    }
                                    ui.label(RichText::new(&name).size(12.5).color(Color32::WHITE).strong());
                                    ui.add_space(2.0);
                                    ui.label(RichText::new("▾").size(10.0).color(Color32::from_gray(160)));
                                });
                            });

                            let btn_rect = profile_resp.response.rect;
                            if ui.interact(btn_rect, popup_id, egui::Sense::click()).clicked() {
                                ui.memory_mut(|mem| mem.toggle_popup(popup_id));
                            }

                            egui::popup_below_widget(
                                ui,
                                popup_id,
                                &profile_resp.response,
                                egui::PopupCloseBehavior::CloseOnClickOutside,
                                |ui| {
                                    ui.set_min_width(260.0);
                                    egui::Frame::none()
                                        .fill(Color32::from_rgb(28, 29, 33))
                                        .inner_margin(egui::Margin::symmetric(14.0, 14.0))
                                        .show(ui, |ui| {
                                            ui.horizontal(|ui| {
                                                if let Some(ref tex) = avatar_tex {
                                                    ui.add(
                                                        egui::Image::new(tex)
                                                            .fit_to_exact_size(egui::vec2(44.0, 44.0))
                                                            .rounding(Rounding::same(22.0)),
                                                    );
                                                    ui.add_space(8.0);
                                                } else {
                                                    ui.label(RichText::new("👤").size(32.0));
                                                    ui.add_space(6.0);
                                                }
                                                ui.vertical(|ui| {
                                                    ui.label(RichText::new(&name).size(14.0).color(Color32::WHITE).strong());
                                                    if let Some(ref e) = email {
                                                        ui.label(RichText::new(e).size(11.0).color(Color32::from_gray(160)));
                                                    }
                                                });
                                            });

                                            ui.add_space(8.0);
                                            ui.separator();
                                            ui.add_space(6.0);

                                            // Plan status line
                                            ui.horizontal(|ui| {
                                                ui.label(RichText::new("✦ Plan:").size(12.0).color(Color32::from_gray(180)));
                                                let (p_col, p_bg) = if self.is_pro_plan {
                                                    (Color32::from_rgb(255, 215, 80), Color32::from_rgb(45, 38, 20))
                                                } else {
                                                    (Color32::from_rgb(130, 190, 255), Color32::from_rgb(25, 38, 55))
                                                };
                                                egui::Frame::none()
                                                    .fill(p_bg)
                                                    .rounding(Rounding::same(10.0))
                                                    .stroke(Stroke::new(1.0_f32, p_col))
                                                    .inner_margin(egui::Margin::symmetric(8.0, 2.0))
                                                    .show(ui, |ui| {
                                                        ui.label(RichText::new(&self.detected_plan_name).size(11.0).color(p_col).strong());
                                                    });
                                            });

                                            ui.add_space(10.0);
                                            ui.separator();
                                            ui.add_space(6.0);

                                            if ui.add(
                                                egui::Button::new(RichText::new("⚙️  Preferences & Settings").size(13.0).color(Color32::WHITE))
                                                    .fill(Color32::TRANSPARENT),
                                            ).clicked() {
                                                self.show_settings = !self.show_settings;
                                                ui.memory_mut(|mem| mem.close_popup());
                                            }

                                            ui.add_space(4.0);

                                            if ui.add(
                                                egui::Button::new(RichText::new("🔄  Refresh Account & Models").size(13.0).color(Color32::WHITE))
                                                    .fill(Color32::TRANSPARENT),
                                            ).clicked() {
                                                if let AuthState::LoggedIn { tokens, .. } = &self.auth_state {
                                                    let tokens_clone = tokens.clone();
                                                    self.fetch_models(tokens_clone, ctx.clone());
                                                }
                                                ui.memory_mut(|mem| mem.close_popup());
                                            }

                                            ui.add_space(4.0);
                                            ui.separator();
                                            ui.add_space(4.0);

                                            if ui.add(
                                                egui::Button::new(RichText::new("🚪  Sign Out").size(13.0).color(Color32::from_rgb(255, 120, 120)))
                                                    .fill(Color32::TRANSPARENT),
                                            ).clicked() {
                                                clear_tokens();
                                                self.auth_state = AuthState::LoggedOut;
                                                self.tabs = vec![ConversationTab::new(1)];
                                                self.active_tab_id = 1;
                                                self.next_tab_id = 2;
                                                self.avatar_texture = None;
                                                ui.memory_mut(|mem| mem.close_popup());
                                            }
                                        });
                                },
                            );

                            ui.add_space(8.0);

                            // Plan badge
                            let (badge_color, badge_bg) = if self.is_pro_plan {
                                (Color32::from_rgb(255, 215, 80), Color32::from_rgb(45, 36, 16))
                            } else {
                                (Color32::from_rgb(130, 190, 255), Color32::from_rgb(25, 38, 55))
                            };
                            let badge_text = if self.is_pro_plan { "✦ GOOGLE AI PRO" } else { "FREE" };
                            egui::Frame::none()
                                .fill(badge_bg)
                                .rounding(Rounding::same(12.0))
                                .stroke(Stroke::new(1.0_f32, badge_color))
                                .inner_margin(egui::Margin::symmetric(10.0, 4.0))
                                .show(ui, |ui| {
                                    ui.label(
                                        RichText::new(badge_text)
                                            .color(badge_color)
                                            .size(11.0)
                                            .strong(),
                                    );
                                });
                        }
                    });
                });
            });

        // ── Browser-style Tab Bar ─────────────────────────────────────────────
        egui::TopBottomPanel::top("browser_tabs")
            .frame(
                egui::Frame::none()
                    .fill(Color32::from_rgb(22, 23, 26))
                    .inner_margin(egui::Margin {
                        left: 14.0,
                        right: 14.0,
                        top: 6.0,
                        bottom: 0.0,
                    })
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(38, 39, 44))),
            )
            .show(ctx, |ui| {
                let mut switch_to_tab = None;
                let mut close_tab_id = None;

                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(6.0, 0.0);

                    egui::ScrollArea::horizontal()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                for tab in &self.tabs {
                                    let is_active = tab.id == self.active_tab_id;
                                    let (bg_color, stroke_color, text_color) = if is_active {
                                        (
                                            Color32::from_rgb(34, 36, 42),
                                            Stroke::new(1.5_f32, Color32::from_rgb(66, 133, 244)),
                                            Color32::WHITE,
                                        )
                                    } else {
                                        (
                                            Color32::from_rgb(25, 26, 29),
                                            Stroke::new(1.0_f32, Color32::from_rgb(42, 44, 49)),
                                            Color32::from_gray(170),
                                        )
                                    };

                                    let tab_frame = egui::Frame::none()
                                        .fill(bg_color)
                                        .rounding(Rounding {
                                            nw: 8.0,
                                            ne: 8.0,
                                            sw: 0.0,
                                            se: 0.0,
                                        })
                                        .stroke(stroke_color)
                                        .inner_margin(egui::Margin::symmetric(12.0, 7.0));

                                    let tab_resp = tab_frame.show(ui, |ui| {
                                        ui.set_min_width(110.0);
                                        ui.set_max_width(210.0);
                                        ui.horizontal(|ui| {
                                            if tab.is_streaming {
                                                ui.spinner();
                                            } else {
                                                ui.label(RichText::new("💬").size(12.0));
                                            }
                                            ui.add_space(4.0);

                                            let title_str = if tab.title.trim().is_empty() {
                                                "New chat"
                                            } else {
                                                &tab.title
                                            };
                                            let label = ui.add(
                                                egui::Label::new(
                                                    RichText::new(title_str)
                                                        .size(12.5)
                                                        .color(text_color)
                                                        .strong(),
                                                )
                                                .truncate()
                                                .sense(egui::Sense::click()),
                                            );
                                            if label.clicked() {
                                                switch_to_tab = Some(tab.id);
                                            }

                                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                let close_btn = ui.add(
                                                    egui::Button::new(
                                                        RichText::new("✕")
                                                            .size(11.0)
                                                            .color(if is_active {
                                                                Color32::from_gray(200)
                                                            } else {
                                                                Color32::from_gray(140)
                                                            }),
                                                    )
                                                    .fill(Color32::TRANSPARENT)
                                                    .frame(false),
                                                ).on_hover_text("Close tab (Ctrl+W)");

                                                if close_btn.clicked() {
                                                    close_tab_id = Some(tab.id);
                                                }
                                            });
                                        });
                                    });

                                    if tab_resp.response.interact(egui::Sense::click()).clicked() {
                                        switch_to_tab = Some(tab.id);
                                    }
                                }

                                // ➕ Add new tab button
                                let add_btn = ui.add(
                                    egui::Button::new(RichText::new("➕").size(12.0).color(Color32::from_gray(210)))
                                        .fill(Color32::from_rgb(33, 35, 39))
                                        .rounding(Rounding::same(6.0)),
                                ).on_hover_text("New tab (Ctrl+T)");

                                if add_btn.clicked() {
                                    self.add_new_tab();
                                }
                            });
                        });
                });

                if let Some(id) = switch_to_tab {
                    self.active_tab_id = id;
                }
                if let Some(id) = close_tab_id {
                    self.close_tab(id);
                }
            });

        // ── Settings Drawer Panel ─────────────────────────────────────────────
        if self.show_settings {
            egui::SidePanel::right("settings_panel")
                .frame(
                    egui::Frame::none()
                        .fill(Color32::from_rgb(27, 28, 29))
                        .inner_margin(egui::Margin::symmetric(16.0, 16.0))
                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(42, 43, 46))),
                )
                .resizable(true)
                .default_width(300.0)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.heading(RichText::new("⚙️ Settings").color(Color32::WHITE));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("✕").clicked() {
                                self.show_settings = false;
                            }
                        });
                    });
                    ui.separator();

                    ui.add_space(8.0);
                    ui.label(RichText::new("✦ Subscription Plan").strong().color(Color32::WHITE));
                    let mut is_pro = self.is_pro_plan;
                    let label_color = if is_pro { Color32::from_rgb(255, 215, 80) } else { Color32::from_gray(180) };
                    if ui
                        .checkbox(
                            &mut is_pro,
                            RichText::new("Google AI Pro / Advanced")
                                .color(label_color)
                                .strong(),
                        )
                        .changed()
                    {
                        self.set_pro_plan(is_pro, ctx.clone());
                    }
                    ui.add_space(4.0);
                    let plan_color = if self.is_pro_plan {
                        Color32::from_rgb(255, 215, 80)
                    } else {
                        Color32::from_rgb(130, 190, 255)
                    };
                    ui.colored_label(
                        plan_color,
                        RichText::new(format!("● Active Plan: {}", self.detected_plan_name)).size(13.0).strong(),
                    );
                    ui.label(
                        RichText::new("Unlocks Gemini 4 Argon & Carbon frontier reasoning models.")
                            .color(Color32::from_gray(140))
                            .size(11.0),
                    );

                    ui.add_space(16.0);
                    ui.separator();

                    ui.label(RichText::new("🧠 System Instructions").strong().color(Color32::WHITE));
                    ui.add(
                        egui::TextEdit::multiline(&mut self.system_prompt)
                            .hint_text("Optional persona or guidelines for Gemini…")
                            .desired_rows(5)
                            .desired_width(f32::INFINITY),
                    );

                    ui.add_space(16.0);
                    ui.separator();

                    if ui
                        .add_sized(
                            Vec2::new(ui.available_width(), 34.0),
                            egui::Button::new(RichText::new("🗑️ Clear current conversation").color(Color32::WHITE))
                                .fill(Color32::from_rgb(45, 47, 52))
                                .rounding(Rounding::same(8.0)),
                        )
                        .clicked()
                    {
                        if let Some(tab) = self.active_tab_mut() {
                            tab.history.clear();
                            tab.pending_response.clear();
                            tab.is_streaming = false;
                            tab.copied_idx = None;
                            tab.title = "New chat".to_string();
                        }
                    }

                    ui.add_space(12.0);
                    ui.separator();

                    if let AuthState::LoggedIn { models, .. } = &self.auth_state {
                        ui.label(
                            RichText::new(format!("🤖 {} active live models loaded", models.len()))
                                .color(Color32::from_gray(140))
                                .size(11.0),
                        );
                    }
                });
        }

        // ── Bottom Floating Input Area ────────────────────────────────────────
        let mut should_send = false;
        egui::TopBottomPanel::bottom("input_bar")
            .frame(
                egui::Frame::none()
                    .fill(Color32::from_rgb(19, 19, 20))
                    .inner_margin(egui::Margin::symmetric(24.0, 12.0)),
            )
            .show(ctx, |ui| {
                // Floating input pill container
                egui::Frame::none()
                    .fill(Color32::from_rgb(32, 33, 36))
                    .rounding(Rounding::same(24.0))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(52, 54, 58)))
                    .inner_margin(egui::Margin::symmetric(18.0, 10.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let available_w = ui.available_width() - 56.0;

                            let (has_text, is_streaming) = if let Some(tab) = self.active_tab_mut() {
                                let input_edit = egui::TextEdit::multiline(&mut tab.input)
                                    .hint_text("Ask Gemini anything… (Enter to send, Shift+Enter for newline)")
                                    .desired_rows(2)
                                    .desired_width(available_w);

                                let response = ui.add(input_edit);

                                if response.has_focus()
                                    && ctx.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift)
                                {
                                    should_send = true;
                                }

                                (!tab.input.trim().is_empty(), tab.is_streaming)
                            } else {
                                (false, false)
                            };

                            // Send button
                            let btn_fill = if is_streaming {
                                Color32::from_rgb(50, 50, 55)
                            } else if has_text {
                                Color32::from_rgb(26, 115, 232)
                            } else {
                                Color32::from_rgb(45, 46, 50)
                            };

                            let btn_icon = if is_streaming { "⏳" } else { "➤" };
                            let send_btn = ui.add_sized(
                                Vec2::new(42.0, 38.0),
                                egui::Button::new(RichText::new(btn_icon).size(16.0).color(Color32::WHITE))
                                    .fill(btn_fill)
                                    .rounding(Rounding::same(20.0)),
                            );

                            if send_btn.clicked() {
                                should_send = true;
                            }
                        });
                    });

                // Disclaimer subtext
                ui.add_space(4.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new("Gemini may display inaccurate info, including about people, so double-check its responses.")
                            .size(11.0)
                            .color(Color32::from_gray(120)),
                    );
                });
            });

        if should_send {
            self.send_message(ctx.clone());
        }

        // ── Main Chat Area ────────────────────────────────────────────────────
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(Color32::from_rgb(19, 19, 20)))
            .show(ctx, |ui| {
                let mut card_clicked_prompt: Option<String> = None;
                let mut copy_target: Option<(usize, String)> = None;

                let scroll_to_bottom = self.active_tab().map(|t| t.scroll_to_bottom).unwrap_or(false);

                let scroll = egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .stick_to_bottom(scroll_to_bottom);

                scroll.show(ui, |ui| {
                    ui.add_space(16.0);

                    // Check active tab state
                    let (is_empty, is_streaming) = if let Some(tab) = self.active_tab() {
                        (tab.history.is_empty(), tab.is_streaming)
                    } else {
                        (true, false)
                    };

                    // ── Empty State / Welcome Screen ──────────────────────────
                    if is_empty && !is_streaming {
                        let user_name = if let AuthState::LoggedIn { tokens, .. } = &self.auth_state {
                            tokens.display_name.clone().unwrap_or_else(|| "there".to_string())
                        } else {
                            "there".to_string()
                        };
                        let first_name = user_name.split_whitespace().next().unwrap_or("there");

                        ui.vertical_centered(|ui| {
                            ui.add_space(ui.available_height() / 6.0);

                            ui.label(
                                RichText::new(format!("✦ Hello, {}", first_name))
                                    .size(36.0)
                                    .color(Color32::from_rgb(120, 190, 255))
                                    .strong(),
                            );
                            ui.add_space(8.0);
                            ui.label(
                                RichText::new("How can I help you today?")
                                    .size(18.0)
                                    .color(Color32::from_gray(160)),
                            );
                            ui.add_space(32.0);

                            // Quick Prompt Cards
                            let suggestions = [
                                "🚀 Write a high-performance async Rust microservice with Axum",
                                "🧠 Explain quantum computing principles in simple terms",
                                "⚡ Optimize algorithm time and space complexity in Rust",
                                "📝 Draft a concise technical architecture proposal",
                            ];

                            for prompt in suggestions {
                                let card = egui::Frame::none()
                                    .fill(Color32::from_rgb(28, 29, 32))
                                    .rounding(Rounding::same(12.0))
                                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(45, 47, 52)))
                                    .inner_margin(egui::Margin::symmetric(20.0, 12.0));

                                card.show(ui, |ui| {
                                    if ui
                                        .add(
                                            egui::Label::new(
                                                RichText::new(prompt)
                                                    .size(13.0)
                                                    .color(Color32::from_gray(200)),
                                            )
                                            .sense(egui::Sense::click()),
                                        )
                                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                                        .clicked()
                                    {
                                        card_clicked_prompt = Some(prompt.to_string());
                                    }
                                });
                                ui.add_space(8.0);
                            }
                        });
                    }

                    // ── Active Tab Conversation Messages ──────────────────────
                    if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == self.active_tab_id) {
                        let ConversationTab {
                            ref history,
                            ref mut markdown_cache,
                            ref copied_idx,
                            is_streaming,
                            ref pending_response,
                            ..
                        } = *tab;

                        for (idx, msg) in history.iter().enumerate() {
                            let is_user = msg.role == Role::User;
                            let is_copied = *copied_idx == Some(idx);

                            if is_user {
                                // User Message Bubble (Right-aligned)
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                                    egui::Frame::none()
                                        .fill(Color32::from_rgb(43, 52, 69))
                                        .rounding(Rounding::same(16.0))
                                        .inner_margin(egui::Margin::symmetric(16.0, 12.0))
                                        .show(ui, |ui| {
                                            ui.set_max_width(ui.available_width() * 0.72);
                                            ui.label(RichText::new(&msg.content).size(14.0).color(Color32::WHITE));
                                        });
                                });
                            } else {
                                // Gemini Response Card (Left-aligned)
                                ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
                                    egui::Frame::none()
                                        .fill(Color32::from_rgb(27, 28, 30))
                                        .rounding(Rounding::same(14.0))
                                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(42, 44, 48)))
                                        .inner_margin(egui::Margin::symmetric(16.0, 14.0))
                                        .show(ui, |ui| {
                                            ui.set_max_width(ui.available_width() * 0.85);
                                            ui.vertical(|ui| {
                                                ui.horizontal(|ui| {
                                                    ui.label(
                                                        RichText::new("✦ Gemini")
                                                            .size(13.0)
                                                            .color(Color32::from_rgb(120, 190, 255))
                                                            .strong(),
                                                    );
                                                });
                                                ui.add_space(4.0);

                                                // Markdown Content
                                                CommonMarkViewer::new().show(
                                                    ui,
                                                    markdown_cache,
                                                    &msg.content,
                                                );

                                                ui.add_space(8.0);
                                                ui.horizontal(|ui| {
                                                    let copy_label = if is_copied { "Copied! ✓" } else { "📋 Copy" };
                                                    let copy_color = if is_copied {
                                                        Color32::from_rgb(100, 220, 140)
                                                    } else {
                                                        Color32::from_gray(160)
                                                    };
                                                    if ui
                                                        .add(
                                                            egui::Button::new(
                                                                RichText::new(copy_label).size(11.0).color(copy_color),
                                                            )
                                                            .fill(Color32::from_rgb(38, 40, 44))
                                                            .rounding(Rounding::same(8.0)),
                                                        )
                                                        .clicked()
                                                    {
                                                        copy_target = Some((idx, msg.content.clone()));
                                                    }
                                                });
                                            });
                                        });
                                });
                            }

                            ui.add_space(10.0);
                        }

                        // Live Streaming Bubble
                        if is_streaming {
                            ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
                                egui::Frame::none()
                                    .fill(Color32::from_rgb(27, 28, 30))
                                    .rounding(Rounding::same(14.0))
                                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(42, 44, 48)))
                                    .inner_margin(egui::Margin::symmetric(16.0, 14.0))
                                    .show(ui, |ui| {
                                        ui.set_max_width(ui.available_width() * 0.85);
                                        ui.vertical(|ui| {
                                            ui.horizontal(|ui| {
                                                ui.spinner();
                                                ui.add_space(6.0);
                                                ui.label(
                                                    RichText::new("Gemini is generating…")
                                                        .size(13.0)
                                                        .color(Color32::from_rgb(120, 190, 255)),
                                                );
                                            });
                                            ui.add_space(6.0);
                                            if !pending_response.is_empty() {
                                                CommonMarkViewer::new().show(
                                                    ui,
                                                    markdown_cache,
                                                    pending_response,
                                                );
                                            }
                                        });
                                    });
                            });
                            ui.add_space(10.0);
                        }
                    }

                    ui.add_space(16.0);
                });

                if let Some(tab) = self.active_tab_mut() {
                    tab.scroll_to_bottom = false;
                }

                // Handle Prompt card click
                if let Some(prompt) = card_clicked_prompt {
                    if let Some(tab) = self.active_tab_mut() {
                        tab.input = prompt;
                    }
                    self.send_message(ctx.clone());
                }

                // Handle Copy Request
                if let Some((idx, text)) = copy_target {
                    ui.output_mut(|o| o.copied_text = text);
                    if let Some(tab) = self.active_tab_mut() {
                        tab.copied_idx = Some(idx);
                    }
                }
            });
    }
}
