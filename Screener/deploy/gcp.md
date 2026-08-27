# Deploying on one GCP Compute Engine VM

Runs IB Gateway + `screener-service` + Caddy (TLS) from `docker-compose.yml` on
a single always-on VM. ~$15–25/month.

## 1. Create the VM

Console: Compute Engine → Create instance. Or `gcloud`:

```bash
gcloud compute instances create screener \
  --zone=us-central1-a \
  --machine-type=e2-small \
  --image-family=debian-12 --image-project=debian-cloud \
  --boot-disk-size=20GB \
  --tags=screener-web
```

`e2-small` (2 GB) is the floor; `e2-medium` (4 GB) is more comfortable — IB
Gateway alone wants ~1 GB.

## 2. Open ports 80 + 443

```bash
gcloud compute firewall-rules create screener-web \
  --allow=tcp:80,tcp:443 --target-tags=screener-web \
  --source-ranges=0.0.0.0/0
```

(SSH/22 is already allowed by GCP's default rules.)

## 3. Point a hostname at the VM

Get the external IP:

```bash
gcloud compute instances describe screener --zone=us-central1-a \
  --format='get(networkInterfaces[0].accessConfigs[0].natIP)'
```

Then either:

- **DuckDNS** (free): duckdns.org → pick `myscreener` → set its IP to the value
  above → `SCREENER_DOMAIN=myscreener.duckdns.org`
- **Your own domain**: add an `A` record `screener` → that IP →
  `SCREENER_DOMAIN=screener.yourdomain.com`

Consider reserving the IP as static (`gcloud compute addresses create …`) so it
survives a stop/start.

Verify: `dig +short <your hostname>` prints the VM's IP.

## 4. On the VM

```bash
gcloud compute ssh screener --zone=us-central1-a
```

Install Docker:

```bash
curl -fsSL https://get.docker.com | sudo sh
sudo usermod -aG docker "$USER" && exec sg docker newgrp   # or log out/in
```

Get the code (whichever fits — a private repo clone, `gcloud compute scp`, or
`git archive` from your machine). You need the whole `Screener/` workspace
because the Docker build compiles `screener-core` too:

```bash
git clone <your repo url> && cd <repo>/Screener/deploy
```

Configure and launch:

```bash
cp .env.example .env
chmod 600 .env
nano .env        # SCREENER_DOMAIN, TWS_USERID, TWS_PASSWORD
                 # (SCREENER_AUTH_TOKEN is already filled if you copied your .env up)
docker compose up -d --build
docker compose logs -f
```

## 5. Verify

```bash
curl https://<your hostname>/health                       # 200
curl -H "Authorization: Bearer <token>" https://<your hostname>/filter-config
```

Then in the logs watch for `IBC: Login has completed` (ib-gateway) and
`IBKR reachable` (screener-service). First bring-up: keep `TRADING_MODE=paper`
(no 2FA) and optionally `SCREENER_DEMO_ENGINE=1` to prove the app↔API↔TLS path
before IBKR is in the picture.

## 6. Point the app at it

```bash
cd Screener/apps/screener-app
SCREENER_API_BASE_URL=https://<your hostname> cargo tauri android build --debug --target aarch64
```

Install the APK, open it, Dashboard → **API token** → paste `SCREENER_AUTH_TOKEN`
→ Save. "Gateway: reachable" means it's talking to the VM.

## Updating later

```bash
git pull && cd Screener/deploy && docker compose up -d --build
```

See `deploy/README.md` for the IBKR/market-data details, the "IbkrClient is
unproven against a live socket" caveat, and known gaps (in-memory persistence,
no `/universe` endpoint).
