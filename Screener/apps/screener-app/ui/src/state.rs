#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Screen {
    #[default]
    Dashboard,
    FilterConfig,
    LiveRun,
    Watchlist,
    History,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RunSummary {
    pub status: String,
    pub final_watchlist: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct AppState {
    pub token: Option<String>,
    pub active_run_id: Option<String>,
    pub last_run: Option<RunSummary>,
    pub screen: Screen,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_state_starts_on_the_dashboard_with_nothing_loaded() {
        let state = AppState::default();
        assert_eq!(state.token, None);
        assert_eq!(state.active_run_id, None);
        assert_eq!(state.last_run, None);
        assert_eq!(state.screen, Screen::Dashboard);
    }
}
