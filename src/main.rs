use actix_web::{middleware::from_fn, web, App, HttpServer};
use cosy_gameapi::{
    metrics::{track_usage, UsageMetrics},
    routes::{configure, API_ROUTES, HEALTH_ROUTE},
    GlobalState,
};

#[actix_web::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Ok(auth_key) = std::env::var("COSY_GAMEAPI_SGDB_API_KEY") else {
        return Err("COSY_GAMEAPI_SGDB_API_KEY environment variable not set".into());
    };

    let global_state = web::Data::new(GlobalState::new(&auth_key)?);

    let usage = web::Data::new(UsageMetrics::start(&[HEALTH_ROUTE])?);
    usage.track(&API_ROUTES);

    let server = {
        let usage = usage.clone();
        HttpServer::new(move || {
            App::new()
                .wrap(from_fn(track_usage))
                .configure(configure)
                .app_data(global_state.clone())
                .app_data(usage.clone())
        })
        .bind(("0.0.0.0", 8080))?
        .run()
    };

    // The server returns once SIGINT/SIGTERM has stopped it. Push the metrics
    // counted since the last export so they are not lost with the process.
    let result = server.await;
    usage.shutdown();
    result.map_err(|e| format!("failed to run server: {}", e))?;

    Ok(())
}
