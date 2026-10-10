use std::collections::HashMap;

use actix_web::{
    middleware::from_fn,
    test::{self, TestRequest},
    web::{self, Data},
    App, HttpResponse,
};
use cosy_gameapi::metrics::{track_usage, UsageMetrics};
use opentelemetry::{metrics::MeterProvider, KeyValue};
use opentelemetry_sdk::metrics::{
    data::{AggregatedMetrics, MetricData},
    InMemoryMetricExporter, SdkMeterProvider,
};

const HEALTH_ROUTE: &str = "/healthz";

fn test_metrics() -> (UsageMetrics, SdkMeterProvider, InMemoryMetricExporter) {
    let exporter = InMemoryMetricExporter::default();
    let provider = SdkMeterProvider::builder()
        .with_periodic_exporter(exporter.clone())
        .build();
    let metrics = UsageMetrics::with_meter(&provider.meter("test"), &[HEALTH_ROUTE], None);
    (metrics, provider, exporter)
}

fn attribute<'a>(mut attributes: impl Iterator<Item = &'a KeyValue>, key: &str) -> String {
    attributes
        .find(|kv| kv.key.as_str() == key)
        .map(|kv| kv.value.to_string())
        .unwrap_or_default()
}

/// Returns the exported request counter's data points keyed by `"route status"`.
fn requests(
    provider: &SdkMeterProvider,
    exporter: &InMemoryMetricExporter,
) -> HashMap<String, u64> {
    provider.force_flush().unwrap();

    let mut points = HashMap::new();
    for resource_metrics in exporter.get_finished_metrics().unwrap() {
        for metric in resource_metrics
            .scope_metrics()
            .flat_map(|scope| scope.metrics())
        {
            assert_eq!(metric.name(), "cosy_gameapi_http_requests");
            let AggregatedMetrics::U64(MetricData::Sum(sum)) = metric.data() else {
                panic!("unexpected data: {:?}", metric.data());
            };
            for dp in sum.data_points() {
                let key = format!(
                    "{} {}",
                    attribute(dp.attributes(), "route"),
                    attribute(dp.attributes(), "status")
                );
                points.insert(key, dp.value());
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
    let app = test::init_service(
        App::new()
            .wrap(from_fn(track_usage))
            .app_data(Data::new(metrics))
            .route("/assets/{game_id}", web::get().to(HttpResponse::Ok))
            .route("/games", web::get().to(HttpResponse::Ok))
            .route(HEALTH_ROUTE, web::get().to(HttpResponse::Ok)),
    )
    .await;

    for (uri, agent) in [
        ("/assets/1", "ReactorNetty/1.2"),
        ("/assets/2", "ReactorNetty/1.2"),
        ("/games?query=zelda", "ReactorNetty/1.2"),
        ("/games?query=anything", "kube-probe/1.33"),
        ("/healthz", "curl/8"),
        ("/.env", "scanner"),
        ("/wp-config.php", "scanner"),
    ] {
        let req = TestRequest::get()
            .uri(uri)
            .insert_header(("User-Agent", agent))
            .to_request();
        test::call_service(&app, req).await;
    }

    assert_eq!(
        requests(&provider, &exporter),
        points(&[
            ("/assets/{game_id} 200", 2),
            ("/games 200", 1),
            ("unmatched 404", 2),
        ])
    );
}

#[test]
fn tracked_routes_report_zero() {
    let (metrics, provider, exporter) = test_metrics();
    metrics.track(&["/game"]);

    assert_eq!(requests(&provider, &exporter), points(&[("/game 200", 0)]));
}
