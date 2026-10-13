//! Branch names for ADO work: initials, slugs, the template and validation.

/// The longest slug, in characters (all ASCII, so also bytes).
pub const SLUG_MAX: usize = 40;

/// The default branch template.
pub const DEFAULT_TEMPLATE: &str = "{initials}/{slug}";

/// First two letters of the first word plus first two of the last word of
/// `git config user.name`, lowercased and folded to ASCII. One word gives
/// just its first two letters. `None` when nothing usable is left.
pub fn initials(user_name: &str) -> Option<String> {
    let words: Vec<String> = user_name
        .split_whitespace()
        .map(fold)
        .filter(|w| !w.is_empty())
        .collect();
    let first = words.first()?;
    let mut out: String = first.chars().take(2).collect();
    if words.len() > 1 {
        out.extend(words[words.len() - 1].chars().take(2));
    }
    Some(out)
}

/// Lowercase, fold common accented letters to ASCII and keep `[a-z0-9]`.
fn fold(word: &str) -> String {
    let mut out = String::new();
    for c in word.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if let Some(f) = fold_char(c) {
            out.push_str(f);
        }
    }
    out
}

fn fold_char(c: char) -> Option<&'static str> {
    Some(match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ą' | 'ă' => "a",
        'æ' => "ae",
        'ç' | 'ć' | 'č' => "c",
        'ď' | 'đ' | 'ð' => "d",
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ę' | 'ě' => "e",
        'ì' | 'í' | 'î' | 'ï' | 'ī' => "i",
        'ł' => "l",
        'ñ' | 'ń' | 'ň' => "n",
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ő' => "o",
        'œ' => "oe",
        'ř' => "r",
        'ś' | 'š' | 'ş' => "s",
        'ß' => "ss",
        'ť' | 'ţ' => "t",
        'þ' => "th",
        'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' | 'ű' => "u",
        'ý' | 'ÿ' => "y",
        'ź' | 'ż' | 'ž' => "z",
        _ => return None,
    })
}

/// Kebab-case ASCII slug of a work item title, at most [`SLUG_MAX`]
/// characters, cut at a `-` boundary when one exists.
pub fn slug(title: &str) -> String {
    let mut out = String::new();
    let mut pending_dash = false;
    for c in title.chars().flat_map(char::to_lowercase) {
        let piece = if c.is_ascii_alphanumeric() {
            Some(c.to_string())
        } else {
            fold_char(c).map(str::to_string)
        };
        match piece {
            Some(p) => {
                if pending_dash && !out.is_empty() {
                    out.push('-');
                }
                pending_dash = false;
                out.push_str(&p);
            }
            None => pending_dash = true,
        }
    }
    if out.len() > SLUG_MAX {
        let cut = &out[..SLUG_MAX];
        let cut = if out.as_bytes()[SLUG_MAX] == b'-' {
            cut
        } else {
            match cut.rfind('-') {
                Some(i) if i > 0 => &cut[..i],
                _ => cut,
            }
        };
        out = cut.trim_end_matches('-').to_string();
    }
    out
}

/// Fill `{initials}`, `{slug}` and `{id}` in `template`. With no id, `{id}`
/// and the separator next to it are dropped, so `{initials}/{id}-{slug}`
/// gives `jari/slug`, not `jari/-slug`.
pub fn branch_name(template: &str, initials: &str, slug: &str, id: Option<u32>) -> String {
    let template = match id {
        Some(_) => template.to_string(),
        None => template
            .replace("{id}-", "")
            .replace("-{id}", "")
            .replace("{id}_", "")
            .replace("_{id}", "")
            .replace("{id}/", "")
            .replace("/{id}", "")
            .replace("{id}", ""),
    };
    template
        .replace("{initials}", initials)
        .replace("{slug}", slug)
        .replace("{id}", &id.map(|i| i.to_string()).unwrap_or_default())
}

/// `Ok` when `name` is a valid new branch: not already in `existing`, and
/// passing the `git check-ref-format --branch` rules.
pub fn validate_branch_name(name: &str, existing: &[String]) -> Result<(), String> {
    if existing.iter().any(|e| e == name) {
        return Err(format!("A branch named {name} already exists."));
    }
    check_ref_format(name)
}

fn check_ref_format(name: &str) -> Result<(), String> {
    let bad = |why: &str| Err(format!("{name} is not a valid branch name: {why}."));
    if name.is_empty() {
        return bad("it is empty");
    }
    if name == "@" {
        return bad("it cannot be @");
    }
    if name.starts_with('-') {
        return bad("it cannot start with -");
    }
    if name.starts_with('/') || name.ends_with('/') {
        return bad("it cannot start or end with /");
    }
    if name.ends_with('.') {
        return bad("it cannot end with .");
    }
    if name.contains("..") {
        return bad("it cannot contain ..");
    }
    if name.contains("@{") {
        return bad("it cannot contain @{");
    }
    if let Some(c) = name
        .chars()
        .find(|c| c.is_whitespace() || c.is_control() || "~^:?*[\\".contains(*c))
    {
        return if c == ' ' || c.is_whitespace() {
            bad("it cannot contain spaces")
        } else {
            Err(format!(
                "{name} is not a valid branch name: it cannot contain {c}."
            ))
        };
    }
    for part in name.split('/') {
        if part.is_empty() {
            return bad("it cannot contain an empty part (//)");
        }
        if part.starts_with('.') {
            return bad("no part can start with .");
        }
        if part.ends_with(".lock") {
            return bad("no part can end with .lock");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initials_from_names() {
        assert_eq!(initials("Jay Richardson").as_deref(), Some("jari"));
        assert_eq!(initials("Ana María López").as_deref(), Some("anlo"));
        assert_eq!(initials("Cher").as_deref(), Some("ch"));
        assert_eq!(initials("  ").as_deref(), None);
    }

    #[test]
    fn slug_and_template() {
        assert_eq!(
            slug("Fix GST rounding on cart totals!"),
            "fix-gst-rounding-on-cart-totals"
        );
        assert!(slug(&"word ".repeat(30)).len() <= 40);
        assert_eq!(
            branch_name("{initials}/{slug}", "jari", "gst-rounding", Some(4211)),
            "jari/gst-rounding"
        );
        assert_eq!(
            branch_name("{initials}/{id}-{slug}", "jari", "gst", Some(4211)),
            "jari/4211-gst"
        );
    }

    #[test]
    fn branch_validation() {
        assert!(validate_branch_name("jari/ok-name", &[]).is_ok());
        assert!(validate_branch_name("jari/has space", &[]).is_err());
        assert!(validate_branch_name("jari/a..b", &[]).is_err());
        assert!(validate_branch_name("jari/x", &["jari/x".into()])
            .unwrap_err()
            .contains("exists"));
    }
}
