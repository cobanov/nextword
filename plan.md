# NextWord — System-Wide AI Word Suggestions

A floating, system-wide next-word prediction tool for macOS (Windows later). When the user presses space, three local-LLM-generated word suggestions appear near the caret. Tab/⌘1-3 to accept, Esc to dismiss. Everything runs locally. No telemetry.

This is a rewrite of an existing FastAPI/RoBERTa prototype (https://github.com/cobanov/next-word-predict). The web demo is being thrown away. The new project is a Rust + Tauri desktop app with `llama-server` as a sidecar.

## Goals & Non-Goals

**Goals**
- macOS-first, Apple Silicon (M1+). Windows is a later milestone, do not branch the codebase for it yet.
- Suggestions appear within ~80ms of pressing space, in the median case.
- Works system-wide: Notes, Mail, Notion, VSCode, Slack, Safari, Chrome, etc.
- One binary the user can download and run. No `pip install`, no Docker, no terminal setup.
- Model is downloaded on first launch with a visible progress bar. User does not have to find or place files manually.
- Privacy: nothing leaves the machine. No analytics, no model API calls, no error reporting service.

**Non-goals (for v1)**
- Not a full sentence/paragraph completer. Single-word suggestions only.
- Not a grammar checker, not a rewriter.
- No cloud fallback.
- No custom user dictionaries, no learning from user behavior (v2 territory).
- No iOS, no Linux.

## Architecture Overview

```
┌────────────────────────────────────────────────────┐
│  Tauri App (single process)                        │
│                                                    │
│  ┌────────────────┐   ┌──────────────────────┐    │
│  │ Input Listener │   │  Floating Suggestion │    │
│  │ (macOS AX +    │   │  Window (WebView)    │    │
│  │  CGEventTap)   │   │  3 suggestion chips  │    │
│  └────────┬───────┘   └──────────▲───────────┘    │
│           │                      │                 │
│           ▼                      │                 │
│  ┌───────────────────────────────┴────────────┐   │
│  │  Predictor Service (Rust)                  │   │
│  │  - context buffer (last 256 chars)         │   │
│  │  - 50ms debounce                           │   │
│  │  - cancellation tokens                     │   │
│  │  - response parser (dedupe, trim)          │   │
│  └───────────────────┬────────────────────────┘   │
└──────────────────────┼─────────────────────────────┘
                       │ HTTP localhost
                       ▼
            ┌──────────────────────────┐
            │  llama-server (sidecar)  │
            │  Llama-3.2-1B-Instruct   │
            │  Q4_K_M, Metal backend   │
            └──────────────────────────┘
```

Two logical components, one OS process: the Tauri Rust app (UI + input + predictor client) and `llama-server` started as a child process (Tauri sidecar).

## Tech Stack — Locked Decisions

| Concern | Choice | Why |
|---|---|---|
| Language | Rust | Single binary, native perf, FFI to macOS APIs, owner is learning Rust |
| Desktop framework | Tauri 2 | Small bundle, native webview, mature sidecar support |
| UI | HTML + TypeScript + Vite (Tauri default) | Floating window is small, doesn't need a heavy framework |
| Inference | `llama.cpp`'s `llama-server` binary as sidecar | Mature Metal backend, KV cache reuse, OpenAI-compatible API, hot-swappable models |
| Default model | `Llama-3.2-1B-Instruct-Q4_K_M.gguf` | Most mature llama.cpp support, multilingual (incl. Turkish), no thinking mode, ~80 t/s on M-series |
| Input capture (macOS) | Accessibility API first, `CGEventTap` keylog fallback | Best of both — clean text from supported apps, fallback for Electron/web |
| HTTP client | `reqwest` (async, tokio) | Standard |
| IPC core ↔ UI | Tauri events | Native, no extra deps |

**Models to consider later** (must be config-swappable): Qwen2.5-0.5B, SmolLM2-360M, Phi-3.5-mini. The predictor must not hardcode anything Llama-specific beyond the prompt template.

## Repo Structure

```
nextword/
├── Cargo.toml                       # workspace root
├── PLAN.md                          # this file
├── README.md
├── crates/
│   ├── nextword-core/               # platform-agnostic logic, no Tauri deps
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── context.rs           # context buffer, char/word counting
│   │   │   ├── trigger.rs           # space-detection rules
│   │   │   ├── predictor.rs         # llama-server HTTP client
│   │   │   ├── parser.rs            # raw completion → 3 deduped suggestions
│   │   │   └── debounce.rs          # 50ms debouncer with cancellation
│   │   └── Cargo.toml
│   ├── nextword-macos/              # macOS-specific, gated by cfg(target_os)
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── ax.rs                # Accessibility API (text field + caret)
│   │   │   ├── keytap.rs            # CGEventTap fallback
│   │   │   ├── caret.rs             # caret screen position resolver
│   │   │   └── permissions.rs       # AX permission check + prompt
│   │   └── Cargo.toml
│   └── nextword-app/                # the Tauri app — binary entry point
│       ├── src-tauri/
│       │   ├── src/
│       │   │   ├── main.rs
│       │   │   ├── window.rs        # floating panel positioning
│       │   │   ├── sidecar.rs       # llama-server lifecycle
│       │   │   ├── model_download.rs # first-launch downloader
│       │   │   └── ipc.rs           # core ↔ webview event bridge
│       │   ├── Cargo.toml
│       │   ├── tauri.conf.json
│       │   └── build.rs             # builds llama.cpp submodule
│       ├── src/                     # webview UI (TS + HTML)
│       │   ├── main.ts
│       │   ├── suggestions.ts
│       │   ├── styles.css
│       │   └── index.html
│       ├── package.json
│       └── vite.config.ts
├── vendor/
│   └── llama.cpp/                   # git submodule
├── models/                          # gitignored; populated by app at runtime
│   └── .gitkeep
└── .github/
    └── workflows/
        └── ci.yml                   # build + clippy + test on macos-latest
```

## Implementation Plan — Milestones

Each milestone has acceptance criteria. Do not move on until they pass. Commit at the end of each milestone.

---

### Milestone 0 — Scaffold (half day)

Set up the workspace, get an empty Tauri app running.

**Tasks**
1. `cargo new --lib` the three crates, set up workspace `Cargo.toml`.
2. Initialize Tauri 2 in `crates/nextword-app` via `cargo tauri init`.
3. Add `vendor/llama.cpp` as a git submodule pinned to the latest tagged release.
4. Write a `build.rs` in `nextword-app` that runs `cmake` + `make` on `vendor/llama.cpp` if `binaries/llama-server` is missing, building only the `llama-server` target with `LLAMA_METAL=ON`. Cache the built binary in `binaries/`.
5. Configure Tauri's `externalBin` / sidecar to ship `llama-server`.
6. App should launch, show a blank window saying "NextWord — initializing", and exit cleanly.

**Acceptance**
- `cargo tauri dev` opens an empty window.
- `cargo tauri build` produces a `.app` bundle.
- `binaries/llama-server` is built from the submodule and runnable directly (`./binaries/llama-server --help` prints help).

---

### Milestone 1 — Sidecar lifecycle & health check (half day)

Get llama-server starting and stopping cleanly. No model yet — use a tiny dummy GGUF for testing.

**Tasks**
1. Implement `sidecar.rs`: spawn `llama-server` on `127.0.0.1:0` (let the OS pick a free port), capture stdout/stderr, read the chosen port back.
2. On app start: launch sidecar. On app quit (including crash via panic hook): kill sidecar. On macOS, register a signal handler so `Cmd+Q` cleans up.
3. Poll `GET /health` until it returns 200 or times out at 30s.
4. Show "Loading model…" in the main window until health passes, then "Ready".
5. For testing this milestone, download `Qwen2.5-0.5B-Instruct-Q4_K_M.gguf` manually into `models/` (script in `scripts/dev-download-test-model.sh`). The auto-downloader comes in M5.

**Acceptance**
- App starts, sidecar boots, health check passes, "Ready" appears.
- Quitting the app via Cmd+Q leaves no orphaned `llama-server` process. Verify with `ps aux | grep llama-server`.
- Killing the sidecar manually causes the app to display "Inference service crashed" and offer a Retry button.

---

### Milestone 2 — Core prediction pipeline (1 day)

Build the predictor with a fake input source (keyboard event in the main window), no system-wide capture yet.

**Tasks**
1. In `nextword-core`, implement `Predictor` struct:
   - `async fn predict(&self, context: &str) -> Result<Vec<String>>` returns 3 deduped, trimmed suggestions.
   - Uses `llama-server`'s `/completion` endpoint (NOT `/v1/chat/completions` — we want raw completion).
   - Request body: `{"prompt": context, "n_predict": 4, "temperature": 0.7, "top_k": 20, "top_p": 0.9, "n_probs": 0, "stop": [" ", "\n", ".", ",", "!", "?", ";", ":"], "cache_prompt": true, "n_keep": -1}`.
   - `cache_prompt: true` is what gives us KV cache reuse across calls.
2. Generate 3 distinct suggestions: call `/completion` once with `"n": 3` (llama-server supports parallel sampling) OR three times with different seeds. Try parallel first.
3. Parser (`parser.rs`):
   - Strip leading whitespace.
   - Cut at first non-letter/non-apostrophe character.
   - Lowercase unless preceding char in context is `.`, `!`, `?` followed by space → then capitalize.
   - Drop empty results, drop duplicates (case-insensitive), drop suggestions identical to the last word in context.
   - If <3 valid suggestions remain, that's fine — UI shows what we have.
4. Context buffer (`context.rs`):
   - Keep last 256 chars.
   - Trim to nearest sentence boundary at the start if possible.
5. Debouncer (`debounce.rs`):
   - 50ms window after last space.
   - In-flight request gets cancelled via `tokio_util::sync::CancellationToken` if a new space arrives.

**Acceptance**
- A standalone test binary (`cargo run --bin predict_test -- "I went to the"`) prints 3 suggestions in <100ms after warmup.
- Repeated calls with the same prefix are noticeably faster than the first (KV cache working). Log timing.
- Sending two requests back-to-back: the first one cancels cleanly, no zombie HTTP request.

---

### Milestone 3 — macOS input capture (1.5 days)

Listen to keystrokes system-wide, maintain a context buffer, fire predictor on space.

**Tasks**
1. **AX permission flow** (`permissions.rs`):
   - Check `AXIsProcessTrustedWithOptions` on startup.
   - If not trusted, show a modal: "NextWord needs Accessibility access to read text near your cursor. Click 'Open Settings' to enable it."
   - Open `x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility`.
   - Poll permission status every 2s and proceed when granted.
2. **Keystroke listener** (`keytap.rs`):
   - Create a `CGEventTap` on `kCGSessionEventTap`, listening to `kCGEventKeyDown`.
   - Run the tap on a dedicated thread with a `CFRunLoop`.
   - Forward events to a `tokio::sync::mpsc::UnboundedSender<KeyEvent>` for the core to consume.
3. **Context strategy** (hybrid, AX-first):
   - On every space press: try `ax.rs::get_focused_text_context()` first.
     - Get focused element via `AXUIElementCreateSystemWide` → `kAXFocusedUIElementAttribute`.
     - Read `kAXValueAttribute` (full text) and `kAXSelectedTextRangeAttribute` (caret position).
     - Slice 256 chars before caret.
   - If AX call fails OR returns empty: fall back to a keylog buffer (last 256 chars typed into this app, reset on focus change or Cmd+Tab).
4. **Trigger rules** (`trigger.rs`):
   - Fire only on space.
   - Require context length ≥ 10 characters AND ≥ 2 words.
   - Skip if previous keystroke was Cmd/Ctrl/Option (likely a shortcut, not typing).
   - Skip if focused app is a password field (`kAXSubrole` == `AXSecureTextField`).
5. Wire it up: keystroke → trigger check → debounce → predictor → log result to console.

**Acceptance**
- Open TextEdit, type "I went to the ". Console logs 3 suggestions within ~100ms.
- Open a Chrome address bar, type "best coffee in ". Console logs suggestions (AX may not work in Chrome — verify fallback engages).
- Type into a password field — nothing happens, nothing logged.
- Hold Cmd and press space — no trigger.
- Switch apps with Cmd+Tab — fallback buffer resets.

---

### Milestone 4 — Floating suggestion window (1.5 days)

The UI. This is the user-facing payoff.

**Tasks**
1. **Window config** (`window.rs`):
   - Create a Tauri window: `decorations: false, transparent: true, always_on_top: true, skip_taskbar: true, resizable: false, focus: false`.
   - Size: 320×56px.
   - Show/hide via Tauri commands from Rust.
   - **Critical**: the window must NOT steal focus when shown. On macOS, set `NSWindowCollectionBehaviorTransient` + `NSWindowStyleMaskNonactivatingPanel`. This requires dropping to AppKit FFI — use the `objc2` crate. There's no clean Tauri API for non-activating panels yet; we'll set the style mask on the underlying `NSWindow` after Tauri creates it.
2. **Caret position resolver** (`caret.rs`):
   - From AX: `AXUIElementCopyParameterizedAttributeValue` with `kAXBoundsForRangeParameterizedAttribute` on the caret's range → screen coords.
   - Fallback: position window in the bottom-right of the active screen, 40px from edges.
3. **UI** (`src/suggestions.ts` + `index.html`):
   - Three chips side by side, monospace font, ~13px.
   - Each chip shows: keyboard hint (`⇥`, `⌘1`, `⌘2`, `⌘3`) and the word.
   - First chip is highlighted (default accept target).
   - Subtle drop shadow, rounded corners, dark mode following system appearance.
   - Listens for Tauri events: `suggestions:show` (payload: `{words: string[], x: number, y: number}`), `suggestions:hide`.
4. **Acceptance keystrokes** — intercepted by the same `CGEventTap`:
   - `Tab` → insert the first suggestion + a trailing space, suppress the actual Tab keystroke.
   - `Cmd+1/2/3` → insert the corresponding suggestion + space, suppress the shortcut.
   - `Esc` → hide window, suppress Esc.
   - Insertion: synthesize keystrokes via `CGEventCreateKeyboardEvent` for each character. Alternative: paste via pasteboard + Cmd+V — faster but pollutes clipboard, do NOT use.
   - **Important**: when synthesizing the insertion, set a flag in the event listener to ignore our own synthesized events (use `CGEventSetIntegerValueField` with a custom field).
5. Any key other than Tab/Cmd+1-3/Esc → hide window. The user moved on.

**Acceptance**
- Type "I went to the " in TextEdit. Floating window appears next to caret with 3 suggestions.
- Press Tab — first suggestion is inserted with a trailing space. Window disappears.
- Type more, get new suggestions, press Cmd+2 — second suggestion inserts.
- Press Esc — window hides, Esc does NOT propagate to TextEdit (otherwise dialogs would close).
- The floating window never steals focus from TextEdit. The text caret keeps blinking in TextEdit while the window is up.
- Window appears in dark mode when system is dark.

---

### Milestone 5 — First-launch model download (1 day)

Replace the manual model placement with an in-app downloader.

**Tasks**
1. `model_download.rs`:
   - On startup, check if `~/Library/Application Support/NextWord/models/Llama-3.2-1B-Instruct-Q4_K_M.gguf` exists and matches expected SHA-256.
   - If missing: show the main window with a download UI before starting the sidecar.
   - Download from Hugging Face direct URL: `https://huggingface.co/bartowski/Llama-3.2-1B-Instruct-GGUF/resolve/main/Llama-3.2-1B-Instruct-Q4_K_M.gguf` (verify URL + filename at build time and pin them in a constant).
   - Stream download with `reqwest` + chunked write to a `.tmp` file. Rename on success.
   - Emit `model_download:progress` events (payload: `{bytes_done, bytes_total, speed_bps}`) every 100ms.
2. UI for download:
   - Progress bar, percentage, MB downloaded / MB total, speed (MB/s), ETA.
   - Cancel button → delete `.tmp`, quit app.
   - On error (network, disk full): show error + Retry button.
3. After successful download: hash-verify, then transition to sidecar startup (M1 flow).

**Acceptance**
- Fresh install (no model on disk): main window shows downloader, progress moves, completes, app proceeds to "Ready".
- Cancel mid-download: app quits, partial file is cleaned up.
- Subsequent launches skip the download.
- Tamper with the file (truncate it): app detects bad hash, redownloads.

---

### Milestone 6 — Polish & resilience (1 day)

The boring but critical stuff.

**Tasks**
1. **Settings window**:
   - Toggle: enabled / disabled (global pause).
   - Model selector dropdown (initially just Llama-3.2-1B; design for adding more).
   - "Suggestion count" slider (1-5, default 3).
   - "Trigger after N characters" slider (default 10).
   - All settings persisted to `~/Library/Application Support/NextWord/settings.json`.
2. **Menu bar icon**:
   - Tauri `tauri::tray::TrayIconBuilder`.
   - Click: open Settings.
   - Right-click menu: Enable/Disable, Settings, Quit.
3. **Logs**:
   - All logs to `~/Library/Logs/NextWord/nextword.log`, rotated daily, max 5 files.
   - Use `tracing` + `tracing-appender`.
4. **Crash recovery**:
   - If sidecar crashes 3x in 60s, stop auto-restarting and show an alert with a "Show logs" button.
5. **Performance verification**:
   - Add timing logs around: AX read, debounce, predictor call, UI render.
   - Target median end-to-end (space press → window visible) < 100ms on M1+.
   - If above target, profile and optimize before declaring v1 done.

**Acceptance**
- Settings persist across restarts.
- Menu bar icon works.
- Quit from menu bar leaves no orphaned processes.
- Sidecar killed 3 times → app stops trying, user is notified.
- Timing log shows median < 100ms on dev machine.

---

### Milestone 7 — Distribution (half day)

**Tasks**
1. Code-signing: developer ID Apple cert. Sign the app bundle and the sidecar binary.
2. Notarization: submit to Apple via `notarytool`, staple the ticket.
3. DMG packaging via Tauri's built-in DMG bundler.
4. Auto-update: skip for v1. Document manual update process in README.

**Acceptance**
- Downloaded DMG opens without Gatekeeper warnings.
- App runs on a clean Mac that has never seen the dev environment.

---

## Out-of-Scope but Worth Noting

- **Windows port**: after v1 macOS ships. Will use UI Automation + low-level keyboard hook. Plan the `nextword-windows` crate to mirror `nextword-macos`'s interface so `nextword-core` stays unchanged.
- **Multilingual quality**: Llama-3.2-1B is OK in Turkish but not great. Document that users wanting better Turkish can swap to a finetuned model via Settings.
- **Speculative decoding**: llama.cpp supports it. Consider for v1.1 if latency target isn't hit.
- **Onboarding tutorial**: a 30-second interactive walkthrough on first launch. Nice-to-have.

## What "v1 Done" Looks Like

- Signed, notarized DMG on the repo's Releases page.
- README with a 20-second demo GIF, install instructions, and known limitations.
- Works on M1/M2/M3/M4 Macs running macOS 13+.
- Median latency under 100ms on a stock M1 with the default model.
- Zero crashes in a 30-minute typing session in TextEdit, Notes, Mail, Slack, Chrome, and VSCode.

## Open Questions to Confirm Before Starting

1. App name: "NextWord"? Bikeshed-friendly, change anytime.
2. Bundle ID: `com.cobanov.nextword`?
3. License: project is currently unlicensed. MIT or Apache-2.0? (llama.cpp is MIT — pick something compatible.)
4. Minimum macOS version: 13 (Ventura) is reasonable for Tauri 2 + Metal.

## Useful References

- Tauri 2 sidecar: https://tauri.app/develop/sidecar/
- llama.cpp server README: https://github.com/ggerganov/llama.cpp/tree/master/tools/server
- CGEventTap: https://developer.apple.com/documentation/coregraphics/quartz_event_services
- macOS Accessibility API: https://developer.apple.com/documentation/applicationservices/axuielement_h
- `objc2` crate (NSWindow style mask manipulation): https://github.com/madsmtm/objc2
- Llama-3.2-1B-Instruct GGUF: https://huggingface.co/bartowski/Llama-3.2-1B-Instruct-GGUF