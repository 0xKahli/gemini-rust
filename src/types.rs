use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Role {
    User,
    Model,
}

impl Role {
    pub fn label(&self) -> &str {
        match self {
            Role::User => "You",
            Role::Model => "Gemini",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

impl Message {
    pub fn user(content: impl Into<String>) -> Self {
        Self { role: Role::User, content: content.into() }
    }

    pub fn model(content: impl Into<String>) -> Self {
        Self { role: Role::Model, content: content.into() }
    }
}
