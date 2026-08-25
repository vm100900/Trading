use screener_core::data::yahoo::{YahooClient, YahooClientConfig};
use screener_core::DailyDataSource;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn sample_chart_body() -> serde_json::Value {
    serde_json::json!({
        "chart": {
            "result": [{
                "meta": { "symbol": "AAPL" },
                "timestamp": [1700000000, 1700086400, 1700172800],
                "indicators": {
                    "quote": [{
                        "open": [10.0, 11.0, 12.0],
                        "high": [10.5, 11.5, 12.5],
                        "low": [9.5, 10.5, 11.5],
                        "close": [10.2, 11.2, 12.2],
                        "volume": [1000, 1100, 1200]
                    }],
                    "adjclose": [{
                        "adjclose": [10.1, 11.1, 12.1]
                    }]
                }
            }],
            "error": null
        }
    })
}

async fn mock_server_with_crumb_and_chart(chart_body: serde_json::Value) -> MockServer {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/session"))
        .respond_with(ResponseTemplate::new(200).insert_header("set-cookie", "A1=session; Path=/"))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/crumb"))
        .respond_with(ResponseTemplate::new(200).set_body_string("test-crumb"))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/chart/AAPL"))
        .and(query_param("crumb", "test-crumb"))
        .respond_with(ResponseTemplate::new(200).set_body_json(chart_body))
        .mount(&server)
        .await;

    server
}

fn client_for(server: &MockServer) -> YahooClient {
    let config = YahooClientConfig {
        session_url: format!("{}/session", server.uri()),
        crumb_url: format!("{}/crumb", server.uri()),
        chart_base_url: format!("{}/chart", server.uri()),
        max_concurrent_requests: 5,
        max_retries: 2,
    };
    YahooClient::new(config)
}

#[tokio::test]
async fn fetches_and_parses_daily_bars_using_adjusted_close() {
    let server = mock_server_with_crumb_and_chart(sample_chart_body()).await;
    let client = client_for(&server);

    let bars = client.fetch_daily_bars("AAPL", 3).await.unwrap();

    assert_eq!(bars.len(), 3);
    // adjclose values, not raw close
    assert_eq!(bars[0].close, 10.1);
    assert_eq!(bars[1].close, 11.1);
    assert_eq!(bars[2].close, 12.1);
    assert_eq!(bars[0].volume, 1000);
}

#[tokio::test]
async fn skips_bars_with_null_fields() {
    // volume has no fallback (unlike close, which falls back from adjclose to
    // raw close), so nulling it is what actually exercises the skip path.
    let mut body = sample_chart_body();
    body["chart"]["result"][0]["indicators"]["quote"][0]["volume"][1] = serde_json::Value::Null;
    let server = mock_server_with_crumb_and_chart(body).await;
    let client = client_for(&server);

    let bars = client.fetch_daily_bars("AAPL", 1).await.unwrap();

    assert_eq!(bars.len(), 2); // the bar with a null volume is skipped
}

#[tokio::test]
async fn returns_yahoo_data_error_when_result_is_empty() {
    let empty_body = serde_json::json!({ "chart": { "result": [], "error": null } });
    let server = mock_server_with_crumb_and_chart(empty_body).await;
    let client = client_for(&server);

    let result = client.fetch_daily_bars("AAPL", 1).await;

    assert!(matches!(result, Err(screener_core::ScreeningError::YahooDataError { .. })));
}
