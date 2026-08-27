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

// Defaults mirror `screener_core`'s locked config values exactly — see
// `crates/screener-core/src/config.rs` (and `screener-service`'s
// `Phase{1,2,3}ConfigDto`, which derive theirs from `screener_core` directly).
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase1_default_matches_screener_core() {
        let dto = Phase1ConfigDto::default();
        assert_eq!(dto.sma10_period, 10);
        assert_eq!(dto.sma20_period, 20);
        assert_eq!(dto.sma50_period, 50);
        assert_eq!(dto.sma200_period, 200);
        assert_eq!(dto.slope_lookback_bars, 5);
        assert!(dto.enable_close_gt_sma10);
        assert!(dto.enable_sma200_slope);
    }

    #[test]
    fn phase2_default_matches_screener_core() {
        let dto = Phase2ConfigDto::default();
        assert_eq!(dto.sma7_period, 7);
        assert_eq!(dto.sma17_period, 17);
        assert_eq!(dto.sma33_period, 33);
        assert_eq!(dto.sma65_period, 65);
        assert_eq!(dto.slope_lookback_bars, 1);
        assert!(dto.enable_sma65_slope);
    }

    #[test]
    fn phase3_default_matches_screener_core() {
        let dto = Phase3ConfigDto::default();
        assert_eq!(dto.sma20_period, 20);
        assert_eq!(dto.sma50_period, 50);
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
