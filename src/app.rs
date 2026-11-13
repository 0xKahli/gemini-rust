use crate::auth::{
    clear_tokens, fetch_available_models, load_tokens, refresh_access_token, start_login_flow,
    ModelInfo, TokenStore,
};
use crate::gemini::{stream_message, AuthMethod};
use crate::types::{Message, Role};
use egui::{Color32, FontId, RichText, Rounding, Stroke, Vec2};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use std::sync::Arc;
use tokio::sync::mpsc;

// ── App state ────────────────────────────────────────────────────────────────

fn default_models() -> Vec<ModelInfo> {
    vec![
        ModelInfo {
            name: "models/gemini-2.5-flash".to_string(),
            display_name: "Gemini 2.5 Flash".to_string(),
            description: "Fast, versatile multimodal intelligence".to_string(),
            supported_generation_methods: vec!["generateContent".to_string()],
        },
        ModelInfo {
            name: "models/gemini-2.5-pro".to_string(),
            display_name: "Gemini 2.5 Pro".to_string(),
            description: "Advanced reasoning and complex coding".to_string(),
            supported_generation_methods: vec!["generateContent".to_string()],
        },
        ModelInfo {
            name: "models/gemini-2.0-flash".to_string(),
            display_name: "Gemini 2.0 Flash".to_string(),
            description: "High speed, multimodal generation".to_string(),
            supported_generation_methods: vec!["generateContent".to_string()],
        },
        ModelInfo {
            name: "models/gemini-1.5-flash".to_string(),
            display_name: "Gemini 1.5 Flash".to_string(),
            description: "Fast and lightweight".to_string(),
            supported_generation_methods: vec!["generateContent".to_string()],
        },
        ModelInfo {
            name: "models/gemini-1.5-pro".to_string(),
            display_name: "Gemini 1.5 Pro".to_string(),
            description: "Large context reasoning".to_string(),
            supported_generation_methods: vec!["generateContent".to_string()],
        },
    ]
}

enum AuthState {
    /// Not logged in
    LoggedOut,
    /// Login in progress (browser open)
    LoggingIn,
    /// Logged in with token + available models
    LoggedIn {
        tokens: TokenStore,
        models: Vec<ModelInfo>,
        loading_models: bool,
    },
    /// Auth error message
    Error(String),
}

pub struct GeminiApp {
    // Auth
    auth_state: AuthState,

    // Chat
    history: Vec<Message>,
    input: String,
    selected_model_idx: usize,
    system_prompt: String,
    show_settings: bool,
    #[allow(dead_code)]
    show_system_prompt: bool,

    // Streaming
    runtime: Arc<tokio::runtime::Runtime>,
    // Channels for auth tasks
    #[allow(dead_code)]
    auth_tx: Option<mpsc::Sender<AuthTaskResult>>,
    auth_rx: Option<mpsc::Receiver<AuthTaskResult>>,
    // Channels for chat streaming
    chat_rx: Option<mpsc::Receiver<String>>,
    is_streaming: bool,
    pending_response: String,

    // UI
    markdown_cache: CommonMarkCache,
    scroll_to_bottom: bool,
}

enum AuthTaskResult {
    LoginSuccess(TokenStore),
    LoginError(String),
    ModelsLoaded(Vec<ModelInfo>),
    ModelsError(String),
    TokenRefreshed(TokenStore),
}

impl GeminiApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("Failed to create Tokio runtime"),
        );

        // Try loading saved tokens from keychain
        let saved = load_tokens();
        let auth_state = match saved {
            Some(tokens) => AuthState::LoggedIn {
                tokens,
                models: default_models(),
                loading_models: true, // will trigger model fetch on first frame
            },
            None => AuthState::LoggedOut,
        };

        Self {
            auth_state,
            history: Vec::new(),
            input: String::new(),
            selected_model_idx: 0,
            system_prompt: String::new(),
            show_settings: false,
            show_system_prompt: false,
            runtime,
            auth_tx: None,
            auth_rx: None,
            chat_rx: None,
            is_streaming: false,
            pending_response: String::new(),
            markdown_cache: CommonMarkCache::default(),
            scroll_to_bottom: false,
        }
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
        let (tx, rx) = mpsc::channel::<AuthTaskResult>(4);
        self.auth_rx = Some(rx);
        let access_token = tokens.access_token.clone();
        self.runtime.spawn(async move {
            match fetch_available_models(&access_token).await {
                Ok(models) => {
                    let _ = tx.send(AuthTaskResult::ModelsLoaded(models)).await;
                }
                Err(e) => {
                    let _ = tx.send(AuthTaskResult::ModelsError(e)).await;
                }
            }
            ctx.request_repaint();
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
                        Err(_) => {} // Will just fail on next API call
                    }
                    ctx.request_repaint();
                });
            }
        }
    }

    fn poll_auth(&mut self, ctx: &egui::Context) {
        // Check for pending model fetch trigger
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
                    models: Vec::new(),
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
                    *m = if models.is_empty() { default_models() } else { models };
                    *loading_models = false;
                    self.selected_model_idx = 0;
                }
            }
            Some(AuthTaskResult::ModelsError(e)) => {
                self.auth_rx = None;
                eprintln!("Models fetch error: {}", e);
                if let AuthState::LoggedIn { models: m, loading_models, .. } = &mut self.auth_state {
                    if m.is_empty() {
                        *m = default_models();
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
            None => {}
        }
    }

    // ── Chat helpers ──────────────────────────────────────────────────────────

    fn send_message(&mut self, ctx: egui::Context) {
        let input = self.input.trim().to_string();
        if input.is_empty() || self.is_streaming {
            return;
        }

        let auth = match &self.auth_state {
            AuthState::LoggedIn { tokens, .. } => {
                AuthMethod::Bearer(tokens.access_token.clone())
            }
            _ => return,
        };

        let model = match &self.auth_state {
            AuthState::LoggedIn { models, .. } if !models.is_empty() => {
                models
                    .get(self.selected_model_idx)
                    .map(|m| m.api_name().to_string())
                    .unwrap_or_else(|| "gemini-2.5-flash".to_string())
            }
            _ => "gemini-2.5-flash".to_string(),
        };

        self.history.push(Message::user(&input));
        self.input.clear();
        self.is_streaming = true;
        self.pending_response.clear();
        self.scroll_to_bottom = true;

        let (tx, rx) = mpsc::channel::<String>(256);
        self.chat_rx = Some(rx);

        let mut history_to_send = self.history.clone();
        if !self.system_prompt.is_empty() && history_to_send.len() == 1 {
            history_to_send.insert(0, Message { role: Role::User, content: format!("System instructions: {}\n\nAcknowledge briefly.", self.system_prompt) });
            history_to_send.insert(1, Message { role: Role::Model, content: "Understood.".to_string() });
        }

        self.runtime.spawn(async move {
            stream_message(auth, model, history_to_send, tx).await;
            ctx.request_repaint();
        });
    }

    fn poll_chat(&mut self) {
        if !self.is_streaming { return; }

        let mut done = false;
        if let Some(rx) = &mut self.chat_rx {
            loop {
                match rx.try_recv() {
                    Ok(token) => {
                        self.pending_response.push_str(&token);
                        self.scroll_to_bottom = true;
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
            if !self.pending_response.is_empty() {
                self.history.push(Message::model(self.pending_response.clone()));
            }
            self.pending_response.clear();
            self.is_streaming = false;
            self.chat_rx = None;
        }
    }
}

// ── egui UI ───────────────────────────────────────────────────────────────────

impl eframe::App for GeminiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_auth(ctx);
        self.poll_chat();
        self.refresh_token_if_needed(ctx.clone());

        if self.is_streaming || matches!(self.auth_state, AuthState::LoggingIn) {
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
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() / 4.0);

                ui.label(RichText::new("✦").size(48.0).color(Color32::from_rgb(100, 180, 255)));
                ui.add_space(8.0);
                ui.label(RichText::new("Gemini Client").size(28.0).strong());
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Native. Fast. No browser required.")
                        .size(14.0)
                        .color(Color32::from_gray(160)),
                );
                ui.add_space(32.0);

                match &self.auth_state {
                    AuthState::LoggedOut => {
                        let btn = ui.add_sized(
                            Vec2::new(220.0, 44.0),
                            egui::Button::new(
                                RichText::new("🔑  Sign in with Google").size(15.0),
                            ),
                        );
                        if btn.clicked() {
                            self.start_login(ctx.clone());
                        }
                    }
                    AuthState::LoggingIn => {
                        ui.spinner();
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new("Opening browser… complete sign-in there.")
                                .color(Color32::from_gray(180)),
                        );
                    }
                    AuthState::Error(e) => {
                        ui.colored_label(Color32::RED, format!("⚠ {}", e));
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
        // ── Top bar ───────────────────────────────────────────────────────────
        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("✦ Gemini").size(17.0).color(Color32::from_rgb(100, 180, 255)));
                ui.separator();

                // Model selector from live API list
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
                            .selected_text(selected_name)
                            .width(300.0)
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

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // User avatar / sign out
                    if let AuthState::LoggedIn { tokens, .. } = &self.auth_state {
                        let name = tokens
                            .display_name
                            .as_deref()
                            .or(tokens.email.as_deref())
                            .unwrap_or("Account")
                            .to_string();
                        let email = tokens.email.clone();

                        ui.menu_button(format!("👤 {}", name), |ui| {
                            ui.set_min_width(160.0);
                            if let Some(ref e) = email {
                                ui.label(RichText::new(e).color(Color32::from_gray(160)).size(11.0));
                                ui.separator();
                            }
                            if ui.button("⚙ Settings").clicked() {
                                self.show_settings = !self.show_settings;
                                ui.close_menu();
                            }
                            ui.separator();
                            if ui.button("🚪 Sign out").clicked() {
                                clear_tokens();
                                self.auth_state = AuthState::LoggedOut;
                                self.history.clear();
                                ui.close_menu();
                            }
                        });
                    }
                });
            });
        });

        // ── Settings side panel ───────────────────────────────────────────────
        if self.show_settings {
            egui::SidePanel::right("settings_panel")
                .resizable(true)
                .default_width(280.0)
                .show(ctx, |ui| {
                    ui.heading("Settings");
                    ui.separator();

                    ui.label("System Prompt:");
                    ui.add(
                        egui::TextEdit::multiline(&mut self.system_prompt)
                            .hint_text("Optional instructions for Gemini…")
                            .desired_rows(5)
                            .desired_width(f32::INFINITY),
                    );

                    ui.add_space(12.0);

                    if ui.button("🗑 Clear chat history").clicked() {
                        self.history.clear();
                        self.pending_response.clear();
                        self.is_streaming = false;
                    }

                    ui.add_space(8.0);
                    ui.separator();

                    if let AuthState::LoggedIn { models, .. } = &self.auth_state {
                        ui.label(RichText::new(format!("{} models available", models.len())).color(Color32::from_gray(150)).size(11.0));
                    }
                });
        }

        // ── Bottom input bar ──────────────────────────────────────────────────
        egui::TopBottomPanel::bottom("input_bar").show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let input_edit = egui::TextEdit::multiline(&mut self.input)
                    .hint_text("Message Gemini… (Enter to send, Shift+Enter for newline)")
                    .desired_rows(2)
                    .desired_width(ui.available_width() - 84.0);

                let response = ui.add(input_edit);

                if response.has_focus()
                    && ctx.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift)
                {
                    self.send_message(ctx.clone());
                }

                ui.vertical(|ui| {
                    let send_label = if self.is_streaming { "⏳" } else { "➤ Send" };
                    if ui
                        .add_sized(Vec2::new(74.0, 44.0), egui::Button::new(send_label))
                        .clicked()
                    {
                        self.send_message(ctx.clone());
                    }
                });
            });
            ui.add_space(4.0);
        });

        // ── Chat scroll area ──────────────────────────────────────────────────
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .stick_to_bottom(self.scroll_to_bottom)
                .show(ui, |ui| {
                    ui.add_space(8.0);

                    for msg in &self.history {
                        let is_user = msg.role == Role::User;
                        draw_bubble(ui, &msg.content, is_user, &mut self.markdown_cache);
                        ui.add_space(6.0);
                    }

                    // Live streaming bubble
                    if self.is_streaming {
                        if self.pending_response.is_empty() {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(RichText::new("Thinking…").color(Color32::from_gray(150)));
                            });
                        } else {
                            draw_bubble(ui, &self.pending_response.clone(), false, &mut self.markdown_cache);
                        }
                    }

                    ui.add_space(8.0);
                });

            self.scroll_to_bottom = false;
        });
    }
}

fn draw_bubble(ui: &mut egui::Ui, content: &str, is_user: bool, cache: &mut CommonMarkCache) {
    ui.with_layout(
        if is_user {
            egui::Layout::right_to_left(egui::Align::TOP)
        } else {
            egui::Layout::left_to_right(egui::Align::TOP)
        },
        |ui| {
            let bubble_color = if is_user {
                Color32::from_rgb(45, 85, 160)
            } else {
                Color32::from_gray(40)
            };

            egui::Frame::none()
                .fill(bubble_color)
                .rounding(Rounding::same(10.0))
                .inner_margin(egui::Margin::symmetric(12.0, 8.0))
                .stroke(Stroke::new(1.0_f32, Color32::from_gray(60)))
                .show(ui, |ui| {
                    ui.set_max_width(ui.available_width() * 0.78);
                    let label = if is_user { "You" } else { "Gemini" };
                    ui.label(
                        RichText::new(label)
                            .font(FontId::proportional(11.0))
                            .color(Color32::from_gray(170)),
                    );
                    if is_user {
                        ui.label(RichText::new(content).size(14.0));
                    } else {
                        CommonMarkViewer::new().show(ui, cache, content);
                    }
                });
        },
    );
}
