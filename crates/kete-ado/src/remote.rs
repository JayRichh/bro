//! Recognise an Azure DevOps git remote and split it into org, project, repo.

/// An Azure DevOps repo, as named by its git remote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdoRemote {
    /// The HTTPS host (`dev.azure.com` or `{org}.visualstudio.com`), also for
    /// SSH remotes, so GCM and the REST client agree with `org_url`.
    pub host: String,
    pub org: String,
    pub project: String,
    pub repo: String,
    /// `https://dev.azure.com/{org}` or `https://{org}.visualstudio.com`.
    pub org_url: String,
}

/// Parse the three ADO remote shapes:
/// `https://[user@]dev.azure.com/{org}/{project}/_git/{repo}`,
/// `https://[user@]{org}.visualstudio.com/[DefaultCollection/]{project}/_git/{repo}`
/// and `git@ssh.dev.azure.com:v3/{org}/{project}/{repo}`
/// (also `ssh://git@ssh.dev.azure.com/v3/...` and the legacy
/// `{org}@vs-ssh.visualstudio.com:v3/...`).
pub fn parse_remote(url: &str) -> Option<AdoRemote> {
    let url = url.trim();
    if url.is_empty() {
        return None;
    }
    if let Some(rest) = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
    {
        return parse_https(rest);
    }
    parse_ssh(url)
}

fn parse_https(rest: &str) -> Option<AdoRemote> {
    let (authority, path) = rest.split_once('/')?;
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, h)| h)
        .to_ascii_lowercase();
    let segs = segments(path)?;
    if host == "dev.azure.com" {
        // org / project / _git / repo
        match segs.as_slice() {
            [org, project, git, repo] if git == "_git" => {
                build(host.clone(), org, project, repo, dev_url(org))
            }
            _ => None,
        }
    } else if let Some(org) = host.strip_suffix(".visualstudio.com") {
        if org.is_empty() || org.contains('.') {
            return None;
        }
        let segs: Vec<&String> = segs
            .iter()
            .filter(|s| !s.eq_ignore_ascii_case("DefaultCollection"))
            .collect();
        match segs.as_slice() {
            [project, git, repo] if git.as_str() == "_git" => {
                build(host.clone(), org, project, repo, format!("https://{host}"))
            }
            _ => None,
        }
    } else {
        None
    }
}

fn parse_ssh(url: &str) -> Option<AdoRemote> {
    let rest = url.strip_prefix("ssh://").unwrap_or(url);
    let (authority, path) = match rest.split_once(':') {
        Some((a, p)) if !a.contains('/') => (a, p),
        _ => rest.split_once('/')?,
    };
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, h)| h)
        .to_ascii_lowercase();
    if host != "ssh.dev.azure.com" && host != "vs-ssh.visualstudio.com" {
        return None;
    }
    let segs = segments(path.trim_start_matches('/'))?;
    match segs.as_slice() {
        [v3, org, project, repo] if v3 == "v3" => {
            if host == "ssh.dev.azure.com" {
                build("dev.azure.com".into(), org, project, repo, dev_url(org))
            } else {
                let h = format!("{org}.visualstudio.com");
                let u = format!("https://{h}");
                build(h, org, project, repo, u)
            }
        }
        _ => None,
    }
}

fn dev_url(org: &str) -> String {
    format!("https://dev.azure.com/{org}")
}

fn build(host: String, org: &str, project: &str, repo: &str, org_url: String) -> Option<AdoRemote> {
    let repo = repo.trim_end_matches(".git");
    if !org_ok(org) || !name_ok(project) || !name_ok(repo) {
        return None;
    }
    Some(AdoRemote {
        host,
        org: org.to_string(),
        project: project.to_string(),
        repo: repo.to_string(),
        org_url,
    })
}

/// An organization name as ADO allows it: letters, digits, hyphens (dots
/// and underscores let through for old collections). It goes into the org
/// URL and from there into the `git credential` stdin, so nothing else.
fn org_ok(org: &str) -> bool {
    !org.is_empty()
        && org
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_'))
}

/// A project or repo name: ADO allows spaces and non-ASCII letters, but no
/// control characters and none of `\ / : * ? " < > | ; # $ { } , + = [ ]`.
/// Refusing those also keeps a decoded name from breaking a REST path or
/// the line-based credential protocol (CR, LF, `=`).
fn name_ok(name: &str) -> bool {
    !name.is_empty()
        && name.chars().all(|c| {
            !c.is_control()
                && !matches!(
                    c,
                    '\\' | '/'
                        | ':'
                        | '*'
                        | '?'
                        | '"'
                        | '<'
                        | '>'
                        | '|'
                        | ';'
                        | '#'
                        | '$'
                        | '{'
                        | '}'
                        | ','
                        | '+'
                        | '='
                        | '['
                        | ']'
                )
        })
}

/// Split a path into percent-decoded, non-empty segments, ignoring any
/// query or fragment.
fn segments(path: &str) -> Option<Vec<String>> {
    let path = path.split(['?', '#']).next().unwrap_or("");
    let segs: Vec<String> = path
        .split('/')
        .filter(|s| !s.is_empty())
        .map(percent_decode)
        .collect::<Option<_>>()?;
    if segs.iter().any(|s| s.is_empty()) {
        return None;
    }
    Some(segs)
}

fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = s.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_three_remote_shapes() {
        let a = parse_remote("https://dev.azure.com/tnt/Platform/_git/api").unwrap();
        assert_eq!(
            (a.org.as_str(), a.project.as_str(), a.repo.as_str()),
            ("tnt", "Platform", "api")
        );
        assert_eq!(a.org_url, "https://dev.azure.com/tnt");
        let b = parse_remote("https://tnt.visualstudio.com/Platform/_git/api").unwrap();
        assert_eq!(b.org_url, "https://tnt.visualstudio.com");
        let c = parse_remote("git@ssh.dev.azure.com:v3/tnt/Platform/api").unwrap();
        assert_eq!(c.org_url, "https://dev.azure.com/tnt");
        let d = parse_remote("https://jay@dev.azure.com/tnt/My%20Project/_git/web").unwrap();
        assert_eq!(d.project, "My Project");
    }

    #[test]
    fn non_ado_remotes_are_none() {
        assert!(parse_remote("https://github.com/JayRichh/kete.git").is_none());
        assert!(parse_remote("").is_none());
    }

    /// Decoded segments go into REST paths and, for the org, the
    /// `git credential` stdin: a remote smuggling a line break, an `=` or
    /// a path character through percent-encoding is not an ADO remote.
    #[test]
    fn segments_that_could_break_a_request_or_the_credential_protocol_are_refused() {
        for url in [
            "https://dev.azure.com/tnt%0Ahost=evil/Platform/_git/api",
            "https://dev.azure.com/tnt%3Dx/Platform/_git/api",
            "https://dev.azure.com/t%20nt/Platform/_git/api",
            "https://dev.azure.com/tnt/Plat%0D%0Aform/_git/api",
            "https://dev.azure.com/tnt/Platform/_git/a%00pi",
            "https://dev.azure.com/tnt/Plat%2Fform/_git/api",
            "https://dev.azure.com/tnt/Platform/_git/a%3Dpi",
            "https://dev.azure.com/tnt/Plat%5Cform/_git/api",
            "git@ssh.dev.azure.com:v3/tnt/Platform/a%0Api",
        ] {
            assert!(parse_remote(url).is_none(), "{url}");
        }
        // ADO allows spaces, dots, hyphens and non-ASCII letters in
        // project and repo names.
        let ok =
            parse_remote("https://dev.azure.com/my-org/My%20Proj.v2/_git/Caf%C3%A9_web-1").unwrap();
        assert_eq!(
            (ok.org.as_str(), ok.project.as_str(), ok.repo.as_str()),
            ("my-org", "My Proj.v2", "Café_web-1")
        );
    }
}
