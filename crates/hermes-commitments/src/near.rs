//! Same-debt detector. This is **one owed instance**, not memory TF-IDF
//! (same rule restated). "改稿" vs "改合同" must stay apart.

use std::collections::HashSet;

/// Ask the user / return a near-hit (do not create a second row).
pub const NEAR_ASK: f64 = 0.48;
/// Safe to fold without asking (containment or very high overlap).
pub const NEAR_FOLD: f64 = 0.78;

#[derive(Debug, Clone)]
pub struct NearHit {
    pub id: String,
    pub title: String,
    pub score: f64,
}

/// Score how likely `a` and `b` name the same open debt (0.0–1.0).
pub fn score_near(a: &str, b: &str) -> f64 {
    let na = normalize(a);
    let nb = normalize(b);
    if na.is_empty() || nb.is_empty() {
        return 0.0;
    }
    if na == nb {
        return 1.0;
    }
    // One title contains the other (and the shorter is a real phrase).
    let (shorter, longer) = if na.chars().count() <= nb.chars().count() {
        (na.as_str(), nb.as_str())
    } else {
        (nb.as_str(), na.as_str())
    };
    if shorter.chars().count() >= 2 && longer.contains(shorter) {
        return 0.86;
    }

    let ta = tokens(&na);
    let tb = tokens(&nb);
    if ta.is_empty() || tb.is_empty() {
        return 0.0;
    }
    let inter = ta.intersection(&tb).count() as f64;
    let union = ta.union(&tb).count() as f64;
    if union == 0.0 {
        return 0.0;
    }
    inter / union
}

fn normalize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || is_cjk(c) {
                c.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn tokens(s: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    let mut latin = String::new();
    let chars: Vec<char> = s.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if c.is_ascii_alphanumeric() {
            latin.push(c);
            continue;
        }
        flush_latin(&mut latin, &mut out);
        if is_cjk(c) {
            out.insert(c.to_string());
            if let Some(&n) = chars.get(i + 1) {
                if is_cjk(n) {
                    out.insert(format!("{c}{n}"));
                }
            }
        }
    }
    flush_latin(&mut latin, &mut out);
    out
}

fn flush_latin(buf: &mut String, out: &mut HashSet<String>) {
    if buf.chars().count() >= 2 {
        out.insert(std::mem::take(buf));
    } else {
        buf.clear();
    }
}

fn is_cjk(c: char) -> bool {
    matches!(
        c as u32,
        0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_title_is_one() {
        assert_eq!(score_near("周五交改稿", "周五交改稿"), 1.0);
    }

    #[test]
    fn containment_folds() {
        let s = score_near("交改稿", "周五交改稿");
        assert!(s >= NEAR_FOLD, "{s}");
    }

    #[test]
    fn different_deliverables_stay_apart() {
        let s = score_near("交改稿", "约设计");
        assert!(s < NEAR_ASK, "{s}");
        let s2 = score_near("改稿", "改合同");
        assert!(s2 < NEAR_FOLD, "{s2}");
    }

    #[test]
    fn latin_rephrase() {
        let s = score_near("send the draft Friday", "send draft friday");
        assert!(s >= NEAR_ASK, "{s}");
    }
}
