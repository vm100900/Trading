use thiserror::Error;

#[derive(Debug, Clone, Error)]
pub enum ScreeningError {
    #[error("technical filter failed for {symbol}")]
    TechnicalFilterFailed { symbol: String },

    #[error("insufficient data for {symbol}: needed {needed}, got {got}")]
    InsufficientData {
        symbol: String,
        needed: usize,
        got: usize,
    },

    #[error("invalid symbol: {0}")]
    InvalidSymbol(String),

    #[error("contract qualification failed for {0}")]
    ContractQualificationFailed(String),

    #[error("no data returned for {0}")]
    NoData(String),

    #[error("request timed out for {0}")]
    Timeout(String),

    #[error("IBKR pacing violation")]
    PacingViolation,

    #[error("IBKR API error: {0}")]
    IBKRApiError(String),

    #[error("connection error: {0}")]
    ConnectionError(String),

    #[error("calculation error: {0}")]
    CalculationError(String),

    #[error("Yahoo data error for {symbol}: {message}")]
    YahooDataError { symbol: String, message: String },

    #[error("Yahoo auth error: {0}")]
    YahooAuthError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insufficient_data_formats_with_symbol_and_counts() {
        let err = ScreeningError::InsufficientData {
            symbol: "AAPL".to_string(),
            needed: 207,
            got: 150,
        };
        assert_eq!(
            err.to_string(),
            "insufficient data for AAPL: needed 207, got 150"
        );
    }

    #[test]
    fn technical_filter_failed_formats_with_symbol() {
        let err = ScreeningError::TechnicalFilterFailed { symbol: "MSFT".to_string() };
        assert_eq!(err.to_string(), "technical filter failed for MSFT");
    }
}
