use std::{error::Error, sync::Arc};

use reqwest::header::{HeaderMap, HeaderValue};
use steamgriddb_api::{games::GameInfo, search::SearchResult};

use crate::services::steamgriddb_service::SteamgriddbService;

pub struct GlobalState {
    steamgriddb_api_client: Arc<steamgriddb_api::client::Client>,
    reqwest_client: Arc<reqwest::Client>,
    base_url: String,
}

impl GlobalState {
    pub fn new(auth_key: &str) -> Result<Self, Box<dyn Error>> {
        let mut client_headers: HeaderMap<HeaderValue> = reqwest::header::HeaderMap::default();
        client_headers.insert(
            "Authorization",
            format!("Bearer {}", auth_key)
                .parse()
                .map_err(|e| format!("Failed to parse auth header: {}", e))?,
        );

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .default_headers(client_headers)
            .build()?;

        let sgdb_client = steamgriddb_api::Client::new(auth_key);
        let base_url = sgdb_client.base_url().to_string();

        Ok(Self {
            steamgriddb_api_client: Arc::new(sgdb_client),
            reqwest_client: Arc::new(client),
            base_url,
        })
    }

    pub async fn search_api(&self, query: &str) -> Result<Vec<SearchResult>, Box<dyn Error>> {
        self.steamgriddb_api_client.search(query).await
    }

    pub fn steamgriddb_service(&self) -> SteamgriddbService {
        SteamgriddbService::new(
            self.steamgriddb_api_client.clone(),
            self.reqwest_client.clone(),
            self.base_url.clone(),
        )
    }
}
