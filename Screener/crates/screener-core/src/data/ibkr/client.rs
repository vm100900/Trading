use async_trait::async_trait;
use ibapi::market_data::historical::BarTimestamp;
use ibapi::prelude::*;

use crate::data::ibkr::{ContractCache, IbkrRateLimiter, QualifiedContract};
use crate::data::traits::IntradayDataSource;
use crate::error::ScreeningError;
use crate::models::{IntradayBar, IntradayBarSize};

pub struct IbkrClient {
    client: Client,
    contracts: ContractCache,
    rate_limiter: IbkrRateLimiter,
}

impl IbkrClient {
    /// Opens a single persistent connection to TWS or IB Gateway at `address`
    /// (e.g. "127.0.0.1:7497") using `client_id` to identify this session.
    /// This connection should be reused for the entire screening run rather
    /// than reconnecting per symbol.
    pub async fn connect(address: &str, client_id: i32) -> Result<Self, ScreeningError> {
        let client = Client::connect(address, client_id)
            .await
            .map_err(|e| map_ibkr_error(&e))?;
        Ok(Self {
            client,
            contracts: ContractCache::new(),
            rate_limiter: IbkrRateLimiter::new(),
        })
    }

    async fn qualify(&self, symbol: &str) -> Result<QualifiedContract, ScreeningError> {
        self.contracts
            .get_or_qualify(symbol, |symbol| async move {
                let contract = Contract::stock(symbol.clone()).build();
                let details = self
                    .client
                    .contract_details(&contract)
                    .await
                    .map_err(|e| map_ibkr_error(&e))?;
                let first = details
                    .into_iter()
                    .next()
                    .ok_or_else(|| ScreeningError::ContractQualificationFailed(symbol.clone()))?;
                Ok(QualifiedContract {
                    symbol,
                    contract_id: first.contract.contract_id,
                })
            })
            .await
    }
}

#[async_trait]
impl IntradayDataSource for IbkrClient {
    async fn fetch_intraday_bars(
        &self,
        symbol: &str,
        bar_size: IntradayBarSize,
        _min_bars: usize,
    ) -> Result<Vec<IntradayBar>, ScreeningError> {
        // Qualification also serves as an existence/entitlement check before
        // spending a rate-limited historical-data request on this symbol.
        self.qualify(symbol).await?;
        self.rate_limiter.acquire(symbol).await;

        let (ib_bar_size, duration_days) = match bar_size {
            // ~10 trading days of 30-min bars, per the Global Constraints
            // safety-margin rationale (13 bars/day; a half-day session can
            // remove ~6 bars, so 7 days is too thin a margin over the
            // 66-bar default requirement).
            IntradayBarSize::ThirtyMinutes => (HistoricalBarSize::Min30, 10),
            // 2-3 trading days of 2-min bars is ample for a 100-bar SMA
            // requirement (~195 bars/day); this plan only wires the
            // ThirtyMinutes path through Phase 2 — TwoMinutes support is for
            // the Phase 3 plan that builds on this client.
            IntradayBarSize::TwoMinutes => (HistoricalBarSize::Min2, 3),
        };

        let contract = Contract::stock(symbol).build();
        let historical_data = self
            .client
            .historical_data(&contract, ib_bar_size)
            .what_to_show(HistoricalWhatToShow::Trades)
            .trading_hours(TradingHours::Regular)
            .duration(duration_days.days())
            .fetch()
            .await
            .map_err(|e| map_ibkr_error(&e))?;

        let bars = historical_data
            .bars
            .into_iter()
            .filter_map(|bar| {
                let timestamp = match bar.date {
                    BarTimestamp::DateTime(odt) => chrono::DateTime::from_timestamp(odt.unix_timestamp(), 0)?,
                    // Daily/weekly timestamps are not expected for intraday
                    // bar sizes; skip defensively rather than fail the whole
                    // request over one malformed bar.
                    BarTimestamp::Date(_) => return None,
                };
                Some(IntradayBar {
                    timestamp,
                    open: bar.open,
                    high: bar.high,
                    low: bar.low,
                    close: bar.close,
                    volume: bar.volume,
                })
            })
            .collect();

        Ok(bars)
    }
}

/// Maps ibapi's error type to our ScreeningError taxonomy. The specific
/// IBKR error-code-to-variant mapping below (100 = pacing, 162/200 = no
/// data) covers the most common cases; it should be extended as real pacing
/// violations and data errors are observed against a live/paper account —
/// everything else currently falls through to IBKRApiError with the raw
/// code and message preserved for diagnosis.
fn map_ibkr_error(err: &Error) -> ScreeningError {
    match err {
        Error::Notice(notice) => match notice.code {
            100 => ScreeningError::PacingViolation,
            162 | 200 => ScreeningError::NoData(notice.message.clone()),
            code => ScreeningError::IBKRApiError(format!("[{code}] {}", notice.message)),
        },
        Error::ConnectionFailed | Error::ConnectionRejected(_) | Error::ConnectionReset => {
            ScreeningError::ConnectionError(err.to_string())
        }
        other => ScreeningError::IBKRApiError(other.to_string()),
    }
}
