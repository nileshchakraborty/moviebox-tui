use moviebox_tui::ai::parse_ai_media_json;
use moviebox_tui::cache::{get_working_source_cache, set_working_source_cache};
use moviebox_tui::player::aniskip::{AniSkipInterval, AniSkipTimings};
use moviebox_tui::providers::anime::crypto::decode_allanime_url;
use moviebox_tui::providers::models::ProviderKind;
use moviebox_tui::trie::{damerau_levenshtein, word_similarity, TitleTrie};
use moviebox_tui::tui::commands::{ParsedCommand, SlashCommand};

#[test]
fn test_damerau_levenshtein_distance_and_similarity() {
    // Exact match
    assert_eq!(damerau_levenshtein("matrix", "matrix"), 0);
    assert!((word_similarity("matrix", "matrix") - 1.0).abs() < 1e-6);

    // Single transposition
    assert_eq!(damerau_levenshtein("jhon", "john"), 1);

    // Typo deletion/insertion
    assert_eq!(damerau_levenshtein("pokmon", "pokemon"), 1);

    // Transposition scoring
    let sim = word_similarity("jhon", "john");
    assert!(sim >= 0.75);
}

#[test]
fn test_title_trie_fuzzy_indexing_and_search() {
    let mut trie = TitleTrie::new();
    trie.insert("101", "Spider-Man: Across the Spider-Verse", 101);
    trie.insert("102", "Oppenheimer", 102);
    trie.insert("103", "John Wick: Chapter 4", 103);
    trie.insert("104", "Interstellar", 104);

    // Search with exact prefix
    let matches = trie.search("Oppen", 5);
    assert!(!matches.is_empty());
    assert_eq!(matches[0].0, 102);

    // Search with noise words and typo
    let matches = trie.search("movie about jhon wick", 5);
    assert!(!matches.is_empty());
    assert_eq!(matches[0].0, 103);

    // Search with missing vowels
    let matches = trie.search("spdr mn", 5);
    assert!(!matches.is_empty());
    assert_eq!(matches[0].0, 101);
}

#[test]
fn test_aniskip_timings_structure() {
    let timings = AniSkipTimings {
        op: Some(AniSkipInterval {
            start_time: 90.0,
            end_time: 180.0,
        }),
        ed: Some(AniSkipInterval {
            start_time: 1350.0,
            end_time: 1440.0,
        }),
    };

    let serialized = serde_json::to_string(&timings).expect("serialize aniskip");
    let deserialized: AniSkipTimings =
        serde_json::from_str(&serialized).expect("deserialize aniskip");
    assert_eq!(timings, deserialized);
    assert_eq!(deserialized.op.unwrap().start_time, 90.0);
}

#[test]
fn test_ai_media_candidate_json_parsing() {
    let markdown_json = r#"
Here are the recommendations based on your storyline description:
```json
[
  { "title": "About Time", "type": "movie", "year": "2013" },
  { "title": "The Time Traveler's Wife", "type": "movie", "year": "2009" }
]
```
Enjoy streaming!
"#;

    let candidates = parse_ai_media_json(markdown_json);
    assert_eq!(candidates.len(), 2);
    assert_eq!(candidates[0].title, "About Time");
    assert_eq!(candidates[0].year.as_deref(), Some("2013"));
    assert_eq!(candidates[1].title, "The Time Traveler's Wife");
}

#[test]
fn test_allanime_hex_cipher_decoding() {
    // "--797a7b" -> "ABC"
    let decoded = decode_allanime_url("--797a7b");
    assert_eq!(decoded, "ABC");

    // Standard clock path decoding
    let clock_encoded = "--021717595a";
    let decoded_clock = decode_allanime_url(clock_encoded);
    assert_eq!(decoded_clock, "://ab");
}

#[test]
fn test_source_failover_cache() {
    let provider = ProviderKind::MovieBox;
    let subject_id = "test_show_12345";
    let working_source = "https://cdn.example.com/stream/index.m3u8";

    set_working_source_cache(provider, subject_id, working_source);
    let cached = get_working_source_cache(provider, subject_id);
    assert_eq!(cached.as_deref(), Some(working_source));
}

#[test]
fn test_ai_slash_command_parsing() {
    let cmd = SlashCommand::parse("/ai time traveler trying to save his family");
    assert_eq!(
        cmd,
        Some(ParsedCommand::Ai("time traveler trying to save his family"))
    );

    let empty_cmd = SlashCommand::parse("/ai");
    assert_eq!(cmd != empty_cmd, true);
    assert_eq!(empty_cmd, Some(ParsedCommand::Ai("")));
}
