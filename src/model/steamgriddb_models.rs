#![allow(dead_code)] // we have to keep unused fields for deserialization, so don't warn about them

use serde::Deserialize;

#[derive(Deserialize)]
pub struct GameResponseData {
    pub id: usize,
    pub name: String,
    pub types: Vec<String>,
    pub verified: bool,
}

#[derive(Deserialize)]
pub struct GetGameResponse {
    pub success: bool,
    pub data: GameResponseData,
}

#[derive(Deserialize)]
pub struct HeroesResponse {
    pub success: bool,
    page: u32,
    total: u32,
    limit: u32,
    pub data: Vec<HeroResponseData>,
}

#[derive(Deserialize)]
pub struct HeroResponseData {
    id: u32,
    score: u32,
    style: String,
    width: u32,
    height: u32,
    nsfw: bool,
    humor: bool,
    notes: Option<String>,
    mime: String,
    language: String,
    pub url: String,
    pub thumb: String,
    lock: bool,
    epilepsy: bool,
    upvotes: u32,
    downvotes: u32,
    author: ResponseAuthor,
    tags: Option<Vec<String>>,
}

#[derive(Deserialize)]
pub struct LogosResponse {
    pub success: bool,
    page: u32,
    total: u32,
    limit: u32,
    pub data: Vec<LogoResposeData>,
}

#[derive(Deserialize)]
pub struct LogoResposeData {
    id: u32,
    score: u32,
    style: String,
    width: u32,
    height: u32,
    nsfw: bool,
    humor: bool,
    notes: Option<String>,
    mime: String,
    language: String,
    pub url: String,
    pub thumb: String,
    lock: bool,
    epilepsy: bool,
    upvotes: u32,
    downvotes: u32,
    author: ResponseAuthor,
    tags: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct ResponseAuthor {
    name: String,
    steam64: String,
    avatar: String,
}
