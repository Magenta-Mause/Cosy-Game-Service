//! Usage metrics: how often each route is called, so this legacy service can
//! be retired once nobody uses it any more.

use actix_web::{
    body::MessageBody,
    dev::{ServiceRequest, ServiceResponse},
    http::header::USER_AGENT,
    middleware::Next,
    web::Data,
    Error,
};
use opentelemetry::{
    metrics::{Counter, Meter, MeterProvider},
    KeyValue,
};
use opentelemetry_otlp::MetricExporter;
use opentelemetry_sdk::{metrics::SdkMeterProvider, Resource};

const SERVICE_NAME: &str = "cosy-gameapi";

/// Groups every request that hit no registered route, so scanners probing
/// random paths cannot create one series per path.
const UNMATCHED_ROUTE: &str = "unmatched";

pub struct UsageMetrics {
    requests: Counter<u64>,
    ignored_routes: Vec<String>,
    /// Dropping the provider stops the export, so it lives as long as the metrics.
    provider: Option<SdkMeterProvider>,
}

impl UsageMetrics {
    /// Returns metrics that are pushed over OTLP/HTTP. The exporter is
    /// configured through the standard `OTEL_EXPORTER_OTLP_*` environment
    /// variables; without an endpoint nothing is exported. Requests to the
    /// ignored routes are not counted.
    pub fn start(ignored_routes: &[&str]) -> Result<Self, Box<dyn std::error::Error>> {
        let endpoint_set = [
            "OTEL_EXPORTER_OTLP_ENDPOINT",
            "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT",
        ]
        .iter()
        .any(|name| std::env::var(name).is_ok_and(|value| !value.is_empty()));
        if !endpoint_set {
            let meter = opentelemetry::metrics::noop::NoopMeterProvider::new().meter(SERVICE_NAME);
            return Ok(Self::with_meter(&meter, ignored_routes, None));
        }

        // OTEL_SERVICE_NAME and OTEL_RESOURCE_ATTRIBUTES are picked up by the
        // default resource and take precedence over the default service name.
        let mut resource = Resource::builder();
        if std::env::var("OTEL_SERVICE_NAME").is_err() {
            resource = resource.with_service_name(SERVICE_NAME);
        }
        let provider = SdkMeterProvider::builder()
            .with_resource(resource.build())
            .with_periodic_exporter(MetricExporter::builder().with_http().build()?)
            .build();

        let meter = provider.meter(SERVICE_NAME);
        Ok(Self::with_meter(&meter, ignored_routes, Some(provider)))
    }

    /// Returns metrics that report through the given meter and keep
    /// `provider` alive for as long as they exist.
    pub fn with_meter(
        meter: &Meter,
        ignored_routes: &[&str],
        provider: Option<SdkMeterProvider>,
    ) -> Self {
        let requests = meter
            .u64_counter("cosy_gameapi_http_requests")
            .with_description(
                "HTTP requests by route and status code. Kubernetes probes are not counted.",
            )
            .build();

        Self {
            requests,
            ignored_routes: ignored_routes
                .iter()
                .map(|route| route.to_string())
                .collect(),
            provider,
        }
    }

    /// Makes the given routes report zero from the start, so a route nobody
    /// calls shows up as 0 rather than as missing data.
    pub fn track(&self, routes: &[&str]) {
        for route in routes {
            self.requests.add(0, &request_attributes(route, 200));
        }
    }

    /// Exports what has been counted since the last interval and stops the
    /// exporter.
    pub fn shutdown(&self) {
        if let Some(provider) = &self.provider {
            if let Err(error) = provider.shutdown() {
                eprintln!("failed to flush usage metrics: {error}");
            }
        }
    }

    fn record(&self, route: Option<&str>, status: u16) {
        self.requests.add(
            1,
            &request_attributes(route.unwrap_or(UNMATCHED_ROUTE), status),
        );
    }
}

fn request_attributes(route: &str, status: u16) -> [KeyValue; 2] {
    [
        KeyValue::new("route", route.to_owned()),
        KeyValue::new("status", status.to_string()),
    ]
}

/// Middleware that counts every request after it has been handled and writes
/// one log line per request. Needs a `Data<UsageMetrics>` registered on the app.
pub async fn track_usage(
    req: ServiceRequest,
    next: Next<impl MessageBody>,
) -> Result<ServiceResponse<impl MessageBody>, Error> {
    let metrics = req.app_data::<Data<UsageMetrics>>().cloned();
    let agent = req
        .headers()
        .get(USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("-")
        .to_owned();
    let client = req
        .connection_info()
        .realip_remote_addr()
        .unwrap_or("-")
        .to_owned();
    let method = req.method().clone();

    let res = next.call(req).await?;

    let route = res.request().match_pattern();
    let status = res.status().as_u16();
    // The readiness probe calls a real endpoint every few seconds and would
    // otherwise drown out the actual callers.
    let is_probe = agent.starts_with("kube-probe/");
    let is_ignored = metrics.as_ref().is_some_and(|metrics| {
        route
            .as_ref()
            .is_some_and(|route| metrics.ignored_routes.contains(route))
    });
    if is_probe || is_ignored {
        return Ok(res);
    }

    if let Some(metrics) = metrics {
        metrics.record(route.as_deref(), status);
    }
    // The request log is where the number of distinct callers is read from.
    println!(
        "request method={method} route={route:?} status={status} client={client} agent={agent:?}",
        route = route.as_deref().unwrap_or(UNMATCHED_ROUTE),
    );
    Ok(res)
}
