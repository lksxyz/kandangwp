//! Cloudflare sign-in and API access (ADR 0017).
//!
//! Sign-in is OAuth 2.0 authorization code with PKCE through the browser: the
//! browser approves, Cloudflare redirects to a loopback listener, and the code
//! is exchanged for tokens. The session lives in the desktop keyring as one
//! JSON item and is refreshed shortly before the access token expires.

use std::collections::HashMap;
use std::fmt;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Stdio};
use std::sync::{LazyLock, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use ureq::http::Response;
use url::Url;

use crate::credentials;

/// Wrangler's public OAuth client. Kandang's own client would work only for
/// the Cloudflare account that owns it (ADR 0014); see ADR 0017.
const CLIENT_ID: &str = "54d11594-84e4-41aa-b438-e81b8fa78ee7";
const AUTH_URL: &str = "https://dash.cloudflare.com/oauth2/auth";
const TOKEN_URL: &str = "https://dash.cloudflare.com/oauth2/token";
const REVOKE_URL: &str = "https://dash.cloudflare.com/oauth2/revoke";
const API_URL: &str = "https://api.cloudflare.com/client/v4";
const CALLBACK_PORT: u16 = 8976;
const CALLBACK_PATH: &str = "/oauth/callback";
const REDIRECT_URI: &str = "http://localhost:8976/oauth/callback";
const SCOPES: &str = "account:read pages:write offline_access";
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const REFRESH_MARGIN_SECS: u64 = 60;
const MAX_ATTEMPTS: u32 = 3;

#[derive(Serialize, Deserialize)]
struct Session {
    access_token: String,
    refresh_token: String,
    expires_at: u64,
}

#[derive(Deserialize)]
struct Tokens {
    access_token: String,
    expires_in: u64,
    refresh_token: Option<String>,
}

/// The token endpoint refused the grant: the code or refresh token is no longer valid.
#[derive(Debug)]
struct Rejected(String);

impl fmt::Display for Rejected {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Rejected {}

/// Who a request is made for: the signed-in session, or a short-lived bearer
/// token such as the Pages upload JWT.
pub enum Auth<'a> {
    Session,
    Bearer(&'a str),
}

/// Starts the browser sign-in. Returns once the browser has been opened; the
/// approval is completed on a background thread that stores the session.
pub fn start() -> Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", CALLBACK_PORT)).with_context(|| {
        format!("port {CALLBACK_PORT} is in use; finish or close the other Cloudflare sign-in")
    })?;
    let verifier = random_urlsafe(32)?;
    let state = random_urlsafe(16)?;
    open_browser(&authorize_url(&state, &code_challenge(&verifier)))?;
    thread::spawn(move || {
        if let Err(e) = wait_for_callback(&listener, &state, &verifier) {
            eprintln!("cloudflare sign-in failed: {e:#}");
        }
    });
    Ok(())
}

pub fn connected() -> bool {
    matches!(load(), Ok(Some(_)))
}

/// Signs out here and revokes the refresh token at Cloudflare.
pub fn disconnect() -> Result<()> {
    let session = load()?;
    credentials::clear_cloudflare_session()?;
    if let Some(session) = session {
        let response = AGENT
            .post(REVOKE_URL)
            .send_form([
                ("client_id", CLIENT_ID),
                ("token_type_hint", "refresh_token"),
                ("token", session.refresh_token.as_str()),
            ])
            .context("contacting Cloudflare to revoke the sign-in")?;
        if !response.status().is_success() {
            bail!(
                "Cloudflare did not revoke the sign-in (HTTP {}); signed out on this machine",
                response.status()
            );
        }
    }
    Ok(())
}

/// The first account the signed-in session can see.
pub fn account_id() -> Result<String> {
    let accounts = get(Auth::Session, "/accounts")?;
    accounts
        .as_array()
        .and_then(|list| list.first())
        .and_then(|account| account["id"].as_str())
        .map(str::to_string)
        .context("this Cloudflare sign-in has no accounts")
}

pub fn get(auth: Auth, path: &str) -> Result<Value> {
    let url = format!("{API_URL}{path}");
    let authorization = bearer(auth)?;
    let response = execute(|| {
        AGENT
            .get(&url)
            .header("Authorization", authorization.as_str())
            .call()
    })?;
    unwrap_envelope(response)
}

pub fn post(auth: Auth, path: &str, content_type: &str, body: &[u8]) -> Result<Value> {
    let url = format!("{API_URL}{path}");
    let authorization = bearer(auth)?;
    let response = execute(|| {
        AGENT
            .post(&url)
            .header("Authorization", authorization.as_str())
            .header("Content-Type", content_type)
            .send(body)
    })?;
    unwrap_envelope(response)
}

pub fn random_urlsafe(len: usize) -> Result<String> {
    let mut bytes = vec![0u8; len];
    File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

fn code_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

fn authorize_url(state: &str, challenge: &str) -> String {
    let mut url = Url::parse(AUTH_URL).expect("AUTH_URL is a valid URL");
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", CLIENT_ID)
        .append_pair("redirect_uri", REDIRECT_URI)
        .append_pair("scope", SCOPES)
        .append_pair("state", state)
        .append_pair("code_challenge", challenge)
        .append_pair("code_challenge_method", "S256");
    url.to_string()
}

fn open_browser(url: &str) -> Result<()> {
    Command::new("xdg-open")
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("could not open a browser; open this link to sign in: {url}"))?;
    Ok(())
}

fn wait_for_callback(listener: &TcpListener, state: &str, verifier: &str) -> Result<()> {
    listener.set_nonblocking(true)?;
    let deadline = Instant::now() + SIGN_IN_TIMEOUT;
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false)?;
                stream.set_read_timeout(Some(Duration::from_secs(10)))?;
                if handle_request(stream, state, verifier)? {
                    return Ok(());
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    bail!("no approval in the browser within 5 minutes");
                }
                thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return Err(e.into()),
        }
    }
}

/// Returns `Ok(true)` once the session is stored, `Ok(false)` for unrelated requests.
fn handle_request(mut stream: TcpStream, state: &str, verifier: &str) -> Result<bool> {
    let mut line = String::new();
    BufReader::new(&stream).read_line(&mut line)?;
    let target = line.split_whitespace().nth(1).unwrap_or_default();
    let code = match parse_callback(target, state) {
        Ok(Some(code)) => code,
        Ok(None) => {
            respond(&mut stream, "404 Not Found", "Not found.");
            return Ok(false);
        }
        Err(e) => {
            respond(
                &mut stream,
                "400 Bad Request",
                "Cloudflare sign-in was not completed. Return to Kandang and try again.",
            );
            return Err(e);
        }
    };
    match exchange_code(&code, verifier) {
        Ok(()) => {
            respond(
                &mut stream,
                "200 OK",
                "Signed in to Cloudflare. You can close this tab.",
            );
            Ok(true)
        }
        Err(e) => {
            respond(
                &mut stream,
                "500 Internal Server Error",
                "Cloudflare sign-in failed. See the Kandang terminal output.",
            );
            Err(e)
        }
    }
}

/// `Ok(None)` for any path other than the callback.
fn parse_callback(target: &str, state: &str) -> Result<Option<String>> {
    let url =
        Url::parse(&format!("http://localhost{target}")).context("malformed callback request")?;
    if url.path() != CALLBACK_PATH {
        return Ok(None);
    }
    let pairs: HashMap<String, String> = url.query_pairs().into_owned().collect();
    if let Some(error) = pairs.get("error") {
        bail!("Cloudflare did not approve the sign-in: {error}");
    }
    if pairs.get("state").map(String::as_str) != Some(state) {
        bail!("the callback does not belong to this sign-in");
    }
    let code = pairs
        .get("code")
        .context("the callback has no authorization code")?;
    Ok(Some(code.clone()))
}

fn respond(stream: &mut TcpStream, status: &str, body: &str) {
    // The browser may already have closed the tab; nothing to do then.
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
}

fn exchange_code(code: &str, verifier: &str) -> Result<()> {
    let tokens = token_request(&[
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", REDIRECT_URI),
        ("client_id", CLIENT_ID),
        ("code_verifier", verifier),
    ])?;
    let refresh_token = tokens
        .refresh_token
        .context("Cloudflare returned no refresh token")?;
    save(&Session {
        access_token: tokens.access_token,
        refresh_token,
        expires_at: now() + tokens.expires_in,
    })
}

fn token_request(form: &[(&str, &str)]) -> Result<Tokens> {
    let mut response = AGENT
        .post(TOKEN_URL)
        .send_form(form.iter().copied())
        .context("contacting Cloudflare sign-in")?;
    let status = response.status();
    let text = response
        .body_mut()
        .read_to_string()
        .context("reading the Cloudflare sign-in response")?;
    if status.is_success() {
        return serde_json::from_str(&text).context("reading Cloudflare's token response");
    }
    let detail = serde_json::from_str::<Value>(&text)
        .ok()
        .and_then(|v| {
            v["error_description"]
                .as_str()
                .or(v["error"].as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| status.to_string());
    if status.is_client_error() {
        return Err(Rejected(detail).into());
    }
    bail!("Cloudflare sign-in failed: {detail}")
}

fn save(session: &Session) -> Result<()> {
    credentials::store_cloudflare_session(&serde_json::to_string(session)?)
}

fn load() -> Result<Option<Session>> {
    match credentials::load_cloudflare_session()? {
        None => Ok(None),
        Some(json) => Ok(Some(serde_json::from_str(&json).context(
            "the stored Cloudflare session is unreadable; connect Cloudflare again",
        )?)),
    }
}

fn access_token() -> Result<String> {
    let session = load()?.context("Cloudflare is not connected")?;
    if is_fresh(&session) {
        return Ok(session.access_token);
    }
    static REFRESH_LOCK: Mutex<()> = Mutex::new(());
    let _guard = REFRESH_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // Another caller may have refreshed while this one waited for the lock.
    let session = load()?.context("Cloudflare is not connected")?;
    if is_fresh(&session) {
        return Ok(session.access_token);
    }
    refresh(&session)
}

fn is_fresh(session: &Session) -> bool {
    session.expires_at > now() + REFRESH_MARGIN_SECS
}

fn refresh(session: &Session) -> Result<String> {
    let tokens = match token_request(&[
        ("grant_type", "refresh_token"),
        ("refresh_token", session.refresh_token.as_str()),
        ("client_id", CLIENT_ID),
    ]) {
        Ok(tokens) => tokens,
        Err(e) if e.downcast_ref::<Rejected>().is_some() => {
            credentials::clear_cloudflare_session()?;
            bail!("the Cloudflare sign-in has expired; connect Cloudflare again");
        }
        Err(e) => return Err(e),
    };
    let next = Session {
        access_token: tokens.access_token,
        refresh_token: tokens
            .refresh_token
            .unwrap_or_else(|| session.refresh_token.clone()),
        expires_at: now() + tokens.expires_in,
    };
    save(&next)?;
    Ok(next.access_token)
}

fn bearer(auth: Auth) -> Result<String> {
    Ok(match auth {
        Auth::Session => format!("Bearer {}", access_token()?),
        Auth::Bearer(token) => format!("Bearer {token}"),
    })
}

static AGENT: LazyLock<ureq::Agent> = LazyLock::new(|| {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(60)))
        .build()
        .into()
});

/// Sends a request, retrying transport errors, 429, and 5xx with backoff.
fn execute(
    mut send: impl FnMut() -> std::result::Result<Response<ureq::Body>, ureq::Error>,
) -> Result<Response<ureq::Body>> {
    let mut attempt = 1;
    loop {
        let failure = match send() {
            Ok(response) => {
                let status = response.status();
                if !(status.is_server_error() || status.as_u16() == 429) {
                    return Ok(response);
                }
                format!("HTTP {status}")
            }
            Err(e) => e.to_string(),
        };
        if attempt >= MAX_ATTEMPTS {
            bail!("Cloudflare API request failed after {MAX_ATTEMPTS} attempts: {failure}");
        }
        thread::sleep(Duration::from_secs(1 << (attempt - 1)));
        attempt += 1;
    }
}

#[derive(Deserialize)]
struct Envelope {
    success: bool,
    #[serde(default)]
    errors: Vec<ApiMessage>,
    #[serde(default)]
    result: Value,
}

#[derive(Deserialize)]
struct ApiMessage {
    message: String,
}

fn unwrap_envelope(mut response: Response<ureq::Body>) -> Result<Value> {
    let status = response.status();
    let text = response.body_mut().read_to_string()?;
    let envelope: Envelope = serde_json::from_str(&text)
        .with_context(|| format!("Cloudflare API returned HTTP {status} without a JSON body"))?;
    if !envelope.success || !status.is_success() {
        let detail = envelope
            .errors
            .iter()
            .map(|e| e.message.as_str())
            .collect::<Vec<_>>()
            .join("; ");
        bail!("Cloudflare API error (HTTP {status}): {detail}");
    }
    Ok(envelope.result)
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_challenge_matches_the_rfc_7636_example() {
        assert_eq!(
            code_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn callback_yields_the_code_when_the_state_matches() {
        let code = parse_callback("/oauth/callback?code=abc&state=s1", "s1").unwrap();
        assert_eq!(code.as_deref(), Some("abc"));
    }

    #[test]
    fn callback_rejects_a_state_from_another_sign_in() {
        assert!(parse_callback("/oauth/callback?code=abc&state=other", "s1").is_err());
    }

    #[test]
    fn callback_reports_a_sign_in_the_user_denied() {
        assert!(parse_callback("/oauth/callback?error=access_denied&state=s1", "s1").is_err());
    }

    #[test]
    fn other_paths_are_not_the_callback() {
        assert_eq!(parse_callback("/favicon.ico", "s1").unwrap(), None);
    }
}
