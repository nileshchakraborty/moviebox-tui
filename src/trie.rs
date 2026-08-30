use std::collections::{HashMap, HashSet};

/// Words commonly present in natural language queries that add noise to title matching.
pub static QUERY_NOISE_WORDS: &[&str] = &[
    "movie", "movies", "film", "films", "show", "shows", "series", "tv", "anime",
    "starring", "actor", "actress", "director", "filmmaker", "where", "with", "in",
    "is", "of", "the", "a", "an", "by", "best", "top", "list", "find", "me", "who", "played",
];

/// Normalizes a string for search comparison:
/// Converts to lowercase, strips accents/diacritics, replaces punctuation with spaces.
pub fn normalize_title(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch.is_alphanumeric() {
            for lower in ch.to_lowercase() {
                normalized.push(lower);
            }
        } else if !normalized.ends_with(' ') {
            normalized.push(' ');
        }
    }
    normalized.trim().to_string()
}

/// Computes Damerau-Levenshtein distance between two strings with support
/// for single-character transpositions, deletions, insertions, and substitutions.
pub fn damerau_levenshtein(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let (n, m) = (a_chars.len(), b_chars.len());

    if n == 0 {
        return m;
    }
    if m == 0 {
        return n;
    }

    let mut dp = vec![vec![0usize; m + 1]; n + 1];

    for i in 0..=n {
        dp[i][0] = i;
    }
    for j in 0..=m {
        dp[0][j] = j;
    }

    for i in 1..=n {
        for j in 1..=m {
            let cost = if a_chars[i - 1] == b_chars[j - 1] { 0 } else { 1 };

            dp[i][j] = (dp[i - 1][j] + 1) // deletion
                .min(dp[i][j - 1] + 1) // insertion
                .min(dp[i - 1][j - 1] + cost); // substitution

            // Transposition check
            if i > 1
                && j > 1
                && a_chars[i - 1] == b_chars[j - 2]
                && a_chars[i - 2] == b_chars[j - 1]
            {
                dp[i][j] = dp[i][j].min(dp[i - 2][j - 2] + cost);
            }
        }
    }

    dp[n][m]
}

/// Calculates normalized word similarity between 0.0 and 1.0.
pub fn word_similarity(w1: &str, w2: &str) -> f64 {
    if w1.is_empty() || w2.is_empty() {
        return 0.0;
    }
    if w1 == w2 {
        return 1.0;
    }
    let max_len = w1.chars().count().max(w2.chars().count());
    if max_len == 0 {
        return 0.0;
    }
    let dist = damerau_levenshtein(w1, w2);
    (1.0 - (dist as f64 / max_len as f64)).max(0.0)
}

#[derive(Debug, Clone)]
struct TrieNode<T: Clone> {
    children: HashMap<char, TrieNode<T>>,
    items: Vec<(String, String, T)>, // (item_id, item_title, data)
}

impl<T: Clone> Default for TrieNode<T> {
    fn default() -> Self {
        Self {
            children: HashMap::new(),
            items: Vec::new(),
        }
    }
}

/// In-memory prefix Trie for fast typo-tolerant searching across titles and entities.
#[derive(Debug, Clone)]
pub struct TitleTrie<T: Clone> {
    root: TrieNode<T>,
}

impl<T: Clone> Default for TitleTrie<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Clone> TitleTrie<T> {
    pub fn new() -> Self {
        Self {
            root: TrieNode::default(),
        }
    }

    /// Inserts an item into the Trie under its full title and individual words.
    pub fn insert(&mut self, id: &str, title: &str, data: T) {
        let norm_title = normalize_title(title);
        if norm_title.is_empty() {
            return;
        }

        // Insert full title
        self.insert_sequence(&norm_title, id, title, data.clone());

        // Insert individual words (>= 2 chars) for multi-word partial matching
        for word in norm_title.split_whitespace().filter(|w| w.len() >= 2) {
            self.insert_sequence(word, id, title, data.clone());
        }
    }

    fn insert_sequence(&mut self, seq: &str, id: &str, raw_title: &str, data: T) {
        let mut curr = &mut self.root;
        for ch in seq.chars() {
            curr = curr.children.entry(ch).or_default();
        }
        if !curr.items.iter().any(|(i, _, _)| i == id) {
            curr.items.push((id.to_string(), raw_title.to_string(), data));
        }
    }

    /// Searches the Trie for the best matching items for the given query.
    /// Returns pairs of (Data, Score) sorted by score descending.
    pub fn search(&self, query: &str, limit: usize) -> Vec<(T, f64)> {
        let norm_query = normalize_title(query);
        if norm_query.is_empty() {
            return Vec::new();
        }

        let raw_tokens: Vec<&str> = norm_query.split_whitespace().collect();
        let noise_set: HashSet<&str> = QUERY_NOISE_WORDS.iter().copied().collect();
        let filtered_tokens: Vec<&str> = raw_tokens
            .iter()
            .copied()
            .filter(|tok| !noise_set.contains(tok))
            .collect();

        let query_tokens = if filtered_tokens.is_empty() {
            &raw_tokens
        } else {
            &filtered_tokens
        };
        let clean_query = query_tokens.join(" ");

        let mut scored_map: HashMap<String, (T, f64)> = HashMap::new();

        // Traverse down the Trie along the prefix
        let mut curr = &self.root;
        let mut matched_chars = 0usize;
        let compact_query: Vec<char> = clean_query.chars().filter(|c| !c.is_whitespace()).collect();

        for &ch in &compact_query {
            if let Some(next) = curr.children.get(&ch) {
                curr = next;
                matched_chars += 1;
            } else {
                break;
            }
        }

        let prefix_ratio = if !compact_query.is_empty() {
            (matched_chars as f64 / compact_query.len() as f64).max(0.5)
        } else {
            1.0
        };

        // Collect matching items from candidate nodes
        self.evaluate_subtree(curr, query_tokens, &clean_query, prefix_ratio, &mut scored_map);

        // Also evaluate root items with a slight depth penalty for fuzzy fallback
        if matched_chars < compact_query.len() {
            self.evaluate_subtree(&self.root, query_tokens, &clean_query, 0.70, &mut scored_map);
        }

        let mut results: Vec<(T, f64)> = scored_map.into_values().collect();
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(limit);
        results
    }

    fn evaluate_subtree(
        &self,
        node: &TrieNode<T>,
        query_tokens: &[&str],
        clean_query: &str,
        depth_weight: f64,
        scored_map: &mut HashMap<String, (T, f64)>,
    ) {
        for (id, raw_title, data) in &node.items {
            let item_norm = normalize_title(raw_title);
            let item_tokens: Vec<&str> = item_norm.split_whitespace().collect();

            let mut token_match_sum = 0.0;
            for &q_tok in query_tokens {
                let mut best_sim = 0.0f64;
                for &t_tok in &item_tokens {
                    let sim = word_similarity(q_tok, t_tok);
                    if sim > best_sim {
                        best_sim = sim;
                    }
                }
                token_match_sum += best_sim;
            }

            let token_sim = token_match_sum / (query_tokens.len().max(1) as f64);
            let global_dist = damerau_levenshtein(clean_query, &item_norm);
            let max_full_len = clean_query.chars().count().max(item_norm.chars().count()).max(1);
            let global_sim = (1.0 - (global_dist as f64 / max_full_len as f64)).max(0.0);

            let composite_score = (token_sim * 0.75 + global_sim * 0.25) * depth_weight;

            if composite_score >= 0.25 || token_sim >= 0.40 {
                if let Some((_, prev_score)) = scored_map.get(id) {
                    if composite_score > *prev_score {
                        scored_map.insert(id.clone(), (data.clone(), composite_score));
                    }
                } else {
                    scored_map.insert(id.clone(), (data.clone(), composite_score));
                }
            }
        }

        for child in node.children.values() {
            self.evaluate_subtree(child, query_tokens, clean_query, depth_weight * 0.98, scored_map);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_damerau_levenshtein_transposition() {
        // "jhon" vs "john" is 1 transposition
        assert_eq!(damerau_levenshtein("jhon", "john"), 1);
        // "pokmon" vs "pokemon" is 1 deletion/insertion
        assert_eq!(damerau_levenshtein("pokmon", "pokemon"), 1);
    }

    #[test]
    fn test_title_trie_fuzzy_search() {
        let mut trie = TitleTrie::new();
        trie.insert("1", "Pokémon Horizons: The Series", "pokemon_horizons");
        trie.insert("2", "John Wick: Chapter 4", "john_wick_4");
        trie.insert("3", "Avengers: Endgame", "avengers_endgame");
        trie.insert("4", "Spider-Man: Across the Spider-Verse", "spider_man");

        // Test typo: "pokmon"
        let matches = trie.search("pokmon", 5);
        assert!(!matches.is_empty());
        assert_eq!(matches[0].0, "pokemon_horizons");

        // Test typo with transposition: "jhon wick"
        let matches = trie.search("jhon wick", 5);
        assert!(!matches.is_empty());
        assert_eq!(matches[0].0, "john_wick_4");

        // Test noise words: "movie about avengrs"
        let matches = trie.search("movie about avengrs", 5);
        assert!(!matches.is_empty());
        assert_eq!(matches[0].0, "avengers_endgame");
    }
}
