use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use actix_web::{
    middleware::from_fn,
    test::{self, TestRequest},
    web::{self, Data},
    App, HttpResponse,
};
use cosy_gameapi::metrics::{track_usage, UsageMetrics, HEALTH_PATH};
use opentelemetry::{metrics::MeterProvider, KeyValue};
use opentelemetry_sdk::metrics::{
    data::{AggregatedMetrics, MetricData},
    InMemoryMetricExporter, SdkMeterProvider,
};

const DAY: Duration = Duration::from_secs(24 * 60 * 60);
const REQUESTS: &str = "cosy_gameapi_http_requests";
const CLIENTS: &str = "cosy_gameapi_unique_clients";

fn test_metrics() -> (UsageMetrics, SdkMeterProvider, InMemoryMetricExporter) {
    let exporter = InMemoryMetricExporter::default();
    let provider = SdkMeterProvider::builder()
        .with_periodic_exporter(exporter.clone())
        .build();
    let metrics = UsageMetrics::new(&provider.meter("test"));
    (metrics, provider, exporter)
}

fn attribute<'a>(mut attributes: impl Iterator<Item = &'a KeyValue>, key: &str) -> String {
    attributes
        .find(|kv| kv.key.as_str() == key)
        .map(|kv| kv.value.to_string())
        .unwrap_or_default()
}

/// Returns every exported data point of the named metric, keyed by its two
/// attribute values, e.g. `"/games 200"` or `"/games 7d"`.
fn exported(
    provider: &SdkMeterProvider,
    exporter: &InMemoryMetricExporter,
    name: &str,
    attr_a: &str,
    attr_b: &str,
) -> HashMap<String, u64> {
    exporter.reset();
    provider.force_flush().unwrap();

    let mut points = HashMap::new();
    for resource_metrics in exporter.get_finished_metrics().unwrap() {
        for metric in resource_metrics
            .scope_metrics()
            .flat_map(|scope| scope.metrics())
            .filter(|metric| metric.name() == name)
        {
            let key = |a: String, b: String| format!("{a} {b}");
            match metric.data() {
                AggregatedMetrics::U64(MetricData::Sum(sum)) => {
                    for dp in sum.data_points() {
                        points.insert(
                            key(
                                attribute(dp.attributes(), attr_a),
                                attribute(dp.attributes(), attr_b),
                            ),
                            dp.value(),
                        );
                    }
                }
                AggregatedMetrics::U64(MetricData::Gauge(gauge)) => {
                    for dp in gauge.data_points() {
                        points.insert(
                            key(
                                attribute(dp.attributes(), attr_a),
                                attribute(dp.attributes(), attr_b),
                            ),
                            dp.value(),
                        );
                    }
                }
                other => panic!("unexpected data for {name}: {other:?}"),
            }
        }
    }
    points
}

fn points(expected: &[(&str, u64)]) -> HashMap<String, u64> {
    expected
        .iter()
        .map(|(key, value)| (key.to_string(), *value))
        .collect()
}

#[actix_web::test]
async fn middleware_counts_routes_and_skips_probes() {
    let (metrics, provider, exporter) = test_metrics();
    let metrics = Data::new(metrics);
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

    assert_eq!(
        exported(&provider, &exporter, REQUESTS, "route", "status"),
        points(&[
            ("/assets/{game_id} 200", 2),
            ("/games 200", 1),
            ("unmatched 404", 2),
        ])
    );
    // Neither the health check nor unmatched requests are tracked as clients.
    assert_eq!(
        exported(&provider, &exporter, CLIENTS, "route", "window"),
        points(&[
            ("/assets/{game_id} 1d", 2),
            ("/assets/{game_id} 7d", 2),
            ("/assets/{game_id} 30d", 2),
            ("/games 1d", 1),
            ("/games 7d", 1),
            ("/games 30d", 1),
        ])
    );
}

#[test]
fn tracked_routes_report_zero() {
    let (metrics, provider, exporter) = test_metrics();
    metrics.track(&["/game"]);

    assert_eq!(
        exported(&provider, &exporter, REQUESTS, "route", "status"),
        points(&[("/game 200", 0)])
    );
    assert_eq!(
        exported(&provider, &exporter, CLIENTS, "route", "window"),
        points(&[("/game 1d", 0), ("/game 7d", 0), ("/game 30d", 0)])
    );
}

#[test]
fn unique_clients_per_window() {
    let (metrics, _provider, _exporter) = test_metrics();
    let start = Instant::now();
    let counts = |now: Instant| -> HashMap<String, u64> {
        metrics
            .unique_clients_at(now)
            .into_iter()
            .map(|(route, window, count)| (format!("{route} {window}"), count))
            .collect()
    };

    // Two callers at the start, one of them again ten days later.
    metrics.record_at(Some("/games"), 200, Some("198.51.100.1"), start);
    metrics.record_at(Some("/games"), 200, Some("198.51.100.2"), start);
    let later = start + 10 * DAY;
    metrics.record_at(Some("/games"), 200, Some("198.51.100.1"), later);

    assert_eq!(
        counts(later),
        points(&[("/games 1d", 1), ("/games 7d", 1), ("/games 30d", 2)])
    );

    // After 30 days without a call the route reports nobody.
    assert_eq!(
        counts(later + 31 * DAY),
        points(&[("/games 1d", 0), ("/games 7d", 0), ("/games 30d", 0)])
    );
}
