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
