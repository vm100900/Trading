use std::collections::HashMap;

use async_trait::async_trait;
use screener_core::{DailyBar, DailyDataSource, IntradayBar, IntradayBarSize, IntradayDataSource, ScreeningError};

pub enum MockResponse {
    Bars(Vec<DailyBar>),
    Error(ScreeningError),
}

pub struct MockDailyDataSource {
    pub responses: HashMap<String, MockResponse>,
}

impl MockDailyDataSource {
    pub fn new() -> Self {
        Self { responses: HashMap::new() }
    }

    pub fn with_bars(mut self, symbol: &str, bars: Vec<DailyBar>) -> Self {
        self.responses.insert(symbol.to_string(), MockResponse::Bars(bars));
        self
    }

    pub fn with_error(mut self, symbol: &str, error: ScreeningError) -> Self {
        self.responses.insert(symbol.to_string(), MockResponse::Error(error));
        self
    }
}

#[async_trait]
impl DailyDataSource for MockDailyDataSource {
    async fn fetch_daily_bars(&self, symbol: &str, _min_bars: usize) -> Result<Vec<DailyBar>, ScreeningError> {
        match self.responses.get(symbol) {
            Some(MockResponse::Bars(bars)) => Ok(bars.clone()),
            Some(MockResponse::Error(e)) => Err(e.clone()),
            None => Err(ScreeningError::NoData(symbol.to_string())),
        }
    }
}

pub struct MockIntradayDataSource {
    pub responses: HashMap<String, MockIntradayResponse>,
}

impl MockIntradayDataSource {
    pub fn new() -> Self {
        Self { responses: HashMap::new() }
    }

    pub fn with_bars(mut self, symbol: &str, bars: Vec<IntradayBar>) -> Self {
        self.responses.insert(symbol.to_string(), MockIntradayResponse::Bars(bars));
        self
    }

    pub fn with_error(mut self, symbol: &str, error: ScreeningError) -> Self {
        self.responses.insert(symbol.to_string(), MockIntradayResponse::Error(error));
        self
    }
}

pub enum MockIntradayResponse {
    Bars(Vec<IntradayBar>),
    Error(ScreeningError),
}

#[async_trait]
impl IntradayDataSource for MockIntradayDataSource {
    async fn fetch_intraday_bars(
        &self,
        symbol: &str,
        _bar_size: IntradayBarSize,
        _min_bars: usize,
    ) -> Result<Vec<IntradayBar>, ScreeningError> {
        match self.responses.get(symbol) {
            Some(MockIntradayResponse::Bars(bars)) => Ok(bars.clone()),
            Some(MockIntradayResponse::Error(e)) => Err(e.clone()),
            None => Err(ScreeningError::NoData(symbol.to_string())),
        }
    }
}
