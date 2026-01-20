use std::error::Error;
use std::sync::Arc;

use reqwest::{Client as ReqwestClient, StatusCode};

use crate::{
    steamgriddb_models::{self, GetGameResponse},
    Game,
};

pub struct SteamgriddbService {
    sg_client: Arc<steamgriddb_api::client::Client>,
    req_client: Arc<ReqwestClient>,
    base_url: String,
}

impl SteamgriddbService {
    pub fn new(
        sg_client: Arc<steamgriddb_api::client::Client>,
        req_client: Arc<ReqwestClient>,
        base_url: String,
    ) -> Self {
        Self {
            sg_client,
            req_client,
            base_url,
        }
    }

    pub async fn fetch_assets_by_game_id(
        &self,
        game_id: usize,
    ) -> Result<Vec<steamgriddb_api::images::Image>, Box<dyn Error>> {
        self.sg_client
            .get_images_for_id(game_id, &steamgriddb_api::QueryType::Grid(None))
            .await
    }

    pub async fn get_game_by_id(&self, game_id: usize) -> Result<Game, Box<dyn Error>> {
        let game_url = format!("{}/games/id/{}", self.base_url, game_id);
        let game_res = self.req_client.get(game_url).send().await?;

        if game_res.status() == StatusCode::NOT_FOUND {
            return Err("game not found".into());
        }

        if !game_res.status().is_success() {
            let error_msg = game_res.status().to_string();

            return Err(<Box<dyn Error>>::from(format!(
                "failed to fetch game: {}",
                error_msg
            )));
        };

        let game_res_json: GetGameResponse = serde_json::from_str(&game_res.text().await?)?;

        if !game_res_json.success {
            return Err("steamgriddb API returned success=false for game".into());
        }

        Ok(Game {
            id: game_res_json.data.id,
            name: game_res_json.data.name,
            logo_url: None,
            hero_url: None,
        })
    }

    pub async fn get_first_logo_by_game_id(
        &self,
        game_id: usize,
    ) -> Result<Option<String>, Box<dyn Error>> {
        let logos_url = format!("{}/logos/game/{}", self.base_url, game_id);

        let logos_resp = self
            .req_client
            .get(logos_url)
            .query(&[("limit", 1)])
            .send()
            .await?;

        if !logos_resp.status().is_success() {
            return Err("Failed to fetch logo".into());
        }

        let logos_resp_json: steamgriddb_models::LogosResponse =
            serde_json::from_str(&logos_resp.text().await?)?;

        if !logos_resp_json.success {
            return Err("steamgriddb API returned success=false for logos".into());
        }

        if logos_resp_json.data.is_empty() {
            return Ok(None);
        }

        let first = logos_resp_json
            .data
            .first()
            .expect("checked non-empty above");

        Ok(Some(first.url.to_owned()))
    }

    pub async fn get_first_hero_by_game_id(
        &self,
        game_id: usize,
    ) -> Result<Option<String>, Box<dyn Error>> {
        let heroes_url = format!("{}/heroes/game/{}", self.base_url, game_id);

        let heroes_resp = self
            .req_client
            .get(heroes_url)
            .query(&[("limit", 1)])
            .send()
            .await?;

        if !heroes_resp.status().is_success() {
            return Err("Failed to fetch heroes".into());
        }

        let heroes_resp_json: steamgriddb_models::HeroesResponse =
            serde_json::from_str(&heroes_resp.text().await?)?;

        if !heroes_resp_json.success {
            return Err("steamgriddb API returned success=false for heroes".into());
        }

        if heroes_resp_json.data.is_empty() {
            return Ok(None);
        }

        let first = heroes_resp_json
            .data
            .first()
            .expect("checked non-empty above");

        Ok(Some(first.url.to_owned()))
    }
}
