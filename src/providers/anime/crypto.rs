use aes::cipher::{KeyIvInit, StreamCipher};
use base64::Engine;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::LazyLock;

type Aes256Ctr64BE = ctr::Ctr64BE<aes::Aes256>;

static ALLANIME_HEX_MAP: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert("79", "A");
    m.insert("7a", "B");
    m.insert("7b", "C");
    m.insert("7c", "D");
    m.insert("7d", "E");
    m.insert("7e", "F");
    m.insert("7f", "G");
    m.insert("70", "H");
    m.insert("71", "I");
    m.insert("72", "J");
    m.insert("73", "K");
    m.insert("74", "L");
    m.insert("75", "M");
    m.insert("76", "N");
    m.insert("77", "O");
    m.insert("68", "P");
    m.insert("69", "Q");
    m.insert("6a", "R");
    m.insert("6b", "S");
    m.insert("6c", "T");
    m.insert("6d", "U");
    m.insert("6e", "V");
    m.insert("6f", "W");
    m.insert("60", "X");
    m.insert("61", "Y");
    m.insert("62", "Z");
    m.insert("59", "a");
    m.insert("5a", "b");
    m.insert("5b", "c");
    m.insert("5c", "d");
    m.insert("5d", "e");
    m.insert("5e", "f");
    m.insert("5f", "g");
    m.insert("50", "h");
    m.insert("51", "i");
    m.insert("52", "j");
    m.insert("53", "k");
    m.insert("54", "l");
    m.insert("55", "m");
    m.insert("56", "n");
    m.insert("57", "o");
    m.insert("48", "p");
    m.insert("49", "q");
    m.insert("4a", "r");
    m.insert("4b", "s");
    m.insert("4c", "t");
    m.insert("4d", "u");
    m.insert("4e", "v");
    m.insert("4f", "w");
    m.insert("40", "x");
    m.insert("41", "y");
    m.insert("42", "z");
    m.insert("08", "0");
    m.insert("09", "1");
    m.insert("0a", "2");
    m.insert("0b", "3");
    m.insert("0c", "4");
    m.insert("0d", "5");
    m.insert("0e", "6");
    m.insert("0f", "7");
    m.insert("00", "8");
    m.insert("01", "9");
    m.insert("15", "-");
    m.insert("16", ".");
    m.insert("67", "_");
    m.insert("46", "~");
    m.insert("02", ":");
    m.insert("17", "/");
    m.insert("07", "?");
    m.insert("1b", "#");
    m.insert("63", "[");
    m.insert("65", "]");
    m.insert("78", "@");
    m.insert("19", "!");
    m.insert("1c", "$");
    m.insert("1e", "&");
    m.insert("10", "(");
    m.insert("11", ")");
    m.insert("12", "*");
    m.insert("13", "+");
    m.insert("14", ",");
    m.insert("03", ";");
    m.insert("05", "=");
    m.insert("1d", "%");
    m
});

/// Decodes an obfuscated AllAnime hex URL string.
pub fn decode_allanime_url(encoded: &str) -> String {
    let raw = encoded.strip_prefix("--").unwrap_or(encoded);
    let mut result = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();

    while let (Some(c1), Some(c2)) = (chars.next(), chars.next()) {
        let pair = format!("{c1}{c2}");
        if let Some(&mapped) = ALLANIME_HEX_MAP.get(pair.as_str()) {
            result.push_str(mapped);
        } else {
            result.push_str(&pair);
        }
    }

    result.replace("\\u002F", "/").replace("\\|", "")
}

#[derive(Debug, Clone, PartialEq)]
pub struct DecryptedSource {
    pub source_url: String,
    pub source_name: String,
    pub priority: f64,
}

/// Decrypts AES-256-CTR "tobeparsed" encrypted AllAnime responses.
pub fn decode_tobeparsed(blob: &str) -> Vec<DecryptedSource> {
    let buf = match base64::engine::general_purpose::STANDARD.decode(blob) {
        Ok(b) => b,
        Err(_) => return Vec::new(),
    };

    if buf.len() < 30 {
        return Vec::new();
    }

    // 12-byte IV + 4-byte counter 0x00000002
    let iv12 = &buf[1..13];
    let mut iv16 = [0u8; 16];
    iv16[..12].copy_from_slice(iv12);
    iv16[12..16].copy_from_slice(&[0, 0, 0, 2]);

    // Ciphertext: strip 13-byte prefix and 16-byte auth tag at the end
    let ct_end = buf.len().saturating_sub(16);
    if ct_end <= 13 {
        return Vec::new();
    }
    let mut ct = buf[13..ct_end].to_vec();

    // Key: SHA256("Xot36i3lK3:v1")
    let key = Sha256::digest(b"Xot36i3lK3:v1");

    let mut cipher = match Aes256Ctr64BE::new_from_slices(&key, &iv16) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    cipher.apply_keystream(&mut ct);

    let plain = match String::from_utf8(ct) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let mut sources = Vec::new();

    // Parse source objects from plain decrypted text
    for chunk in plain.split(['{', '}']) {
        if chunk.contains("\"sourceUrl\"") {
            let mut source_url = None;
            let mut source_name = None;
            let mut priority = 0.0;

            if let Some(pos) = chunk.find("\"sourceUrl\"") {
                let rest = &chunk[pos..];
                if let Some(quote1) = rest.find(':').and_then(|c| rest[c..].find('"').map(|q| c + q + 1)) {
                    if let Some(quote2) = rest[quote1..].find('"') {
                        source_url = Some(rest[quote1..quote1 + quote2].to_string());
                    }
                }
            }

            if let Some(pos) = chunk.find("\"sourceName\"") {
                let rest = &chunk[pos..];
                if let Some(quote1) = rest.find(':').and_then(|c| rest[c..].find('"').map(|q| c + q + 1)) {
                    if let Some(quote2) = rest[quote1..].find('"') {
                        source_name = Some(rest[quote1..quote1 + quote2].to_string());
                    }
                }
            }

            if let Some(pos) = chunk.find("\"priority\"") {
                let rest = &chunk[pos..];
                if let Some(colon) = rest.find(':') {
                    let num_str: String = rest[colon + 1..]
                        .chars()
                        .take_while(|c| c.is_ascii_digit() || *c == '.')
                        .collect();
                    if let Ok(p) = num_str.parse::<f64>() {
                        priority = p;
                    }
                }
            }

            if let Some(url) = source_url {
                sources.push(DecryptedSource {
                    source_url: url,
                    source_name: source_name.unwrap_or_default(),
                    priority,
                });
            }
        }
    }

    sources
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_allanime_url() {
        // Obfuscated "--021717" -> "://"
        let decoded = decode_allanime_url("--021717");
        assert_eq!(decoded, "://");
    }
}
