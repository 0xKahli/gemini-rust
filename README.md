# gemini-rust

> A fast, native Windows desktop client for Google Gemini — built in Rust.

No browser. No Electron. No RAM hog. Just a single `.exe`.

---

## Why?

The official Gemini web UI runs in a browser tab:
- Stutters under load
- Uses 300–600 MB of RAM just sitting idle
- Slow to start

This app is a **native Windows binary** built with Rust:
- Starts in under 100 ms
- Uses ~30 MB RAM
- GPU-accelerated immediate-mode UI via `egui`
- Real-time token streaming over SSE

---

## Features

- 🔑 **Sign in with Google** — full OAuth2 PKCE flow, tokens stored in Windows Credential Manager
- 🤖 **Live model selector** — fetches all models your Google account has access to (based on your plan)
- ⚡ **Streaming responses** — tokens appear as they're generated, with markdown rendered live
- 💬 **Multi-turn memory** — full conversation history sent with each request
- ⚙️ **System prompt** — set custom instructions for Gemini
- 🗑️ **Clear chat** — start fresh anytime

---

## Tech Stack

| Layer | Library |
|---|---|
| UI framework | [`egui`](https://github.com/emilk/egui) / `eframe` |
| Async HTTP | `reqwest` + `tokio` |
| Auth | `oauth2` (PKCE) + `keyring` |
| Streaming | Server-Sent Events (SSE) |
| Markdown | `egui_commonmark` |
| Secure storage | Windows Credential Manager |

---

## Build

**Requirements:**
- Rust 1.70+ — [rustup.rs](https://rustup.rs)
- Visual Studio 2022 Build Tools (C++ workload)

```powershell
git clone https://github.com/0xKahli/gemini-rust.git
cd gemini-rust
cargo build --release
```

Binary output: `target\release\gemini_client.exe`

---

## First Launch

No API key needed. On first launch:

1. Click **Sign in with Google**
2. Complete OAuth consent in your browser
3. App fetches your available Gemini models automatically
4. Start chatting

Tokens are stored securely in **Windows Credential Manager** — you stay logged in across restarts.

---

## Project Structure

```
src/
├── main.rs      # Entry point, window setup
├── app.rs       # egui UI — chat, login screen, model selector
├── auth.rs      # Google OAuth2 PKCE, token refresh, model fetching
├── gemini.rs    # Gemini API streaming client (SSE)
└── types.rs     # Message / Role structs
```

---

## License

MIT
