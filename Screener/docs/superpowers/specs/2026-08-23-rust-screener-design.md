# Multi-Timeframe Stock Screener — System Design

Date: 2026-08-23
Status: Approved for planning

## 1. Overview

A three-phase, sequential technical screener that narrows a stock/ETF universe
through progressively more expensive filters — daily (Yahoo Finance) → 30-minute
(IBKR) → 2-minute + VWAP (IBKR) — to minimize IBKR historical-data API usage.
The screener runs as an always-on Google Cloud Run service (which also hosts a
headless IBKR Gateway) and is controlled entirely from a Tauri Mobile phone app:
trigger runs, toggle/tune filter conditions, and view live progress and results.

## 2. Goals / Non-goals

**Goals:** correctness of the screening logic, no look-ahead bias, minimal IBKR
API usage, IBKR pacing compliance, single-user remote control from a phone,
runtime-configurable filter conditions (on/off + tunable periods, defaults
preserved).

**Non-goals:** multi-user support, order execution/trading, scheduled/automatic
runs (manual trigger only, by explicit choice), a full charting/analytics UI.

## 3. Component Architecture

```text
screener-core (Rust library)
  Phase 1/2/3 engine, indicators, rate limiter, data-source traits.
  No network server, no UI. Pure library, fully unit-testable via mocks.
        │ used by
screener-service (Rust/Axum, Google Cloud Run, min-instances=1)
  REST API + WebSocket, owns headless IB Gateway lifecycle, Firestore
  persistence, bearer-token auth.
        │ HTTPS + WebSocket, bearer token
screener-app (Tauri Mobile)
  Thin client: trigger runs, toggle filter conditions, watch live
  progress, view watchlist/history.
```

Build order: **core → service → app** (each depends on the prior layer's
contract being stable).

## 4. screener-core — Screening Engine

### 4.1 Pipeline

```text
Full Universe
    ↓
Phase 1 — Daily Macro Filter (Yahoo Finance)
    ↓
Phase 2 — 30-Minute Micro-Trend Filter (IBKR)
    ↓
Phase 3 — 2-Minute Intraday + VWAP Filter (IBKR)
    ↓
Final Watchlist
```

A symbol that fails a phase never proceeds to the next. IBKR historical-data
requests are made **only** for the prior phase's survivors — this is the
primary cost-control mechanism.

### 4.2 No Look-Ahead Bias (hard requirement)

Every phase evaluates only **completed** bars:
- Phase 1 → latest completed daily bar
- Phase 2 → latest completed 30-minute bar
- Phase 3 → latest completed 2-minute bar

Never evaluate against a forming/incomplete bar.

### 4.3 Runtime-Configurable `FilterConfig`

Filter conditions are runtime input, not hardcoded, so they can be toggled and
tuned from the phone app. Each phase's config carries:
- an on/off flag per comparison condition (e.g. "SMA50 > SMA200")
- the numeric parameters behind each condition (SMA periods, slope lookback
  distances)

**Defaults exactly match the locked spec below and must not silently change.**
Changing a period changes the minimum required bar count for that phase (e.g.
SMA200 → SMA300 requires ~305+ daily bars instead of 205) — the engine derives
its historical-data request window from the *active* config, not the defaults.

### 4.4 Phase 1 — Daily Macro Filter (Yahoo Finance)

- Universe: cached list of S&P 500 (scraped from Wikipedia's constituent
  table — no free official index API exists) + Nasdaq-listed symbols (from
  Nasdaq Trader's public symbol directory, `nasdaqlisted.txt`/`otherlisted.txt`
  — official, authentication-free). Merged, deduplicated, cached; refreshed
  only on explicit manual request from the app (`POST /universe/refresh`),
  never automatically.
- Data: daily OHLCV, adjusted close, ≥205 valid completed bars (200 for
  SMA200 default + 5 for the slope comparison; recalculated per active config).
- **Yahoo auth**: session cookie + `crumb` bootstrap, reused across the run,
  refreshed on 401.
- **Yahoo scale handling**: bounded concurrency (default 5–10 concurrent
  requests), exponential backoff + jitter on 429/5xx, capped retries — no
  batch history endpoint exists, so this is one request per symbol across a
  universe that may be 1,000+ names.
- Indicators (defaults): SMA10, SMA20, SMA50, SMA200 — arithmetic mean of the
  latest N completed closes. No EMA.
- Slope (default): `SMA200[current] > SMA200[5 completed trading bars ago]`
  — indexed by actual returned bars, not calendar days (this is what makes
  Phase 1 holiday-safe without a maintained calendar).
- Filter (default): `close > sma10 > sma20 > sma50 > sma200 && slope condition`.
  Strict `>` only, never `>=`.
- Any missing indicator / insufficient data / malformed dataset →
  `InsufficientData`/`YahooDataError`, **not** a technical filter failure.

### 4.5 Phase 2 — 30-Minute Micro-Trend Filter (IBKR)

- Only Phase 1 survivors are processed.
- Connection: single persistent TWS/Gateway connection for the run via the
  `ibapi` crate, bridged into tokio (IB client on a dedicated thread,
  communicating via `mpsc`/`oneshot` channels — `ibapi` is callback/thread
  based, not natively async).
- Contracts: `STK`/`SMART`/`USD`, qualified once and cached; qualification
  failures/invalid/delisted symbols are handled gracefully without aborting
  the run.
- Data: 30-min RTH TRADES bars, **default 10 trading days** (not 7 — RTH
  30-min bars = 13/day; 7 days leaves too little margin over the 66-bar
  requirement once a half-day session is accounted for). If a symbol returns
  fewer bars than required, widen the request once before failing it as
  `InsufficientData`.
- Indicators (defaults): SMA7, SMA17, SMA33, SMA65.
- Slope (default): `SMA65[current] > SMA65[previous completed bar]`.
- Filter (default): `close > sma7 > sma17 > sma33 > sma65 && slope condition`.

### 4.6 Phase 3 — 2-Minute Intraday + VWAP Filter (IBKR)

- Only Phase 2 survivors are processed.
- Data: 2-min RTH TRADES bars, 2–3 trading days (≥100 bars required for
  SMA100 default; ample margin even with an early close, ~390 bars/2 days).
- Indicators (defaults): SMA20, SMA50, SMA100.
- VWAP: current-session RTH only, resets at 09:30 America/New_York (session
  boundary derived from IBKR contract trading-hours data, not a hand-rolled
  holiday calendar). `TypicalPrice = (H+L+C)/3`, volume-weighted across
  current-session bars only. No signal if cumulative session volume is
  zero/invalid.
- Filter (default): `close > sma20 > sma50 > sma100 && close > current_session_vwap`.

### 4.7 Timezone

All session calculations use `America/New_York` explicitly, never OS local
time. RTH = 09:30–16:00 America/New_York. Session boundaries and early
closes are sourced from IBKR's `reqContractDetails` (`tradingHours`/
`liquidHours`) rather than a maintained calendar table.

### 4.8 IBKR Rate Limiting

Centralized async token-bucket limiter (`governor` crate) shared across
Phase 2 and Phase 3, modeling IBKR's actual pacing rules (not a flat sleep
interval):
- ≤60 historical-data requests per rolling 10-minute window (global)
- ≤6 requests for the identical contract/exchange/tick-type within 2 seconds
- ~15s cooldown before repeating an identical historical-data request

Bounded worker/queue concurrency, retry with exponential backoff + jitter,
capped retry count, pacing-error detection (IBKR error codes 162/165/166/321
etc.), connection-error recovery. Pacing compliance takes priority over
throughput.

### 4.9 Error Taxonomy

```text
TechnicalFilterFailed, InsufficientData, InvalidSymbol,
ContractQualificationFailed, NoData, Timeout, PacingViolation,
IBKRApiError, ConnectionError, CalculationError,
YahooDataError, YahooAuthError
```

Unavailable data is never counted as a technical filter failure. Errors are
recorded per-symbol; screening continues for the rest of the universe.

### 4.10 Testability

`DataSource` traits (e.g. `DailyDataSource`, `IntradayDataSource`) abstract
Yahoo and IBKR access so unit/integration tests run against mocks/fixtures —
required to make pacing, retry, connection-failure, DST, and holiday tests
runnable without live services. Progress is reported upward via a callback/
channel (`progress_tx`) so the core has no knowledge of HTTP/WebSockets.

## 5. screener-service — Backend (Google Cloud Run)

### 5.1 API

```text
GET  /universe                 current cached universe + last-refreshed time
POST /universe/refresh         re-pull S&P 500 + Nasdaq, replace cache
GET  /filter-config             current FilterConfig (defaults + overrides)
PUT  /filter-config             update FilterConfig
POST /runs                      trigger a run (rejects if one is active)
GET  /runs/{id}                 run summary/result
GET  /runs                      run history
WS   /runs/{id}/stream          live phase-progress
GET  /health                    Gateway connection status
```

### 5.2 Persistence

Firestore: `FilterConfig`, cached universe, qualified-contract cache, run
history/results. Chosen over Cloud SQL for a single-instance, small-document,
no-relational-joins workload — simpler ops for personal scale.

### 5.3 Auth

Single shared bearer token, generated once, stored in the Tauri app's secure
storage, required on every request. Deliberately simpler than Firebase Auth —
this is a single-user app.

### 5.4 IB Gateway Hosting

Headless IB Gateway + IBC automation in the same container (or a sidecar via
Cloud Run's multi-container support). `min-instances=1` with CPU always
allocated so the Gateway session and in-memory contract cache persist between
requests.

**Accepted risk:** this is not officially supported by IBKR; IBC is a
community tool, and IBKR's 2FA (IBKR Mobile push) can interrupt a fully
automated session depending on account security settings. `/health` exposes
Gateway connection state explicitly so a run failure is distinguishable from
"Gateway needs re-auth."

### 5.5 Run Flow

```text
1. App: POST /runs (bearer token). Service rejects if a run is already active.
2. Service pre-flight: check Gateway health BEFORE starting Phase 1 — if the
   Gateway is down, fail fast with GatewayUnavailable rather than burning
   Yahoo requests and time on a run that can't reach Phase 2.
3. Service opens the run's WebSocket stream; app connects.
4. screener-core::run_screening(universe, filter_config, progress_tx) executes
   Phase 1 → 2 → 3 in-process; progress_tx emits per-phase counts as each
   phase finalizes; service relays them over the WebSocket.
5. Service persists the completed run (config used, per-phase counts, final
   watchlist, timestamps) to Firestore; closes the WebSocket with a
   "complete" message.
```

### 5.6 Error Handling

Service-level error types (`GatewayUnavailable`, `RunAlreadyInProgress`,
`AuthFailed`, `PersistenceError`) map to HTTP status codes for REST and typed
messages over the WebSocket. Gateway health is surfaced independently of
run-specific errors.

## 6. screener-app — Mobile Client (Tauri Mobile)

### 6.1 Screens

1. **Dashboard** — "Run Screener" button, last-run status, Gateway health
   indicator, universe refresh button + last-refreshed timestamp.
2. **Live Run** — streamed via WebSocket: per-phase started/passed/
   technical-failures/errors counts, updating live.
3. **Watchlist/Results** — final survivor list for the latest run.
4. **Filter Config** — per-phase condition toggles + editable period/lookback
   fields, "Reset to defaults," explicit Save (`PUT /filter-config`).
5. **History** — past runs, tap through to that run's watchlist.

### 6.2 Resilience

WebSocket disconnects (e.g. app backgrounded mid-run) are recoverable — on
reopen, the app reconnects to `/runs/{id}/stream` or polls `GET /runs/{id}`
to resume showing progress rather than losing state.

## 7. Testing Strategy

- **core**: full unit-test suite against `DataSource` mocks — SMA/slope/VWAP
  math, session-boundary/DST handling, insufficient/malformed data, duplicate
  timestamps, incomplete-bar exclusion, zero-volume VWAP, pacing/retry/backoff
  behavior, empty-phase-result handling. No live services required.
- **service**: integration tests against a mocked `screener-core`, the
  Firestore emulator, auth middleware, and WebSocket streaming — no live
  IBKR/Yahoo/Gateway required for CI.
- **app**: state-management tests against a mocked service API; manual
  verification against a running service instance.
- **end-to-end**: a staging Cloud Run service pointed at an IBKR **paper
  account**, for full-run smoke testing before pointing the app at the live
  account.

## 8. Open Risks / Assumptions

- IB Gateway headless automation (IBC) and 2FA interruption risk is accepted
  per section 5.4 — no automated mitigation beyond `/health` visibility exists
  yet; a Gateway outage requires manual intervention.
- Yahoo Finance's chart endpoint auth requirements (cookie/crumb) may change
  without notice; Phase 1 has no fallback data source if Yahoo blocks access
  entirely.
- S&P 500 constituent scraping from Wikipedia is not an official source and
  could break if the page structure changes; acceptable given manual-refresh
  (low frequency, easily noticed and fixed).
- Cloud Run `min-instances=1` with CPU always allocated runs 24/7 and is
  billed accordingly (not scale-to-zero) — ongoing cost, not evaluated here.

## 9. Delivery Order

1. `screener-core` — Phase 1 (Yahoo) fully working and tested standalone.
2. `screener-core` — Phase 2 + Phase 3 (IBKR), completing the engine.
3. `screener-service` — API, Firestore persistence, Gateway hosting,
   Cloud Run deployment.
4. `screener-app` — Tauri Mobile client against the deployed service.

Each stage should be working and reviewed before the next begins.
