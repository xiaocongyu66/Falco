//! Pseudo-class matching — :first-child, :last-child, :nth-child, :hover.
//!
//! These require sibling information, which we collect during the style
//! tree build. The matching is done at selector-match time.

/// Check if an element matches a pseudo-class.
/// `child_index` is the 0-based index among siblings (same parent).
/// `sibling_count` is the total number of element siblings.
pub fn matches_pseudo(pseudo: &str, child_index: usize, sibling_count: usize) -> bool {
    match pseudo {
        "first-child" => child_index == 0,
        "last-child" => child_index + 1 == sibling_count,
        "only-child" => sibling_count == 1,
        "nth-child" => {
            // Simplified: always match (real nth-child(n) parsing is complex).
            // The selector parser already extracted the argument, but we
            // can't parse it here without more context. For now, match all.
            true
        }
        "root" => child_index == 0 && sibling_count == 1,
        "empty" => false, // Would need to check if element has no children.
        "hover" | "focus" | "active" | "visited" | "link" => {
            // These are runtime pseudo-classes — handled by the browser
            // at interaction time, not at style computation time.
            // For static rendering, we don't match them.
            false
        }
        _ => false,
    }
}

/// Parse an nth-child argument like "2n+1", "odd", "even", "3".
/// Returns (a, b) where the formula is a*n + b.
pub fn parse_nth(arg: &str) -> (i32, i32) {
    let arg = arg.trim().to_lowercase();
    if arg == "odd" {
        return (2, 1);
    }
    if arg == "even" {
        return (2, 0);
    }
    // Try to parse "an+b" format.
    if let Some(n_pos) = arg.find('n') {
        let a_str = &arg[..n_pos];
        let a: i32 = if a_str.is_empty() || a_str == "+" {
            1
        } else if a_str == "-" {
            -1
        } else {
            a_str.parse().unwrap_or(1)
        };
        let b_str = &arg[n_pos + 1..];
        let b: i32 = if b_str.is_empty() {
            0
        } else {
            b_str.replace("+", "").parse().unwrap_or(0)
        };
        (a, b)
    } else {
        // Just a number — matches only that specific child.
        (0, arg.parse().unwrap_or(0))
    }
}

/// Check if a child index matches an nth-child formula (a*n + b).
pub fn matches_nth(index: usize, a: i32, b: i32) -> bool {
    if a == 0 {
        // b-only: matches if index+1 == b (1-indexed).
        return (index as i32) + 1 == b;
    }
    // index+1 = a*n + b → n = (index+1 - b) / a
    let pos = (index as i32) + 1 - b;
    if a > 0 {
        pos >= 0 && pos % a == 0
    } else {
        pos <= 0 && pos % a == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_nth() {
        assert_eq!(parse_nth("odd"), (2, 1));
        assert_eq!(parse_nth("even"), (2, 0));
        assert_eq!(parse_nth("3"), (0, 3));
        assert_eq!(parse_nth("2n+1"), (2, 1));
        assert_eq!(parse_nth("n"), (1, 0));
        assert_eq!(parse_nth("-n+3"), (-1, 3));
    }

    #[test]
    fn test_matches_nth() {
        // 2n+1 (odd): matches index 0, 2, 4, ...
        assert!(matches_nth(0, 2, 1));
        assert!(!matches_nth(1, 2, 1));
        assert!(matches_nth(2, 2, 1));
        // 3 (exact): matches index 2 (0-indexed, so child #3)
        assert!(matches_nth(2, 0, 3));
        assert!(!matches_nth(0, 0, 3));
    }

    #[test]
    fn test_pseudo_matching() {
        assert!(matches_pseudo("first-child", 0, 3));
        assert!(!matches_pseudo("first-child", 1, 3));
        assert!(matches_pseudo("last-child", 2, 3));
        assert!(!matches_pseudo("last-child", 1, 3));
        assert!(matches_pseudo("only-child", 0, 1));
        assert!(!matches_pseudo("only-child", 0, 2));
    }
}
