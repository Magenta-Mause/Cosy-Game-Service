//! Usage metrics: how often each route is called and by how many distinct
//! callers, so this legacy service can be retired once nobody uses it any more.

use std::{
    collections::{hash_map::RandomState, HashMap},
    hash::BuildHasher,
    sync::Mutex,
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
use prometheus::{Encoder, IntCounterVec, IntGaugeVec, Opts, Registry, TextEncoder};

/// Excluded from the usage metrics.
pub const HEALTH_PATH: &str = "/healthz";

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
    registry: Registry,
    requests: IntCounterVec,
    unique_clients: IntGaugeVec,
    /// Random per process: caller addresses are only ever held as a seeded
    /// hash, never stored or exported.
    hasher: RandomState,
    /// route -> caller hash -> last request
    last_seen: Mutex<HashMap<String, HashMap<u64, Instant>>>,
}

impl UsageMetrics {
    pub fn new() -> Result<Self, prometheus::Error> {
        let registry = Registry::new();
        let requests = IntCounterVec::new(
            Opts::new(
                "cosy_gameapi_http_requests_total",
                "HTTP requests by route and status code. Kubernetes probes are not counted.",
            ),
            &["route", "status"],
        )?;
        let unique_clients = IntGaugeVec::new(
            Opts::new(
                "cosy_gameapi_unique_clients",
                "Distinct callers per route within the look-back window. Counted in memory, so it starts from zero after a restart.",
            ),
            &["route", "window"],
        )?;
        registry.register(Box::new(requests.clone()))?;
        registry.register(Box::new(unique_clients.clone()))?;

        Ok(Self {
            registry,
            requests,
            unique_clients,
            hasher: RandomState::new(),
            last_seen: Mutex::new(HashMap::new()),
        })
    }

    /// Makes the given routes report zero from the start, so a route nobody
    /// calls shows up as 0 rather than as missing data.
    pub fn track(&self, routes: &[&str]) {
        let mut last_seen = self.last_seen.lock().unwrap();
        for route in routes {
            self.requests.with_label_values(&[route, "200"]);
            last_seen.entry(route.to_string()).or_default();
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
        self.requests
            .with_label_values(&[route.unwrap_or(UNMATCHED_ROUTE), &status.to_string()])
            .inc();

        let (Some(route), Some(client)) = (route, client) else {
            return;
        };
        let key = self.hasher.hash_one(client);
        let mut last_seen = self.last_seen.lock().unwrap();
        let clients = last_seen.entry(route.to_string()).or_default();
        if !clients.contains_key(&key) && clients.len() >= MAX_CLIENTS_PER_ROUTE {
            prune(clients, now);
            if clients.len() >= MAX_CLIENTS_PER_ROUTE {
                return;
            }
        }
        clients.insert(key, now);
    }

    /// Renders all metrics in the Prometheus text format.
    pub fn render(&self) -> String {
        self.render_at(Instant::now())
    }

    pub fn render_at(&self, now: Instant) -> String {
        {
            let mut last_seen = self.last_seen.lock().unwrap();
            for (route, clients) in last_seen.iter_mut() {
                prune(clients, now);
                for (label, window) in CLIENT_WINDOWS {
                    let count = clients
                        .values()
                        .filter(|last| now.saturating_duration_since(**last) <= window)
                        .count();
                    self.unique_clients
                        .with_label_values(&[route.as_str(), label])
                        .set(count as i64);
                }
            }
        }

        let mut buffer = Vec::new();
        // Encoding into a Vec cannot fail.
        let _ = TextEncoder::new().encode(&self.registry.gather(), &mut buffer);
        String::from_utf8(buffer).unwrap_or_default()
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
