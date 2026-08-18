//! CJK-aware tokens. Same idea as memory relevance — cheap enough per ingest
//! and per query. No model, no extra crate.

const MIN_TOKEN_LEN: usize = 2;

pub fn tokenise(s: &str) -> Vec<String> {
    let lower = s.to_lowercase();
    let mut tokens = Vec::new();
    for segment in lower.split(|c: char| !c.is_alphanumeric()) {
        let char_count = segment.chars().count();
        if char_count == 0 {
            continue;
        }
        if char_count >= MIN_TOKEN_LEN {
            tokens.push(segment.to_string());
        }
        if char_count >= 2 && segment.chars().any(is_cjk) {
            let chars: Vec<char> = segment.chars().collect();
            for window in chars.windows(2) {
                tokens.push(window.iter().collect::<String>());
            }
        }
    }
    tokens
}

pub fn is_cjk(c: char) -> bool {
    matches!(c,
        '\u{4E00}'..='\u{9FFF}'
        | '\u{3400}'..='\u{4DBF}'
        | '\u{F900}'..='\u{FAFF}'
        | '\u{3040}'..='\u{30FF}'
        | '\u{AC00}'..='\u{D7AF}'
    )
}

/// Cheap overlap in 0.0..=1.0 for "is this the same document, new version?"
pub fn overlap(a: &str, b: &str) -> f64 {
    let ta: std::collections::HashSet<String> = tokenise(a).into_iter().collect();
    let tb: std::collections::HashSet<String> = tokenise(b).into_iter().collect();
    if ta.is_empty() || tb.is_empty() {
        return 0.0;
    }
    let inter = ta.intersection(&tb).count() as f64;
    let union = ta.union(&tb).count() as f64;
    if union == 0.0 {
        0.0
    } else {
        inter / union
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cjk_bigrams_match() {
        let t = tokenise("服务合同违约金");
        assert!(t.iter().any(|x| x.contains("违约") || x == "违约金"));
    }

    #[test]
    fn overlap_same_high() {
        let a = "甲方应按第七条支付违约金，比例为合同总额的百分之二十。";
        let b = "甲方应按第七条支付违约金，比例为合同总额的百分之二十。另附签署页。";
        assert!(overlap(a, b) > 0.5);
    }
}
