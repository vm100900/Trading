# screener-service Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the `screener-service` HTTP+WebSocket API layer that wraps `screener-core`'s Phase 1/2/3 engine — bearer-token auth, filter-config CRUD, run triggering with concurrency guarding, live progress streaming, and run history — fully testable in this environment via in-memory persistence and a fake screening engine, with the real Yahoo/IBKR wiring included but compile-verified only.

**Architecture:** Axum service with a `ScreeningEngine` trait abstracting "the thing that actually runs Phase 1→2→3" (mirroring `screener-core`'s `DailyDataSource`/`IntradayDataSource` pattern), so the API layer — routing, auth, concurrency guarding, progress broadcasting, run history — can be built and fully tested against a `FakeScreeningEngine` test double without any live Yahoo/IBKR access. `FilterConfigStore` and `RunStore` traits similarly abstract persistence; this plan implements only in-memory versions (Firestore-backed versions are a separate future plan). A `RealScreeningEngine` wiring actual `run_phase1`/`run_phase2`/`run_phase3` with `YahooClient`/`IbkrClient` is included and must compile, but — like `IbkrClient` itself — cannot be behavior-tested here without live services.

**Tech Stack:** `axum` 0.8, `tokio`, `tower` (for `oneshot` testing), `tokio-tungstenite` (dev-dependency, for a real WebSocket integration test over an ephemeral TCP port), `uuid`, `serde`/`serde_json`, `chrono`, `thiserror`, `async-trait`. Every Axum API used below (path-parameter syntax, middleware, WebSocket upgrade, `axum::serve`) was verified against the real 0.8.9 docs and the framework's own examples at plan-writing time — not recalled from memory. In particular: **Axum 0.8 uses `{id}` path-parameter syntax, not the old `:id` syntax** — using `:id` panics at startup unless `.without_v07_checks()` is called.

**Spec:** `/home/vijay/Study/Screener/docs/superpowers/specs/2026-08-23-rust-screener-design.md` (section 5: API surface, persistence, auth, run flow; section 5.6 error handling)

**Builds on:** `screener-core`, fully implemented and committed on `master` in `/home/vijay/Study/Screener/crates/screener-core` (Phase 1/2/3 complete — `run_phase1`/`run_phase2`/`run_phase3`, `Phase1Config`/`Phase2Config`/`Phase3Config`, `Phase1Progress`/`Phase2Progress`/`Phase3Progress`, `DailyDataSource`/`IntradayDataSource`, `data::yahoo::YahooClient`, `data::ibkr::IbkrClient`).

## Explicitly deferred (not in this plan)

- **Firestore persistence** — `FilterConfigStore`/`RunStore` are traits specifically so a Firestore-backed implementation can be added later without touching the API layer. This plan only implements in-memory versions.
- **`/universe` and `POST /universe/refresh`** — these depend on a Wikipedia/Nasdaq-Trader scraper that doesn't exist yet anywhere in this codebase. Building it is a separate future plan. `RealScreeningEngine` in this plan takes a static universe list at construction time as a placeholder for that.
- **Real Gateway health in `/health`** — reporting actual `IbkrClient` connection status requires a "is connected" capability `IbkrClient` doesn't currently expose. This plan's `/health` reports service-up status only.
- **Dockerfile / IBC headless-Gateway automation / Cloud Run deployment config** — infrastructure, not Rust code; a separate future plan once this API foundation exists.
- **Real secret management for the bearer token** — this plan reads it from a config value at construction; wiring it to a real secret store is deployment-plan territory.

## Global Constraints

- `POST /runs` must reject a new run while one is already active — enforced atomically in `RunStore`, not just checked-then-set at the call site (a race there would let two runs start concurrently).
- Only Phase 1 survivors reach Phase 2, only Phase 2 survivors reach Phase 3 — `RealScreeningEngine` must pass each phase's survivor list as the next phase's universe, never the full universe.
- Progress and run-state types exposed over the API are service-level DTOs, not `screener-core`'s internal types directly — keeps `screener-core` free of API/serialization concerns (it has no `serde` dependency on its config/progress types, and this plan doesn't add one).
- All protected routes (`/filter-config`, `/runs`, `/runs/{id}`, `/runs/{id}/stream`) require a valid bearer token; `/health` does not.
- `FilterConfigDto`'s defaults must exactly match `screener-core`'s `Phase1Config::default()`/`Phase2Config::default()`/`Phase3Config::default()` — no drift between the API's defaults and the engine's locked defaults.

---

## File Structure

```text
crates/screener-service/
├── Cargo.toml
├── src/
│   ├── main.rs           # binary entry point: builds AppState with real dependencies, serves
│   ├── lib.rs             # AppState, build_router() — used by both main.rs and tests
│   ├── error.rs            # ServiceError + IntoResponse impl
│   ├── auth.rs              # bearer-token middleware
│   ├── config.rs             # FilterConfigDto + Phase{1,2,3}ConfigDto, FilterConfigStore trait + in-memory impl
│   ├── runs.rs                 # RunRecord, RunStatus, RunStore trait + in-memory impl
│   ├── engine.rs                # ScreeningEngine trait, ProgressEvent, RealScreeningEngine
│   └── routes.rs                 # handlers (filter-config, runs, WS stream, health) + router assembly
└── tests/
    ├── common/mod.rs               # FakeScreeningEngine
    └── api_test.rs                  # end-to-end integration tests (oneshot + real WS client)
```

---

### Task 1: Crate scaffold, AppState, ServiceError, health endpoint

**Files:**
- Create: `crates/screener-service/Cargo.toml`
- Create: `crates/screener-service/src/lib.rs`
- Create: `crates/screener-service/src/error.rs`
- Modify: `/home/vijay/Study/Screener/Cargo.toml` (workspace root)

**Interfaces:**
- Produces: `pub enum ServiceError { RunAlreadyInProgress, RunNotFound, Unauthorized }` implementing `IntoResponse`; `pub struct AppState { .. }` (placeholder fields added by later tasks); `pub fn build_router(state: AppState) -> axum::Router` with a `GET /health` route; `async fn health() -> impl IntoResponse`

- [ ] **Step 1: Add the crate to the workspace**

Modify `/home/vijay/Study/Screener/Cargo.toml`:
```toml
[workspace]
resolver = "2"
members = ["crates/screener-core", "crates/screener-service"]
```

- [ ] **Step 2: Create the crate manifest**

`crates/screener-service/Cargo.toml`:
```toml
[package]
name = "screener-service"
version = "0.1.0"
edition = "2021"

[dependencies]
screener-core = { path = "../screener-core" }
axum = { version = "0.8", features = ["ws"] }
tokio = { version = "1", features = ["rt-multi-thread", "macros", "sync", "time", "net"] }
tower = "0.5"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
async-trait = "0.1"
thiserror = "1"
tracing = "0.1"
tracing-subscriber = "0.3"
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }

[dev-dependencies]
http-body-util = "0.1"
tokio-tungstenite = "0.30"
futures-util = "0.3"
```

- [ ] **Step 3: Write the failing test for `ServiceError`**

`crates/screener-service/src/error.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use axum::response::IntoResponse;
    use axum::http::StatusCode;

    #[tokio::test]
    async fn run_already_in_progress_maps_to_409() {
        let response = ServiceError::RunAlreadyInProgress.into_response();
        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn run_not_found_maps_to_404() {
        let response = ServiceError::RunNotFound.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn unauthorized_maps_to_401() {
        let response = ServiceError::Unauthorized.into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
```

- [ ] **Step 4: Run the test to verify it fails**

Run: `cargo test -p screener-service`
Expected: FAIL to compile — `ServiceError` not defined.

- [ ] **Step 5: Implement `ServiceError`**

Add above the `#[cfg(test)]` module in `crates/screener-service/src/error.rs`:
```rust
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

#[derive(Debug, Clone, thiserror::Error)]
pub enum ServiceError {
    #[error("a run is already in progress")]
    RunAlreadyInProgress,
    #[error("run not found")]
    RunNotFound,
    #[error("unauthorized")]
    Unauthorized,
}

impl IntoResponse for ServiceError {
    fn into_response(self) -> Response {
        let status = match self {
            ServiceError::RunAlreadyInProgress => StatusCode::CONFLICT,
            ServiceError::RunNotFound => StatusCode::NOT_FOUND,
            ServiceError::Unauthorized => StatusCode::UNAUTHORIZED,
        };
        (status, Json(json!({ "error": self.to_string() }))).into_response()
    }
}
```

- [ ] **Step 6: Write the failing test for the health route**

`crates/screener-service/src/lib.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    #[tokio::test]
    async fn health_route_returns_200() {
        let state = AppState { auth_token: "test-token".to_string() };
        let app = build_router(state);

        let response = app
            .oneshot(Request::get("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}
```

- [ ] **Step 7: Run the test to verify it fails**

Run: `cargo test -p screener-service`
Expected: FAIL to compile — `AppState`/`build_router` not defined.

- [ ] **Step 8: Implement `AppState`, `build_router`, and the health handler**

Add above the `#[cfg(test)]` module in `crates/screener-service/src/lib.rs`:
```rust
pub mod error;

use axum::http::StatusCode;
use axum::routing::get;
use axum::Router;

#[derive(Clone)]
pub struct AppState {
    pub auth_token: String,
}

async fn health() -> StatusCode {
    StatusCode::OK
}

pub fn build_router(state: AppState) -> Router {
    Router::new().route("/health", get(health)).with_state(state)
}
```

- [ ] **Step 9: Run the tests to verify they pass**

Run: `cargo test -p screener-service`
Expected: PASS (4 tests)

- [ ] **Step 10: Commit**

```bash
git add Cargo.toml crates/screener-service
git commit -m "feat(screener-service): scaffold crate, ServiceError, AppState, health endpoint"
```

---

### Task 2: Bearer-token auth middleware

**Files:**
- Create: `crates/screener-service/src/auth.rs`
- Modify: `crates/screener-service/src/lib.rs`

**Interfaces:**
- Consumes: `AppState` (lib)
- Produces: `pub async fn require_bearer_token(State(state): State<AppState>, request: Request, next: Next) -> Result<Response, StatusCode>`

- [ ] **Step 1: Write the failing test**

`crates/screener-service/src/auth.rs`:
```rust
#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::middleware;
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    use crate::auth::require_bearer_token;
    use crate::AppState;

    fn protected_app() -> Router {
        let state = AppState { auth_token: "correct-token".to_string() };
        Router::new()
            .route("/protected", get(|| async { "ok" }))
            .route_layer(middleware::from_fn_with_state(state.clone(), require_bearer_token))
            .with_state(state)
    }

    #[tokio::test]
    async fn missing_authorization_header_is_rejected() {
        let response = protected_app()
            .oneshot(Request::get("/protected").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn wrong_token_is_rejected() {
        let response = protected_app()
            .oneshot(
                Request::get("/protected")
                    .header("authorization", "Bearer wrong-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn correct_token_is_accepted() {
        let response = protected_app()
            .oneshot(
                Request::get("/protected")
                    .header("authorization", "Bearer correct-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p screener-service auth::tests`
Expected: FAIL to compile — `require_bearer_token` not defined.

- [ ] **Step 3: Implement the middleware**

Add above the `#[cfg(test)]` module in `crates/screener-service/src/auth.rs`:
```rust
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::Response;

use crate::AppState;

pub async fn require_bearer_token(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let provided = request
        .headers()
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "));

    match provided {
        Some(token) if token == state.auth_token => Ok(next.run(request).await),
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p screener-service auth::tests`
Expected: PASS (3 tests)

- [ ] **Step 5: Wire up the module**

Modify `crates/screener-service/src/lib.rs`, add:
```rust
pub mod auth;
```

- [ ] **Step 6: Commit**

```bash
git add crates/screener-service/src/auth.rs crates/screener-service/src/lib.rs
git commit -m "feat(screener-service): add bearer-token auth middleware"
```

---

### Task 3: FilterConfig DTOs + store trait + endpoints

**Files:**
- Create: `crates/screener-service/src/config.rs`
- Modify: `crates/screener-service/src/lib.rs`

**Interfaces:**
- Consumes: `screener_core::{Phase1Config, Phase2Config, Phase3Config}`, `AppState` (lib)
- Produces: `pub struct FilterConfigDto { pub phase1: Phase1ConfigDto, pub phase2: Phase2ConfigDto, pub phase3: Phase3ConfigDto }` (`Serialize + Deserialize + Clone + PartialEq + Default`) with `From`/`Into` conversions to/from `screener_core`'s config types; `#[async_trait] pub trait FilterConfigStore: Send + Sync { async fn get(&self) -> FilterConfigDto; async fn set(&self, config: FilterConfigDto); }`; `pub struct InMemoryFilterConfigStore { .. }` implementing it; `pub async fn get_filter_config(..) -> Json<FilterConfigDto>`, `pub async fn put_filter_config(..) -> Json<FilterConfigDto>`

- [ ] **Step 1: Write the failing tests**

`crates/screener-service/src/config.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_config_dto_default_matches_screener_core_defaults() {
        let dto = FilterConfigDto::default();
        assert_eq!(dto.phase1.sma200_period, 200);
        assert_eq!(dto.phase1.slope_lookback_bars, 5);
        assert_eq!(dto.phase2.sma65_period, 65);
        assert_eq!(dto.phase2.slope_lookback_bars, 1);
        assert_eq!(dto.phase3.sma100_period, 100);
        assert!(dto.phase3.enable_vwap);
    }

    #[test]
    fn round_trips_through_screener_core_types() {
        let dto = FilterConfigDto::default();
        let phase1: screener_core::Phase1Config = dto.phase1.clone().into();
        assert_eq!(phase1, screener_core::Phase1Config::default());
        let back: Phase1ConfigDto = phase1.into();
        assert_eq!(back, dto.phase1);
    }

    #[tokio::test]
    async fn in_memory_store_returns_default_then_reflects_updates() {
        let store = InMemoryFilterConfigStore::new();
        assert_eq!(store.get().await, FilterConfigDto::default());

        let mut updated = FilterConfigDto::default();
        updated.phase1.enable_sma200_slope = false;
        store.set(updated.clone()).await;

        assert_eq!(store.get().await, updated);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p screener-service config::tests`
Expected: FAIL to compile — `FilterConfigDto`/`InMemoryFilterConfigStore` not defined.

- [ ] **Step 3: Implement the DTOs, conversions, and store**

Add above the `#[cfg(test)]` module in `crates/screener-service/src/config.rs`:
```rust
use async_trait::async_trait;
use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::AppState;

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

impl From<screener_core::Phase1Config> for Phase1ConfigDto {
    fn from(c: screener_core::Phase1Config) -> Self {
        Self {
            sma10_period: c.sma10_period,
            sma20_period: c.sma20_period,
            sma50_period: c.sma50_period,
            sma200_period: c.sma200_period,
            slope_lookback_bars: c.slope_lookback_bars,
            enable_close_gt_sma10: c.enable_close_gt_sma10,
            enable_sma10_gt_sma20: c.enable_sma10_gt_sma20,
            enable_sma20_gt_sma50: c.enable_sma20_gt_sma50,
            enable_sma50_gt_sma200: c.enable_sma50_gt_sma200,
            enable_sma200_slope: c.enable_sma200_slope,
        }
    }
}

impl From<Phase1ConfigDto> for screener_core::Phase1Config {
    fn from(d: Phase1ConfigDto) -> Self {
        Self {
            sma10_period: d.sma10_period,
            sma20_period: d.sma20_period,
            sma50_period: d.sma50_period,
            sma200_period: d.sma200_period,
            slope_lookback_bars: d.slope_lookback_bars,
            enable_close_gt_sma10: d.enable_close_gt_sma10,
            enable_sma10_gt_sma20: d.enable_sma10_gt_sma20,
            enable_sma20_gt_sma50: d.enable_sma20_gt_sma50,
            enable_sma50_gt_sma200: d.enable_sma50_gt_sma200,
            enable_sma200_slope: d.enable_sma200_slope,
        }
    }
}

impl Default for Phase1ConfigDto {
    fn default() -> Self {
        screener_core::Phase1Config::default().into()
    }
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

impl From<screener_core::Phase2Config> for Phase2ConfigDto {
    fn from(c: screener_core::Phase2Config) -> Self {
        Self {
            sma7_period: c.sma7_period,
            sma17_period: c.sma17_period,
            sma33_period: c.sma33_period,
            sma65_period: c.sma65_period,
            slope_lookback_bars: c.slope_lookback_bars,
            enable_close_gt_sma7: c.enable_close_gt_sma7,
            enable_sma7_gt_sma17: c.enable_sma7_gt_sma17,
            enable_sma17_gt_sma33: c.enable_sma17_gt_sma33,
            enable_sma33_gt_sma65: c.enable_sma33_gt_sma65,
            enable_sma65_slope: c.enable_sma65_slope,
        }
    }
}

impl From<Phase2ConfigDto> for screener_core::Phase2Config {
    fn from(d: Phase2ConfigDto) -> Self {
        Self {
            sma7_period: d.sma7_period,
            sma17_period: d.sma17_period,
            sma33_period: d.sma33_period,
            sma65_period: d.sma65_period,
            slope_lookback_bars: d.slope_lookback_bars,
            enable_close_gt_sma7: d.enable_close_gt_sma7,
            enable_sma7_gt_sma17: d.enable_sma7_gt_sma17,
            enable_sma17_gt_sma33: d.enable_sma17_gt_sma33,
            enable_sma33_gt_sma65: d.enable_sma33_gt_sma65,
            enable_sma65_slope: d.enable_sma65_slope,
        }
    }
}

impl Default for Phase2ConfigDto {
    fn default() -> Self {
        screener_core::Phase2Config::default().into()
    }
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

impl From<screener_core::Phase3Config> for Phase3ConfigDto {
    fn from(c: screener_core::Phase3Config) -> Self {
        Self {
            sma20_period: c.sma20_period,
            sma50_period: c.sma50_period,
            sma100_period: c.sma100_period,
            enable_close_gt_sma20: c.enable_close_gt_sma20,
            enable_sma20_gt_sma50: c.enable_sma20_gt_sma50,
            enable_sma50_gt_sma100: c.enable_sma50_gt_sma100,
            enable_vwap: c.enable_vwap,
        }
    }
}

impl From<Phase3ConfigDto> for screener_core::Phase3Config {
    fn from(d: Phase3ConfigDto) -> Self {
        Self {
            sma20_period: d.sma20_period,
            sma50_period: d.sma50_period,
            sma100_period: d.sma100_period,
            enable_close_gt_sma20: d.enable_close_gt_sma20,
            enable_sma20_gt_sma50: d.enable_sma20_gt_sma50,
            enable_sma50_gt_sma100: d.enable_sma50_gt_sma100,
            enable_vwap: d.enable_vwap,
        }
    }
}

impl Default for Phase3ConfigDto {
    fn default() -> Self {
        screener_core::Phase3Config::default().into()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct FilterConfigDto {
    pub phase1: Phase1ConfigDto,
    pub phase2: Phase2ConfigDto,
    pub phase3: Phase3ConfigDto,
}

#[async_trait]
pub trait FilterConfigStore: Send + Sync {
    async fn get(&self) -> FilterConfigDto;
    async fn set(&self, config: FilterConfigDto);
}

pub struct InMemoryFilterConfigStore {
    config: RwLock<FilterConfigDto>,
}

impl InMemoryFilterConfigStore {
    pub fn new() -> Self {
        Self {
            config: RwLock::new(FilterConfigDto::default()),
        }
    }
}

#[async_trait]
impl FilterConfigStore for InMemoryFilterConfigStore {
    async fn get(&self) -> FilterConfigDto {
        self.config.read().await.clone()
    }

    async fn set(&self, config: FilterConfigDto) {
        *self.config.write().await = config;
    }
}

pub async fn get_filter_config(State(state): State<AppState>) -> Json<FilterConfigDto> {
    Json(state.filter_config_store.get().await)
}

pub async fn put_filter_config(
    State(state): State<AppState>,
    Json(config): Json<FilterConfigDto>,
) -> Json<FilterConfigDto> {
    state.filter_config_store.set(config.clone()).await;
    Json(config)
}
```

- [ ] **Step 4: Add `filter_config_store` to `AppState`**

Modify `crates/screener-service/src/lib.rs`. Change:
```rust
#[derive(Clone)]
pub struct AppState {
    pub auth_token: String,
}
```
to:
```rust
#[derive(Clone)]
pub struct AppState {
    pub auth_token: String,
    pub filter_config_store: std::sync::Arc<dyn crate::config::FilterConfigStore>,
}
```

This breaks the existing tests in `lib.rs` and `auth.rs` (they construct `AppState` with only `auth_token`) — fix both call sites now:

In `crates/screener-service/src/lib.rs`'s test, change:
```rust
let state = AppState { auth_token: "test-token".to_string() };
```
to:
```rust
let state = AppState {
    auth_token: "test-token".to_string(),
    filter_config_store: std::sync::Arc::new(crate::config::InMemoryFilterConfigStore::new()),
};
```

In `crates/screener-service/src/auth.rs`'s `protected_app()` helper, apply the same change.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p screener-service`
Expected: PASS — all tests including the 3 new `config::tests` and the previously-passing `lib`/`auth` tests (now updated for the new `AppState` field).

- [ ] **Step 6: Wire up the module and routes**

Modify `crates/screener-service/src/lib.rs`, add:
```rust
pub mod config;
```

Modify `build_router` to include the filter-config routes behind auth:
```rust
pub fn build_router(state: AppState) -> Router {
    use axum::middleware;
    use axum::routing::{get as get_route};

    let protected = Router::new()
        .route(
            "/filter-config",
            get_route(config::get_filter_config).put(config::put_filter_config),
        )
        .route_layer(middleware::from_fn_with_state(state.clone(), auth::require_bearer_token));

    Router::new()
        .route("/health", get(health))
        .merge(protected)
        .with_state(state)
}
```

- [ ] **Step 7: Commit**

```bash
git add crates/screener-service/src/config.rs crates/screener-service/src/lib.rs crates/screener-service/src/auth.rs
git commit -m "feat(screener-service): add FilterConfig DTOs, in-memory store, and GET/PUT /filter-config"
```

---

### Task 4: RunStore

**Files:**
- Create: `crates/screener-service/src/runs.rs`
- Modify: `crates/screener-service/src/lib.rs`

**Interfaces:**
- Consumes: nothing new
- Produces: `pub enum RunStatus { Running, Completed, Failed }` (`Serialize`), `pub struct RunRecord { pub id: Uuid, pub status: RunStatus, pub started_at: DateTime<Utc>, pub finished_at: Option<DateTime<Utc>>, pub final_watchlist: Vec<String>, pub error_message: Option<String> }` (`Serialize + Clone`), `#[async_trait] pub trait RunStore: Send + Sync { async fn try_start_run(&self) -> Option<RunRecord>; async fn complete_run(&self, id: Uuid, final_watchlist: Vec<String>); async fn fail_run(&self, id: Uuid, message: String); async fn get_run(&self, id: Uuid) -> Option<RunRecord>; async fn list_runs(&self) -> Vec<RunRecord>; }`, `pub struct InMemoryRunStore { .. }` implementing it

- [ ] **Step 1: Write the failing tests**

`crates/screener-service/src/runs.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn try_start_run_succeeds_when_no_run_is_active() {
        let store = InMemoryRunStore::new();
        let record = store.try_start_run().await;
        assert!(record.is_some());
        assert_eq!(record.unwrap().status, RunStatus::Running);
    }

    #[tokio::test]
    async fn try_start_run_fails_while_one_is_already_active() {
        let store = InMemoryRunStore::new();
        let first = store.try_start_run().await;
        assert!(first.is_some());

        let second = store.try_start_run().await;
        assert!(second.is_none());
    }

    #[tokio::test]
    async fn completing_a_run_allows_a_new_one_to_start() {
        let store = InMemoryRunStore::new();
        let first = store.try_start_run().await.unwrap();
        store.complete_run(first.id, vec!["AAPL".to_string()]).await;

        let second = store.try_start_run().await;
        assert!(second.is_some());

        let completed = store.get_run(first.id).await.unwrap();
        assert_eq!(completed.status, RunStatus::Completed);
        assert_eq!(completed.final_watchlist, vec!["AAPL".to_string()]);
        assert!(completed.finished_at.is_some());
    }

    #[tokio::test]
    async fn failing_a_run_allows_a_new_one_to_start() {
        let store = InMemoryRunStore::new();
        let first = store.try_start_run().await.unwrap();
        store.fail_run(first.id, "boom".to_string()).await;

        let second = store.try_start_run().await;
        assert!(second.is_some());

        let failed = store.get_run(first.id).await.unwrap();
        assert_eq!(failed.status, RunStatus::Failed);
        assert_eq!(failed.error_message, Some("boom".to_string()));
    }

    #[tokio::test]
    async fn get_run_returns_none_for_unknown_id() {
        let store = InMemoryRunStore::new();
        assert!(store.get_run(uuid::Uuid::new_v4()).await.is_none());
    }

    #[tokio::test]
    async fn list_runs_returns_all_created_runs() {
        let store = InMemoryRunStore::new();
        let first = store.try_start_run().await.unwrap();
        store.complete_run(first.id, vec![]).await;
        store.try_start_run().await.unwrap();

        assert_eq!(store.list_runs().await.len(), 2);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p screener-service runs::tests`
Expected: FAIL to compile — `InMemoryRunStore`/`RunStatus` not defined.

- [ ] **Step 3: Implement `RunStore`**

Add above the `#[cfg(test)]` module in `crates/screener-service/src/runs.rs`:
```rust
use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunRecord {
    pub id: Uuid,
    pub status: RunStatus,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub final_watchlist: Vec<String>,
    pub error_message: Option<String>,
}

#[async_trait]
pub trait RunStore: Send + Sync {
    /// Atomically starts a new run iff none is currently active. Returns
    /// `None` without creating anything if a run is already running.
    async fn try_start_run(&self) -> Option<RunRecord>;
    async fn complete_run(&self, id: Uuid, final_watchlist: Vec<String>);
    async fn fail_run(&self, id: Uuid, message: String);
    async fn get_run(&self, id: Uuid) -> Option<RunRecord>;
    async fn list_runs(&self) -> Vec<RunRecord>;
}

#[derive(Default)]
pub struct InMemoryRunStore {
    runs: RwLock<HashMap<Uuid, RunRecord>>,
    active: RwLock<bool>,
}

impl InMemoryRunStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl RunStore for InMemoryRunStore {
    async fn try_start_run(&self) -> Option<RunRecord> {
        let mut active = self.active.write().await;
        if *active {
            return None;
        }
        *active = true;

        let record = RunRecord {
            id: Uuid::new_v4(),
            status: RunStatus::Running,
            started_at: Utc::now(),
            finished_at: None,
            final_watchlist: Vec::new(),
            error_message: None,
        };
        self.runs.write().await.insert(record.id, record.clone());
        Some(record)
    }

    async fn complete_run(&self, id: Uuid, final_watchlist: Vec<String>) {
        if let Some(record) = self.runs.write().await.get_mut(&id) {
            record.status = RunStatus::Completed;
            record.finished_at = Some(Utc::now());
            record.final_watchlist = final_watchlist;
        }
        *self.active.write().await = false;
    }

    async fn fail_run(&self, id: Uuid, message: String) {
        if let Some(record) = self.runs.write().await.get_mut(&id) {
            record.status = RunStatus::Failed;
            record.finished_at = Some(Utc::now());
            record.error_message = Some(message);
        }
        *self.active.write().await = false;
    }

    async fn get_run(&self, id: Uuid) -> Option<RunRecord> {
        self.runs.read().await.get(&id).cloned()
    }

    async fn list_runs(&self) -> Vec<RunRecord> {
        self.runs.read().await.values().cloned().collect()
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p screener-service runs::tests`
Expected: PASS (6 tests)

- [ ] **Step 5: Wire up the module**

Modify `crates/screener-service/src/lib.rs`, add:
```rust
pub mod runs;
```

- [ ] **Step 6: Commit**

```bash
git add crates/screener-service/src/runs.rs crates/screener-service/src/lib.rs
git commit -m "feat(screener-service): add RunStore with atomic single-run concurrency guard"
```

---

### Task 5: ScreeningEngine trait + run-triggering endpoints

**Files:**
- Create: `crates/screener-service/src/engine.rs`
- Create: `crates/screener-service/src/routes.rs`
- Create: `crates/screener-service/tests/common/mod.rs`
- Modify: `crates/screener-service/src/lib.rs`

**Interfaces:**
- Consumes: `FilterConfigDto` (config), `RunStore`/`RunRecord` (runs), `AppState` (lib)
- Produces: `#[derive(Serialize, Clone, Debug)] pub enum ProgressEvent { Phase1 {..}, Phase2 {..}, Phase3 {..}, Complete { final_watchlist: Vec<String> }, Failed { message: String } }`, `#[async_trait] pub trait ScreeningEngine: Send + Sync { async fn run(&self, config: FilterConfigDto, progress_tx: tokio::sync::broadcast::Sender<ProgressEvent>) -> Result<Vec<String>, String>; }`, `pub async fn trigger_run(..) -> Result<Json<RunRecord>, ServiceError>`, `pub async fn get_run(..) -> Result<Json<RunRecord>, ServiceError>`, `pub async fn list_runs(..) -> Json<Vec<RunRecord>>`; test-only `FakeScreeningEngine` in `tests/common`

- [ ] **Step 1: Write the failing test**

`crates/screener-service/tests/common/mod.rs`:
```rust
use async_trait::async_trait;
use screener_service::config::FilterConfigDto;
use screener_service::engine::{ProgressEvent, ScreeningEngine};
use tokio::sync::broadcast;

pub struct FakeScreeningEngine {
    pub final_watchlist: Vec<String>,
}

#[async_trait]
impl ScreeningEngine for FakeScreeningEngine {
    async fn run(&self, _config: FilterConfigDto, progress_tx: broadcast::Sender<ProgressEvent>) -> Result<Vec<String>, String> {
        let _ = progress_tx.send(ProgressEvent::Phase1 { total: 1, started: 1, passed: 1, technical_failures: 0, errors: 0 });
        let _ = progress_tx.send(ProgressEvent::Phase2 { total: 1, started: 1, passed: 1, technical_failures: 0, errors: 0 });
        let _ = progress_tx.send(ProgressEvent::Phase3 { total: 1, started: 1, passed: 1, technical_failures: 0, errors: 0 });
        Ok(self.final_watchlist.clone())
    }
}
```

`crates/screener-service/tests/runs_api_test.rs`:
```rust
mod common;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use common::FakeScreeningEngine;
use http_body_util::BodyExt;
use screener_service::config::InMemoryFilterConfigStore;
use screener_service::runs::InMemoryRunStore;
use screener_service::{build_router, AppState};
use tower::ServiceExt;

fn app_with_fake_engine(final_watchlist: Vec<String>) -> axum::Router {
    let state = AppState {
        auth_token: "test-token".to_string(),
        filter_config_store: Arc::new(InMemoryFilterConfigStore::new()),
        run_store: Arc::new(InMemoryRunStore::new()),
        engine: Arc::new(FakeScreeningEngine { final_watchlist }),
    };
    build_router(state)
}

#[tokio::test]
async fn triggering_a_run_returns_a_running_record_and_it_completes_shortly_after() {
    let app = app_with_fake_engine(vec!["AAPL".to_string()]);

    let trigger_response = app
        .clone()
        .oneshot(
            Request::post("/runs")
                .header("authorization", "Bearer test-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(trigger_response.status(), StatusCode::OK);

    let body = trigger_response.into_body().collect().await.unwrap().to_bytes();
    let record: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let run_id = record["id"].as_str().unwrap().to_string();
    assert_eq!(record["status"], "running");

    // Give the spawned run task a moment to finish (FakeScreeningEngine
    // resolves near-instantly).
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let get_response = app
        .oneshot(
            Request::get(format!("/runs/{run_id}"))
                .header("authorization", "Bearer test-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get_response.status(), StatusCode::OK);
    let body = get_response.into_body().collect().await.unwrap().to_bytes();
    let record: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(record["status"], "completed");
    assert_eq!(record["final_watchlist"], serde_json::json!(["AAPL"]));
}

#[tokio::test]
async fn triggering_a_second_run_while_one_is_active_returns_409() {
    let app = app_with_fake_engine(vec![]);

    let first = app
        .clone()
        .oneshot(
            Request::post("/runs")
                .header("authorization", "Bearer test-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);

    let second = app
        .oneshot(
            Request::post("/runs")
                .header("authorization", "Bearer test-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(second.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn getting_an_unknown_run_returns_404() {
    let app = app_with_fake_engine(vec![]);
    let response = app
        .oneshot(
            Request::get("/runs/00000000-0000-0000-0000-000000000000")
                .header("authorization", "Bearer test-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p screener-service --test runs_api_test`
Expected: FAIL to compile — `screener_service::engine` and `run_store`/`engine` fields on `AppState` not defined.

- [ ] **Step 3: Implement `ScreeningEngine`, `ProgressEvent`, and the run-triggering handlers**

`crates/screener-service/src/engine.rs`:
```rust
use async_trait::async_trait;
use serde::Serialize;
use tokio::sync::broadcast;

use crate::config::FilterConfigDto;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "phase")]
pub enum ProgressEvent {
    Phase1 { total: usize, started: usize, passed: usize, technical_failures: usize, errors: usize },
    Phase2 { total: usize, started: usize, passed: usize, technical_failures: usize, errors: usize },
    Phase3 { total: usize, started: usize, passed: usize, technical_failures: usize, errors: usize },
    Complete { final_watchlist: Vec<String> },
    Failed { message: String },
}

#[async_trait]
pub trait ScreeningEngine: Send + Sync {
    /// Runs the full Phase 1 -> 2 -> 3 pipeline, emitting progress on
    /// `progress_tx` as it goes, and returns the final watchlist on
    /// success or an error message on failure.
    async fn run(&self, config: FilterConfigDto, progress_tx: broadcast::Sender<ProgressEvent>) -> Result<Vec<String>, String>;
}
```

`crates/screener-service/src/routes.rs`:
```rust
use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

use crate::engine::ProgressEvent;
use crate::error::ServiceError;
use crate::runs::RunRecord;
use crate::AppState;

pub async fn trigger_run(State(state): State<AppState>) -> Result<Json<RunRecord>, ServiceError> {
    let record = state
        .run_store
        .try_start_run()
        .await
        .ok_or(ServiceError::RunAlreadyInProgress)?;

    let (tx, _rx) = broadcast::channel(64);
    state.progress_channels.write().await.insert(record.id, tx.clone());

    let run_id = record.id;
    let engine = Arc::clone(&state.engine);
    let run_store = Arc::clone(&state.run_store);
    let config = state.filter_config_store.get().await;

    tokio::spawn(async move {
        // RunStore is updated BEFORE broadcasting (not after — see Task 6's
        // design note for why this ordering matters for WebSocket clients
        // that subscribe late).
        match engine.run(config, tx.clone()).await {
            Ok(final_watchlist) => {
                run_store.complete_run(run_id, final_watchlist.clone()).await;
                let _ = tx.send(ProgressEvent::Complete { final_watchlist });
            }
            Err(message) => {
                run_store.fail_run(run_id, message.clone()).await;
                let _ = tx.send(ProgressEvent::Failed { message });
            }
        }
    });

    Ok(Json(record))
}

pub async fn get_run(State(state): State<AppState>, Path(id): Path<Uuid>) -> Result<Json<RunRecord>, ServiceError> {
    state.run_store.get_run(id).await.map(Json).ok_or(ServiceError::RunNotFound)
}

pub async fn list_runs(State(state): State<AppState>) -> Json<Vec<RunRecord>> {
    Json(state.run_store.list_runs().await)
}

pub type ProgressChannels = Arc<RwLock<HashMap<Uuid, broadcast::Sender<ProgressEvent>>>>;
```

- [ ] **Step 4: Add `run_store`, `engine`, and `progress_channels` to `AppState`**

Modify `crates/screener-service/src/lib.rs`. Change the `AppState` definition to:
```rust
#[derive(Clone)]
pub struct AppState {
    pub auth_token: String,
    pub filter_config_store: std::sync::Arc<dyn crate::config::FilterConfigStore>,
    pub run_store: std::sync::Arc<dyn crate::runs::RunStore>,
    pub engine: std::sync::Arc<dyn crate::engine::ScreeningEngine>,
    pub progress_channels: crate::routes::ProgressChannels,
}
```

Update the two existing test `AppState` constructions (in `lib.rs`'s own test and `auth.rs`'s `protected_app()`) to include the three new fields:
```rust
run_store: std::sync::Arc::new(crate::runs::InMemoryRunStore::new()),
engine: std::sync::Arc::new(NoOpEngine),
progress_channels: Default::default(),
```
where `NoOpEngine` is a minimal local test-only stub. Add this same struct+impl block separately to **both** `lib.rs`'s `#[cfg(test)] mod tests` and `auth.rs`'s `#[cfg(test)] mod tests` — they are two different compilation-scoped test modules, so each needs its own copy (a few lines of duplication is fine here; this is test-only scaffolding, not shared production code):
```rust
struct NoOpEngine;

#[async_trait::async_trait]
impl crate::engine::ScreeningEngine for NoOpEngine {
    async fn run(&self, _config: crate::config::FilterConfigDto, _progress_tx: tokio::sync::broadcast::Sender<crate::engine::ProgressEvent>) -> Result<Vec<String>, String> {
        Ok(Vec::new())
    }
}
```

Add the modules and wire the new routes into `build_router`:
```rust
pub mod engine;
pub mod routes;
```

```rust
pub fn build_router(state: AppState) -> Router {
    use axum::middleware;
    use axum::routing::{get as get_route, post};

    let protected = Router::new()
        .route(
            "/filter-config",
            get_route(config::get_filter_config).put(config::put_filter_config),
        )
        .route("/runs", post(routes::trigger_run).get(routes::list_runs))
        .route("/runs/{id}", get_route(routes::get_run))
        .route_layer(middleware::from_fn_with_state(state.clone(), auth::require_bearer_token));

    Router::new()
        .route("/health", get(health))
        .merge(protected)
        .with_state(state)
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p screener-service`
Expected: PASS — all prior tests plus the 3 new `runs_api_test` tests.

- [ ] **Step 6: Commit**

```bash
git add crates/screener-service/src/engine.rs crates/screener-service/src/routes.rs crates/screener-service/src/lib.rs crates/screener-service/tests
git commit -m "feat(screener-service): add ScreeningEngine trait and POST/GET /runs endpoints"
```

---

### Task 6: WebSocket progress streaming

**Files:**
- Modify: `crates/screener-service/src/routes.rs`
- Modify: `crates/screener-service/src/lib.rs`
- Create: `crates/screener-service/tests/ws_stream_test.rs`

**Interfaces:**
- Consumes: `ProgressEvent` (engine), `AppState` (lib), `FakeScreeningEngine` (tests/common)
- Produces: `pub async fn run_stream(State(state): State<AppState>, Path(id): Path<Uuid>, ws: WebSocketUpgrade) -> Response`

**Design note — a real race condition, not just a test artifact:** `tokio::sync::broadcast` never replays history to a subscriber that joins late. If the engine finishes (and broadcasts `Complete`/`Failed`) before a WebSocket client has connected and subscribed — entirely possible, since subscribing requires a full TCP connect + HTTP upgrade round-trip — that terminal event is broadcast to zero receivers and lost forever, and the client's `rx.recv().await` then blocks indefinitely. This isn't hypothetical: it reproduces reliably with `FakeScreeningEngine`, which finishes near-instantly. The fix has two parts, both required:
1. `trigger_run` (Task 5) updates `RunStore` **before** broadcasting the terminal event, not after.
2. `run_stream` subscribes to the broadcast channel **before** checking `RunStore`. With (1) in place, this ordering guarantees: if the post-subscribe check observes `Running`, the terminal broadcast hasn't happened yet, so the already-taken subscription is guaranteed to receive it later; if the check observes `Completed`/`Failed`, the terminal broadcast already happened (and was possibly lost) — so `run_stream` synthesizes the same event directly from `RunStore`'s authoritative state instead of waiting on a channel that will never deliver.

Also note: `/runs/{id}/stream` sits behind the same bearer-auth middleware as the other protected routes, so a WebSocket client must send an `Authorization` header on the handshake — a plain `ws://.../stream` URL string with no headers will get `401`, not `101`.

- [ ] **Step 1: Write the failing test**

`crates/screener-service/tests/ws_stream_test.rs`:
```rust
mod common;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use common::FakeScreeningEngine;
use futures_util::StreamExt;
use http_body_util::BodyExt;
use screener_service::config::InMemoryFilterConfigStore;
use screener_service::runs::InMemoryRunStore;
use screener_service::{build_router, AppState};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tower::ServiceExt;

/// The WS route requires the same bearer auth as everything else — a plain
/// URL string with no headers gets 401, not a successful upgrade.
fn authorized_ws_request(url: &str) -> tokio_tungstenite::tungstenite::handshake::client::Request {
    let mut request = url.into_client_request().unwrap();
    request
        .headers_mut()
        .insert("authorization", "Bearer test-token".parse().unwrap());
    request
}

#[tokio::test]
async fn streams_progress_events_and_closes_after_completion() {
    let state = AppState {
        auth_token: "test-token".to_string(),
        filter_config_store: Arc::new(InMemoryFilterConfigStore::new()),
        run_store: Arc::new(InMemoryRunStore::new()),
        engine: Arc::new(FakeScreeningEngine { final_watchlist: vec!["AAPL".to_string()] }),
        progress_channels: Default::default(),
    };
    let app = build_router(state);

    // Trigger a run over the in-process router first, to get a real run id
    // and populate the progress-channel registry.
    let trigger_response = app
        .clone()
        .oneshot(
            Request::post("/runs")
                .header("authorization", "Bearer test-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = trigger_response.into_body().collect().await.unwrap().to_bytes();
    let record: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let run_id = record["id"].as_str().unwrap().to_string();

    // Serve the same router on a real ephemeral TCP port so we can connect
    // an actual WebSocket client to it.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let url = format!("ws://{addr}/runs/{run_id}/stream");
    let (mut ws_stream, response) = connect_async(authorized_ws_request(&url)).await.unwrap();
    assert_eq!(response.status(), StatusCode::SWITCHING_PROTOCOLS);

    let mut saw_complete = false;
    while let Some(Ok(msg)) = ws_stream.next().await {
        if let tokio_tungstenite::tungstenite::Message::Text(text) = msg {
            let event: serde_json::Value = serde_json::from_str(&text).unwrap();
            if event["phase"] == "Complete" {
                assert_eq!(event["final_watchlist"], serde_json::json!(["AAPL"]));
                saw_complete = true;
                break;
            }
        }
    }
    assert!(saw_complete, "expected a Complete event before the stream ended");

    let _ = ws_stream.close(None).await;
}

#[tokio::test]
async fn streaming_an_unknown_run_id_returns_404() {
    let state = AppState {
        auth_token: "test-token".to_string(),
        filter_config_store: Arc::new(InMemoryFilterConfigStore::new()),
        run_store: Arc::new(InMemoryRunStore::new()),
        engine: Arc::new(FakeScreeningEngine { final_watchlist: vec![] }),
        progress_channels: Default::default(),
    };
    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let url = format!("ws://{addr}/runs/00000000-0000-0000-0000-000000000000/stream");
    let result = connect_async(authorized_ws_request(&url)).await;
    assert!(result.is_err(), "connecting to an unknown run id should fail the WS handshake");
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p screener-service --test ws_stream_test`
Expected: FAIL — likely a compile error on `axum::extract::ws` if the `ws` feature isn't enabled on the `axum` dependency yet (see Step 1 of Task 1 — that Cargo.toml already includes `features = ["ws"]`, so this should just be a routing failure: the route doesn't exist yet, so the WS handshake gets a non-101 response and `connect_async` returns `Err` in both tests).

- [ ] **Step 3: Implement `run_stream`**

Add to `crates/screener-service/src/routes.rs`:
```rust
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
```
(merge with the existing `use` block at the top of the file, and change the existing `use crate::runs::RunRecord;` line to `use crate::runs::{RunRecord, RunStatus};`)

Add at the end of `crates/screener-service/src/routes.rs`:
```rust
pub async fn run_stream(State(state): State<AppState>, Path(id): Path<Uuid>, ws: WebSocketUpgrade) -> Response {
    let tx = {
        let channels = state.progress_channels.read().await;
        channels.get(&id).cloned()
    };

    let Some(tx) = tx else {
        return (StatusCode::NOT_FOUND, "run not found").into_response();
    };

    // Subscribe BEFORE checking RunStore: this guarantees that if the check
    // below observes RunStatus::Running, the terminal broadcast has not
    // been sent yet (trigger_run updates the store first), so this
    // subscription is guaranteed to receive it when it eventually happens.
    let rx = tx.subscribe();
    let existing_record = state.run_store.get_run(id).await;

    ws.on_upgrade(move |socket| handle_socket(socket, rx, existing_record)).into_response()
}

async fn handle_socket(mut socket: WebSocket, mut rx: broadcast::Receiver<ProgressEvent>, existing_record: Option<RunRecord>) {
    // If the run already finished before we subscribed, its terminal event
    // was broadcast to zero receivers and is lost to `rx` forever — recover
    // it from the authoritative RunStore instead of waiting indefinitely.
    if let Some(record) = existing_record {
        let synthetic = match record.status {
            RunStatus::Completed => Some(ProgressEvent::Complete { final_watchlist: record.final_watchlist }),
            RunStatus::Failed => Some(ProgressEvent::Failed {
                message: record.error_message.unwrap_or_default(),
            }),
            RunStatus::Running => None,
        };
        if let Some(event) = synthetic {
            if let Ok(json) = serde_json::to_string(&event) {
                let _ = socket.send(Message::Text(json.into())).await;
            }
            return;
        }
    }

    while let Ok(event) = rx.recv().await {
        let is_terminal = matches!(event, ProgressEvent::Complete { .. } | ProgressEvent::Failed { .. });
        let Ok(json) = serde_json::to_string(&event) else {
            break;
        };
        if socket.send(Message::Text(json.into())).await.is_err() {
            break;
        }
        if is_terminal {
            break;
        }
    }
}
```

Modify `crates/screener-service/src/lib.rs`'s `build_router` to add the stream route to the protected router:
```rust
        .route("/runs/{id}", get_route(routes::get_run))
        .route("/runs/{id}/stream", get_route(routes::run_stream))
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p screener-service --test ws_stream_test`
Expected: PASS (2 tests)

- [ ] **Step 5: Run the full crate test suite**

Run: `cargo test -p screener-service`
Expected: PASS — every test from Tasks 1-6 passes together.

- [ ] **Step 6: Commit**

```bash
git add crates/screener-service/src/routes.rs crates/screener-service/src/lib.rs crates/screener-service/tests/ws_stream_test.rs
git commit -m "feat(screener-service): add WebSocket live progress streaming (GET /runs/{id}/stream)"
```

---

### Task 7: RealScreeningEngine

**Files:**
- Modify: `crates/screener-service/src/engine.rs`
- Modify: `crates/screener-service/Cargo.toml`

**Interfaces:**
- Consumes: `screener_core::{run_phase1, run_phase2, run_phase3, data::yahoo::YahooClient, data::ibkr::IbkrClient}`, `FilterConfigDto` (config), `ScreeningEngine`/`ProgressEvent` (engine, this file)
- Produces: `pub struct RealScreeningEngine { .. }` implementing `ScreeningEngine`, with `pub fn new(yahoo: Arc<YahooClient>, ibkr: Arc<IbkrClient>, universe: Vec<String>) -> Self`

**This task cannot be automatically tested** — it requires live Yahoo and IBKR access, neither available in this environment (Phase 1's Yahoo client has never been tested against real Yahoo either, only via `wiremock`; IBKR needs a live/paper TWS connection, per the Phase 2 plan's established limitation). Its only verification here is that it compiles.

- [ ] **Step 1: Add the dependency**

Modify `crates/screener-service/Cargo.toml`, `[dependencies]`, add:
```toml
tracing = "0.1"
```
(if not already present from Task 1 — confirm and skip if it's already there).

- [ ] **Step 2: Implement `RealScreeningEngine`**

Add to `crates/screener-service/src/engine.rs`:
```rust
use std::sync::Arc;

use screener_core::data::ibkr::IbkrClient;
use screener_core::data::yahoo::YahooClient;
use screener_core::{run_phase1, run_phase2, run_phase3};

pub struct RealScreeningEngine {
    yahoo: Arc<YahooClient>,
    ibkr: Arc<IbkrClient>,
    universe: Vec<String>,
}

impl RealScreeningEngine {
    /// `universe` is a static symbol list for now — real universe
    /// management (S&P 500 / Nasdaq scraping, refreshable via the API) is
    /// a separate future plan.
    pub fn new(yahoo: Arc<YahooClient>, ibkr: Arc<IbkrClient>, universe: Vec<String>) -> Self {
        Self { yahoo, ibkr, universe }
    }

    async fn relay_phase1(rx: &mut tokio::sync::mpsc::UnboundedReceiver<screener_core::Phase1Progress>, tx: &broadcast::Sender<ProgressEvent>) {
        while let Some(p) = rx.recv().await {
            let _ = tx.send(ProgressEvent::Phase1 {
                total: p.total,
                started: p.started,
                passed: p.passed,
                technical_failures: p.technical_failures,
                errors: p.errors,
            });
        }
    }

    async fn relay_phase2(rx: &mut tokio::sync::mpsc::UnboundedReceiver<screener_core::Phase2Progress>, tx: &broadcast::Sender<ProgressEvent>) {
        while let Some(p) = rx.recv().await {
            let _ = tx.send(ProgressEvent::Phase2 {
                total: p.total,
                started: p.started,
                passed: p.passed,
                technical_failures: p.technical_failures,
                errors: p.errors,
            });
        }
    }

    async fn relay_phase3(rx: &mut tokio::sync::mpsc::UnboundedReceiver<screener_core::Phase3Progress>, tx: &broadcast::Sender<ProgressEvent>) {
        while let Some(p) = rx.recv().await {
            let _ = tx.send(ProgressEvent::Phase3 {
                total: p.total,
                started: p.started,
                passed: p.passed,
                technical_failures: p.technical_failures,
                errors: p.errors,
            });
        }
    }
}

#[async_trait]
impl ScreeningEngine for RealScreeningEngine {
    async fn run(&self, config: FilterConfigDto, progress_tx: broadcast::Sender<ProgressEvent>) -> Result<Vec<String>, String> {
        let phase1_config: screener_core::Phase1Config = config.phase1.into();
        let phase2_config: screener_core::Phase2Config = config.phase2.into();
        let phase3_config: screener_core::Phase3Config = config.phase3.into();

        let (p1_tx, mut p1_rx) = tokio::sync::mpsc::unbounded_channel();
        let relay_tx = progress_tx.clone();
        let relay1 = tokio::spawn(async move { Self::relay_phase1(&mut p1_rx, &relay_tx).await });
        let phase1_results = run_phase1(&self.universe, &phase1_config, Arc::clone(&self.yahoo) as _, Some(p1_tx)).await;
        let _ = relay1.await;

        if phase1_results.survivors.is_empty() {
            return Ok(Vec::new());
        }

        let (p2_tx, mut p2_rx) = tokio::sync::mpsc::unbounded_channel();
        let relay_tx = progress_tx.clone();
        let relay2 = tokio::spawn(async move { Self::relay_phase2(&mut p2_rx, &relay_tx).await });
        let phase2_results = run_phase2(&phase1_results.survivors, &phase2_config, Arc::clone(&self.ibkr) as _, Some(p2_tx)).await;
        let _ = relay2.await;

        if phase2_results.survivors.is_empty() {
            return Ok(Vec::new());
        }

        let (p3_tx, mut p3_rx) = tokio::sync::mpsc::unbounded_channel();
        let relay_tx = progress_tx.clone();
        let relay3 = tokio::spawn(async move { Self::relay_phase3(&mut p3_rx, &relay_tx).await });
        let phase3_results = run_phase3(&phase2_results.survivors, &phase3_config, Arc::clone(&self.ibkr) as _, Some(p3_tx)).await;
        let _ = relay3.await;

        Ok(phase3_results.survivors)
    }
}
```

Add the missing `use` at the top of `crates/screener-service/src/engine.rs` (merge with the existing block):
```rust
use tokio::sync::broadcast;
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build -p screener-service`
Expected: builds cleanly. If not, the fix is confined to this file (`engine.rs`) — the surrounding trait/DTO/store architecture doesn't need to change.

- [ ] **Step 4: Run the full test suite to confirm nothing else broke**

Run: `cargo test -p screener-service`
Expected: PASS — `RealScreeningEngine` has no automated tests itself (see the task header note), but nothing else regresses.

- [ ] **Step 5: Commit**

```bash
git add crates/screener-service/src/engine.rs crates/screener-service/Cargo.toml
git commit -m "feat(screener-service): add RealScreeningEngine wiring run_phase1/2/3 to YahooClient/IbkrClient

Compile-verified only — no live Yahoo/IBKR access in this environment.
Requires manual verification before relying on it for real screening runs."
```

---

### Task 8: Binary entry point + end-to-end integration test

**Files:**
- Create: `crates/screener-service/src/main.rs`
- Create: `crates/screener-service/tests/end_to_end_test.rs`

**Interfaces:**
- Consumes: everything from Tasks 1-7
- Produces: a runnable binary, no new library API

- [ ] **Step 1: Write the failing end-to-end test**

`crates/screener-service/tests/end_to_end_test.rs`:
```rust
mod common;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use common::FakeScreeningEngine;
use http_body_util::BodyExt;
use screener_service::config::{FilterConfigDto, InMemoryFilterConfigStore};
use screener_service::runs::InMemoryRunStore;
use screener_service::{build_router, AppState};
use tower::ServiceExt;

#[tokio::test]
async fn full_lifecycle_config_update_then_run_then_history() {
    let state = AppState {
        auth_token: "test-token".to_string(),
        filter_config_store: Arc::new(InMemoryFilterConfigStore::new()),
        run_store: Arc::new(InMemoryRunStore::new()),
        engine: Arc::new(FakeScreeningEngine { final_watchlist: vec!["NVDA".to_string()] }),
        progress_channels: Default::default(),
    };
    let app = build_router(state);
    let auth = "Bearer test-token";

    // 1. Update filter config.
    let mut config = FilterConfigDto::default();
    config.phase3.enable_vwap = false;
    let put_response = app
        .clone()
        .oneshot(
            Request::put("/filter-config")
                .header("authorization", auth)
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&config).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(put_response.status(), StatusCode::OK);

    // 2. Confirm it persisted.
    let get_config_response = app
        .clone()
        .oneshot(
            Request::get("/filter-config")
                .header("authorization", auth)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = get_config_response.into_body().collect().await.unwrap().to_bytes();
    let stored: FilterConfigDto = serde_json::from_slice(&body).unwrap();
    assert!(!stored.phase3.enable_vwap);

    // 3. Trigger a run.
    let trigger_response = app
        .clone()
        .oneshot(
            Request::post("/runs")
                .header("authorization", auth)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(trigger_response.status(), StatusCode::OK);
    let body = trigger_response.into_body().collect().await.unwrap().to_bytes();
    let record: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let run_id = record["id"].as_str().unwrap().to_string();

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // 4. Confirm it shows up completed in history.
    let list_response = app
        .oneshot(
            Request::get("/runs")
                .header("authorization", auth)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = list_response.into_body().collect().await.unwrap().to_bytes();
    let runs: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let runs = runs.as_array().unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0]["id"], run_id);
    assert_eq!(runs[0]["status"], "completed");
    assert_eq!(runs[0]["final_watchlist"], serde_json::json!(["NVDA"]));
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p screener-service --test end_to_end_test`
Expected: FAIL — likely a compile error if any wiring from earlier tasks is inconsistent, or a straightforward assertion failure if the modules are already correct from Task 6. Given Tasks 1-6 were each verified independently, this should already pass; if it does, treat this as the verification step rather than a red step, and proceed directly to Step 4's full-suite run.

- [ ] **Step 3: Fix any issues found**

If Step 2 fails, the fix will be in whichever module the failure points to (`routes.rs`, `lib.rs`'s route wiring, or `config.rs`'s (de)serialization) — apply it and re-run Step 2 until it passes.

- [ ] **Step 4: Write the binary entry point**

`crates/screener-service/src/main.rs`:
```rust
use std::sync::Arc;

use screener_service::config::InMemoryFilterConfigStore;
use screener_service::runs::InMemoryRunStore;
use screener_service::{build_router, AppState};

#[tokio::main]
async fn main() {
    let _ = tracing_subscriber::fmt::try_init();

    let auth_token = std::env::var("SCREENER_AUTH_TOKEN")
        .unwrap_or_else(|_| panic!("SCREENER_AUTH_TOKEN environment variable must be set"));

    // NOTE: this wires an in-memory FilterConfigStore/RunStore and has no
    // ScreeningEngine configured yet — RealScreeningEngine needs a live
    // IBKR connection (IbkrClient::connect) and a universe list, neither of
    // which this binary sources yet (deployment config and universe
    // management are separate future plans). This binary is runnable and
    // serves /health, /filter-config, and run history, but POST /runs will
    // need a real engine wired in before it does anything useful.
    let state = AppState {
        auth_token,
        filter_config_store: Arc::new(InMemoryFilterConfigStore::new()),
        run_store: Arc::new(InMemoryRunStore::new()),
        engine: Arc::new(NotYetConfiguredEngine),
        progress_channels: Default::default(),
    };

    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    tracing::info!("screener-service listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

struct NotYetConfiguredEngine;

#[async_trait::async_trait]
impl screener_service::engine::ScreeningEngine for NotYetConfiguredEngine {
    async fn run(
        &self,
        _config: screener_service::config::FilterConfigDto,
        _progress_tx: tokio::sync::broadcast::Sender<screener_service::engine::ProgressEvent>,
    ) -> Result<Vec<String>, String> {
        Err("no ScreeningEngine configured — wire RealScreeningEngine with a live IbkrClient before triggering runs".to_string())
    }
}
```

- [ ] **Step 5: Verify the binary compiles**

Run: `cargo build -p screener-service --bin screener-service`
Expected: builds cleanly.

- [ ] **Step 6: Run the full test suite one final time**

Run: `cargo test -p screener-service`
Expected: PASS — every test from all 8 tasks passes together.

- [ ] **Step 7: Commit**

```bash
git add crates/screener-service/src/main.rs crates/screener-service/tests/end_to_end_test.rs
git commit -m "feat(screener-service): add binary entry point and full-lifecycle end-to-end test"
```

---

## Self-Review Notes

**Spec coverage:** section 5.1's API surface is covered except `/universe` and `POST /universe/refresh`, explicitly deferred (see "Explicitly deferred" above — no scraper exists yet to back them). Section 5.5's run flow (trigger, stream progress, persist result) is covered by Tasks 5-6 and exercised end-to-end in Task 8. Section 5.3 (bearer auth) is Task 2. Section 5.6 (service-level error types distinct from run-specific errors) is Task 1's `ServiceError`. Section 5.2 (Firestore) and 5.4 (IB Gateway hosting) are explicitly out of scope per this plan's scoping constraint — separate future plans once this foundation exists.

**Placeholder scan:** no TBD/TODO. `main.rs`'s `NotYetConfiguredEngine` is not a placeholder in the prohibited sense — it's a real, working `ScreeningEngine` implementation (returns a clear error) documenting an explicit, deferred wiring decision, not unfinished code within this plan's own scope.

**Type consistency:** `FilterConfigDto`/`Phase{1,2,3}ConfigDto`, `RunRecord`/`RunStatus`, `ProgressEvent`, `ScreeningEngine`, `FilterConfigStore`, `RunStore`, `AppState`, `build_router` are each defined once and referenced identically across every task, the example test files, and `main.rs`.

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-08-24-screener-service-foundation.md`. Two execution options:

**1. Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration.

**2. Inline Execution** — Execute tasks in this session using executing-plans, batch execution with checkpoints.

Which approach?
