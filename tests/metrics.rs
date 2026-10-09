use std::time::{Duration, Instant};

use actix_web::{
    middleware::from_fn,
    test::{self, TestRequest},
    web::{self, Data},
    App, HttpResponse,
};
use cosy_gameapi::metrics::{track_usage, UsageMetrics, HEALTH_PATH};

const DAY: Duration = Duration::from_secs(24 * 60 * 60);

fn has_line(rendered: &str, line: &str) -> bool {
    rendered.lines().any(|l| l == line)
}

#[actix_web::test]
async fn middleware_counts_routes_and_skips_probes() {
    let metrics = Data::new(UsageMetrics::new().unwrap());
    let app = test::init_service(
        App::new()
            .wrap(from_fn(track_usage))
            .app_data(metrics.clone())
            .route("/assets/{game_id}", web::get().to(HttpResponse::Ok))
            .route("/games", web::get().to(HttpResponse::Ok))
            .route(HEALTH_PATH, web::get().to(HttpResponse::Ok)),
    )
    .await;

    for (uri, forwarded_for, agent) in [
        ("/assets/1", "198.51.100.1", "ReactorNetty/1.2"),
        ("/assets/2", "198.51.100.2", "ReactorNetty/1.2"),
        ("/games?query=zelda", "198.51.100.1", "ReactorNetty/1.2"),
        ("/games?query=anything", "10.42.0.1", "kube-probe/1.33"),
        ("/healthz", "10.42.0.1", "curl/8"),
        ("/.env", "203.0.113.7", "scanner"),
        ("/wp-config.php", "203.0.113.7", "scanner"),
    ] {
        let req = TestRequest::get()
            .uri(uri)
            .insert_header(("X-Forwarded-For", forwarded_for))
            .insert_header(("User-Agent", agent))
            .to_request();
        test::call_service(&app, req).await;
    }

    let rendered = metrics.render();
    for line in [
        r#"cosy_gameapi_http_requests_total{route="/assets/{game_id}",status="200"} 2"#,
        r#"cosy_gameapi_http_requests_total{route="/games",status="200"} 1"#,
        r#"cosy_gameapi_http_requests_total{route="unmatched",status="404"} 2"#,
        r#"cosy_gameapi_unique_clients{route="/assets/{game_id}",window="1d"} 2"#,
        r#"cosy_gameapi_unique_clients{route="/games",window="1d"} 1"#,
    ] {
        assert!(
            has_line(&rendered, line),
            "missing `{line}` in:\n{rendered}"
        );
    }
    assert!(!rendered.contains("healthz"), "{rendered}");
    assert!(!rendered.contains(r#"cosy_gameapi_unique_clients{route="unmatched""#));
}

#[test]
fn tracked_routes_report_zero() {
    let metrics = UsageMetrics::new().unwrap();
    metrics.track(&["/game"]);

    let rendered = metrics.render();
    for line in [
        r#"cosy_gameapi_http_requests_total{route="/game",status="200"} 0"#,
        r#"cosy_gameapi_unique_clients{route="/game",window="30d"} 0"#,
    ] {
        assert!(
            has_line(&rendered, line),
            "missing `{line}` in:\n{rendered}"
        );
    }
}

#[test]
fn unique_clients_per_window() {
    let metrics = UsageMetrics::new().unwrap();
    let start = Instant::now();

    // Two callers at the start, one of them again ten days later.
    metrics.record_at(Some("/games"), 200, Some("198.51.100.1"), start);
    metrics.record_at(Some("/games"), 200, Some("198.51.100.2"), start);
    let later = start + 10 * DAY;
    metrics.record_at(Some("/games"), 200, Some("198.51.100.1"), later);

    let rendered = metrics.render_at(later);
    for line in [
        r#"cosy_gameapi_unique_clients{route="/games",window="1d"} 1"#,
        r#"cosy_gameapi_unique_clients{route="/games",window="7d"} 1"#,
        r#"cosy_gameapi_unique_clients{route="/games",window="30d"} 2"#,
    ] {
        assert!(
            has_line(&rendered, line),
            "missing `{line}` in:\n{rendered}"
        );
    }

    // After 30 days without a call the route reports nobody.
    let rendered = metrics.render_at(later + 31 * DAY);
    assert!(
        has_line(
            &rendered,
            r#"cosy_gameapi_unique_clients{route="/games",window="30d"} 0"#
        ),
        "{rendered}"
    );
}
