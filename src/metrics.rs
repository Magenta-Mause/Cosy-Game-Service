//! Usage metrics: how often each route is called and by how many distinct
//! callers, so this legacy service can be retired once nobody uses it any more.

use std::{
    collections::{hash_map::RandomState, HashMap},
    hash::BuildHasher,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

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

/// Excluded from the usage metrics.
pub const HEALTH_PATH: &str = "/healthz";

const SERVICE_NAME: &str = "cosy-gameapi";

/// Groups every request that hit no registered route, so scanners probing
/// random paths cannot create one series per path.
const UNMATCHED_ROUTE: &str = "unmatched";

/// Bounds the memory the distinct-caller tracking can use.
const MAX_CLIENTS_PER_ROUTE: usize = 50_000;

const DAY: Duration = Duration::from_secs(24 * 60 * 60);

/// Look-back windows reported by the unique-clients gauge, shortest first.
const CLIENT_WINDOWS: [(&str, Duration); 3] = [
    ("1d", DAY),
    ("7d", DAY.saturating_mul(7)),
    ("30d", DAY.saturating_mul(30)),
];

pub struct UsageMetrics {
    requests: Counter<u64>,
    clients: Arc<ClientTracker>,
    /// Dropping the provider stops the export, so it lives as long as the metrics.
    _provider: Option<SdkMeterProvider>,
}

impl UsageMetrics {
    /// Returns metrics that are pushed over OTLP/HTTP. The exporter is
    /// configured through the standard `OTEL_EXPORTER_OTLP_*` environment
    /// variables; without an endpoint nothing is exported.
    pub fn start() -> Result<Self, Box<dyn std::error::Error>> {
        let endpoint_set = [
            "OTEL_EXPORTER_OTLP_ENDPOINT",
            "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT",
        ]
        .iter()
        .any(|name| std::env::var(name).is_ok_and(|value| !value.is_empty()));
        if !endpoint_set {
            let meter = opentelemetry::metrics::noop::NoopMeterProvider::new().meter(SERVICE_NAME);
            return Ok(Self::new(&meter));
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

        let mut metrics = Self::new(&provider.meter(SERVICE_NAME));
        metrics._provider = Some(provider);
        Ok(metrics)
    }

    /// Returns metrics that report through the given meter.
    pub fn new(meter: &Meter) -> Self {
        let requests = meter
            .u64_counter("cosy_gameapi_http_requests")
            .with_description(
                "HTTP requests by route and status code. Kubernetes probes are not counted.",
            )
            .build();

        let clients = Arc::new(ClientTracker::default());
        let observed = clients.clone();
        meter
            .u64_observable_gauge("cosy_gameapi_unique_clients")
            .with_description("Distinct callers per route within the look-back window. Counted in memory, so it starts from zero after a restart.")
            .with_callback(move |observer| {
                for (route, window, count) in observed.counts(Instant::now()) {
                    observer.observe(
                        count,
                        &[KeyValue::new("route", route), KeyValue::new("window", window)],
                    );
                }
            })
            .build();

        Self {
            requests,
            clients,
            _provider: None,
        }
    }

    /// Makes the given routes report zero from the start, so a route nobody
    /// calls shows up as 0 rather than as missing data.
    pub fn track(&self, routes: &[&str]) {
        for route in routes {
            self.requests.add(0, &request_attributes(route, 200));
            self.clients.track(route);
        }
    }

    /// Counts one handled request. `route` is the matched route pattern, or
    /// `None` if the request matched no route.
    pub fn record(&self, route: Option<&str>, status: u16, client: Option<&str>) {
        self.record_at(route, status, client, Instant::now());
    }

    pub fn record_at(&self, route: Option<&str>, status: u16, client: Option<&str>, now: Instant) {
        if route == Some(HEALTH_PATH) {
            return;
        }
        self.requests.add(
            1,
            &request_attributes(route.unwrap_or(UNMATCHED_ROUTE), status),
        );
        if let (Some(route), Some(client)) = (route, client) {
            self.clients.seen(route, client, now);
        }
    }

    /// Distinct callers as `(route, window, count)` for every look-back window.
    pub fn unique_clients_at(&self, now: Instant) -> Vec<(String, &'static str, u64)> {
        self.clients.counts(now)
    }
}

fn request_attributes(route: &str, status: u16) -> [KeyValue; 2] {
    [
        KeyValue::new("route", route.to_owned()),
        KeyValue::new("status", status.to_string()),
    ]
}

struct ClientTracker {
    /// Random per process: caller addresses are only ever held as a seeded
    /// hash, never stored or exported.
    hasher: RandomState,
    /// route -> caller hash -> last request
    last_seen: Mutex<HashMap<String, HashMap<u64, Instant>>>,
}

impl Default for ClientTracker {
    fn default() -> Self {
        Self {
            hasher: RandomState::new(),
            last_seen: Mutex::new(HashMap::new()),
        }
    }
}

impl ClientTracker {
    fn track(&self, route: &str) {
        let mut last_seen = self.last_seen.lock().unwrap();
        last_seen.entry(route.to_owned()).or_default();
    }

    fn seen(&self, route: &str, client: &str, now: Instant) {
        let key = self.hasher.hash_one(client);
        let mut last_seen = self.last_seen.lock().unwrap();
        let clients = last_seen.entry(route.to_owned()).or_default();
        if !clients.contains_key(&key) && clients.len() >= MAX_CLIENTS_PER_ROUTE {
            prune(clients, now);
            if clients.len() >= MAX_CLIENTS_PER_ROUTE {
                return;
            }
        }
        clients.insert(key, now);
    }

    fn counts(&self, now: Instant) -> Vec<(String, &'static str, u64)> {
        let mut last_seen = self.last_seen.lock().unwrap();
        let mut counts = Vec::with_capacity(last_seen.len() * CLIENT_WINDOWS.len());
        for (route, clients) in last_seen.iter_mut() {
            prune(clients, now);
            for (label, window) in CLIENT_WINDOWS {
                let count = clients
                    .values()
                    .filter(|last| now.saturating_duration_since(**last) <= window)
                    .count();
                counts.push((route.clone(), label, count as u64));
            }
        }
        counts
    }
}

/// Drops callers that are older than the longest window.
fn prune(clients: &mut HashMap<u64, Instant>, now: Instant) {
    let (_, longest) = CLIENT_WINDOWS[CLIENT_WINDOWS.len() - 1];
    clients.retain(|_, last| now.saturating_duration_since(*last) <= longest);
}

/// Middleware that counts every request after it has been handled. Needs a
/// `Data<UsageMetrics>` registered on the app.
pub async fn track_usage(
    req: ServiceRequest,
    next: Next<impl MessageBody>,
) -> Result<ServiceResponse<impl MessageBody>, Error> {
    let metrics = req.app_data::<Data<UsageMetrics>>().cloned();
    // The readiness probe calls a real endpoint every few seconds and would
    // otherwise drown out the actual callers.
    let is_probe = req
        .headers()
        .get(USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|agent| agent.starts_with("kube-probe/"));
    let client = req
        .connection_info()
        .realip_remote_addr()
        .map(str::to_owned);

    let res = next.call(req).await?;

    if let (Some(metrics), false) = (metrics, is_probe) {
        metrics.record(
            res.request().match_pattern().as_deref(),
            res.status().as_u16(),
            client.as_deref(),
        );
    }
    Ok(res)
}
