# screener-app Dioxus Rewrite Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace `screener-app`'s vanilla-JS/Vite frontend with a Dioxus (Rust → WASM) frontend, keeping Tauri as the shell/bundler so the already-verified Android SDK/NDK build pipeline (`cargo tauri android build`) keeps working unchanged.

**Architecture:** A new standalone Rust crate, `apps/screener-app/ui` (package name `screener-ui`), compiled to WASM via `dx build --platform web` (Dioxus CLI 0.7.10, already installed and verified in this environment) and served as static files through Tauri's `frontendDist`. This directly fixes the root cause of the "white screen" bug: the old `main.js` used a bare ES module specifier (`import { invoke } from '@tauri-apps/api/core'`) that browsers/webviews cannot resolve without a bundler, and `tauri.conf.json` had no `devUrl`/`beforeDevCommand` wiring a bundler in — `dx build`'s output is self-contained static files (`index.html` + a wasm-bindgen-generated JS glue file using only relative script paths), so there is no bare-specifier resolution step to fail. The Rust-side `TokenStore`/Tauri-command IPC bridge that existed solely to hand a bearer token from Rust to JS is deleted outright: since the frontend is now Rust/WASM itself, it persists the token directly via browser `localStorage` (through `gloo-storage`), with no IPC bridge needed at all.

**Tech Stack:** Dioxus 0.7.10 (`dioxus-cli` aka `dx`, already installed and verified in this environment via real `dx build --platform web` runs), `reqwest` 0.12 (default-features off, `json` only — same choice `screener-core`'s Yahoo client already makes, and verified here to compile for both `wasm32-unknown-unknown` and the host target), `gloo-net` 0.6 (`websocket` feature) for the WebSocket client, `gloo-storage` 0.3 for token persistence, `wiremock` 0.6 for HTTP-client tests (same pattern as `crates/screener-core/src/data/yahoo.rs`), Tauri 2.11 (unchanged, already installed and verified).

**Spec:** `/home/vijay/Study/Screener/docs/superpowers/specs/2026-08-23-rust-screener-design.md` (section 6: screener-app screens and resilience requirements — note section 4 explicitly requires the Filter Config screen to expose "per-phase condition toggles + editable period/lookback fields, 'Reset to defaults,' explicit Save"; the JS predecessor only ever implemented a single Phase 3 VWAP checkbox, so Task 7 below also closes that gap, not just ports what existed).

**Builds on:** `screener-service` (unchanged) and the original JS `screener-app` (being replaced) — see `docs/superpowers/plans/2026-08-24-screener-app-foundation.md` for the exact API shapes this plan's DTOs must match byte-for-byte: `FilterConfigDto`/`Phase1ConfigDto`/`Phase2ConfigDto`/`Phase3ConfigDto` (`crates/screener-service/src/config.rs`), `RunRecord`/`RunStatus` (`crates/screener-service/src/runs.rs`), `ProgressEvent` with `#[serde(tag = "phase")]` (`crates/screener-service/src/engine.rs`), endpoints `GET/PUT /filter-config`, `POST/GET /runs`, `GET /runs/{id}`, `GET /runs/{id}/stream?token=`, `GET /health` (returns bare `200`, empty body — no JSON).

## Explicitly out of scope for this environment (not just this plan)

- **Running the built app on a real phone/emulator** — same limitation as the original plan; `cargo tauri android build` succeeding is this plan's ceiling of Android verification.
- **Visual/UI polish beyond functional plain HTML** — same scope boundary as the original plan.
- **A settings screen for the API base URL** — `API_BASE_URL` stays a compile-time constant in `main.rs`, exactly matching the JS predecessor's default (`http://10.0.2.2:8080`) and its explicitly-deferred settings-UI scope.
- **Behavioral testing of anything that needs a live browser or Tauri runtime** — `gloo-net` WebSocket usage and `gloo-storage` localStorage usage are compile-verified only, mirroring exactly how `FileTokenStore`/`RealScreeningEngine`/`IbkrClient` are handled elsewhere in this codebase: an in-memory/pure equivalent is fully unit-tested, the real implementation is compile-verified only.

## Global Constraints

- Every DTO field name in `ui/src/dto.rs` must exactly match `screener-service`'s `config.rs`/`runs.rs`/`engine.rs` types — no renaming. `ProgressEvent` must use `#[serde(tag = "phase")]` to match the server's internally-tagged JSON.
- `RunRecord.id`, `.started_at`, `.finished_at` are plain `String` in the client DTO (not `Uuid`/`DateTime<Utc>`) — the UI only displays and round-trips these values, never parses them, so pulling in `uuid`/`chrono` as dependencies would be unused complexity. This mirrors the JS predecessor, which also treated them as opaque strings.
- The `ui` crate needs its own empty `[workspace]` table in `Cargo.toml`, exactly like `src-tauri/Cargo.toml` already has — otherwise Cargo's implicit-workspace-membership detection tries to fold it into the root `Screener` workspace (`/home/vijay/Study/Screener/Cargo.toml`), which only lists `crates/screener-core` and `crates/screener-service` as members and does not expect this crate.
- Anything that touches a real browser API (`gloo-net::websocket`, `gloo-storage::LocalStorage`) is compile-verified only — do not write a test that claims to prove its runtime behavior in this environment.
- After every task that adds or changes `ui` crate code, run `cargo build` (host target, from `apps/screener-app/ui/`) AND `cd apps/screener-app/ui && dx build --platform web` — the host build catches ordinary Rust errors fast; the `dx build` catches anything Dioxus-macro/wasm-specific that only shows up when actually targeting `wasm32-unknown-unknown`. Both must succeed before a task is considered done.
- Disk space in this environment is tight (single ~29G volume). If a build fails with "No space left on device," run `cargo clean` inside whichever crate directory you were building (`ui/`, `src-tauri/`, or the workspace root) before investigating further — build artifacts are always regenerable.

---

## File Structure

```text
apps/screener-app/
├── ui/                                  # CREATE: new Dioxus frontend crate
│   ├── Cargo.toml
│   ├── index.html                         # custom template; dx injects the wasm <script> tag before </body>
│   ├── assets/
│   │   └── styles.css                      # ported from the old src/styles.css
│   └── src/
│       ├── main.rs                          # dioxus::launch(App); App component: nav + screen router + init effect
│       ├── dto.rs                            # Phase1/2/3ConfigDto, FilterConfigDto, RunRecord, RunStatus, ProgressEvent
│       ├── token_storage.rs                   # TokenStorage trait, InMemoryTokenStorage (tested), LocalTokenStorage (compile-verified)
│       ├── api.rs                              # ApiClient (reqwest), wiremock-tested
│       ├── ws.rs                                # build_stream_url (tested), connect_run_stream (compile-verified)
│       ├── state.rs                              # AppState, Screen, RunSummary
│       └── screens/
│           ├── mod.rs
│           ├── dashboard.rs
│           ├── filter_config.rs
│           ├── live_run.rs
│           ├── watchlist.rs
│           └── history.rs
│
├── src-tauri/
│   ├── Cargo.toml                        # UNCHANGED
│   ├── tauri.conf.json                    # MODIFY: frontendDist + beforeDevCommand/beforeBuildCommand point at the ui crate's dx build output
│   └── src/
│       ├── lib.rs                          # MODIFY: strip back to a bare tauri::Builder (no TokenStore/commands wiring)
│       ├── commands.rs                      # DELETE
│       └── token_store.rs                   # DELETE
│
# DELETE entirely (old JS frontend, fully replaced by ui/):
apps/screener-app/src/
apps/screener-app/test/
apps/screener-app/package.json
apps/screener-app/package-lock.json
apps/screener-app/vite.config.js
apps/screener-app/node_modules/
```

---

### Task 1: Remove the JS frontend; scaffold the Dioxus `ui` crate; wire Tauri to it; prove it actually renders

**Files:**
- Delete: `apps/screener-app/src/`, `apps/screener-app/test/`, `apps/screener-app/package.json`, `apps/screener-app/package-lock.json`, `apps/screener-app/vite.config.js`, `apps/screener-app/node_modules/`
- Create: `apps/screener-app/ui/Cargo.toml`
- Create: `apps/screener-app/ui/index.html`
- Create: `apps/screener-app/ui/assets/styles.css`
- Create: `apps/screener-app/ui/src/main.rs`
- Modify: `apps/screener-app/src-tauri/tauri.conf.json`

**Interfaces:**
- Produces: a working `dx build --platform web` pipeline whose output Tauri serves; a placeholder `App` component later tasks replace piece by piece.

- [ ] **Step 1: Delete the old JS frontend**

```bash
cd /home/vijay/Study/Screener/apps/screener-app
rm -rf src test package.json package-lock.json vite.config.js node_modules
```

- [ ] **Step 2: Create the `ui` crate**

`apps/screener-app/ui/Cargo.toml`:

```toml
[workspace]

[package]
name = "screener-ui"
version = "0.1.0"
edition = "2021"

[dependencies]
dioxus = { version = "0.7", features = ["web"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
reqwest = { version = "0.12", default-features = false, features = ["json"] }
gloo-net = { version = "0.6", features = ["websocket"] }
gloo-storage = "0.3"
wasm-bindgen-futures = "0.4"
futures-util = "0.3"
urlencoding = "2"
thiserror = "1"

[dev-dependencies]
wiremock = "0.6"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

`apps/screener-app/ui/index.html`:

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Screener</title>
    <link rel="stylesheet" href="/assets/styles.css" />
  </head>
  <body>
    <div id="main"></div>
  </body>
</html>
```

`apps/screener-app/ui/assets/styles.css` (ported verbatim from the deleted `src/styles.css`):

```css
body {
  font-family: system-ui, sans-serif;
  margin: 0;
  padding: 1rem;
}

nav {
  display: flex;
  gap: 0.5rem;
  margin-bottom: 1rem;
  flex-wrap: wrap;
}

nav button {
  padding: 0.5rem 1rem;
}
```

`apps/screener-app/ui/src/main.rs` (placeholder — later tasks build out the real `App`):

```rust
use dioxus::prelude::*;

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        nav { "screener-app" }
        main { p { "loading..." } }
    }
}
```

- [ ] **Step 3: Verify the crate builds for both targets**

```bash
cd /home/vijay/Study/Screener/apps/screener-app/ui
cargo build
dx build --platform web
```

Expected: both succeed. `dx build` prints `Client build completed successfully!` with a path ending in `ui/target/dx/screener-ui/debug/web/public`.

- [ ] **Step 4: Point Tauri at the Dioxus build output**

In `apps/screener-app/src-tauri/tauri.conf.json`, replace the `"build"` section (currently `{ "frontendDist": "../dist", "devUrl": "http://localhost:1420", "beforeDevCommand": "npm run dev", "beforeBuildCommand": "npm run build" }`, an interim Vite-based fix from earlier debugging) with:

```json
  "build": {
    "frontendDist": "../ui/target/dx/screener-ui/debug/web/public",
    "beforeDevCommand": "cd ui && dx build --platform web",
    "beforeBuildCommand": "cd ui && dx build --platform web"
  },
```

No `devUrl` — `beforeDevCommand` always produces a fresh static build and Tauri serves it directly, avoiding a second moving part (a live dev server) while this gets proven out.

- [ ] **Step 5: Real, visual verification that it renders — not just that it compiles**

```bash
pkill -f target/debug/screener-app 2>/dev/null
source "$HOME/.cargo/env"
cd /home/vijay/Study/Screener/apps/screener-app
export DISPLAY=:20
nohup cargo tauri dev > /tmp/tauri-dioxus-verify.log 2>&1 &
```

Wait for `Finished` / `Running` in the log, then take a real screenshot of the app window (raise it first so no other window occludes it, since this X server has no compositor and occluded regions of `xwd -root` can return garbage):

```bash
sleep 15
WID=$(DISPLAY=:20 xdotool search --name "screener-app" | head -1)
DISPLAY=:20 xdotool windowactivate "$WID"; DISPLAY=:20 xdotool windowraise "$WID"
sleep 1
DISPLAY=:20 xwd -root -silent -out /tmp/verify.xwd
convert /tmp/verify.xwd /tmp/verify.png
```

Read `/tmp/verify.png` and confirm it shows the "screener-app" nav text and "loading..." — not a blank white window. This is the actual regression test for the original bug; do not skip it.

- [ ] **Step 6: Commit**

```bash
cd /home/vijay/Study/Screener
git add apps/screener-app/ui apps/screener-app/src-tauri/tauri.conf.json
git add -u apps/screener-app
git commit -m "Replace screener-app JS frontend with a Dioxus (WASM) crate"
```

---

### Task 2: DTOs matching screener-service exactly

**Files:**
- Create: `apps/screener-app/ui/src/dto.rs`
- Modify: `apps/screener-app/ui/src/main.rs` (add `mod dto;`)

**Interfaces:**
- Produces: `Phase1ConfigDto`, `Phase2ConfigDto`, `Phase3ConfigDto`, `FilterConfigDto` (all `Debug + Clone + PartialEq + Serialize + Deserialize + Default`), `RunStatus` (`Running`/`Completed`/`Failed`, `#[serde(rename_all = "snake_case")]`), `RunRecord { id: String, status: RunStatus, started_at: String, finished_at: Option<String>, final_watchlist: Vec<String>, error_message: Option<String> }`, `ProgressEvent` (`#[serde(tag = "phase")]`, variants `Phase1`/`Phase2`/`Phase3 { total: usize, started: usize, passed: usize, technical_failures: usize, errors: usize }`, `Complete { final_watchlist: Vec<String> }`, `Failed { message: String }`).

- [ ] **Step 1: Write the failing tests**

`apps/screener-app/ui/src/dto.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Phase1ConfigDto {
    pub sma10_period: usize,
    pub sma20_period: usize,
    pub sma50_period: usize,
    pub sma200_period: usize,
    pub slope_lookback_bars: usize,
    pub enable_close_gt_sma10: bool,
    pub enable_sma10_gt_sma20: bool,
    pub enable_sma20_gt_sma50: bool,
    pub enable_sma50_gt_sma200: bool,
    pub enable_sma200_slope: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Phase2ConfigDto {
    pub sma7_period: usize,
    pub sma17_period: usize,
    pub sma33_period: usize,
    pub sma65_period: usize,
    pub slope_lookback_bars: usize,
    pub enable_close_gt_sma7: bool,
    pub enable_sma7_gt_sma17: bool,
    pub enable_sma17_gt_sma33: bool,
    pub enable_sma33_gt_sma65: bool,
    pub enable_sma65_slope: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Phase3ConfigDto {
    pub sma20_period: usize,
    pub sma50_period: usize,
    pub sma100_period: usize,
    pub enable_close_gt_sma20: bool,
    pub enable_sma20_gt_sma50: bool,
    pub enable_sma50_gt_sma100: bool,
    pub enable_vwap: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct FilterConfigDto {
    pub phase1: Phase1ConfigDto,
    pub phase2: Phase2ConfigDto,
    pub phase3: Phase3ConfigDto,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunRecord {
    pub id: String,
    pub status: RunStatus,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub final_watchlist: Vec<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "phase")]
pub enum ProgressEvent {
    Phase1 { total: usize, started: usize, passed: usize, technical_failures: usize, errors: usize },
    Phase2 { total: usize, started: usize, passed: usize, technical_failures: usize, errors: usize },
    Phase3 { total: usize, started: usize, passed: usize, technical_failures: usize, errors: usize },
    Complete { final_watchlist: Vec<String> },
    Failed { message: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase1_default_matches_screener_core() {
        todo!()
    }

    #[test]
    fn phase2_default_matches_screener_core() {
        todo!()
    }

    #[test]
    fn phase3_default_matches_screener_core() {
        todo!()
    }

    #[test]
    fn filter_config_dto_round_trips_through_json() {
        todo!()
    }

    #[test]
    fn progress_event_phase1_deserializes_from_tagged_json() {
        let json = r#"{"phase":"Phase1","total":10,"started":5,"passed":2,"technical_failures":1,"errors":0}"#;
        let event: ProgressEvent = serde_json::from_str(json).unwrap();
        assert_eq!(
            event,
            ProgressEvent::Phase1 { total: 10, started: 5, passed: 2, technical_failures: 1, errors: 0 }
        );
    }

    #[test]
    fn progress_event_complete_deserializes_from_tagged_json() {
        let json = r#"{"phase":"Complete","final_watchlist":["AAPL"]}"#;
        let event: ProgressEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event, ProgressEvent::Complete { final_watchlist: vec!["AAPL".to_string()] });
    }
}
```

- [ ] **Step 2: Run to verify the `todo!()` tests fail**

```bash
cd /home/vijay/Study/Screener/apps/screener-app/ui
cargo test dto:: -- --nocapture 2>&1 | tail -30
```

Expected: `progress_event_*` tests pass (already fully written), the three `todo!()` tests panic with `not yet implemented`.

- [ ] **Step 3: Fill in the default-matching and round-trip tests**

Replace the three `todo!()` bodies:

```rust
    #[test]
    fn phase1_default_matches_screener_core() {
        let dto = Phase1ConfigDto {
            sma10_period: 10,
            sma20_period: 20,
            sma50_period: 50,
            sma200_period: 200,
            slope_lookback_bars: 5,
            enable_close_gt_sma10: true,
            enable_sma10_gt_sma20: true,
            enable_sma20_gt_sma50: true,
            enable_sma50_gt_sma200: true,
            enable_sma200_slope: true,
        };
        assert_eq!(dto.sma200_period, 200);
        assert_eq!(dto.slope_lookback_bars, 5);
    }

    #[test]
    fn phase2_default_matches_screener_core() {
        let dto = Phase2ConfigDto {
            sma7_period: 7,
            sma17_period: 17,
            sma33_period: 33,
            sma65_period: 65,
            slope_lookback_bars: 1,
            enable_close_gt_sma7: true,
            enable_sma7_gt_sma17: true,
            enable_sma17_gt_sma33: true,
            enable_sma33_gt_sma65: true,
            enable_sma65_slope: true,
        };
        assert_eq!(dto.sma65_period, 65);
        assert_eq!(dto.slope_lookback_bars, 1);
    }

    #[test]
    fn phase3_default_matches_screener_core() {
        let dto = Phase3ConfigDto {
            sma20_period: 20,
            sma50_period: 50,
            sma100_period: 100,
            enable_close_gt_sma20: true,
            enable_sma20_gt_sma50: true,
            enable_sma50_gt_sma100: true,
            enable_vwap: true,
        };
        assert_eq!(dto.sma100_period, 100);
        assert!(dto.enable_vwap);
    }

    #[test]
    fn filter_config_dto_round_trips_through_json() {
        let dto = FilterConfigDto::default();
        let json = serde_json::to_string(&dto).unwrap();
        let back: FilterConfigDto = serde_json::from_str(&json).unwrap();
        assert_eq!(dto, back);
    }
```

This exercises `#[derive(Default)]` on the three phase DTOs too, so also add `Default` impls matching `screener_core`'s exact defaults (mirrored from `crates/screener-core/src/config.rs`):

```rust
impl Default for Phase1ConfigDto {
    fn default() -> Self {
        Self {
            sma10_period: 10,
            sma20_period: 20,
            sma50_period: 50,
            sma200_period: 200,
            slope_lookback_bars: 5,
            enable_close_gt_sma10: true,
            enable_sma10_gt_sma20: true,
            enable_sma20_gt_sma50: true,
            enable_sma50_gt_sma200: true,
            enable_sma200_slope: true,
        }
    }
}

impl Default for Phase2ConfigDto {
    fn default() -> Self {
        Self {
            sma7_period: 7,
            sma17_period: 17,
            sma33_period: 33,
            sma65_period: 65,
            slope_lookback_bars: 1,
            enable_close_gt_sma7: true,
            enable_sma7_gt_sma17: true,
            enable_sma17_gt_sma33: true,
            enable_sma33_gt_sma65: true,
            enable_sma65_slope: true,
        }
    }
}

impl Default for Phase3ConfigDto {
    fn default() -> Self {
        Self {
            sma20_period: 20,
            sma50_period: 50,
            sma100_period: 100,
            enable_close_gt_sma20: true,
            enable_sma20_gt_sma50: true,
            enable_sma50_gt_sma100: true,
            enable_vwap: true,
        }
    }
}
```

- [ ] **Step 4: Run tests, verify all pass**

```bash
cd /home/vijay/Study/Screener/apps/screener-app/ui
cargo test dto:: -- --nocapture 2>&1 | tail -30
```

Expected: 6 passed.

- [ ] **Step 5: Wire the module in and verify both build targets**

Add `mod dto;` to `apps/screener-app/ui/src/main.rs`.

```bash
cd /home/vijay/Study/Screener/apps/screener-app/ui
cargo build
dx build --platform web
```

- [ ] **Step 6: Commit**

```bash
cd /home/vijay/Study/Screener
git add apps/screener-app/ui/src/dto.rs apps/screener-app/ui/src/main.rs
git commit -m "Add screener-app DTOs mirroring screener-service's JSON shapes"
```

---

### Task 3: TokenStorage — replace the src-tauri IPC bridge with direct browser storage

**Files:**
- Create: `apps/screener-app/ui/src/token_storage.rs`
- Modify: `apps/screener-app/ui/src/main.rs` (add `mod token_storage;`)
- Delete: `apps/screener-app/src-tauri/src/commands.rs`, `apps/screener-app/src-tauri/src/token_store.rs`
- Modify: `apps/screener-app/src-tauri/src/lib.rs`

**Interfaces:**
- Produces: `TokenStorage` trait (`get(&self) -> Option<String>`, `set(&self, token: &str)`), `InMemoryTokenStorage` (fully tested), `LocalTokenStorage` (compile-verified only, backed by `gloo_storage::LocalStorage`).

- [ ] **Step 1: Write the failing tests**

`apps/screener-app/ui/src/token_storage.rs`:

```rust
use std::sync::Mutex;

pub trait TokenStorage: Send + Sync {
    fn get(&self) -> Option<String>;
    fn set(&self, token: &str);
}

#[derive(Default)]
pub struct InMemoryTokenStorage {
    token: Mutex<Option<String>>,
}

impl InMemoryTokenStorage {
    pub fn new() -> Self {
        Self::default()
    }
}

impl TokenStorage for InMemoryTokenStorage {
    fn get(&self) -> Option<String> {
        todo!()
    }

    fn set(&self, token: &str) {
        todo!()
    }
}

const STORAGE_KEY: &str = "screener_auth_token";

/// Persists the token in the webview's browser `localStorage`. Needs a
/// running wasm/browser environment (`web_sys::window()` must resolve), so
/// — like `FileTokenStore` in this app's JS predecessor — this is
/// compile-verified only in this environment, not behavior-tested.
pub struct LocalTokenStorage;

impl TokenStorage for LocalTokenStorage {
    fn get(&self) -> Option<String> {
        gloo_storage::LocalStorage::get(STORAGE_KEY).ok()
    }

    fn set(&self, token: &str) {
        let _ = gloo_storage::LocalStorage::set(STORAGE_KEY, token);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_none_before_anything_is_set() {
        let store = InMemoryTokenStorage::new();
        assert_eq!(store.get(), None);
    }

    #[test]
    fn returns_the_token_after_set() {
        let store = InMemoryTokenStorage::new();
        store.set("abc123");
        assert_eq!(store.get(), Some("abc123".to_string()));
    }

    #[test]
    fn set_overwrites_the_previous_token() {
        let store = InMemoryTokenStorage::new();
        store.set("first");
        store.set("second");
        assert_eq!(store.get(), Some("second".to_string()));
    }
}
```

- [ ] **Step 2: Run to verify failure**

```bash
cd /home/vijay/Study/Screener/apps/screener-app/ui
cargo test token_storage:: -- --nocapture 2>&1 | tail -20
```

Expected: all 3 tests panic with `not yet implemented`.

- [ ] **Step 3: Implement `InMemoryTokenStorage`**

```rust
impl TokenStorage for InMemoryTokenStorage {
    fn get(&self) -> Option<String> {
        self.token.lock().expect("token storage mutex poisoned").clone()
    }

    fn set(&self, token: &str) {
        *self.token.lock().expect("token storage mutex poisoned") = Some(token.to_string());
    }
}
```

- [ ] **Step 4: Run tests, verify all pass**

```bash
cd /home/vijay/Study/Screener/apps/screener-app/ui
cargo test token_storage:: -- --nocapture 2>&1 | tail -20
```

Expected: 3 passed.

- [ ] **Step 5: Delete the now-unnecessary src-tauri IPC bridge**

```bash
cd /home/vijay/Study/Screener/apps/screener-app/src-tauri
rm src/commands.rs src/token_store.rs
```

Replace `apps/screener-app/src-tauri/src/lib.rs` entirely with:

```rust
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 6: Wire the `ui` module in, verify both crates still build**

Add `mod token_storage;` to `apps/screener-app/ui/src/main.rs`.

```bash
cd /home/vijay/Study/Screener/apps/screener-app/ui
cargo build && dx build --platform web

cd /home/vijay/Study/Screener/apps/screener-app/src-tauri
cargo build
```

- [ ] **Step 7: Commit**

```bash
cd /home/vijay/Study/Screener
git add apps/screener-app/ui/src/token_storage.rs apps/screener-app/ui/src/main.rs
git add -u apps/screener-app/src-tauri
git commit -m "Add TokenStorage in the ui crate; remove the now-unneeded Tauri IPC bridge"
```

---

### Task 4: API client

**Files:**
- Create: `apps/screener-app/ui/src/api.rs`
- Modify: `apps/screener-app/ui/src/main.rs` (add `mod api;`)

**Interfaces:**
- Consumes: `crate::dto::{FilterConfigDto, RunRecord}`
- Produces: `ApiClient::new(base_url: impl Into<String>) -> Self` (derives `Clone`), `async fn get_filter_config(&self, token: &str) -> Result<FilterConfigDto, ApiError>`, `async fn put_filter_config(&self, token: &str, config: &FilterConfigDto) -> Result<FilterConfigDto, ApiError>`, `async fn trigger_run(&self, token: &str) -> Result<RunRecord, ApiError>`, `async fn get_run(&self, token: &str, run_id: &str) -> Result<RunRecord, ApiError>`, `async fn list_runs(&self, token: &str) -> Result<Vec<RunRecord>, ApiError>`, `async fn get_health(&self) -> Result<(), ApiError>`. `ApiError` is `Message(String)` (server-reported or HTTP-status-derived error) or `Request(reqwest::Error)`.

- [ ] **Step 1: Write the failing tests**

`apps/screener-app/ui/src/api.rs`:

```rust
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::dto::{FilterConfigDto, RunRecord};

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{0}")]
    Message(String),
    #[error("request failed: {0}")]
    Request(#[from] reqwest::Error),
}

#[derive(Debug, Clone)]
pub struct ApiClient {
    http: reqwest::Client,
    base_url: String,
}

impl ApiClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self { http: reqwest::Client::new(), base_url: base_url.into() }
    }

    async fn send(
        &self,
        method: reqwest::Method,
        path: &str,
        token: Option<&str>,
        body: Option<&(impl Serialize + ?Sized)>,
    ) -> Result<String, ApiError> {
        todo!()
    }

    async fn request_json<T: DeserializeOwned>(
        &self,
        method: reqwest::Method,
        path: &str,
        token: Option<&str>,
        body: Option<&(impl Serialize + ?Sized)>,
    ) -> Result<T, ApiError> {
        let text = self.send(method, path, token, body).await?;
        serde_json::from_str(&text).map_err(|e| ApiError::Message(e.to_string()))
    }

    pub async fn get_filter_config(&self, token: &str) -> Result<FilterConfigDto, ApiError> {
        self.request_json(reqwest::Method::GET, "/filter-config", Some(token), None::<&()>).await
    }

    pub async fn put_filter_config(&self, token: &str, config: &FilterConfigDto) -> Result<FilterConfigDto, ApiError> {
        self.request_json(reqwest::Method::PUT, "/filter-config", Some(token), Some(config)).await
    }

    pub async fn trigger_run(&self, token: &str) -> Result<RunRecord, ApiError> {
        self.request_json(reqwest::Method::POST, "/runs", Some(token), None::<&()>).await
    }

    pub async fn get_run(&self, token: &str, run_id: &str) -> Result<RunRecord, ApiError> {
        self.request_json(reqwest::Method::GET, &format!("/runs/{run_id}"), Some(token), None::<&()>).await
    }

    pub async fn list_runs(&self, token: &str) -> Result<Vec<RunRecord>, ApiError> {
        self.request_json(reqwest::Method::GET, "/runs", Some(token), None::<&()>).await
    }

    pub async fn get_health(&self) -> Result<(), ApiError> {
        self.send(reqwest::Method::GET, "/health", None, None::<&()>).await.map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn get_filter_config_sends_bearer_token_and_returns_parsed_json() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/filter-config"))
            .and(header("authorization", "Bearer secret-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(FilterConfigDto::default()))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        let config = client.get_filter_config("secret-token").await.unwrap();
        assert_eq!(config, FilterConfigDto::default());
    }

    #[tokio::test]
    async fn put_filter_config_sends_put_with_json_body() {
        let server = MockServer::start().await;
        let mut updated = FilterConfigDto::default();
        updated.phase3.enable_vwap = false;
        Mock::given(method("PUT"))
            .and(path("/filter-config"))
            .respond_with(ResponseTemplate::new(200).set_body_json(updated.clone()))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        let result = client.put_filter_config("secret-token", &updated).await.unwrap();
        assert_eq!(result, updated);
    }

    #[tokio::test]
    async fn trigger_run_posts_to_runs_and_returns_the_record() {
        todo!()
    }

    #[tokio::test]
    async fn trigger_run_surfaces_the_server_error_message_on_409() {
        todo!()
    }

    #[tokio::test]
    async fn get_run_fetches_runs_by_id() {
        todo!()
    }

    #[tokio::test]
    async fn list_runs_fetches_runs_and_returns_an_array() {
        todo!()
    }

    #[tokio::test]
    async fn get_health_sends_no_authorization_header() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/health"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        client.get_health().await.unwrap();

        let requests = server.received_requests().await.unwrap();
        assert!(requests[0].headers.get("authorization").is_none());
    }
}
```

- [ ] **Step 2: Run to verify failures**

```bash
cd /home/vijay/Study/Screener/apps/screener-app/ui
cargo test api:: -- --nocapture 2>&1 | tail -40
```

Expected: compile error (`send` returns `todo!()`, `!` doesn't unify with `Result<String, ApiError>` — actually `todo!()` type-checks fine as it has type `!` coercing to anything, so expect the two written tests to panic with "not yet implemented" and the four `todo!()`-bodied tests to also panic).

- [ ] **Step 3: Implement `send`**

```rust
    async fn send(
        &self,
        method: reqwest::Method,
        path: &str,
        token: Option<&str>,
        body: Option<&(impl Serialize + ?Sized)>,
    ) -> Result<String, ApiError> {
        let mut req = self.http.request(method, format!("{}{}", self.base_url, path));
        if let Some(token) = token {
            req = req.bearer_auth(token);
        }
        if let Some(body) = body {
            req = req.json(body);
        }
        let response = req.send().await?;
        let status = response.status();
        let text = response.text().await?;
        if !status.is_success() {
            let message = serde_json::from_str::<serde_json::Value>(&text)
                .ok()
                .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string))
                .unwrap_or_else(|| format!("HTTP {status}"));
            return Err(ApiError::Message(message));
        }
        Ok(text)
    }
```

- [ ] **Step 4: Fill in the remaining tests**

```rust
    #[tokio::test]
    async fn trigger_run_posts_to_runs_and_returns_the_record() {
        let server = MockServer::start().await;
        let record = RunRecord {
            id: "run-1".to_string(),
            status: crate::dto::RunStatus::Running,
            started_at: "2026-08-24T00:00:00Z".to_string(),
            finished_at: None,
            final_watchlist: vec![],
            error_message: None,
        };
        Mock::given(method("POST"))
            .and(path("/runs"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&record))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        let result = client.trigger_run("secret-token").await.unwrap();
        assert_eq!(result.id, "run-1");
    }

    #[tokio::test]
    async fn trigger_run_surfaces_the_server_error_message_on_409() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/runs"))
            .respond_with(
                ResponseTemplate::new(409)
                    .set_body_json(serde_json::json!({ "error": "a run is already in progress" })),
            )
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        let err = client.trigger_run("secret-token").await.unwrap_err();
        assert_eq!(err.to_string(), "a run is already in progress");
    }

    #[tokio::test]
    async fn get_run_fetches_runs_by_id() {
        let server = MockServer::start().await;
        let record = RunRecord {
            id: "run-1".to_string(),
            status: crate::dto::RunStatus::Completed,
            started_at: "2026-08-24T00:00:00Z".to_string(),
            finished_at: Some("2026-08-24T00:05:00Z".to_string()),
            final_watchlist: vec!["AAPL".to_string()],
            error_message: None,
        };
        Mock::given(method("GET"))
            .and(path("/runs/run-1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&record))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        let result = client.get_run("secret-token", "run-1").await.unwrap();
        assert_eq!(result.status, crate::dto::RunStatus::Completed);
    }

    #[tokio::test]
    async fn list_runs_fetches_runs_and_returns_an_array() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/runs"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri());
        let result = client.list_runs("secret-token").await.unwrap();
        assert_eq!(result.len(), 0);
    }
```

- [ ] **Step 5: Run tests, verify all pass**

```bash
cd /home/vijay/Study/Screener/apps/screener-app/ui
cargo test api:: -- --nocapture 2>&1 | tail -40
```

Expected: 7 passed.

- [ ] **Step 6: Wire the module in and verify both build targets**

Add `mod api;` to `apps/screener-app/ui/src/main.rs`.

```bash
cd /home/vijay/Study/Screener/apps/screener-app/ui
cargo build
dx build --platform web
```

- [ ] **Step 7: Commit**

```bash
cd /home/vijay/Study/Screener
git add apps/screener-app/ui/src/api.rs apps/screener-app/ui/src/main.rs
git commit -m "Add screener-app REST API client with wiremock-backed tests"
```

---

### Task 5: WebSocket client

**Files:**
- Create: `apps/screener-app/ui/src/ws.rs`
- Modify: `apps/screener-app/ui/src/main.rs` (add `mod ws;`)

**Interfaces:**
- Consumes: `crate::dto::ProgressEvent`
- Produces: `pub fn build_stream_url(http_base: &str, token: &str, run_id: &str) -> String` (pure, tested), `pub struct StreamHandlers { pub on_event: Box<dyn Fn(ProgressEvent)>, pub on_close: Box<dyn Fn()> }`, `pub fn connect_run_stream(http_base: &str, token: &str, run_id: &str, handlers: StreamHandlers)` (compile-verified only — spawns a `wasm_bindgen_futures` task, needs a real browser WebSocket).

- [ ] **Step 1: Write the failing test**

`apps/screener-app/ui/src/ws.rs`:

```rust
use futures_util::StreamExt;
use gloo_net::websocket::futures::WebSocket;
use gloo_net::websocket::Message;

use crate::dto::ProgressEvent;

pub fn build_stream_url(http_base: &str, token: &str, run_id: &str) -> String {
    todo!()
}

pub struct StreamHandlers {
    pub on_event: Box<dyn Fn(ProgressEvent)>,
    pub on_close: Box<dyn Fn()>,
}

/// Opens the run-progress WebSocket and forwards parsed `ProgressEvent`s to
/// `handlers.on_event`, calling `handlers.on_close` when the socket closes.
/// Needs a running browser/wasm environment (`gloo_net` wraps the browser
/// `WebSocket` API) — like `LocalTokenStorage`, this is compile-verified
/// only in this environment, not behavior-tested.
pub fn connect_run_stream(http_base: &str, token: &str, run_id: &str, handlers: StreamHandlers) {
    let url = build_stream_url(http_base, token, run_id);
    wasm_bindgen_futures::spawn_local(async move {
        let ws = match WebSocket::open(&url) {
            Ok(ws) => ws,
            Err(_) => {
                (handlers.on_close)();
                return;
            }
        };
        let (_write, mut read) = ws.split();
        while let Some(Ok(Message::Text(text))) = read.next().await {
            if let Ok(event) = serde_json::from_str::<ProgressEvent>(&text) {
                (handlers.on_event)(event);
            }
        }
        (handlers.on_close)();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_the_url_with_the_token_as_a_query_param_not_a_header() {
        let url = build_stream_url("http://test.local:8080", "secret-token", "run-1");
        assert_eq!(url, "ws://test.local:8080/runs/run-1/stream?token=secret-token");
    }

    #[test]
    fn upgrades_https_to_wss() {
        let url = build_stream_url("https://test.local:8080", "secret-token", "run-1");
        assert_eq!(url, "wss://test.local:8080/runs/run-1/stream?token=secret-token");
    }

    #[test]
    fn percent_encodes_special_characters_in_the_token() {
        let url = build_stream_url("http://test.local:8080", "a+b/c=", "run-1");
        assert_eq!(url, "ws://test.local:8080/runs/run-1/stream?token=a%2Bb%2Fc%3D");
    }
}
```

- [ ] **Step 2: Run to verify failure**

```bash
cd /home/vijay/Study/Screener/apps/screener-app/ui
cargo test ws:: -- --nocapture 2>&1 | tail -20
```

Expected: 3 panics with `not yet implemented`.

- [ ] **Step 3: Implement `build_stream_url`**

```rust
pub fn build_stream_url(http_base: &str, token: &str, run_id: &str) -> String {
    let ws_base = if let Some(rest) = http_base.strip_prefix("https") {
        format!("wss{rest}")
    } else if let Some(rest) = http_base.strip_prefix("http") {
        format!("ws{rest}")
    } else {
        http_base.to_string()
    };
    format!("{ws_base}/runs/{run_id}/stream?token={}", urlencoding::encode(token))
}
```

- [ ] **Step 4: Run tests, verify all pass**

```bash
cd /home/vijay/Study/Screener/apps/screener-app/ui
cargo test ws:: -- --nocapture 2>&1 | tail -20
```

Expected: 3 passed.

- [ ] **Step 5: Wire the module in and verify both build targets**

Add `mod ws;` to `apps/screener-app/ui/src/main.rs`.

```bash
cd /home/vijay/Study/Screener/apps/screener-app/ui
cargo build
dx build --platform web
```

- [ ] **Step 6: Commit**

```bash
cd /home/vijay/Study/Screener
git add apps/screener-app/ui/src/ws.rs apps/screener-app/ui/src/main.rs
git commit -m "Add screener-app WebSocket client with tested URL building"
```

---

### Task 6: App state

**Files:**
- Create: `apps/screener-app/ui/src/state.rs`
- Modify: `apps/screener-app/ui/src/main.rs` (add `mod state;`)

**Interfaces:**
- Produces: `Screen` (`Dashboard` (default) / `FilterConfig` / `LiveRun` / `Watchlist` / `History`, `Debug + Clone + Copy + PartialEq + Eq + Default`), `RunSummary { status: String, final_watchlist: Vec<String> }` (`Debug + Clone + Default + PartialEq`), `AppState { token: Option<String>, active_run_id: Option<String>, last_run: Option<RunSummary>, screen: Screen }` (`Debug + Clone + Default + PartialEq`).

- [ ] **Step 1: Write the failing test**

`apps/screener-app/ui/src/state.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Screen {
    #[default]
    Dashboard,
    FilterConfig,
    LiveRun,
    Watchlist,
    History,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RunSummary {
    pub status: String,
    pub final_watchlist: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct AppState {
    pub token: Option<String>,
    pub active_run_id: Option<String>,
    pub last_run: Option<RunSummary>,
    pub screen: Screen,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_state_starts_on_the_dashboard_with_nothing_loaded() {
        todo!()
    }
}
```

- [ ] **Step 2: Run to verify failure**

```bash
cd /home/vijay/Study/Screener/apps/screener-app/ui
cargo test state:: -- --nocapture 2>&1 | tail -10
```

Expected: panic with `not yet implemented`.

- [ ] **Step 3: Fill in the test**

```rust
    #[test]
    fn default_state_starts_on_the_dashboard_with_nothing_loaded() {
        let state = AppState::default();
        assert_eq!(state.token, None);
        assert_eq!(state.active_run_id, None);
        assert_eq!(state.last_run, None);
        assert_eq!(state.screen, Screen::Dashboard);
    }
```

- [ ] **Step 4: Run tests, verify it passes**

```bash
cd /home/vijay/Study/Screener/apps/screener-app/ui
cargo test state:: -- --nocapture 2>&1 | tail -10
```

Expected: 1 passed.

- [ ] **Step 5: Wire the module in and verify both build targets**

Add `mod state;` to `apps/screener-app/ui/src/main.rs`.

```bash
cd /home/vijay/Study/Screener/apps/screener-app/ui
cargo build
dx build --platform web
```

- [ ] **Step 6: Commit**

```bash
cd /home/vijay/Study/Screener
git add apps/screener-app/ui/src/state.rs apps/screener-app/ui/src/main.rs
git commit -m "Add screener-app AppState/Screen types"
```

---

### Task 7: Dashboard and Filter Config screens

**Files:**
- Create: `apps/screener-app/ui/src/screens/mod.rs`
- Create: `apps/screener-app/ui/src/screens/dashboard.rs`
- Create: `apps/screener-app/ui/src/screens/filter_config.rs`
- Modify: `apps/screener-app/ui/src/main.rs` (add `mod screens;`)

**Interfaces:**
- Consumes: `crate::api::ApiClient` and `crate::state::{AppState, Screen, RunSummary}` via Dioxus context (`use_context::<Signal<AppState>>()`, `use_context::<ApiClient>()` — both provided by `App` in Task 9); `crate::dto::FilterConfigDto`.
- Produces: `#[component] pub fn Dashboard() -> Element`, `#[component] pub fn FilterConfig() -> Element`.

This is the point where the plan closes the scope gap in the JS predecessor: the spec (section 4) requires the Filter Config screen to expose **every** phase's condition toggles and period fields, not just Phase 3's VWAP toggle. There is no unit-testable logic here beyond what Tasks 2–6 already cover (Dioxus component rendering isn't unit-testable without a running app or an SSR harness) — verification is `cargo build` + `dx build --platform web` succeeding, exactly like the JS predecessor's screens were "syntax-checked only."

- [ ] **Step 1: Create the screens module**

`apps/screener-app/ui/src/screens/mod.rs`:

```rust
pub mod dashboard;
pub mod filter_config;
```

- [ ] **Step 2: Write the Dashboard screen**

`apps/screener-app/ui/src/screens/dashboard.rs`:

```rust
use dioxus::prelude::*;

use crate::api::ApiClient;
use crate::state::{AppState, Screen};

#[component]
pub fn Dashboard() -> Element {
    let mut state = use_context::<Signal<AppState>>();
    let api = use_context::<ApiClient>();

    let health = use_resource({
        let api = api.clone();
        move || {
            let api = api.clone();
            async move { api.get_health().await.is_ok() }
        }
    });

    let gateway_text = match &*health.read() {
        Some(true) => "reachable",
        Some(false) => "unreachable",
        None => "checking...",
    };

    let last_run_text = match &state.read().last_run {
        Some(r) => format!("{} ({} symbols)", r.status, r.final_watchlist.len()),
        None => "none yet".to_string(),
    };

    let run_disabled = state.read().active_run_id.is_some();

    rsx! {
        section {
            h2 { "Dashboard" }
            p { "Gateway: {gateway_text}" }
            p { "Last run: {last_run_text}" }
            button {
                disabled: run_disabled,
                onclick: move |_| {
                    let api = api.clone();
                    spawn(async move {
                        let token = state.read().token.clone().unwrap_or_default();
                        if let Ok(record) = api.trigger_run(&token).await {
                            state.write().active_run_id = Some(record.id);
                            state.write().screen = Screen::LiveRun;
                        }
                    });
                },
                "Run Screener"
            }
        }
    }
}
```

- [ ] **Step 3: Write the Filter Config screen (all phases, matching the spec's toggle+period requirement)**

`apps/screener-app/ui/src/screens/filter_config.rs`:

```rust
use dioxus::prelude::*;

use crate::api::ApiClient;
use crate::dto::FilterConfigDto;
use crate::state::AppState;

#[component]
pub fn FilterConfig() -> Element {
    let state = use_context::<Signal<AppState>>();
    let api = use_context::<ApiClient>();

    // `use_signal`/`use_resource`/`use_effect` must run unconditionally on
    // every render (Dioxus, like React, indexes hooks by call order) — so
    // `config`/`save_status` are created up front with a placeholder value
    // and populated from `loaded` via an effect, rather than being created
    // lazily inside a match arm once data arrives.
    let mut config = use_signal(FilterConfigDto::default);
    let mut save_status = use_signal(String::new);
    let mut populated = use_signal(|| false);

    let loaded = use_resource({
        let api = api.clone();
        move || {
            let api = api.clone();
            let token = state.read().token.clone().unwrap_or_default();
            async move { api.get_filter_config(&token).await.ok() }
        }
    });

    use_effect(move || {
        if populated() {
            return;
        }
        if let Some(Some(fetched)) = &*loaded.read() {
            config.set(fetched.clone());
            populated.set(true);
        }
    });

    if loaded.read().is_none() {
        return rsx! { section { h2 { "Filter Config" } p { "Loading..." } } };
    }
    if matches!(&*loaded.read(), Some(None)) {
        return rsx! { section { h2 { "Filter Config" } p { "Failed to load." } } };
    }

    rsx! {
        section {
            h2 { "Filter Config" }

                    h3 { "Phase 1 (Daily)" }
                    label {
                        input {
                            r#type: "checkbox",
                            checked: config.read().phase1.enable_close_gt_sma10,
                            oninput: move |evt| config.write().phase1.enable_close_gt_sma10 = evt.checked(),
                        }
                        "Close > SMA10"
                    }
                    label {
                        input {
                            r#type: "checkbox",
                            checked: config.read().phase1.enable_sma10_gt_sma20,
                            oninput: move |evt| config.write().phase1.enable_sma10_gt_sma20 = evt.checked(),
                        }
                        "SMA10 > SMA20"
                    }
                    label {
                        input {
                            r#type: "checkbox",
                            checked: config.read().phase1.enable_sma20_gt_sma50,
                            oninput: move |evt| config.write().phase1.enable_sma20_gt_sma50 = evt.checked(),
                        }
                        "SMA20 > SMA50"
                    }
                    label {
                        input {
                            r#type: "checkbox",
                            checked: config.read().phase1.enable_sma50_gt_sma200,
                            oninput: move |evt| config.write().phase1.enable_sma50_gt_sma200 = evt.checked(),
                        }
                        "SMA50 > SMA200"
                    }
                    label {
                        input {
                            r#type: "checkbox",
                            checked: config.read().phase1.enable_sma200_slope,
                            oninput: move |evt| config.write().phase1.enable_sma200_slope = evt.checked(),
                        }
                        "SMA200 slope positive"
                    }
                    label {
                        "SMA10 period "
                        input {
                            r#type: "number",
                            value: "{config.read().phase1.sma10_period}",
                            oninput: move |evt| if let Ok(v) = evt.value().parse() { config.write().phase1.sma10_period = v },
                        }
                    }
                    label {
                        "SMA20 period "
                        input {
                            r#type: "number",
                            value: "{config.read().phase1.sma20_period}",
                            oninput: move |evt| if let Ok(v) = evt.value().parse() { config.write().phase1.sma20_period = v },
                        }
                    }
                    label {
                        "SMA50 period "
                        input {
                            r#type: "number",
                            value: "{config.read().phase1.sma50_period}",
                            oninput: move |evt| if let Ok(v) = evt.value().parse() { config.write().phase1.sma50_period = v },
                        }
                    }
                    label {
                        "SMA200 period "
                        input {
                            r#type: "number",
                            value: "{config.read().phase1.sma200_period}",
                            oninput: move |evt| if let Ok(v) = evt.value().parse() { config.write().phase1.sma200_period = v },
                        }
                    }
                    label {
                        "Slope lookback bars "
                        input {
                            r#type: "number",
                            value: "{config.read().phase1.slope_lookback_bars}",
                            oninput: move |evt| if let Ok(v) = evt.value().parse() { config.write().phase1.slope_lookback_bars = v },
                        }
                    }

                    h3 { "Phase 2 (30-Minute)" }
                    label {
                        input {
                            r#type: "checkbox",
                            checked: config.read().phase2.enable_close_gt_sma7,
                            oninput: move |evt| config.write().phase2.enable_close_gt_sma7 = evt.checked(),
                        }
                        "Close > SMA7"
                    }
                    label {
                        input {
                            r#type: "checkbox",
                            checked: config.read().phase2.enable_sma7_gt_sma17,
                            oninput: move |evt| config.write().phase2.enable_sma7_gt_sma17 = evt.checked(),
                        }
                        "SMA7 > SMA17"
                    }
                    label {
                        input {
                            r#type: "checkbox",
                            checked: config.read().phase2.enable_sma17_gt_sma33,
                            oninput: move |evt| config.write().phase2.enable_sma17_gt_sma33 = evt.checked(),
                        }
                        "SMA17 > SMA33"
                    }
                    label {
                        input {
                            r#type: "checkbox",
                            checked: config.read().phase2.enable_sma33_gt_sma65,
                            oninput: move |evt| config.write().phase2.enable_sma33_gt_sma65 = evt.checked(),
                        }
                        "SMA33 > SMA65"
                    }
                    label {
                        input {
                            r#type: "checkbox",
                            checked: config.read().phase2.enable_sma65_slope,
                            oninput: move |evt| config.write().phase2.enable_sma65_slope = evt.checked(),
                        }
                        "SMA65 slope positive"
                    }
                    label {
                        "SMA7 period "
                        input {
                            r#type: "number",
                            value: "{config.read().phase2.sma7_period}",
                            oninput: move |evt| if let Ok(v) = evt.value().parse() { config.write().phase2.sma7_period = v },
                        }
                    }
                    label {
                        "SMA17 period "
                        input {
                            r#type: "number",
                            value: "{config.read().phase2.sma17_period}",
                            oninput: move |evt| if let Ok(v) = evt.value().parse() { config.write().phase2.sma17_period = v },
                        }
                    }
                    label {
                        "SMA33 period "
                        input {
                            r#type: "number",
                            value: "{config.read().phase2.sma33_period}",
                            oninput: move |evt| if let Ok(v) = evt.value().parse() { config.write().phase2.sma33_period = v },
                        }
                    }
                    label {
                        "SMA65 period "
                        input {
                            r#type: "number",
                            value: "{config.read().phase2.sma65_period}",
                            oninput: move |evt| if let Ok(v) = evt.value().parse() { config.write().phase2.sma65_period = v },
                        }
                    }
                    label {
                        "Slope lookback bars "
                        input {
                            r#type: "number",
                            value: "{config.read().phase2.slope_lookback_bars}",
                            oninput: move |evt| if let Ok(v) = evt.value().parse() { config.write().phase2.slope_lookback_bars = v },
                        }
                    }

                    h3 { "Phase 3 (2-Minute + VWAP)" }
                    label {
                        input {
                            r#type: "checkbox",
                            checked: config.read().phase3.enable_close_gt_sma20,
                            oninput: move |evt| config.write().phase3.enable_close_gt_sma20 = evt.checked(),
                        }
                        "Close > SMA20"
                    }
                    label {
                        input {
                            r#type: "checkbox",
                            checked: config.read().phase3.enable_sma20_gt_sma50,
                            oninput: move |evt| config.write().phase3.enable_sma20_gt_sma50 = evt.checked(),
                        }
                        "SMA20 > SMA50"
                    }
                    label {
                        input {
                            r#type: "checkbox",
                            checked: config.read().phase3.enable_sma50_gt_sma100,
                            oninput: move |evt| config.write().phase3.enable_sma50_gt_sma100 = evt.checked(),
                        }
                        "SMA50 > SMA100"
                    }
                    label {
                        input {
                            r#type: "checkbox",
                            checked: config.read().phase3.enable_vwap,
                            oninput: move |evt| config.write().phase3.enable_vwap = evt.checked(),
                        }
                        "Close > VWAP"
                    }
                    label {
                        "SMA20 period "
                        input {
                            r#type: "number",
                            value: "{config.read().phase3.sma20_period}",
                            oninput: move |evt| if let Ok(v) = evt.value().parse() { config.write().phase3.sma20_period = v },
                        }
                    }
                    label {
                        "SMA50 period "
                        input {
                            r#type: "number",
                            value: "{config.read().phase3.sma50_period}",
                            oninput: move |evt| if let Ok(v) = evt.value().parse() { config.write().phase3.sma50_period = v },
                        }
                    }
                    label {
                        "SMA100 period "
                        input {
                            r#type: "number",
                            value: "{config.read().phase3.sma100_period}",
                            oninput: move |evt| if let Ok(v) = evt.value().parse() { config.write().phase3.sma100_period = v },
                        }
                    }

                    div {
                        button {
                            onclick: move |_| config.set(FilterConfigDto::default()),
                            "Reset to defaults"
                        }
                        button {
                            onclick: move |_| {
                                let api = api.clone();
                                let token = state.read().token.clone().unwrap_or_default();
                                let payload = config.read().clone();
                                spawn(async move {
                                    if api.put_filter_config(&token, &payload).await.is_ok() {
                                        save_status.set("saved".to_string());
                                    } else {
                                        save_status.set("save failed".to_string());
                                    }
                                });
                            },
                            "Save"
                        }
                        span { "{save_status}" }
                    }
                }
            }
}
```

- [ ] **Step 4: Wire the module in and verify both build targets**

Add `mod screens;` to `apps/screener-app/ui/src/main.rs`.

```bash
cd /home/vijay/Study/Screener/apps/screener-app/ui
cargo build
dx build --platform web
```

Fix any Dioxus macro errors that surface (rsx! attribute/child syntax can be picky about trailing commas and expression braces) — this build is the correctness gate for this task since there is no unit-testable logic here.

- [ ] **Step 5: Commit**

```bash
cd /home/vijay/Study/Screener
git add apps/screener-app/ui/src/screens apps/screener-app/ui/src/main.rs
git commit -m "Add Dashboard and full per-phase Filter Config screens"
```

---

### Task 8: Live Run, Watchlist, and History screens

**Files:**
- Create: `apps/screener-app/ui/src/screens/live_run.rs`
- Create: `apps/screener-app/ui/src/screens/watchlist.rs`
- Create: `apps/screener-app/ui/src/screens/history.rs`
- Modify: `apps/screener-app/ui/src/screens/mod.rs`

**Interfaces:**
- Consumes: `crate::ws::{connect_run_stream, StreamHandlers}`, `crate::dto::ProgressEvent`, `crate::api::ApiClient`, `crate::state::{AppState, RunSummary}`.
- Produces: `#[component] pub fn LiveRun() -> Element`, `#[component] pub fn Watchlist() -> Element`, `#[component] pub fn History() -> Element`.

Same verification approach as Task 7: `cargo build` + `dx build --platform web` is the correctness gate.

- [ ] **Step 1: Write the Live Run screen**

`apps/screener-app/ui/src/screens/live_run.rs`:

```rust
use std::collections::BTreeMap;

use dioxus::prelude::*;

use crate::dto::ProgressEvent;
use crate::state::{AppState, RunSummary, Screen};
use crate::ws::{connect_run_stream, StreamHandlers};

fn phase_label(phase: &str) -> &str {
    match phase {
        "Phase1" => "Daily",
        "Phase2" => "30-Minute",
        "Phase3" => "2-Minute",
        other => other,
    }
}

const API_BASE_URL: &str = "http://10.0.2.2:8080";

#[component]
pub fn LiveRun() -> Element {
    let mut state = use_context::<Signal<AppState>>();
    let mut rows: Signal<BTreeMap<String, String>> = use_signal(BTreeMap::new);
    // Guards against reopening the WebSocket on every unrelated write to
    // the shared `Signal<AppState>` (it's reactive at whole-struct
    // granularity, and this effect's own handlers write back into it) —
    // without this, any state change elsewhere in the app while this
    // screen is mounted would retrigger the effect and open a duplicate
    // connection.
    let mut connected = use_signal(|| false);

    let run_id = state.read().active_run_id.clone();

    use_effect(move || {
        if connected() {
            return;
        }
        let Some(run_id) = state.read().active_run_id.clone() else { return };
        connected.set(true);
        let token = state.read().token.clone().unwrap_or_default();

        connect_run_stream(
            API_BASE_URL,
            &token,
            &run_id,
            StreamHandlers {
                on_event: Box::new(move |event| match event {
                    ProgressEvent::Phase1 { total, started, passed, technical_failures, errors }
                    | ProgressEvent::Phase2 { total, started, passed, technical_failures, errors }
                    | ProgressEvent::Phase3 { total, started, passed, technical_failures, errors } => {
                        let phase = match &event {
                            ProgressEvent::Phase1 { .. } => "Phase1",
                            ProgressEvent::Phase2 { .. } => "Phase2",
                            _ => "Phase3",
                        };
                        rows.write().insert(
                            phase.to_string(),
                            format!(
                                "{}: {started}/{total} started, {passed} passed, {technical_failures} failed, {errors} errors",
                                phase_label(phase)
                            ),
                        );
                    }
                    ProgressEvent::Complete { final_watchlist } => {
                        state.write().active_run_id = None;
                        state.write().last_run = Some(RunSummary { status: "completed".to_string(), final_watchlist });
                        state.write().screen = Screen::Watchlist;
                    }
                    ProgressEvent::Failed { .. } => {
                        state.write().active_run_id = None;
                        state.write().last_run = Some(RunSummary { status: "failed".to_string(), final_watchlist: vec![] });
                    }
                }),
                on_close: Box::new(|| {}),
            },
        );
    });

    if run_id.is_none() {
        return rsx! { section { h2 { "Live Run" } p { "No run in progress." } } };
    }

    rsx! {
        section {
            h2 { "Live Run" }
            div {
                for (_, text) in rows.read().iter() {
                    p { "{text}" }
                }
            }
        }
    }
}
```

- [ ] **Step 2: Write the Watchlist screen**

`apps/screener-app/ui/src/screens/watchlist.rs`:

```rust
use dioxus::prelude::*;

use crate::state::AppState;

#[component]
pub fn Watchlist() -> Element {
    let state = use_context::<Signal<AppState>>();
    let symbols = state.read().last_run.as_ref().map(|r| r.final_watchlist.clone()).unwrap_or_default();

    rsx! {
        section {
            h2 { "Watchlist" }
            if symbols.is_empty() {
                p { "No results yet." }
            } else {
                ul {
                    for symbol in symbols {
                        li { "{symbol}" }
                    }
                }
            }
        }
    }
}
```

- [ ] **Step 3: Write the History screen**

`apps/screener-app/ui/src/screens/history.rs`:

```rust
use dioxus::prelude::*;

use crate::api::ApiClient;
use crate::state::AppState;

#[component]
pub fn History() -> Element {
    let state = use_context::<Signal<AppState>>();
    let api = use_context::<ApiClient>();

    let runs = use_resource({
        let api = api.clone();
        move || {
            let api = api.clone();
            let token = state.read().token.clone().unwrap_or_default();
            async move { api.list_runs(&token).await.unwrap_or_default() }
        }
    });

    rsx! {
        section {
            h2 { "History" }
            match &*runs.read() {
                None => rsx! { p { "Loading..." } },
                Some(runs) if runs.is_empty() => rsx! { p { "No runs yet." } },
                Some(runs) => rsx! {
                    ul {
                        for run in runs.iter() {
                            li { "{run.started_at} — {run.status:?} — {run.final_watchlist.len()} symbols" }
                        }
                    }
                },
            }
        }
    }
}
```

- [ ] **Step 4: Register the new screens**

`apps/screener-app/ui/src/screens/mod.rs`:

```rust
pub mod dashboard;
pub mod filter_config;
pub mod history;
pub mod live_run;
pub mod watchlist;
```

- [ ] **Step 5: Verify both build targets**

```bash
cd /home/vijay/Study/Screener/apps/screener-app/ui
cargo build
dx build --platform web
```

Fix any Dioxus macro errors that surface.

- [ ] **Step 6: Commit**

```bash
cd /home/vijay/Study/Screener
git add apps/screener-app/ui/src/screens
git commit -m "Add Live Run, Watchlist, and History screens"
```

---

### Task 9: App shell and final verification

**Files:**
- Modify: `apps/screener-app/ui/src/main.rs`

**Interfaces:**
- Consumes: every module from Tasks 2–8.
- Produces: the real `App` component — nav across all 5 screens, providing `Signal<AppState>` and `ApiClient` via context, an init effect that loads the token from `LocalTokenStorage` and (mirroring the JS predecessor's resilience behavior) re-checks an in-progress run's status if one was already active.

- [ ] **Step 1: Replace the placeholder `App` with the real app shell**

`apps/screener-app/ui/src/main.rs`:

```rust
use dioxus::prelude::*;

mod api;
mod dto;
mod screens;
mod state;
mod token_storage;
mod ws;

use api::ApiClient;
use screens::dashboard::Dashboard;
use screens::filter_config::FilterConfig;
use screens::history::History;
use screens::live_run::LiveRun;
use screens::watchlist::Watchlist;
use state::{AppState, Screen};
use token_storage::{LocalTokenStorage, TokenStorage};

const API_BASE_URL: &str = "http://10.0.2.2:8080";

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let mut state = use_context_provider(|| Signal::new(AppState::default()));
    let api = use_context_provider(|| ApiClient::new(API_BASE_URL));

    // Two separate effects, deliberately: `Signal<AppState>` is reactive at
    // whole-struct granularity, so an effect that both reads and writes it
    // would retrigger itself on every write. This one only ever writes
    // `state` and never reads it, so it runs once on mount and never
    // retriggers from its own write.
    use_effect(move || {
        if let Some(token) = LocalTokenStorage.get() {
            state.write().token = Some(token);
        }
    });

    // This one reads `state` (so it reruns whenever any field of it
    // changes, including its own write below) but guards the write with a
    // check against the value it's about to set — after the first run
    // sets `screen`, the guard makes every subsequent rerun a no-op, so
    // the self-triggering chain converges instead of looping forever.
    use_effect(move || {
        let Some(run_id) = state.read().active_run_id.clone() else { return };
        if state.read().screen == Screen::LiveRun {
            return;
        }
        let token = state.read().token.clone().unwrap_or_default();
        let api = api.clone();
        spawn(async move {
            if let Ok(record) = api.get_run(&token, &run_id).await {
                if record.status == dto::RunStatus::Running {
                    state.write().screen = Screen::LiveRun;
                }
            }
        });
    });

    rsx! {
        nav {
            button { onclick: move |_| state.write().screen = Screen::Dashboard, "dashboard" }
            button { onclick: move |_| state.write().screen = Screen::FilterConfig, "filter-config" }
            button { onclick: move |_| state.write().screen = Screen::LiveRun, "live-run" }
            button { onclick: move |_| state.write().screen = Screen::Watchlist, "watchlist" }
            button { onclick: move |_| state.write().screen = Screen::History, "history" }
        }
        main {
            match state.read().screen {
                Screen::Dashboard => rsx! { Dashboard {} },
                Screen::FilterConfig => rsx! { FilterConfig {} },
                Screen::LiveRun => rsx! { LiveRun {} },
                Screen::Watchlist => rsx! { Watchlist {} },
                Screen::History => rsx! { History {} },
            }
        }
    }
}
```

- [ ] **Step 2: Verify both build targets**

```bash
cd /home/vijay/Study/Screener/apps/screener-app/ui
cargo build
dx build --platform web
```

Fix any Dioxus macro errors (e.g. `RunStatus` needs `PartialEq` — already derived in Task 2 — for the `== dto::RunStatus::Running` comparison to compile).

- [ ] **Step 3: Run the full `ui` test suite**

```bash
cd /home/vijay/Study/Screener/apps/screener-app/ui
cargo test 2>&1 | tail -30
```

Expected: all tests from Tasks 2, 3, 4, 5, 6 pass (dto: 6, token_storage: 3, api: 7, ws: 3, state: 1 — 20 total).

- [ ] **Step 4: Real, visual, end-to-end verification — the actual regression test for the original white-screen bug**

```bash
pkill -f target/debug/screener-app 2>/dev/null
source "$HOME/.cargo/env"
cd /home/vijay/Study/Screener/apps/screener-app
export DISPLAY=:20
nohup cargo tauri dev > /tmp/tauri-final-verify.log 2>&1 &
```

Wait for the build to finish (watch the log for `Running` / `target/debug/screener-app`), then screenshot exactly as in Task 1 Step 5 (raise/activate the window first — this X server has no compositor):

```bash
sleep 20
WID=$(DISPLAY=:20 xdotool search --name "screener-app" | head -1)
DISPLAY=:20 xdotool windowactivate "$WID"; DISPLAY=:20 xdotool windowraise "$WID"
sleep 1
DISPLAY=:20 xwd -root -silent -out /tmp/final-verify.xwd
convert /tmp/final-verify.xwd /tmp/final-verify.png
```

Read `/tmp/final-verify.png` and confirm it shows the real nav bar (dashboard/filter-config/live-run/watchlist/history buttons) and the Dashboard screen's "Gateway: unreachable" (no `screener-service` is running in this check) — not a blank white window.

- [ ] **Step 5: Verify the Android build pipeline still works end-to-end with the new frontend**

```bash
source "$HOME/.cargo/env" && source /home/vijay/.android-env
cd /home/vijay/Study/Screener/apps/screener-app
cargo tauri android build --debug --target aarch64 2>&1 | tail -40
```

Expected: succeeds, producing an APK, exactly as it did with the JS frontend before this rewrite — proving the Dioxus swap didn't break the already-verified Android pipeline. If disk space runs out, `cargo clean` inside `apps/screener-app/src-tauri` first (Android cross-compile artifacts are large and fully regenerable).

- [ ] **Step 6: Commit**

```bash
cd /home/vijay/Study/Screener
git add apps/screener-app/ui/src/main.rs
git commit -m "Wire up the full Dioxus app shell; verify desktop render and Android build"
```
