use async_trait::async_trait;

use crate::error::ScreeningError;
use crate::models::{DailyBar, IntradayBar, IntradayBarSize};

#[async_trait]
pub trait DailyDataSource: Send + Sync {
    /// Fetch daily bars for `symbol`, requesting enough history to cover at
    /// least `min_bars` valid completed bars (implementations should request
    /// extra buffer, not exactly `min_bars`).
    async fn fetch_daily_bars(&self, symbol: &str, min_bars: usize) -> Result<Vec<DailyBar>, ScreeningError>;
}

#[async_trait]
pub trait IntradayDataSource: Send + Sync {
    /// Fetch intraday bars for `symbol` at the given `bar_size`, requesting
    /// enough history to cover at least `min_bars` valid completed bars
    /// (implementations should request extra buffer, not exactly `min_bars`).
    async fn fetch_intraday_bars(
        &self,
        symbol: &str,
        bar_size: IntradayBarSize,
        min_bars: usize,
    ) -> Result<Vec<IntradayBar>, ScreeningError>;
}
