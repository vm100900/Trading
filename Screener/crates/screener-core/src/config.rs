#[derive(Debug, Clone, PartialEq)]
pub struct Phase1Config {
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

impl Default for Phase1Config {
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

impl Phase1Config {
    /// Minimum completed daily bars required to evaluate this config's active conditions.
    pub fn min_bars_required(&self) -> usize {
        let longest_period = self
            .sma10_period
            .max(self.sma20_period)
            .max(self.sma50_period)
            .max(self.sma200_period);

        if self.enable_sma200_slope {
            longest_period + self.slope_lookback_bars
        } else {
            longest_period
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Phase2Config {
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

impl Default for Phase2Config {
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

impl Phase2Config {
    /// Minimum completed 30-minute bars required to evaluate this config's active conditions.
    pub fn min_bars_required(&self) -> usize {
        let longest_period = self
            .sma7_period
            .max(self.sma17_period)
            .max(self.sma33_period)
            .max(self.sma65_period);

        if self.enable_sma65_slope {
            longest_period + self.slope_lookback_bars
        } else {
            longest_period
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Phase3Config {
    pub sma20_period: usize,
    pub sma50_period: usize,
    pub sma100_period: usize,
    pub enable_close_gt_sma20: bool,
    pub enable_sma20_gt_sma50: bool,
    pub enable_sma50_gt_sma100: bool,
    pub enable_vwap: bool,
}

impl Default for Phase3Config {
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

impl Phase3Config {
    /// Minimum completed 2-minute bars required to evaluate this config's active conditions.
    /// Unlike Phase1Config/Phase2Config there is no slope lookback to add — Phase 3 has no
    /// slope condition.
    pub fn min_bars_required(&self) -> usize {
        self.sma20_period.max(self.sma50_period).max(self.sma100_period)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_locked_spec() {
        let cfg = Phase1Config::default();
        assert_eq!(cfg.sma10_period, 10);
        assert_eq!(cfg.sma20_period, 20);
        assert_eq!(cfg.sma50_period, 50);
        assert_eq!(cfg.sma200_period, 200);
        assert_eq!(cfg.slope_lookback_bars, 5);
        assert!(cfg.enable_close_gt_sma10);
        assert!(cfg.enable_sma10_gt_sma20);
        assert!(cfg.enable_sma20_gt_sma50);
        assert!(cfg.enable_sma50_gt_sma200);
        assert!(cfg.enable_sma200_slope);
    }

    #[test]
    fn default_min_bars_required_is_205() {
        // 200 bars for the current SMA200, plus 5 more so SMA200 can also be
        // computed 5 bars back for the slope comparison: 200 + 5 = 205.
        let cfg = Phase1Config::default();
        assert_eq!(cfg.min_bars_required(), 205);
    }

    #[test]
    fn min_bars_required_ignores_slope_lookback_when_slope_disabled() {
        let mut cfg = Phase1Config::default();
        cfg.enable_sma200_slope = false;
        assert_eq!(cfg.min_bars_required(), 200);
    }

    #[test]
    fn min_bars_required_tracks_custom_periods() {
        let mut cfg = Phase1Config::default();
        cfg.sma200_period = 300;
        assert_eq!(cfg.min_bars_required(), 305);
    }

    #[test]
    fn phase2_defaults_match_locked_spec() {
        let cfg = Phase2Config::default();
        assert_eq!(cfg.sma7_period, 7);
        assert_eq!(cfg.sma17_period, 17);
        assert_eq!(cfg.sma33_period, 33);
        assert_eq!(cfg.sma65_period, 65);
        assert_eq!(cfg.slope_lookback_bars, 1);
        assert!(cfg.enable_close_gt_sma7);
        assert!(cfg.enable_sma7_gt_sma17);
        assert!(cfg.enable_sma17_gt_sma33);
        assert!(cfg.enable_sma33_gt_sma65);
        assert!(cfg.enable_sma65_slope);
    }

    #[test]
    fn phase2_default_min_bars_required_is_66() {
        let cfg = Phase2Config::default();
        assert_eq!(cfg.min_bars_required(), 66);
    }

    #[test]
    fn phase2_min_bars_required_ignores_slope_lookback_when_slope_disabled() {
        let mut cfg = Phase2Config::default();
        cfg.enable_sma65_slope = false;
        assert_eq!(cfg.min_bars_required(), 65);
    }

    #[test]
    fn phase3_defaults_match_locked_spec() {
        let cfg = Phase3Config::default();
        assert_eq!(cfg.sma20_period, 20);
        assert_eq!(cfg.sma50_period, 50);
        assert_eq!(cfg.sma100_period, 100);
        assert!(cfg.enable_close_gt_sma20);
        assert!(cfg.enable_sma20_gt_sma50);
        assert!(cfg.enable_sma50_gt_sma100);
        assert!(cfg.enable_vwap);
    }

    #[test]
    fn phase3_default_min_bars_required_is_100() {
        let cfg = Phase3Config::default();
        assert_eq!(cfg.min_bars_required(), 100);
    }

    #[test]
    fn phase3_min_bars_required_tracks_custom_periods() {
        let mut cfg = Phase3Config::default();
        cfg.sma100_period = 150;
        assert_eq!(cfg.min_bars_required(), 150);
    }
}
