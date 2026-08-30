use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AiMediaCandidate {
    pub title: String,
    #[serde(rename = "type", default = "default_media_type")]
    pub media_type: String,
    #[serde(default)]
    pub year: Option<String>,
}

fn default_media_type() -> String {
    "movie".to_string()
}

#[derive(Debug, Serialize)]
struct OllamaChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<OllamaChatMessage>,
    stream: bool,
}

#[derive(Debug, Deserialize)]
struct OllamaChatResponse {
    message: Option<OllamaChatMessageContent>,
}

#[derive(Debug, Deserialize)]
struct OllamaChatMessageContent {
    content: String,
}

#[derive(Debug, Deserialize)]
struct OllamaTagsResponse {
    models: Option<Vec<OllamaModelEntry>>,
}

#[derive(Debug, Deserialize)]
struct OllamaModelEntry {
    name: String,
}

/// Resolves the active Ollama host from environment variables or defaults to localhost.
pub fn ollama_host() -> String {
    std::env::var("OLLAMA_HOST")
        .unwrap_or_else(|_| "http://127.0.0.1:11434".to_string())
        .trim_end_matches('/')
        .to_string()
}

/// Fetches available model names from Ollama.
pub async fn get_available_ollama_models(client: &reqwest::Client, host: &str) -> Vec<String> {
    let url = format!("{host}/api/tags");
    let mut request = client.get(&url);
    if let Ok(key) = std::env::var("OLLAMA_API_KEY") {
        if !key.is_empty() {
            request = request.header("Authorization", format!("Bearer {key}"));
        }
    }

    if let Ok(res) = request.send().await {
        if let Ok(tags) = res.json::<OllamaTagsResponse>().await {
            return tags
                .models
                .unwrap_or_default()
                .into_iter()
                .map(|m| m.name)
                .collect();
        }
    }
    Vec::new()
}

/// Extracts a JSON array of AiMediaCandidate from model response text.
pub fn parse_ai_media_json(text: &str) -> Vec<AiMediaCandidate> {
    let trimmed = text.trim();
    if let Ok(list) = serde_json::from_str::<Vec<AiMediaCandidate>>(trimmed) {
        return list;
    }

    // Try finding JSON array within markdown code blocks or text
    if let Some(start) = trimmed.find('[') {
        if let Some(end) = trimmed.rfind(']') {
            if end > start {
                let slice = &trimmed[start..=end];
                if let Ok(list) = serde_json::from_str::<Vec<AiMediaCandidate>>(slice) {
                    return list;
                }
            }
        }
    }

    Vec::new()
}

/// Queries DuckDuckGo HTML for live web search snippets when querying vague plot descriptions.
pub async fn fetch_ddg_snippets(client: &reqwest::Client, query: &str) -> Vec<String> {
    let clean_query = format!("{} movie tv show", query.trim());
    let encoded_query = percent_encoding::utf8_percent_encode(
        &clean_query,
        percent_encoding::NON_ALPHANUMERIC,
    )
    .to_string();
    let url = format!("https://html.duckduckgo.com/html/?q={encoded_query}");

    let res = match client
        .get(&url)
        .header(
            "User-Agent",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36",
        )
        .header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
        .send()
        .await
    {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };

    let html = match res.text().await {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };

    let document = scraper::Html::parse_document(&html);
    let selector = match scraper::Selector::parse(".result__snippet") {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let mut snippets = Vec::new();
    for element in document.select(&selector).take(6) {
        let text = element.text().collect::<Vec<_>>().join(" ");
        let cleaned = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if !cleaned.is_empty() {
            snippets.push(cleaned);
        }
    }

    snippets
}

/// Executes AI-powered semantic media discovery using local or remote Ollama
/// with DuckDuckGo RAG search snippet fallbacks.
pub async fn semantic_search(query: &str) -> Result<Vec<AiMediaCandidate>, String> {
    if query.trim().is_empty() {
        return Ok(Vec::new());
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(12))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {e}"))?;

    let host = ollama_host();
    let models = get_available_ollama_models(&client, &host).await;

    let selected_model = if let Ok(custom) = std::env::var("OLLAMA_MODEL") {
        custom
    } else {
        const PREFERRED: &[&str] = &["qwen2.5:0.5b", "llama3.2:1b", "tinyllama", "llama3.2", "mistral"];
        let found = PREFERRED
            .iter()
            .find_map(|pref| models.iter().find(|m| m.to_lowercase().contains(pref)));
        found.cloned().unwrap_or_else(|| {
            models
                .first()
                .cloned()
                .unwrap_or_else(|| "llama3.2:1b".to_string())
        })
    };

    let snippets = fetch_ddg_snippets(&client, query).await;
    let web_context = if snippets.is_empty() {
        "None available.".to_string()
    } else {
        snippets
            .iter()
            .enumerate()
            .map(|(i, s)| format!("[Web Clue {}]: {}", i + 1, s))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let system_prompt = r#"You are the MovieBox Semantic Media Matcher. The user is describing a movie, TV show, or anime by its plot, characters, or scenario rather than its exact title.
Identify up to 8 real movies or TV shows that best match the storyline.
Return ONLY a raw JSON array. No conversational text, no markdown wrappers, no explanations.
[
  { "title": "Exact Title", "type": "movie" | "series" | "anime", "year": "YYYY" }
]"#;

    let user_prompt = format!(
        "Description / Query: \"{}\"\n\nLive Web Search Clues:\n{}\n\nReturn raw JSON array ONLY:",
        query.trim(),
        web_context
    );

    let chat_req = OllamaChatRequest {
        model: selected_model,
        messages: vec![
            OllamaChatMessage {
                role: "system".to_string(),
                content: system_prompt.to_string(),
            },
            OllamaChatMessage {
                role: "user".to_string(),
                content: user_prompt,
            },
        ],
        stream: false,
    };

    let chat_url = format!("{host}/api/chat");
    let mut request = client.post(&chat_url).json(&chat_req);
    if let Ok(key) = std::env::var("OLLAMA_API_KEY") {
        if !key.is_empty() {
            request = request.header("Authorization", format!("Bearer {key}"));
        }
    }

    match request.send().await {
        Ok(res) => {
            if res.status().is_success() {
                if let Ok(resp) = res.json::<OllamaChatResponse>().await {
                    if let Some(msg) = resp.message {
                        let parsed = parse_ai_media_json(&msg.content);
                        if !parsed.is_empty() {
                            return Ok(parsed);
                        }
                    }
                }
            }
            Err("Ollama did not return matching titles".to_string())
        }
        Err(e) => Err(format!("Ollama connection error: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ai_media_json_clean() {
        let raw = r#"[{"title": "About Time", "type": "movie", "year": "2013"}, {"title": "Dark", "type": "series", "year": "2017"}]"#;
        let parsed = parse_ai_media_json(raw);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].title, "About Time");
        assert_eq!(parsed[0].year.as_deref(), Some("2013"));
    }

    #[test]
    fn test_parse_ai_media_json_with_markdown_wrapper() {
        let raw = "Here are the movies matching your plot:\n```json\n[\n  {\"title\": \"The Matrix\", \"type\": \"movie\", \"year\": \"1999\"}\n]\n```\nHope this helps!";
        let parsed = parse_ai_media_json(raw);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].title, "The Matrix");
    }
}
