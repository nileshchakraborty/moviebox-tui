use crate::providers::anime::crypto::{decode_allanime_url, decode_tobeparsed, DecryptedSource};
use crate::providers::models::{
    CatalogItem, Episode, MediaDetails, MediaType, PlaybackSource, ProviderKind, ProviderMediaId,
    Season,
};
use std::time::Duration;

const ALLANIME_API: &str = "https://api.allanime.day/api";
const EPISODE_GQL_HASH: &str =
    "d405d0edd690624b66baba3068e0edc3ac90f1597d898a1ec8db4e5c43c00fec";

const PROVIDER_PRIORITY: &[&str] = &["S-mp4", "Luf-Mp4", "Yt-mp4", "Default", "Sl-Hls"];

#[derive(Debug, Clone)]
pub struct AnimeClient {
    http: reqwest::Client,
}

impl Default for AnimeClient {
    fn default() -> Self {
        Self::new()
    }
}

impl AnimeClient {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .unwrap_or_default();
        Self { http }
    }

    /// Searches AllAnime for anime series matching the query.
    pub async fn search(&self, query: &str) -> Result<Vec<CatalogItem>, String> {
        let search_query = r#"
            query ($search: SearchInput, $limit: Int, $page: Int, $translationType: VaildTranslationTypeEnumType) {
              shows(search: $search, limit: $limit, page: $page, translationType: $translationType) {
                edges {
                  _id
                  name
                  availableEpisodes
                  thumbnail
                  poster
                }
              }
            }
        "#;

        let variables = serde_json::json!({
            "search": {
                "query": query,
            },
            "limit": 24,
            "page": 1,
            "translationType": "sub"
        });

        let body = serde_json::json!({
            "query": search_query,
            "variables": variables
        });

        let res = self
            .http
            .post(ALLANIME_API)
            .header("Content-Type", "application/json")
            .header(
                "User-Agent",
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:109.0) Gecko/20100101 Firefox/121.0",
            )
            .header("Referer", "https://allmanga.to")
            .header("Origin", "https://allmanga.to")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("AllAnime search network error: {e}"))?;

        if !res.status().is_success() {
            return Err(format!("AllAnime HTTP {}", res.status()));
        }

        let json: serde_json::Value = res
            .json()
            .await
            .map_err(|e| format!("AllAnime parse error: {e}"))?;

        let edges = json
            .pointer("/data/shows/edges")
            .and_then(|e| e.as_array())
            .cloned()
            .unwrap_or_default();

        let mut items = Vec::new();
        for edge in edges {
            let id = edge.get("_id").and_then(|v| v.as_str()).unwrap_or_default();
            let name = edge.get("name").and_then(|v| v.as_str()).unwrap_or_default();
            if id.is_empty() || name.is_empty() {
                continue;
            }

            let poster_url = edge
                .get("thumbnail")
                .or_else(|| edge.get("poster"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let episodes_sub = edge
                .pointer("/availableEpisodes/sub")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;

            items.push(CatalogItem {
                id: ProviderMediaId {
                    provider: ProviderKind::Addons, // Adapts cleanly to provider views
                    value: id.to_string(),
                },
                title: name.to_string(),
                media_type: MediaType::Series,
                year: None,
                poster_url,
                season_count: Some(1.max(episodes_sub / 12)),
            });
        }

        Ok(items)
    }

    /// Fetches media details and episode list for an anime.
    pub async fn details(&self, show_id: &str) -> Result<MediaDetails, String> {
        let query = r#"
            query ($showId: String!) {
              show(_id: $showId) {
                _id
                name
                description
                thumbnail
                poster
                genres
                availableEpisodesDetail
              }
            }
        "#;

        let body = serde_json::json!({
            "query": query,
            "variables": { "showId": show_id }
        });

        let res = self
            .http
            .post(ALLANIME_API)
            .header("Content-Type", "application/json")
            .header(
                "User-Agent",
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:109.0) Gecko/20100101 Firefox/121.0",
            )
            .header("Referer", "https://allmanga.to")
            .header("Origin", "https://allmanga.to")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("AllAnime details error: {e}"))?;

        let json: serde_json::Value = res
            .json()
            .await
            .map_err(|e| format!("AllAnime parse error: {e}"))?;

        let show = json.pointer("/data/show").ok_or("Show not found")?;
        let name = show.get("name").and_then(|v| v.as_str()).unwrap_or("Anime");
        let description = show
            .get("description")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let poster_url = show
            .get("thumbnail")
            .or_else(|| show.get("poster"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let genres: Vec<String> = show
            .get("genres")
            .and_then(|g| g.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let sub_eps = show
            .pointer("/availableEpisodesDetail/sub")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let mut episodes = Vec::new();
        for (i, ep_val) in sub_eps.iter().enumerate() {
            let ep_num = ep_val
                .as_str()
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(i + 1);

            episodes.push(Episode {
                season: 1,
                number: ep_num,
                title: Some(format!("Episode {ep_num}")),
            });
        }

        if episodes.is_empty() {
            episodes.push(Episode {
                season: 1,
                number: 1,
                title: Some("Episode 1".to_string()),
            });
        }

        Ok(MediaDetails {
            id: ProviderMediaId {
                provider: ProviderKind::Addons,
                value: show_id.to_string(),
            },
            title: name.to_string(),
            media_type: MediaType::Series,
            year: None,
            description,
            tagline: None,
            imdb_rating: None,
            director: None,
            stars: None,
            prints: None,
            audios: Some("Japanese (Sub), English (Dub)".to_string()),
            poster_url,
            duration: None,
            genres,
            seasons: vec![Season {
                number: 1,
                episodes,
            }],
        })
    }

    /// Resolves playable stream URL for an episode using APQ (Automatic Persisted Queries)
    /// with `Origin: https://youtu-chan.com` and fallback AES-256-CTR decryption.
    pub async fn resolve_episode(
        &self,
        show_id: &str,
        episode_number: usize,
        dub: bool,
    ) -> Result<PlaybackSource, String> {
        let ep_str = episode_number.to_string();
        let translation_type = if dub { "dub" } else { "sub" };

        let variables = serde_json::json!({
            "showId": show_id,
            "translationType": translation_type,
            "episodeString": ep_str
        });

        let extensions = serde_json::json!({
            "persistedQuery": {
                "version": 1,
                "sha256Hash": EPISODE_GQL_HASH
            }
        });

        let vars_encoded = urlencoding_simple(&variables.to_string());
        let ext_encoded = urlencoding_simple(&extensions.to_string());
        let get_url = format!("{ALLANIME_API}?variables={vars_encoded}&extensions={ext_encoded}");

        let res = self
            .http
            .get(&get_url)
            .header(
                "User-Agent",
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:109.0) Gecko/20100101 Firefox/121.0",
            )
            .header("Referer", "https://allmanga.to")
            .header("Origin", "https://youtu-chan.com")
            .send()
            .await;

        let mut raw_body = String::new();
        if let Ok(r) = res {
            if let Ok(text) = r.text().await {
                raw_body = text;
            }
        }

        // Fallback to POST if APQ GET failed or was blocked
        if raw_body.is_empty() || !raw_body.contains("sourceUrls") && !raw_body.contains("tobeparsed") {
            let fallback_gql = r#"
                query ($showId: String!, $translationType: VaildTranslationTypeEnumType!, $episodeString: String!) {
                  episode(showId: $showId, translationType: $translationType, episodeString: $episodeString) {
                    episodeString
                    sourceUrls
                  }
                }
            "#;

            let post_body = serde_json::json!({
                "query": fallback_gql,
                "variables": variables
            });

            let post_res = self
                .http
                .post(ALLANIME_API)
                .header("Content-Type", "application/json")
                .header(
                    "User-Agent",
                    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:109.0) Gecko/20100101 Firefox/121.0",
                )
                .header("Referer", "https://allmanga.to")
                .header("Origin", "https://allmanga.to")
                .json(&post_body)
                .send()
                .await
                .map_err(|e| format!("AllAnime episode POST failed: {e}"))?;

            raw_body = post_res
                .text()
                .await
                .map_err(|e| format!("Failed reading episode body: {e}"))?;
        }

        let mut sources = Vec::new();

        // Check for encrypted tobeparsed blob
        if let Some(pos) = raw_body.find("\"tobeparsed\"") {
            let rest = &raw_body[pos..];
            if let Some(quote1) = rest.find(':').and_then(|c| rest[c..].find('"').map(|q| c + q + 1)) {
                if let Some(quote2) = rest[quote1..].find('"') {
                    let blob = &rest[quote1..quote1 + quote2];
                    sources = decode_tobeparsed(blob);
                }
            }
        }

        // If not encrypted, parse standard sourceUrls array
        if sources.is_empty() {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw_body) {
                if let Some(arr) = val.pointer("/data/episode/sourceUrls").and_then(|v| v.as_array()) {
                    for item in arr {
                        if let Some(url) = item.get("sourceUrl").and_then(|u| u.as_str()) {
                            let name = item.get("sourceName").and_then(|n| n.as_str()).unwrap_or("");
                            let prio = item.get("priority").and_then(|p| p.as_f64()).unwrap_or(0.0);
                            sources.push(DecryptedSource {
                                source_url: url.to_string(),
                                source_name: name.to_string(),
                                priority: prio,
                            });
                        }
                    }
                }
            }
        }

        if sources.is_empty() {
            return Err("No stream sources returned for episode".to_string());
        }

        // Sort by priority
        sources.sort_by(|a, b| {
            let ai = PROVIDER_PRIORITY
                .iter()
                .position(|&p| p == a.source_name)
                .unwrap_or(99);
            let bi = PROVIDER_PRIORITY
                .iter()
                .position(|&p| p == b.source_name)
                .unwrap_or(99);
            ai.cmp(&bi)
        });

        // Resolve first valid source
        for src in sources {
            let decoded_path = decode_allanime_url(&src.source_url);
            let mut final_url = decoded_path.replace("/clock", "/clock.json");

            if final_url.starts_with("//") {
                final_url = format!("https:{final_url}");
            } else if final_url.starts_with('/') {
                final_url = format!("https://allanime.day{final_url}");
            } else if !final_url.starts_with("http") {
                final_url = format!("https://allanime.day/{final_url}");
            }

            if final_url.contains("/clock.json") {
                // Fetch clock json
                if let Ok(clock_res) = self
                    .http
                    .get(&final_url)
                    .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
                    .header("Referer", "https://allmanga.to")
                    .send()
                    .await
                {
                    if let Ok(clock_json) = clock_res.json::<serde_json::Value>().await {
                        if let Some(links) = clock_json.get("links").and_then(|l| l.as_array()) {
                            if let Some(first_link) = links.first().and_then(|l| l.get("link")).and_then(|l| l.as_str()) {
                                return Ok(PlaybackSource {
                                    provider: ProviderKind::Addons,
                                    url: first_link.to_string(),
                                    headers: vec![
                                        ("Referer".to_string(), "https://allmanga.to".to_string()),
                                        ("User-Agent".to_string(), "Mozilla/5.0".to_string()),
                                    ],
                                    subtitle: None,
                                    source_label: format!("Anime [{}]", src.source_name),
                                });
                            }
                        }
                    }
                }
            } else if final_url.starts_with("http") {
                return Ok(PlaybackSource {
                    provider: ProviderKind::Addons,
                    url: final_url,
                    headers: vec![
                        ("Referer".to_string(), "https://allmanga.to".to_string()),
                        ("User-Agent".to_string(), "Mozilla/5.0".to_string()),
                    ],
                    subtitle: None,
                    source_label: format!("Anime [{}]", src.source_name),
                });
            }
        }

        Err("Failed to resolve playable anime stream".to_string())
    }
}

fn urlencoding_simple(s: &str) -> String {
    percent_encoding::utf8_percent_encode(s, percent_encoding::NON_ALPHANUMERIC).to_string()
}
