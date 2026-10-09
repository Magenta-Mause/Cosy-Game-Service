use actix_web::{middleware::from_fn, web, App, HttpResponse, HttpServer};
use cosy_gameapi::{
    metrics::{track_usage, UsageMetrics, HEALTH_PATH},
    routes::{get_assets_by_id, get_game, search_games},
    GlobalState,
};

#[actix_web::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Ok(auth_key) = std::env::var("COSY_GAMEAPI_SGDB_API_KEY") else {
        return Err("COSY_GAMEAPI_SGDB_API_KEY environment variable not set".into());
    };

    let global_state = web::Data::new(GlobalState::new(&auth_key)?);

    let usage = web::Data::new(UsageMetrics::start()?);
    usage.track(&["/assets/{game_id}", "/game", "/games"]);

    HttpServer::new(move || {
        App::new()
            .wrap(from_fn(track_usage))
            .service(get_assets_by_id)
            .service(get_game)
            .service(search_games)
            .route(HEALTH_PATH, web::get().to(HttpResponse::Ok))
            .app_data(global_state.clone())
            .app_data(usage.clone())
    })
    .bind(("0.0.0.0", 8080))?
    .run()
    .await
    .map_err(|e| format!("failed to run server: {}", e))?;

    Ok(())
}
