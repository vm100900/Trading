# Deploying screener-service with a live IBKR connection

This runs the Phase 1→2→3 screening API on a server: `screener-service`
talking to Yahoo (Phase 1) and a headless **IB Gateway** (Phases 2–3), with
**Caddy** terminating TLS for the Android app.

> IB Gateway is a long-running, stateful process with a forced daily restart —
> **not** something to put on Cloud Run / serverless. Use one always-on Linux
> VM (1–2 vCPU, 2 GB RAM is plenty).

## What you provide

| Thing | Notes |
|---|---|
| A Linux VM with Docker + Docker Compose | Ubuntu 22.04/24.04 is fine |
| A domain name | DNS `A`/`AAAA` → the VM's IP, before first `up` (Caddy needs it for the cert) |
| An IBKR account | **Start with a paper account** — paper logins skip 2FA |
| IBKR market-data subscriptions | US equities (NYSE/NASDAQ) real-time, on the account you log in with. Without them `contract_details` / historical `TRADES` calls fail. Delayed data is possible but needs a code tweak in `crates/screener-core/src/data/ibkr/client.rs`. |

## First run

```bash
cd Screener/deploy
cp .env.example .env
$EDITOR .env                 # domain, auth token, IBKR creds, TRADING_MODE=paper
docker compose up -d --build
docker compose logs -f screener-service
```

> **The image build is fast** — `deploy/Dockerfile` just copies the prebuilt
> static binary `deploy/bin/screener-service` (x86_64 musl, no glibc, runs on
> any x86_64 Linux). After you change `screener-service` / `screener-core`
> source, regenerate it on a machine with RAM to spare — `deploy/build-binary.sh`
> — commit it, `git pull` on the VM, `docker compose up -d --build`. To compile
> inside Docker instead (slow, wants ~2 GB RAM), point compose at
> `deploy/Dockerfile.fromsource`.

Expected in the logs:

```
real engine: IBKR ib-gateway:4004 (client_id 11), 74 symbols from /etc/screener/universe.txt
IBKR reachable at ib-gateway:4004
screener-service listening on 0.0.0.0:8080
```

If you see `Connection refused` at first, that's just the service starting
before Gateway finished logging in — it retries on the next run, and Compose
keeps both alive. Watch `docker compose logs -f ib-gateway` for
`IBC: Login has completed`.

## Verify, in order

1. **TLS + API up**
   ```bash
   curl https://YOUR_DOMAIN/health           # -> 200, empty body
   curl -H "Authorization: Bearer YOUR_TOKEN" https://YOUR_DOMAIN/filter-config
   ```

2. **Prove the app ↔ API ↔ TLS path without IBKR**: set `SCREENER_DEMO_ENGINE=1`
   in `.env`, `docker compose up -d screener-service`, trigger a run from the
   app — you should see the scripted Phase 1→2→3 stream and a 4-symbol
   watchlist. Then unset it again.

3. **Real screening, small universe**: trim `universe.txt` to ~3 liquid names
   (AAPL, MSFT, NVDA), `docker compose restart screener-service`, trigger a run.
   Watch the logs. **This is where `IbkrClient` gets its first real workout** —
   it has only ever been compile-checked against `ibapi` 3.3.0, never a live
   socket. Likely fixes live in `crates/screener-core/src/data/ibkr/client.rs`
   (bar-size / duration args, `what_to_show`, delayed-vs-live data, error
   mapping). Rebuild with `docker compose up -d --build screener-service`.

4. **Scale up**: restore the full `universe.txt`. Phase 2/3 are slow by design —
   `IbkrRateLimiter` allows ~60 IBKR requests / 10 min plus a 15 s per-symbol
   cooldown, and only Phase 1 survivors ever reach IBKR.

5. **Go live**: set `TRADING_MODE=live` and `SCREENER_IBKR_ADDR=ib-gateway:4003`
   in `.env`, `docker compose up -d`. A live Gateway needs the **IBKR Mobile**
   2FA push approved on your phone every time it (re)starts, including the
   nightly `AUTO_RESTART_TIME`. Schedule screening runs away from that window.

## Point the Android app at it

`API_BASE_URL` is a compile-time constant with a build-time override:

```bash
cd Screener/apps/screener-app
SCREENER_API_BASE_URL=https://YOUR_DOMAIN cargo tauri android build --debug --target aarch64
```

Install the APK, open the app, and on the **Dashboard** paste `YOUR_TOKEN` into
the *API token* field and hit *Save token* (it's kept in the webview's
`localStorage`). "Gateway: reachable" confirms it's talking to the server.

## Operational notes / known gaps

- **Persistence is in-memory.** Restarting `screener-service` loses run history
  and any filter-config edits (defaults are reloaded). The `FilterConfigStore`
  / `RunStore` traits exist so a SQLite/Firestore impl can be dropped in — not
  done here.
- **`/health` is service-up only** — it does not report Gateway connectivity.
- **No `/universe` endpoint.** The universe is the static `universe.txt` file;
  editing it needs a `docker compose restart screener-service`.
- **One IBKR connection per run.** `ReconnectingRealEngine` opens a fresh
  `IbkrClient` for each run and drops it after, so the Gateway's nightly
  restart never leaves the service wedged — but two runs can't overlap
  (`RunStore` already enforces that with a 409).
- **VNC** is published on `127.0.0.1:5900` (password = `TWS_PASSWORD`) for
  first-run Gateway debugging: `ssh -L 5900:localhost:5900 vm` then point a VNC
  client at `localhost:5900`.

## Without Docker (systemd)

If you install IB Gateway + IBC natively (or `docker run` just the
`ghcr.io/gnzsnz/ib-gateway` container) and run Caddy/nginx yourself, use
`deploy/screener-service.service` — its header comments have the steps. The
service binary is `cargo build --release -p screener-service` →
`target/release/screener-service`.
