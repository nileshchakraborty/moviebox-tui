use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const ANISKIP_API: &str = "https://api.aniskip.com/v2";
const CACHE_TTL_SECS: u64 = 7 * 24 * 60 * 60; // 7 days

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AniSkipInterval {
    pub start_time: f64,
    pub end_time: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct AniSkipTimings {
    pub op: Option<AniSkipInterval>,
    pub ed: Option<AniSkipInterval>,
}

#[derive(Debug, Deserialize)]
struct AniSkipRawInterval {
    #[serde(rename = "startTime")]
    start_time: f64,
    #[serde(rename = "endTime")]
    end_time: f64,
}

#[derive(Debug, Deserialize)]
struct AniSkipRawResult {
    #[serde(rename = "skipType")]
    skip_type: String,
    interval: AniSkipRawInterval,
}

#[derive(Debug, Deserialize)]
struct AniSkipResponse {
    found: bool,
    results: Option<Vec<AniSkipRawResult>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CachedTimings {
    timings: Option<AniSkipTimings>,
    expires_at: u64,
}

fn aniskip_cache_dir() -> Option<PathBuf> {
    let dir = crate::config::cache_dir().join("aniskip");
    let _ = fs::create_dir_all(&dir);
    Some(dir)
}

fn cache_file_path(mal_id: u64, episode_number: usize) -> Option<PathBuf> {
    aniskip_cache_dir().map(|dir| dir.join(format!("{mal_id}_{episode_number}.json")))
}

/// Fetches intro (OP) and outro (ED) skip timestamps from the AniSkip v2 API
/// with a 7-day disk cache.
pub async fn fetch_aniskip_timings(mal_id: u64, episode_number: usize) -> Option<AniSkipTimings> {
    if mal_id == 0 || episode_number == 0 {
        return None;
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Check disk cache
    if let Some(path) = cache_file_path(mal_id, episode_number) {
        if let Ok(data) = fs::read_to_string(&path) {
            if let Ok(cached) = serde_json::from_str::<CachedTimings>(&data) {
                if now < cached.expires_at {
                    return cached.timings;
                }
            }
        }
    }

    let url = format!(
        "{ANISKIP_API}/skip-times/{mal_id}/{episode_number}?types[]=op&types[]=ed&types[]=mixed-op&types[]=mixed-ed&episodeLength=0"
    );

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(6))
        .build()
        .ok()?;

    let res = client
        .get(&url)
        .header("User-Agent", "MovieBox-Tui/1.0 (https://github.com/nileshchakraborty/moviebox-tui)")
        .header("Accept", "application/json")
        .send()
        .await
        .ok()?;

    let expires_at = now + CACHE_TTL_SECS;

    if res.status() == reqwest::StatusCode::NOT_FOUND {
        // Cache negative result
        if let Some(path) = cache_file_path(mal_id, episode_number) {
            let cached = CachedTimings {
                timings: None,
                expires_at,
            };
            if let Ok(json) = serde_json::to_string(&cached) {
                let _ = fs::write(path, json);
            }
        }
        return None;
    }

    if !res.status().is_success() {
        return None;
    }

    let parsed = res.json::<AniSkipResponse>().await.ok()?;
    if !parsed.found || parsed.results.is_none() {
        if let Some(path) = cache_file_path(mal_id, episode_number) {
            let cached = CachedTimings {
                timings: None,
                expires_at,
            };
            if let Ok(json) = serde_json::to_string(&cached) {
                let _ = fs::write(path, json);
            }
        }
        return None;
    }

    let mut timings = AniSkipTimings::default();
    for item in parsed.results.unwrap_or_default() {
        let skip_type = item.skip_type.to_lowercase();
        let interval = AniSkipInterval {
            start_time: item.interval.start_time,
            end_time: item.interval.end_time,
        };

        if skip_type == "op" || skip_type == "mixed-op" {
            timings.op = Some(interval);
        } else if skip_type == "ed" || skip_type == "mixed-ed" {
            timings.ed = Some(interval);
        }
    }

    let result = if timings.op.is_some() || timings.ed.is_some() {
        Some(timings)
    } else {
        None
    };

    if let Some(path) = cache_file_path(mal_id, episode_number) {
        let cached = CachedTimings {
            timings: result.clone(),
            expires_at,
        };
        if let Ok(json) = serde_json::to_string(&cached) {
            let _ = fs::write(path, json);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aniskip_timings_serialization() {
        let timings = AniSkipTimings {
            op: Some(AniSkipInterval {
                start_time: 85.5,
                end_time: 175.0,
            }),
            ed: Some(AniSkipInterval {
                start_time: 1320.0,
                end_time: 1410.0,
            }),
        };

        let json = serde_json::to_string(&timings).unwrap();
        let parsed: AniSkipTimings = serde_json::from_str(&json).unwrap();
        assert_eq!(timings, parsed);
    }
}
