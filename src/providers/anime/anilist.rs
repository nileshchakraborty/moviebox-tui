use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AniListMediaTitle {
    pub english: Option<String>,
    pub romaji: Option<String>,
    pub native: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AniListMediaItem {
    pub id: u64,
    #[serde(rename = "idMal")]
    pub id_mal: Option<u64>,
    pub title: AniListMediaTitle,
    pub format: Option<String>,
    pub status: Option<String>,
    pub episodes: Option<usize>,
    #[serde(rename = "seasonYear")]
    pub season_year: Option<u32>,
    #[serde(rename = "averageScore")]
    pub average_score: Option<u32>,
    pub genres: Option<Vec<String>>,
    pub description: Option<String>,
}

impl AniListMediaItem {
    pub fn display_title(&self) -> String {
        self.title
            .english
            .as_ref()
            .or(self.title.romaji.as_ref())
            .or(self.title.native.as_ref())
            .cloned()
            .unwrap_or_else(|| "Untitled Anime".to_string())
    }
}

#[derive(Debug, Deserialize)]
struct AniListSearchResponse {
    data: Option<AniListSearchData>,
}

#[derive(Debug, Deserialize)]
struct AniListSearchData {
    #[serde(rename = "Page")]
    page: Option<AniListPageData>,
}

#[derive(Debug, Deserialize)]
struct AniListPageData {
    media: Option<Vec<AniListMediaItem>>,
}

/// Searches AniList GraphQL for anime titles matching a query.
pub async fn search_anilist(query: &str) -> Vec<AniListMediaItem> {
    if query.trim().is_empty() {
        return Vec::new();
    }

    let gql_query = r#"
        query ($search: String) {
          Page(page: 1, perPage: 10) {
            media(search: $search, type: ANIME, sort: POPULARITY_DESC) {
              id
              idMal
              title {
                romaji
                english
                native
              }
              format
              status
              episodes
              seasonYear
              averageScore
              genres
              description(asHtml: false)
            }
          }
        }
    "#;

    let body = serde_json::json!({
        "query": gql_query,
        "variables": { "search": query }
    });

    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
    {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let res = match client
        .post("https://graphql.anilist.co")
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .json(&body)
        .send()
        .await
    {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };

    if !res.status().is_success() {
        return Vec::new();
    }

    let parsed = match res.json::<AniListSearchResponse>().await {
        Ok(p) => p,
        Err(_) => return Vec::new(),
    };

    parsed
        .data
        .and_then(|d| d.page)
        .and_then(|p| p.media)
        .unwrap_or_default()
}

/// Resolves accurate multi-season title names for anime sequels (e.g. S2/S3 specific titles).
pub async fn resolve_season_title(base_title: &str, season: usize) -> Option<String> {
    if season <= 1 {
        return Some(base_title.to_string());
    }

    let gql_query = r#"
        query ($search: String) {
          Media(search: $search, type: ANIME, sort: SEARCH_MATCH) {
            title { english romaji }
            relations {
              edges {
                relationType
                node {
                  type
                  format
                  title { english romaji }
                  startDate { year }
                  seasonYear
                }
              }
            }
          }
        }
    "#;

    let body = serde_json::json!({
        "query": gql_query,
        "variables": { "search": base_title }
    });

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
        .ok()?;

    let res = client
        .post("https://graphql.anilist.co")
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .json(&body)
        .send()
        .await
        .ok()?;

    let json: serde_json::Value = res.json().await.ok()?;
    let edges = json.pointer("/data/Media/relations/edges")?.as_array()?;

    let mut sequels: Vec<&serde_json::Value> = edges
        .iter()
        .filter(|e| {
            e.get("relationType").and_then(|r| r.as_str()) == Some("SEQUEL")
                && e.pointer("/node/type").and_then(|t| t.as_str()) == Some("ANIME")
        })
        .collect();

    // Sort sequels chronologically
    sequels.sort_by_key(|e| {
        e.pointer("/node/startDate/year")
            .and_then(|y| y.as_u64())
            .or_else(|| e.pointer("/node/seasonYear").and_then(|y| y.as_u64()))
            .unwrap_or(9999)
    });

    let target_idx = season - 2;
    if target_idx < sequels.len() {
        let node = sequels[target_idx].get("node")?;
        let english = node.pointer("/title/english").and_then(|t| t.as_str());
        let romaji = node.pointer("/title/romaji").and_then(|t| t.as_str());
        return english.or(romaji).map(|s| s.to_string());
    }

    None
}
