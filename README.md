# Rusty Gemini ✦

> A lightning-fast, native Windows desktop client for Google Gemini — built in 100% Rust.

No browser overhead. No Electron. No high RAM usage. Just a standalone, native, GPU-accelerated `.exe`.

[![Website](https://img.shields.io/badge/Website-rusty--gemini.vercel.app-blue?style=flat&logo=vercel)](https://rusty-gemini.vercel.app)
[![Privacy Policy](https://img.shields.io/badge/Legal-Privacy%20Policy-emerald?style=flat)](https://rusty-gemini.vercel.app/privacy)
[![Terms of Service](https://img.shields.io/badge/Legal-Terms%20of%20Service-purple?style=flat)](https://rusty-gemini.vercel.app/terms)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

---

## ⚡ Performance

The official Gemini web UI runs inside heavy browser tabs:
- Consumes 400–800 MB+ of RAM idle
- Stutters during complex rendering or long chats
- High startup latency

`gemini-rust` is a **native Windows application** powered by `egui`/`eframe`:
- 🚀 **< 100 ms startup time**
- 💾 **~30–50 MB RAM usage**
- 🎨 **GPU-accelerated immediate-mode UI** styled with Gemini obsidian dark aesthetic
- ⚡ **True SSE token streaming** with real-time markdown formatting and code highlighting

---

## 🌟 Key Features

- 🔒 **Persistent Session (Zero Re-Login):** 
  - Dual persistent session caching across `%APPDATA%\gemini_client\auth.json` and Windows Credential Manager (`keyring`).
  - You stay logged in automatically across reboots and app restarts without having to re-authenticate each time.
- ✦ **Automatic Account Plan Detection:**
  - Auto-probes your account quota to detect **Free Tier** vs. **✦ Gemini Pro** dynamically. No manual selection required.
  - Highlights your tier with a real-time badge in the navigation bar.
- 🤖 **Verified Live Models (No 404 Deprecated Models):**
  - **✦ Gemini 4 (Hydrogen)**: Frontier frontier reasoning model (verified live & streaming).
  - **✦ Gemini 4 (Argon)**: Unlocked automatically for Pro subscribers.
  - **Gemini 3 Flash**: Ultra-fast default model for rapid responses.
  - **Gemini 3.5 & 3.1 Flash Lite**: Highly efficient preview models.
  - *All deprecated legacy models (1.5 / 2.0 / 2.5) are filtered out automatically.*
- 🎨 **Modern Gemini Dark UI:**
  - Obsidian `#131314` backdrop with pill-shaped model selector and plan badge.
  - Quick-prompt suggestion cards on empty chat to start instantly.
  - Chat bubbles styled for effortless readability with 1-click **Copy Response** and visual feedback.
  - Floating pill input bar supporting Enter to send and Shift+Enter for multiline inputs.
- ⚙️ **Custom System Instructions & Controls:**
  - Slide-out settings drawer to tune system prompt persona and review account status.
  - Instant "+ New chat" reset.

---

## 🛠️ Tech Stack

| Component | Technology |
|---|---|
| **UI Framework** | [`egui`](https://github.com/emilk/egui) / `eframe` (v0.30) |
| **Async Runtime** | `tokio` (multi-threaded) |
| **HTTP & Networking** | `reqwest` (TLS / HTTP/2) |
| **Streaming** | Server-Sent Events (SSE) stream parser |
| **Markdown** | `egui_commonmark` with syntax highlighting |
| **Session & Auth** | Google OAuth2 PKCE + Keyring + APPDATA JSON + Windows Registry |

---

## 🚀 Building & Running

### Prerequisites
- [Rust & Cargo](https://rustup.rs) (1.75+)
- Visual Studio 2022 C++ Build Tools (MSVC)

### Build Commands

```powershell
# Clone the repository
git clone https://github.com/0xKahli/gemini-rust.git
cd gemini-rust\gemini-client

# Build optimized release binary
cargo build --release
```

The optimized, standalone executable will be generated at:
```
target\release\gemini_client.exe
```

---

## 📂 Project Architecture

```
gemini-client/
├── src/
│   ├── main.rs      # App entry point, window configuration & viewport
│   ├── app.rs       # Complete Gemini dark UI, state management, message loop
│   ├── auth.rs      # Session persistence, auto plan detection & model filtering
│   ├── gemini.rs    # SSE streaming engine, candidate parsing & token dispatcher
│   └── types.rs     # Message, Role, and data contracts
├── Cargo.toml       # Dependencies & build configuration
└── README.md
```

---

## 👤 Author & Credits

Developed by [**Kahli (@0xKahli)**](https://github.com/0xKahli/gemini-rust).  
Repository: [https://github.com/0xKahli/gemini-rust](https://github.com/0xKahli/gemini-rust)

---

## 📜 License

MIT License — Feel free to use, modify, and distribute.
