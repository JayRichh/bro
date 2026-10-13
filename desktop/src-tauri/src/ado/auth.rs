//! ADO tokens: Git Credential Manager first, then the Azure CLI.
//!
//! Tokens live in memory only. Nothing here writes a token to disk or to a
//! log, and no error message carries a runner's stdout.

use async_trait::async_trait;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The Azure DevOps resource id for `az account get-access-token`.
pub const ADO_RESOURCE: &str = "499b84ac-1321-427f-aa17-267ca6975798";

/// How long GCM (or az) may take, including any sign-in window it opens.
pub const AUTH_TIMEOUT: Duration = Duration::from_secs(60);

/// How long `az login` may take: a person signs in, maybe with MFA, in
/// the browser it opens.
pub const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(300);

/// Where a credential came from, so a refused one is only reported back
/// to the helper that handed it out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Gcm,
    Az,
}

/// Called with `(org_url, tenant)` when a token names a new tenant, so it
/// can be saved.
pub type TenantSink = Box<dyn Fn(&str, &str) + Send + Sync>;

/// A credential for the `Authorization` header.
#[derive(Clone, PartialEq, Eq)]
pub enum Credential {
    /// An Entra (OAuth) access token.
    Bearer(String),
    /// `base64(":" + pat)`, ready for the header.
    Basic(String),
}

impl Credential {
    pub fn header(&self) -> String {
        match self {
            Credential::Bearer(t) => format!("Bearer {t}"),
            Credential::Basic(b) => format!("Basic {b}"),
        }
    }
}

impl fmt::Debug for Credential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Credential::Bearer(_) => f.write_str("Bearer(<redacted>)"),
            Credential::Basic(_) => f.write_str("Basic(<redacted>)"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthError {
    /// Neither source had a credential for this org.
    NoCredential,
    /// The source took longer than [`AUTH_TIMEOUT`].
    Timeout,
    /// az needs `az login` (MFA, consent, expired refresh token).
    InteractionRequired(String),
    Other(String),
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuthError::NoCredential => f.write_str(
                "No Azure DevOps credential found. Sign in with Git Credential Manager or run az login.",
            ),
            AuthError::Timeout => {
                f.write_str("Signing in to Azure DevOps took too long. Try again.")
            }
            AuthError::InteractionRequired(line) => {
                write!(f, "Azure DevOps needs you to sign in again (az login). {line}")
            }
            AuthError::Other(msg) => write!(f, "Azure DevOps sign-in failed: {msg}"),
        }
    }
}

impl std::error::Error for AuthError {}

/// Runs a credential helper. The real one spawns a process; tests fake it.
#[async_trait]
pub trait Runner: Send + Sync {
    /// Run `program args` with `env` set, feed it `stdin`, and return its
    /// stdout. A non-zero exit is an error carrying stderr, never stdout.
    async fn run(
        &self,
        program: &str,
        args: &[&str],
        env: &[(&str, &str)],
        stdin: Option<&str>,
        timeout: Duration,
    ) -> Result<String, AuthError>;
}

/// A cached credential: the secret, when it stops being used, and which
/// helper handed it out.
type Cached = (Credential, Instant, Source);

/// Finds a token for an org: GCM first, then az.
pub struct TokenSource {
    runner: Arc<dyn Runner>,
    /// The Entra tenant per org URL, from that org's last OAuth token,
    /// passed to az.
    tenants: Mutex<HashMap<String, String>>,
    /// Told about each newly learned tenant, to save it.
    tenant_sink: Option<TenantSink>,
    /// In-memory credentials per org URL.
    cache: Mutex<HashMap<String, Cached>>,
    /// One fetch at a time per org, so only one sign-in prompt can appear.
    fetching: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    #[cfg(test)]
    fixed: Option<Credential>,
}

/// How long before a JWT's `exp` it is treated as expired.
const EXPIRY_MARGIN_SECS: i64 = 120;
/// How long a PAT, or a JWT without `exp`, is reused.
const OPAQUE_TTL: Duration = Duration::from_secs(600);

impl TokenSource {
    pub fn new(runner: impl Runner + 'static) -> Self {
        Self {
            runner: Arc::new(runner),
            tenants: Mutex::new(HashMap::new()),
            tenant_sink: None,
            cache: Mutex::new(HashMap::new()),
            fetching: Mutex::new(HashMap::new()),
            #[cfg(test)]
            fixed: None,
        }
    }

    /// Start from the tenants saved earlier (org URL to tenant id), and
    /// hand each newly learned one to `sink`.
    pub fn with_tenants(mut self, saved: HashMap<String, String>, sink: TenantSink) -> Self {
        self.tenants = Mutex::new(saved);
        self.tenant_sink = Some(sink);
        self
    }

    /// Forget the cached credential for `org_url`.
    pub fn invalidate(&self, org_url: &str) {
        self.cache.lock().unwrap().remove(org_url);
    }

    /// Drop the cached credential and, when GCM handed it out, tell GCM it
    /// was refused (`git credential reject`), so the next fetch gets a
    /// fresh one. An az token is only dropped: GCM's stored credential had
    /// nothing to do with it, and rejecting would sign the user out of GCM.
    pub async fn reject(&self, org_url: &str) {
        let source = self
            .cache
            .lock()
            .unwrap()
            .remove(org_url)
            .map(|(_, _, s)| s);
        if source != Some(Source::Gcm) {
            return;
        }
        #[cfg(test)]
        if self.fixed.is_some() {
            return;
        }
        if crate::ado::base_override().is_some() {
            return;
        }
        let _ = self
            .runner
            .run(
                "git",
                &[
                    "-c",
                    "credential.azreposCredentialType=oauth",
                    "credential",
                    "reject",
                ],
                &[],
                Some(&credential_input(org_url)),
                AUTH_TIMEOUT,
            )
            .await;
    }

    /// `az login`, for the org's tenant when one is known, then drop the
    /// org's cached credential so the next request uses the new sign-in.
    /// Opens a browser; waits up to [`SIGN_IN_TIMEOUT`].
    pub async fn sign_in(&self, org_url: &str) -> Result<(), AuthError> {
        #[cfg(test)]
        if self.fixed.is_some() {
            self.invalidate(org_url);
            return Ok(());
        }
        if crate::ado::base_override().is_some() {
            self.invalidate(org_url);
            return Ok(());
        }
        let tenant = self.tenant(org_url);
        let mut args = vec!["login", "--output", "none"];
        if let Some(t) = tenant.as_deref() {
            args.extend(["--tenant", t]);
        }
        let res = self
            .runner
            .run("az", &args, &[], None, SIGN_IN_TIMEOUT)
            .await
            .map(|_| ())
            .map_err(|e| match e {
                AuthError::NoCredential => AuthError::Other(
                    "The Azure CLI (az) is not installed, so Kete cannot sign you in.".into(),
                ),
                AuthError::Other(msg) => AuthError::Other(first_line(&msg)),
                e => e,
            });
        self.invalidate(org_url);
        res
    }

    fn cached(&self, org_url: &str) -> Option<Credential> {
        let cache = self.cache.lock().unwrap();
        let (cred, until, _) = cache.get(org_url)?;
        (Instant::now() < *until).then(|| cred.clone())
    }

    /// Where the org's cached credential came from, if one is cached.
    #[cfg(test)]
    pub fn source_of(&self, org_url: &str) -> Option<Source> {
        self.cache.lock().unwrap().get(org_url).map(|(_, _, s)| *s)
    }

    fn remember(&self, org_url: &str, cred: &Credential, source: Source) {
        let ttl = match cred {
            Credential::Bearer(t) => match exp_from_jwt(t) {
                Some(exp) => {
                    let left = exp - now_secs() - EXPIRY_MARGIN_SECS;
                    Duration::from_secs(left.max(0) as u64)
                }
                None => OPAQUE_TTL,
            },
            Credential::Basic(_) => OPAQUE_TTL,
        };
        self.cache.lock().unwrap().insert(
            org_url.to_string(),
            (cred.clone(), Instant::now() + ttl, source),
        );
    }

    /// A source over real processes.
    pub fn system() -> Self {
        Self::new(ProcessRunner)
    }

    /// A source that always hands back `cred`, for client tests.
    #[cfg(test)]
    pub fn fixed(cred: Credential) -> Self {
        Self {
            fixed: Some(cred),
            ..Self::new(ProcessRunner)
        }
    }

    /// The tenant remembered from the org's last OAuth token, if any.
    pub fn tenant(&self, org_url: &str) -> Option<String> {
        self.tenants.lock().unwrap().get(org_url).cloned()
    }

    /// Remember the org's tenant, telling the sink when it is new.
    pub fn set_tenant(&self, org_url: &str, tenant: &str) {
        let changed = self
            .tenants
            .lock()
            .unwrap()
            .insert(org_url.to_string(), tenant.to_string())
            .as_deref()
            != Some(tenant);
        if changed {
            if let Some(sink) = &self.tenant_sink {
                sink(org_url, tenant);
            }
        }
    }

    pub async fn token(&self, org_url: &str) -> Result<Credential, AuthError> {
        self.token_with(org_url, false).await
    }

    /// As [`token`](Self::token), but never shows a sign-in: the cache,
    /// then GCM with `credential.interactive=never`, then az (which never
    /// prompts). For background work such as the PR poll.
    pub async fn token_silent(&self, org_url: &str) -> Result<Credential, AuthError> {
        self.token_with(org_url, true).await
    }

    async fn token_with(&self, org_url: &str, silent: bool) -> Result<Credential, AuthError> {
        #[cfg(test)]
        if let Some(c) = &self.fixed {
            return Ok(c.clone());
        }
        // A local mock server (debug builds) gets a dummy token; real
        // sign-in never runs.
        if crate::ado::base_override().is_some() {
            return Ok(Credential::Bearer("kete-mock".into()));
        }
        if let Some(c) = self.cached(org_url) {
            return Ok(c);
        }
        let gate = self
            .fetching
            .lock()
            .unwrap()
            .entry(org_url.to_string())
            .or_default()
            .clone();
        let _held = gate.lock().await;
        // Another caller may have fetched while this one waited.
        if let Some(c) = self.cached(org_url) {
            return Ok(c);
        }
        let (cred, source) = self.fetch(org_url, silent).await?;
        self.remember(org_url, &cred, source);
        Ok(cred)
    }

    async fn fetch(&self, org_url: &str, silent: bool) -> Result<(Credential, Source), AuthError> {
        let gcm = match self.gcm(org_url, silent).await {
            Ok(c) => return Ok((c, Source::Gcm)),
            Err(e) => e,
        };
        match self.az(org_url).await {
            Ok(c) => Ok((c, Source::Az)),
            Err(az) if gcm == AuthError::NoCredential => Err(az),
            Err(_) => Err(gcm),
        }
    }

    async fn gcm(&self, org_url: &str, silent: bool) -> Result<Credential, AuthError> {
        let input = credential_input(org_url);
        let mut args = vec!["-c", "credential.azreposCredentialType=oauth"];
        let mut env: Vec<(&str, &str)> = Vec::new();
        if silent {
            // GCM's own switch, as config and as its environment variable:
            // fail rather than open a sign-in window.
            args.extend(["-c", "credential.interactive=never"]);
            env.push(("GCM_INTERACTIVE", "never"));
        }
        args.extend(["credential", "fill"]);
        let out = self
            .runner
            .run("git", &args, &env, Some(&input), AUTH_TIMEOUT)
            .await
            .map_err(|e| match e {
                AuthError::Other(msg) => AuthError::Other(first_line(&msg)),
                e => e,
            })?;
        let password = out
            .lines()
            .find_map(|l| l.strip_prefix("password="))
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .ok_or(AuthError::NoCredential)?;
        Ok(self.classify(org_url, password))
    }

    async fn az(&self, org_url: &str) -> Result<Credential, AuthError> {
        let tenant = self.tenant(org_url);
        let mut args = vec![
            "account",
            "get-access-token",
            "--resource",
            ADO_RESOURCE,
            "--output",
            "json",
        ];
        if let Some(t) = tenant.as_deref() {
            args.push("--tenant");
            args.push(t);
        }
        let out = match self.runner.run("az", &args, &[], None, AUTH_TIMEOUT).await {
            Ok(out) => out,
            Err(AuthError::Other(msg)) => return Err(classify_az_error(&msg)),
            Err(e) => return Err(e),
        };
        let token = serde_json::from_str::<serde_json::Value>(&out)
            .ok()
            .and_then(|v| v.get("accessToken")?.as_str().map(str::to_string))
            .filter(|t| !t.is_empty())
            .ok_or_else(|| AuthError::Other("az returned no access token".into()))?;
        Ok(self.classify(org_url, &token))
    }

    /// A JWT is an OAuth token (Bearer); anything else is a PAT (Basic).
    /// A JWT's tenant is remembered for the org.
    fn classify(&self, org_url: &str, secret: &str) -> Credential {
        if looks_like_jwt(secret) {
            if let Some(tid) = tenant_from_jwt(secret) {
                self.set_tenant(org_url, &tid);
            }
            Credential::Bearer(secret.to_string())
        } else {
            Credential::Basic(STANDARD.encode(format!(":{secret}")))
        }
    }
}

/// `https://dev.azure.com/tnt` gives (`dev.azure.com`, `tnt`);
/// `https://tnt.visualstudio.com` gives (`tnt.visualstudio.com`, ``).
fn split_org_url(org_url: &str) -> (&str, &str) {
    let rest = org_url
        .strip_prefix("https://")
        .or_else(|| org_url.strip_prefix("http://"))
        .unwrap_or(org_url)
        .trim_end_matches('/');
    rest.split_once('/').unwrap_or((rest, ""))
}

/// The `git credential fill/reject` stdin for an org.
fn credential_input(org_url: &str) -> String {
    let (host, path) = split_org_url(org_url);
    let mut input = format!("protocol=https\nhost={host}\n");
    if !path.is_empty() {
        input.push_str(&format!("path={path}\n"));
    }
    input.push('\n');
    input
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// The `exp` claim (Unix seconds) of a JWT's payload.
fn exp_from_jwt(token: &str) -> Option<i64> {
    jwt_claims(token)?.get("exp")?.as_i64()
}

fn jwt_claims(token: &str) -> Option<serde_json::Value> {
    let payload = token.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Three non-empty dot-separated parts, all base64url characters.
fn looks_like_jwt(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    parts.len() == 3
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'=')
        })
}

/// The `tid` claim of a JWT's payload.
pub fn tenant_from_jwt(token: &str) -> Option<String> {
    jwt_claims(token)?.get("tid")?.as_str().map(str::to_string)
}

fn classify_az_error(stderr: &str) -> AuthError {
    // An az that was never signed in has no credential; checked first
    // because its message also contains "az login".
    if stderr.contains("Please run 'az login'") || stderr.contains("not logged in") {
        return AuthError::NoCredential;
    }
    const MARKERS: [&str; 3] = ["AADSTS50076", "interaction_required", "az login"];
    if let Some(line) = stderr
        .lines()
        .find(|l| MARKERS.iter().any(|m| l.contains(m)))
    {
        return AuthError::InteractionRequired(line.trim().to_string());
    }
    AuthError::Other(first_line(stderr))
}

fn first_line(s: &str) -> String {
    s.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("unknown error")
        .to_string()
}

/// Spawns the helper with no console window and kills it on timeout.
pub struct ProcessRunner;

#[async_trait]
impl Runner for ProcessRunner {
    async fn run(
        &self,
        program: &str,
        args: &[&str],
        env: &[(&str, &str)],
        stdin: Option<&str>,
        timeout: Duration,
    ) -> Result<String, AuthError> {
        use std::process::Stdio;
        use tokio::io::AsyncWriteExt;

        // On Windows `az` is a .cmd shim, which CreateProcess cannot start
        // by bare name.
        #[cfg(windows)]
        let program = if program == "az" { "az.cmd" } else { program };
        let mut cmd = tokio::process::Command::new(program);
        cmd.args(args)
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("GIT_TERMINAL_PROMPT", "0")
            .envs(env.iter().copied())
            .kill_on_drop(true);
        #[cfg(windows)]
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        let mut child = cmd.spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                AuthError::NoCredential
            } else {
                AuthError::Other(format!("{program}: {e}"))
            }
        })?;
        if let (Some(input), Some(mut pipe)) = (stdin, child.stdin.take()) {
            pipe.write_all(input.as_bytes())
                .await
                .map_err(|e| AuthError::Other(format!("{program}: {e}")))?;
            drop(pipe);
        }
        let out = tokio::time::timeout(timeout, child.wait_with_output())
            .await
            .map_err(|_| AuthError::Timeout)?
            .map_err(|e| AuthError::Other(format!("{program}: {e}")))?;
        if out.status.success() {
            return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
        }
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        if program == "git" && no_git_credential(&stderr) {
            return Err(AuthError::NoCredential);
        }
        Err(AuthError::Other(stderr))
    }
}

/// git's ways of saying it found nothing and could not prompt.
fn no_git_credential(stderr: &str) -> bool {
    stderr.contains("could not read Username")
        || stderr.contains("could not read Password")
        || stderr.contains("terminal prompts disabled")
        || stderr.trim().is_empty()
}

/// A canned [`Runner`] for tests here and in the client.
#[cfg(test)]
pub(crate) mod fake {
    use super::*;
    use std::collections::HashMap;

    type Canned = HashMap<String, Result<String, AuthError>>;
    /// Program, args, stdin, env.
    type Calls = Vec<(String, Vec<String>, Option<String>, Vec<(String, String)>)>;

    #[derive(Clone, Default)]
    pub(crate) struct Fake {
        canned: Arc<Mutex<Canned>>,
        calls: Arc<Mutex<Calls>>,
    }

    impl Fake {
        pub(crate) fn new() -> Self {
            Self::default()
        }
        pub(crate) fn on(self, program: &str, stdout: &str) -> Self {
            self.canned
                .lock()
                .unwrap()
                .insert(program.into(), Ok(stdout.into()));
            self
        }
        pub(crate) fn fail(self, program: &str, e: AuthError) -> Self {
            self.canned.lock().unwrap().insert(program.into(), Err(e));
            self
        }
        pub(crate) fn last_args(&self, program: &str) -> Vec<String> {
            self.calls
                .lock()
                .unwrap()
                .iter()
                .rev()
                .find(|(p, _, _, _)| p == program)
                .map(|(_, a, _, _)| a.clone())
                .unwrap_or_default()
        }
        /// The env of the last call of `program`.
        pub(crate) fn last_env(&self, program: &str) -> Vec<(String, String)> {
            self.calls
                .lock()
                .unwrap()
                .iter()
                .rev()
                .find(|(p, _, _, _)| p == program)
                .map(|(_, _, _, e)| e.clone())
                .unwrap_or_default()
        }
        /// Calls of `program` whose args include `arg`.
        pub(crate) fn count(&self, program: &str, arg: &str) -> usize {
            self.calls
                .lock()
                .unwrap()
                .iter()
                .filter(|(p, a, _, _)| p == program && a.iter().any(|x| x == arg))
                .count()
        }
        /// The stdin of the last call of `program` with `arg`.
        pub(crate) fn stdin_of(&self, program: &str, arg: &str) -> Option<String> {
            self.calls
                .lock()
                .unwrap()
                .iter()
                .rev()
                .find(|(p, a, _, _)| p == program && a.iter().any(|x| x == arg))
                .and_then(|(_, _, s, _)| s.clone())
        }
    }

    #[async_trait]
    impl Runner for Fake {
        async fn run(
            &self,
            program: &str,
            args: &[&str],
            env: &[(&str, &str)],
            stdin: Option<&str>,
            _timeout: Duration,
        ) -> Result<String, AuthError> {
            self.calls.lock().unwrap().push((
                program.into(),
                args.iter().map(|a| a.to_string()).collect(),
                stdin.map(str::to_string),
                env.iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
            ));
            // Let concurrent callers interleave, as a real process would.
            tokio::task::yield_now().await;
            self.canned
                .lock()
                .unwrap()
                .get(program)
                .cloned()
                .unwrap_or(Err(AuthError::NoCredential))
        }
    }

    /// A JWT-shaped token with the given claims.
    pub(crate) fn jwt(claims: serde_json::Value) -> String {
        format!(
            "eyJhbGciOiJub25lIn0.{}.sig",
            URL_SAFE_NO_PAD.encode(claims.to_string())
        )
    }
}

#[cfg(test)]
mod tests {
    use super::fake::{jwt, Fake};
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn gcm_oauth_token_becomes_bearer() {
        let r = Fake::new().on(
            "git",
            "protocol=https\nhost=dev.azure.com\nusername=jay\npassword=eyJhbGciOi.eyJ0aWQiOiJ0LTEifQ.sig\n",
        );
        let t = TokenSource::new(r)
            .token("https://dev.azure.com/tnt")
            .await
            .unwrap();
        assert!(matches!(t, Credential::Bearer(_)));
    }

    #[tokio::test]
    async fn non_jwt_password_is_basic() {
        let r = Fake::new().on("git", "password=abc123pat\n");
        assert!(matches!(
            TokenSource::new(r)
                .token("https://dev.azure.com/tnt")
                .await
                .unwrap(),
            Credential::Basic(_)
        ));
    }

    #[tokio::test]
    async fn falls_back_to_az_with_remembered_tenant() {
        let r = Fake::new()
            .fail("git", AuthError::NoCredential)
            .on("az", "{\"accessToken\":\"eyJ.eyJ0aWQiOiJ0LTkifQ.s\"}");
        let src = TokenSource::new(r.clone());
        src.set_tenant("https://dev.azure.com/tnt", "t-9");
        src.token("https://dev.azure.com/tnt").await.unwrap();
        assert!(r.last_args("az").contains(&"--tenant".to_string()));
        assert!(r.last_args("az").contains(&"t-9".to_string()));
    }

    #[tokio::test]
    async fn tenants_are_per_org_and_new_ones_are_saved() {
        let saved = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
        let sink = saved.clone();
        let tok = jwt(json!({"tid": "t-b"}));
        let r = Fake::new()
            .fail("git", AuthError::NoCredential)
            .on("az", &format!("{{\"accessToken\":\"{tok}\"}}"));
        let src = TokenSource::new(r.clone()).with_tenants(
            HashMap::from([("https://dev.azure.com/a".to_string(), "t-a".to_string())]),
            Box::new(move |org, t| sink.lock().unwrap().push((org.into(), t.into()))),
        );
        // Org b has no tenant yet: az runs without one, and the token's
        // tid is learned for b alone, and saved.
        src.token("https://dev.azure.com/b").await.unwrap();
        assert!(!r.last_args("az").contains(&"--tenant".to_string()));
        assert_eq!(src.tenant("https://dev.azure.com/b").as_deref(), Some("t-b"));
        assert_eq!(src.tenant("https://dev.azure.com/a").as_deref(), Some("t-a"));
        assert_eq!(
            *saved.lock().unwrap(),
            vec![("https://dev.azure.com/b".to_string(), "t-b".to_string())]
        );
        // Org a keeps its own tenant.
        src.token("https://dev.azure.com/a").await.unwrap();
        assert!(r.last_args("az").contains(&"t-a".to_string()));
    }

    #[tokio::test]
    async fn an_az_token_refused_never_rejects_in_gcm() {
        let r = Fake::new()
            .fail("git", AuthError::NoCredential)
            .on("az", "{\"accessToken\":\"eyJ.eyJ0aWQiOiJ0LTkifQ.s\"}");
        let src = TokenSource::new(r.clone());
        src.token("https://dev.azure.com/tnt").await.unwrap();
        assert_eq!(src.source_of("https://dev.azure.com/tnt"), Some(Source::Az));
        src.reject("https://dev.azure.com/tnt").await;
        assert_eq!(r.count("git", "reject"), 0);
        assert_eq!(src.source_of("https://dev.azure.com/tnt"), None);
    }

    #[tokio::test]
    async fn sign_in_runs_az_login_for_the_org_tenant_and_drops_the_cache() {
        let r = Fake::new().on("git", "password=pat\n").on("az", "");
        let src = TokenSource::new(r.clone());
        src.token("https://dev.azure.com/tnt").await.unwrap();
        src.set_tenant("https://dev.azure.com/tnt", "t-7");
        src.sign_in("https://dev.azure.com/tnt").await.unwrap();
        let args = r.last_args("az");
        assert_eq!(args[0], "login");
        assert!(args.windows(2).any(|w| w == ["--tenant", "t-7"]));
        assert_eq!(src.source_of("https://dev.azure.com/tnt"), None);
        // No tenant known: a plain az login.
        src.sign_in("https://dev.azure.com/other").await.unwrap();
        assert!(!r.last_args("az").contains(&"--tenant".to_string()));
    }

    #[tokio::test]
    async fn gcm_timeout_surfaces() {
        let r = Fake::new()
            .fail("git", AuthError::Timeout)
            .fail("az", AuthError::NoCredential);
        assert!(matches!(
            TokenSource::new(r).token("https://dev.azure.com/tnt").await,
            Err(AuthError::Timeout)
        ));
    }

    #[tokio::test]
    async fn az_mfa_is_interaction_required() {
        let r = Fake::new().fail("git", AuthError::NoCredential).fail(
            "az",
            AuthError::Other("WARNING: x\nAADSTS50076: MFA required\n".into()),
        );
        let e = TokenSource::new(r)
            .token("https://dev.azure.com/tnt")
            .await
            .unwrap_err();
        assert_eq!(
            e,
            AuthError::InteractionRequired("AADSTS50076: MFA required".into())
        );
    }

    #[test]
    fn basic_header_encodes_colon_pat() {
        let src = TokenSource::new(Fake::new());
        let c = src.classify("https://dev.azure.com/tnt", "abc");
        assert_eq!(c.header(), "Basic OmFiYw==");
        assert_eq!(format!("{c:?}"), "Basic(<redacted>)");
    }

    #[tokio::test]
    async fn second_call_uses_the_cache() {
        let far = now_secs() + 3600;
        let r = Fake::new().on("git", &format!("password={}\n", jwt(json!({"exp": far}))));
        let src = TokenSource::new(r.clone());
        src.token("https://dev.azure.com/tnt").await.unwrap();
        src.token("https://dev.azure.com/tnt").await.unwrap();
        assert_eq!(r.count("git", "fill"), 1);
        // A PAT is cached too.
        let p = Fake::new().on("git", "password=pat\n");
        let src = TokenSource::new(p.clone());
        src.token("https://dev.azure.com/tnt").await.unwrap();
        src.token("https://dev.azure.com/tnt").await.unwrap();
        assert_eq!(p.count("git", "fill"), 1);
    }

    #[tokio::test]
    async fn expired_token_is_refetched() {
        // exp within the 120 s margin counts as expired.
        let soon = now_secs() + 60;
        let r = Fake::new().on("git", &format!("password={}\n", jwt(json!({"exp": soon}))));
        let src = TokenSource::new(r.clone());
        src.token("https://dev.azure.com/tnt").await.unwrap();
        src.token("https://dev.azure.com/tnt").await.unwrap();
        assert_eq!(r.count("git", "fill"), 2);
    }

    #[tokio::test]
    async fn invalidate_forces_a_refetch_and_orgs_are_separate() {
        let r = Fake::new().on("git", "password=pat\n");
        let src = TokenSource::new(r.clone());
        src.token("https://dev.azure.com/a").await.unwrap();
        src.token("https://dev.azure.com/b").await.unwrap();
        assert_eq!(r.count("git", "fill"), 2);
        src.invalidate("https://dev.azure.com/a");
        src.token("https://dev.azure.com/a").await.unwrap();
        src.token("https://dev.azure.com/b").await.unwrap();
        assert_eq!(r.count("git", "fill"), 3);
    }

    #[tokio::test]
    async fn concurrent_calls_share_one_fetch() {
        let r = Fake::new().on("git", "password=pat\n");
        let src = TokenSource::new(r.clone());
        let (a, b) = tokio::join!(
            src.token("https://dev.azure.com/tnt"),
            src.token("https://dev.azure.com/tnt")
        );
        assert!(a.is_ok() && b.is_ok());
        assert_eq!(r.count("git", "fill"), 1);
    }

    #[tokio::test]
    async fn reject_tells_gcm_and_drops_the_cache() {
        let r = Fake::new().on("git", "password=pat\n");
        let src = TokenSource::new(r.clone());
        src.token("https://dev.azure.com/tnt").await.unwrap();
        src.reject("https://dev.azure.com/tnt").await;
        assert_eq!(r.count("git", "reject"), 1);
        assert_eq!(
            r.stdin_of("git", "reject").as_deref(),
            Some("protocol=https\nhost=dev.azure.com\npath=tnt\n\n")
        );
        src.token("https://dev.azure.com/tnt").await.unwrap();
        assert_eq!(r.count("git", "fill"), 2);
    }

    #[tokio::test]
    async fn unlogged_az_is_no_credential() {
        let r = Fake::new().fail("git", AuthError::NoCredential).fail(
            "az",
            AuthError::Other("ERROR: Please run 'az login' to setup account.\n".into()),
        );
        assert_eq!(
            TokenSource::new(r)
                .token("https://dev.azure.com/tnt")
                .await
                .unwrap_err(),
            AuthError::NoCredential
        );
    }

    #[tokio::test]
    async fn silent_token_asks_gcm_never_to_prompt() {
        let r = Fake::new().on("git", "password=pat\n");
        let src = TokenSource::new(r.clone());
        src.token_silent("https://dev.azure.com/tnt").await.unwrap();
        assert!(r
            .last_args("git")
            .windows(2)
            .any(|w| w == ["-c", "credential.interactive=never"]));
        assert!(r
            .last_env("git")
            .contains(&("GCM_INTERACTIVE".to_string(), "never".to_string())));
        // A cached credential is reused without running anything.
        src.token_silent("https://dev.azure.com/tnt").await.unwrap();
        assert_eq!(r.count("git", "fill"), 1);
        // The interactive path does not pass the switch.
        src.invalidate("https://dev.azure.com/tnt");
        src.token("https://dev.azure.com/tnt").await.unwrap();
        assert!(!r
            .last_args("git")
            .iter()
            .any(|a| a == "credential.interactive=never"));
        assert!(r.last_env("git").is_empty());
    }

    #[test]
    fn tenant_claim() {
        assert_eq!(
            tenant_from_jwt("x.eyJ0aWQiOiJ0LTEifQ.y").as_deref(),
            Some("t-1")
        );
    }
}
