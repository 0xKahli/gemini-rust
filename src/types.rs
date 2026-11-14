use egui::Color32;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccountPlanTier {
    Free,
    Pro,
    Ultra,
}

impl AccountPlanTier {
    pub fn badge_text(&self) -> &'static str {
        match self {
            AccountPlanTier::Free => "FREE",
            AccountPlanTier::Pro => "✦ GOOGLE AI PRO",
            AccountPlanTier::Ultra => "✦ GOOGLE AI ULTRA",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            AccountPlanTier::Free => "Free Plan",
            AccountPlanTier::Pro => "✦ Google AI Pro",
            AccountPlanTier::Ultra => "✦ Google AI Ultra",
        }
    }

    pub fn is_paid(&self) -> bool {
        matches!(self, AccountPlanTier::Pro | AccountPlanTier::Ultra)
    }

    pub fn from_str_name(s: &str) -> Self {
        let lower = s.to_lowercase();
        if lower.contains("ultra") {
            AccountPlanTier::Ultra
        } else if lower.contains("pro") || lower.contains("advanced") {
            AccountPlanTier::Pro
        } else {
            AccountPlanTier::Free
        }
    }

    pub fn to_storage_str(&self) -> &'static str {
        match self {
            AccountPlanTier::Free => "free",
            AccountPlanTier::Pro => "pro",
            AccountPlanTier::Ultra => "ultra",
        }
    }

    pub fn colors(&self) -> (Color32, Color32) {
        match self {
            AccountPlanTier::Free => (
                Color32::from_rgb(130, 190, 255),
                Color32::from_rgb(25, 38, 55),
            ),
            AccountPlanTier::Pro => (
                Color32::from_rgb(255, 215, 80),
                Color32::from_rgb(45, 36, 16),
            ),
            AccountPlanTier::Ultra => (
                Color32::from_rgb(215, 140, 255),
                Color32::from_rgb(45, 20, 60),
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Role {
    User,
    Model,
}

impl Role {
    #[allow(dead_code)]
    pub fn label(&self) -> &str {
        match self {
            Role::User => "You",
            Role::Model => "Gemini",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachedFile {
    pub file_name: String,
    pub mime_type: String,
    pub size_bytes: usize,
    pub data_base64: String,
}

impl AttachedFile {
    pub fn formatted_size(&self) -> String {
        if self.size_bytes < 1024 {
            format!("{} B", self.size_bytes)
        } else if self.size_bytes < 1024 * 1024 {
            format!("{:.1} KB", self.size_bytes as f32 / 1024.0)
        } else {
            format!("{:.1} MB", self.size_bytes as f32 / (1024.0 * 1024.0))
        }
    }

    pub fn icon(&self) -> &'static str {
        if self.mime_type.starts_with("image/") {
            "🖼"
        } else if self.mime_type == "application/pdf" {
            "📄"
        } else if self.mime_type.starts_with("audio/") {
            "🎵"
        } else if self.mime_type.starts_with("video/") {
            "🎬"
        } else if self.mime_type.starts_with("text/") || self.mime_type.contains("json") {
            "📝"
        } else {
            "📎"
        }
    }

    #[allow(dead_code)]
    pub fn is_image(&self) -> bool {
        self.mime_type.starts_with("image/")
    }
}

pub fn guess_mime_type(file_name: &str) -> &'static str {
    let lower = file_name.to_lowercase();
    let ext = lower.rsplit('.').next().unwrap_or("");
    match ext {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        "mp3" => "audio/mp3",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "m4a" => "audio/m4a",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "txt" => "text/plain",
        "md" => "text/markdown",
        "rs" => "text/x-rust",
        "py" => "text/x-python",
        "js" => "text/javascript",
        "ts" => "text/typescript",
        "json" => "application/json",
        "csv" => "text/csv",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "xml" => "text/xml",
        "c" | "cpp" | "h" | "hpp" => "text/x-c",
        "java" => "text/x-java",
        "go" => "text/x-go",
        "sh" | "bash" | "bat" | "ps1" => "text/plain",
        _ => "application/octet-stream",
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<AttachedFile>,
}

impl Message {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
            files: Vec::new(),
        }
    }

    pub fn user_with_files(content: impl Into<String>, files: Vec<AttachedFile>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
            files,
        }
    }

    pub fn model(content: impl Into<String>) -> Self {
        Self {
            role: Role::Model,
            content: content.into(),
            files: Vec::new(),
        }
    }
}
