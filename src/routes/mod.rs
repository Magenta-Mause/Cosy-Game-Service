mod assets;
mod games;

use actix_web::{web, HttpResponse};

pub use assets::get_assets_by_id;
pub use games::get_game;
pub use games::search_games;

pub const ASSETS_ROUTE: &str = "/assets/{game_id}";
pub const GAME_ROUTE: &str = "/game";
pub const GAMES_ROUTE: &str = "/games";

/// Answers 200 without calling SteamGridDB.
pub const HEALTH_ROUTE: &str = "/healthz";

/// Every API route pattern registered by [`configure`].
pub const API_ROUTES: [&str; 3] = [ASSETS_ROUTE, GAME_ROUTE, GAMES_ROUTE];

/// Registers all routes of the service.
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route(ASSETS_ROUTE, web::get().to(get_assets_by_id))
        .route(GAME_ROUTE, web::get().to(get_game))
        .route(GAMES_ROUTE, web::get().to(search_games))
        .route(HEALTH_ROUTE, web::get().to(HttpResponse::Ok));
}
