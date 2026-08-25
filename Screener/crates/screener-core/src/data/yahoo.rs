use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use rand::Rng;
use serde::Deserialize;
use tokio::sync::{RwLock, Semaphore};
use tracing::warn;

use crate::data::traits::DailyDataSource;
use crate::error::ScreeningError;
use crate::models::DailyBar;

#[derive(Debug, Clone)]
pub struct YahooClientConfig {
    pub session_url: String,
    pub crumb_url: String,
    pub chart_base_url: String,
    pub max_concurrent_requests: usize,
    pub max_retries: u32,
}

impl Default for YahooClientConfig {
    fn default() -> Self {
        Self {
            session_url: "https://fc.yahoo.com".to_string(),
            crumb_url: "https://query2.finance.yahoo.com/v1/test/getcrumb".to_string(),
            chart_base_url: "https://query1.finance.yahoo.com/v8/finance/chart".to_string(),
            max_concurrent_requests: 8,
            max_retries: 3,
        }
    }
}

pub struct YahooClient {
    http: reqwest::Client,
    config: YahooClientConfig,
    crumb: RwLock<Option<String>>,
    concurrency: Arc<Semaphore>,
}

impl YahooClient {
    pub fn new(config: YahooClientConfig) -> Self {
        let http = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .expect("reqwest client build should not fail with default settings");
        let concurrency = Arc::new(Semaphore::new(config.max_concurrent_requests));
        Self {
            http,
            config,
            crumb: RwLock::new(None),
            concurrency,
        }
    }

    async fn ensure_crumb(&self) -> Result<String, ScreeningError> {
        if let Some(crumb) = self.crumb.read().await.clone() {
            return Ok(crumb);
        }

        self.http
            .get(&self.config.session_url)
            .send()
            .await
            .map_err(|e| ScreeningError::YahooAuthError(format!("session bootstrap failed: {e}")))?;

        let resp = self
            .http
            .get(&self.config.crumb_url)
            .send()
            .await
            .map_err(|e| ScreeningError::YahooAuthError(format!("crumb request failed: {e}")))?;

        if !resp.status().is_success() {
            return Err(ScreeningError::YahooAuthError(format!(
                "crumb request returned status {}",
                resp.status()
            )));
        }

        let crumb = resp
            .text()
            .await
            .map_err(|e| ScreeningError::YahooAuthError(format!("failed to read crumb body: {e}")))?;

        *self.crumb.write().await = Some(crumb.clone());
        Ok(crumb)
    }

    async fn fetch_once(&self, symbol: &str) -> Result<Vec<DailyBar>, ScreeningError> {
        let crumb = self.ensure_crumb().await?;
        let url = format!("{}/{}", self.config.chart_base_url, symbol);

        let resp = self
            .http
            .get(&url)
            .query(&[
                ("interval", "1d"),
                ("range", "1y"),
                ("events", "div,splits"),
                ("crumb", crumb.as_str()),
            ])
            .send()
            .await
            .map_err(|e| ScreeningError::ConnectionError(e.to_string()))?;

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            *self.crumb.write().await = None;
            return Err(ScreeningError::YahooAuthError("crumb rejected (401)".to_string()));
        }
        if !resp.status().is_success() {
            return Err(ScreeningError::YahooDataError {
                symbol: symbol.to_string(),
                message: format!("HTTP {}", resp.status()),
            });
        }

        let body: ChartResponse = resp
            .json()
            .await
            .map_err(|e| ScreeningError::YahooDataError {
                symbol: symbol.to_string(),
                message: format!("failed to parse response: {e}"),
            })?;

        parse_chart_response(symbol, body)
    }
}

#[async_trait]
impl DailyDataSource for YahooClient {
    async fn fetch_daily_bars(&self, symbol: &str, _min_bars: usize) -> Result<Vec<DailyBar>, ScreeningError> {
        let _permit = self
            .concurrency
            .acquire()
            .await
            .expect("semaphore is never closed");

        let mut attempt = 0;
        loop {
            match self.fetch_once(symbol).await {
                Ok(bars) => return Ok(bars),
                Err(err) if attempt >= self.config.max_retries => return Err(err),
                Err(err) => {
                    warn!(symbol, attempt, %err, "Yahoo fetch failed, retrying");
                    let backoff_ms = 250u64 * 2u64.pow(attempt);
                    let jitter_ms = rand::thread_rng().gen_range(0..100);
                    tokio::time::sleep(Duration::from_millis(backoff_ms + jitter_ms)).await;
                    attempt += 1;
                }
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct ChartResponse {
    chart: ChartBody,
}

#[derive(Debug, Deserialize)]
struct ChartBody {
    result: Option<Vec<ChartResult>>,
}

#[derive(Debug, Deserialize)]
struct ChartResult {
    timestamp: Vec<i64>,
    indicators: Indicators,
}

#[derive(Debug, Deserialize)]
struct Indicators {
    quote: Vec<Quote>,
    adjclose: Option<Vec<AdjClose>>,
}

#[derive(Debug, Deserialize)]
struct Quote {
    open: Vec<Option<f64>>,
    high: Vec<Option<f64>>,
    low: Vec<Option<f64>>,
    close: Vec<Option<f64>>,
    volume: Vec<Option<u64>>,
}

#[derive(Debug, Deserialize)]
struct AdjClose {
    adjclose: Vec<Option<f64>>,
}

fn parse_chart_response(symbol: &str, body: ChartResponse) -> Result<Vec<DailyBar>, ScreeningError> {
    let result = body
        .chart
        .result
        .and_then(|mut r| if r.is_empty() { None } else { Some(r.remove(0)) })
        .ok_or_else(|| ScreeningError::YahooDataError {
            symbol: symbol.to_string(),
            message: "empty chart result".to_string(),
        })?;

    let quote = result.indicators.quote.first().ok_or_else(|| ScreeningError::YahooDataError {
        symbol: symbol.to_string(),
        message: "missing quote indicators".to_string(),
    })?;

    let adjclose = result.indicators.adjclose.as_ref().and_then(|a| a.first());

    let mut bars = Vec::with_capacity(result.timestamp.len());
    for i in 0..result.timestamp.len() {
        let close = adjclose
            .and_then(|a| a.adjclose.get(i).copied().flatten())
            .or_else(|| quote.close.get(i).copied().flatten());

        let (Some(open), Some(high), Some(low), Some(close), Some(volume)) = (
            quote.open.get(i).copied().flatten(),
            quote.high.get(i).copied().flatten(),
            quote.low.get(i).copied().flatten(),
            close,
            quote.volume.get(i).copied().flatten(),
        ) else {
            continue; // skip bars with any missing/null required field
        };

        let date = chrono::DateTime::from_timestamp(result.timestamp[i], 0)
            .ok_or_else(|| ScreeningError::YahooDataError {
                symbol: symbol.to_string(),
                message: format!("invalid timestamp {}", result.timestamp[i]),
            })?
            .date_naive();

        bars.push(DailyBar { date, open, high, low, close, volume });
    }

    Ok(bars)
}
