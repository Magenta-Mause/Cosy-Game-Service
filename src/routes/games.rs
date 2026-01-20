use actix_web::{
    get,
    http::StatusCode,
    web::{Data, Query},
};
use futures::StreamExt;
use serde::Deserialize;

use crate::{
    model::{Game, GameList, Response},
    GlobalState,
};

#[derive(Deserialize)]
pub struct SearchGamesQuery {
    pub query: String,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub include_hero: Option<bool>,
    pub include_logo: Option<bool>,
}

#[derive(Deserialize)]
pub struct GetGameQuery {
    pub id: usize,
    pub include_hero: Option<bool>,
    pub include_logo: Option<bool>,
}

#[get("/game")]
pub async fn get_game(
    global_data: Data<GlobalState>,
    query: Query<GetGameQuery>,
) -> Response<Game> {
    let game_res = global_data
        .steamgriddb_service()
        .get_game_by_id(query.id)
        .await;

    if game_res.is_err() {
        return Response::error("failed to fetch game".into(), StatusCode::NOT_FOUND);
    }

    let mut game = game_res.unwrap();

    let service = global_data.steamgriddb_service();
    if query.include_logo.unwrap_or(false) {
        if let Ok(Some(url)) = service.get_first_logo_by_game_id(game.id).await {
            game.logo_url = Some(url);
        }
    }

    if query.include_hero.unwrap_or(false) {
        if let Ok(Some(url)) = service.get_first_hero_by_game_id(game.id).await {
            game.hero_url = Some(url);
        }
    }

    Response::success(game)
}

#[get("/games")]
pub async fn search_games(
    global_data: Data<GlobalState>,
    query: Query<SearchGamesQuery>,
) -> Response<GameList> {
    let Ok(results) = global_data.search_api(&query.query).await else {
        return Response::error(
            "Failed to fetch search results".into(),
            StatusCode::INTERNAL_SERVER_ERROR,
        );
    };

    let is_final =
        results.len() <= query.limit.unwrap_or(15) as usize + query.offset.unwrap_or(0) as usize;

    let offset = query.offset.unwrap_or(0) as usize;
    let limit = query.limit.unwrap_or(15) as usize;

    let mut games: Vec<Game> = results
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|res| res.into())
        .collect();

    let include_logo = query.include_logo.unwrap_or(false);
    let include_hero = query.include_hero.unwrap_or(false);

    /* Parallelize logo/hero fetching */
    futures::stream::iter(games.iter_mut())
        .map(|game| {
            let global = global_data.clone();
            async move {
                // create a per-task service instance (cheap, clones Arcs)
                let service = global.steamgriddb_service();

                if include_logo {
                    if let Ok(Some(url)) = service.get_first_logo_by_game_id(game.id).await {
                        game.logo_url = Some(url);
                    }
                }
                if include_hero {
                    if let Ok(Some(url)) = service.get_first_hero_by_game_id(game.id).await {
                        game.hero_url = Some(url);
                    }
                }
            }
        })
        .buffer_unordered(8)
        .for_each(|_| async {})
        .await;

    Response::success(GameList { games, is_final })
}
