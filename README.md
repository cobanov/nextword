# NextWord

System-wide local-AI next-word suggestions for macOS. Press space, get three
suggestions near your caret, accept with Tab or Cmd+1/2/3, dismiss with Esc.
Everything runs locally. No telemetry.

This is an in-progress rewrite of an older FastAPI/RoBERTa prototype, now built
as a Rust + Tauri desktop app with `llama-server` as a sidecar.

> Status: scaffolding (Milestone 0). See [plan.md](plan.md) for the full plan
> and milestone breakdown.

## Architecture

```
Tauri app (single process)
├── Input listener     (macOS AX + CGEventTap)
├── Floating window    (WebView with 3 suggestion chips)
├── Predictor service  (Rust, llama-server HTTP client, 50ms debounce)
└── llama-server       (sidecar child process, Metal backend)
```

## Repo layout

```
nextword/
├── Cargo.toml                  # workspace root
├── crates/
│   ├── nextword-core/          # platform-agnostic logic (context, parser, predictor)
│   ├── nextword-macos/         # macOS bits (AX, CGEventTap, caret, permissions)
│   └── nextword-app/           # Tauri app (binary entry)
│       ├── src-tauri/          # Rust side
│       └── src/                # webview TS + HTML
├── vendor/llama.cpp/           # git submodule, built via build.rs
├── binaries/                   # built llama-server (gitignored)
├── models/                     # runtime model store (gitignored)
└── scripts/
```

## Prerequisites

- macOS 13+ on Apple Silicon
- Rust 1.80+ (stable)
- Node 20+
- cmake (`brew install cmake`)
- Xcode Command Line Tools
- Tauri CLI: `cargo install tauri-cli --version "^2.0"`

## Building

```bash
git clone <repo> nextword
cd nextword
git submodule update --init --recursive

# Optional: prefetch a small test model so M1 sidecar boot has something to load.
./scripts/dev-download-test-model.sh

cd crates/nextword-app
npm install
cargo tauri dev          # dev mode with hot-reload
cargo tauri build        # release bundle (.app + .dmg)
```

To skip the llama-server build (useful for working on UI without cmake):

```bash
NEXTWORD_SKIP_LLAMA_BUILD=1 cargo tauri dev
```

## Testing the predictor on its own

Once a model is in place and llama-server is running:

```bash
# In one terminal: start llama-server manually
./binaries/llama-server -m ~/Library/Application\ Support/NextWord/models/<model>.gguf --port 8080

# In another:
cargo run --bin predict_test -- "I went to the"
```

## License

MIT
