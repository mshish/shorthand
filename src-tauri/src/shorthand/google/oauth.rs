//! Google OAuth for a desktop app: PKCE, the authorization URL, a one-shot
//! loopback listener for the redirect, and the token endpoint calls.
//!
//! Ported from shorthand-config's `src-tauri/src/google/oauth.rs`, which this
//! replaces as the writer of `google-credentials.json` (see `credentials.rs`).
//! Two changes from the donor: the loopback listener runs on tokio rather than
//! `tiny_http`, and the PKCE verifier's randomness comes from `uuid` v4, so
//! the app takes no new crate dependency for either.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// How long to wait for the browser to come back before giving up. Without a
/// bound, closing the consent tab leaves the listener (and its port) held for
/// the life of the process.
pub const REDIRECT_LISTEN_TIMEOUT: Duration = Duration::from_secs(300);

const GOOGLE_TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com";

pub struct PkceChallenge {
    pub code_verifier: String,
    pub code_challenge: String,
}

/// 32 random bytes, base64url without padding: a 43-character verifier, the
/// RFC 7636 minimum. The bytes are two v4 UUIDs (244 random bits), which is
/// well above what PKCE needs and keeps `rand` out of the dependency list.
pub fn generate_pkce_challenge() -> PkceChallenge {
    let mut bytes = [0u8; 32];
    bytes[..16].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
    bytes[16..].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
    let code_verifier = URL_SAFE_NO_PAD.encode(bytes);
    let code_challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(code_verifier.as_bytes()));
    PkceChallenge {
        code_verifier,
        code_challenge,
    }
}

pub struct AuthorizationUrlOptions<'a> {
    pub client_id: &'a str,
    pub redirect_uri: &'a str,
    pub code_challenge: &'a str,
    pub scope: &'a str,
    /// Ask Google to show its one-file picker on the consent page. Omitted for
    /// the create flow, where there is nothing to pick yet.
    pub use_picker: bool,
}

pub fn build_authorization_url(options: &AuthorizationUrlOptions) -> String {
    let mut url = url::Url::parse("https://accounts.google.com/o/oauth2/v2/auth")
        .expect("static URL is always valid");
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("client_id", options.client_id);
        pairs.append_pair("redirect_uri", options.redirect_uri);
        pairs.append_pair("response_type", "code");
        pairs.append_pair("scope", options.scope);
        // Both, every time: without them a user who already granted access
        // gets no refresh token on the second consent.
        pairs.append_pair("access_type", "offline");
        pairs.append_pair("prompt", "consent");
        pairs.append_pair("code_challenge", options.code_challenge);
        pairs.append_pair("code_challenge_method", "S256");
        if options.use_picker {
            pairs.append_pair("trigger_onepick", "true");
        }
    }
    url.into()
}

#[derive(Debug, PartialEq, Eq)]
pub struct LoopbackResult {
    pub code: String,
    pub picked_file_ids: Vec<String>,
}

pub async fn bind_loopback(port: u16) -> Result<TcpListener, String> {
    TcpListener::bind(("127.0.0.1", port)).await.map_err(|e| {
        format!(
            "Couldn't listen on 127.0.0.1:{port} for Google's sign-in redirect ({e}). \
             Close any other app using that port (shorthand-config uses the same one) and try again."
        )
    })
}

/// Waits for one request on `listener`, answers it, and returns the code and
/// any picked file ids from its query string.
pub async fn listen_for_redirect(
    listener: TcpListener,
    timeout: Duration,
) -> Result<LoopbackResult, String> {
    let accepted = tokio::time::timeout(timeout, listener.accept())
        .await
        .map_err(|_| {
            "Timed out waiting for Google sign-in. Please try connecting again.".to_string()
        })?
        .map_err(|e| e.to_string())?;
    let (mut stream, _) = accepted;

    let mut head = Vec::new();
    let mut buffer = [0u8; 2048];
    while !head.windows(4).any(|w| w == b"\r\n\r\n") && head.len() < 16 * 1024 {
        let read = tokio::time::timeout(Duration::from_secs(10), stream.read(&mut buffer))
            .await
            .map_err(|_| "Google's redirect arrived but never finished sending.".to_string())?
            .map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        head.extend_from_slice(&buffer[..read]);
    }
    let head = String::from_utf8_lossy(&head);
    let target = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");
    let parsed =
        url::Url::parse(&format!("http://127.0.0.1{target}")).map_err(|e| e.to_string())?;
    let params: HashMap<String, String> = parsed.query_pairs().into_owned().collect();
    let code = params.get("code").cloned();

    let body = if code.is_some() {
        "You can close this tab and go back to Shorthand."
    } else {
        "Google didn't send an authorization code. Go back to Shorthand and try again."
    };
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;

    match code {
        None => Err(match params.get("error").map(String::as_str) {
            Some("access_denied") => "Google sign-in was cancelled.".to_string(),
            Some(other) => format!("Google sign-in failed: {other}"),
            None => "Google's redirect had no authorization code.".to_string(),
        }),
        Some(code) => Ok(LoopbackResult {
            code,
            picked_file_ids: params
                .get("picked_file_ids")
                .map(|v| {
                    v.split(',')
                        .filter(|s| !s.is_empty())
                        .map(String::from)
                        .collect()
                })
                .unwrap_or_default(),
        }),
    }
}

#[derive(serde::Deserialize)]
struct TokenResponse {
    refresh_token: Option<String>,
    access_token: Option<String>,
}

pub struct ClientCredentials<'a> {
    pub client_id: &'a str,
    pub client_secret: &'a str,
}

pub async fn exchange_code(
    http: &reqwest::Client,
    client: &ClientCredentials<'_>,
    code: &str,
    code_verifier: &str,
    redirect_uri: &str,
) -> Result<String, String> {
    exchange_code_at(
        http,
        GOOGLE_TOKEN_ENDPOINT,
        client,
        code,
        code_verifier,
        redirect_uri,
    )
    .await
}

pub(super) async fn exchange_code_at(
    http: &reqwest::Client,
    endpoint: &str,
    client: &ClientCredentials<'_>,
    code: &str,
    code_verifier: &str,
    redirect_uri: &str,
) -> Result<String, String> {
    let response = http
        .post(format!("{endpoint}/token"))
        .form(&[
            ("client_id", client.client_id),
            ("client_secret", client.client_secret),
            ("code", code),
            ("code_verifier", code_verifier),
            ("redirect_uri", redirect_uri),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "Google token exchange failed: {}",
            response.text().await.unwrap_or_default()
        ));
    }
    let parsed: TokenResponse = response.json().await.map_err(|e| e.to_string())?;
    // A missing refresh token yields a credential that works exactly once,
    // so this is an error, not an empty value.
    parsed
        .refresh_token
        .ok_or_else(|| "Google didn't return a refresh token. Try connecting again.".to_string())
}

pub async fn refresh_access_token(
    http: &reqwest::Client,
    client: &ClientCredentials<'_>,
    refresh_token: &str,
) -> Result<String, String> {
    refresh_access_token_at(http, GOOGLE_TOKEN_ENDPOINT, client, refresh_token).await
}

pub(super) async fn refresh_access_token_at(
    http: &reqwest::Client,
    endpoint: &str,
    client: &ClientCredentials<'_>,
    refresh_token: &str,
) -> Result<String, String> {
    let response = http
        .post(format!("{endpoint}/token"))
        .form(&[
            ("client_id", client.client_id),
            ("client_secret", client.client_secret),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "Google token refresh failed: {}",
            response.text().await.unwrap_or_default()
        ));
    }
    let parsed: TokenResponse = response.json().await.map_err(|e| e.to_string())?;
    parsed
        .access_token
        .ok_or_else(|| "Google's token refresh returned no access token.".to_string())
}

#[cfg(test)]
mod tests {
    use super::super::test_http::{client, serve, Route};
    use super::*;
    use tokio::net::TcpStream;

    fn params(url: &str) -> HashMap<String, String> {
        url::Url::parse(url)
            .unwrap()
            .query_pairs()
            .into_owned()
            .collect()
    }

    fn options(use_picker: bool) -> AuthorizationUrlOptions<'static> {
        AuthorizationUrlOptions {
            client_id: "test-client-id",
            redirect_uri: "http://127.0.0.1:8721/callback",
            code_challenge: "test-challenge",
            scope: super::super::GOOGLE_DOCS_SCOPE,
            use_picker,
        }
    }

    const CLIENT: ClientCredentials<'static> = ClientCredentials {
        client_id: "cid",
        client_secret: "csecret",
    };

    #[test]
    fn pkce_verifier_is_url_safe_and_challenge_is_its_s256() {
        let challenge = generate_pkce_challenge();
        assert!((43..=128).contains(&challenge.code_verifier.len()));
        assert!(!challenge.code_verifier.contains(['+', '/', '=']));
        assert_eq!(
            challenge.code_challenge,
            URL_SAFE_NO_PAD.encode(Sha256::digest(challenge.code_verifier.as_bytes()))
        );
        assert_ne!(
            challenge.code_verifier,
            generate_pkce_challenge().code_verifier
        );
    }

    #[test]
    fn authorization_url_asks_for_offline_access_consent_pkce_and_drive_file_only() {
        let url = build_authorization_url(&options(true));
        assert!(url.starts_with("https://accounts.google.com/o/oauth2/v2/auth?"));
        let p = params(&url);
        assert_eq!(p["access_type"], "offline");
        assert_eq!(p["prompt"], "consent");
        assert_eq!(p["code_challenge"], "test-challenge");
        assert_eq!(p["code_challenge_method"], "S256");
        assert_eq!(p["scope"], "https://www.googleapis.com/auth/drive.file");
        assert_eq!(p["client_id"], "test-client-id");
        assert_eq!(p["redirect_uri"], "http://127.0.0.1:8721/callback");
        assert_eq!(p["response_type"], "code");
    }

    #[test]
    fn picker_flag_only_appears_when_asked_for() {
        assert_eq!(
            params(&build_authorization_url(&options(true)))["trigger_onepick"],
            "true"
        );
        assert!(!params(&build_authorization_url(&options(false))).contains_key("trigger_onepick"));
    }

    async fn hit(port: u16, path: &str) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        stream
            .write_all(format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n").as_bytes())
            .await
            .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).await.unwrap();
        response
    }

    #[tokio::test]
    async fn loopback_returns_code_and_drops_empty_picked_ids() {
        let listener = bind_loopback(0).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let waiting = tokio::spawn(listen_for_redirect(listener, Duration::from_secs(5)));
        let response = hit(port, "/callback?code=abc123&picked_file_ids=file1,file2,").await;
        assert!(response.starts_with("HTTP/1.1 200"));
        assert!(response.contains("go back to Shorthand"));
        let result = waiting.await.unwrap().unwrap();
        assert_eq!(
            result,
            LoopbackResult {
                code: "abc123".into(),
                picked_file_ids: vec!["file1".into(), "file2".into()],
            }
        );
    }

    #[tokio::test]
    async fn loopback_with_no_picker_round_trip_yields_no_ids() {
        let listener = bind_loopback(0).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let waiting = tokio::spawn(listen_for_redirect(listener, Duration::from_secs(5)));
        hit(port, "/callback?code=abc123").await;
        assert!(waiting.await.unwrap().unwrap().picked_file_ids.is_empty());
    }

    #[tokio::test]
    async fn loopback_reports_a_cancelled_consent_and_still_answers_the_browser() {
        let listener = bind_loopback(0).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let waiting = tokio::spawn(listen_for_redirect(listener, Duration::from_secs(5)));
        let response = hit(port, "/callback?error=access_denied").await;
        assert!(response.starts_with("HTTP/1.1 200"));
        assert_eq!(
            waiting.await.unwrap().unwrap_err(),
            "Google sign-in was cancelled."
        );
    }

    #[tokio::test]
    async fn loopback_times_out_instead_of_holding_the_port_forever() {
        let listener = bind_loopback(0).await.unwrap();
        let err = listen_for_redirect(listener, Duration::from_millis(50))
            .await
            .unwrap_err();
        assert!(err.contains("Timed out"), "unexpected error: {err}");
    }

    #[tokio::test]
    async fn exchange_returns_the_refresh_token() {
        let (base, requests) = serve(vec![Route::json(
            "POST /token",
            200,
            r#"{"access_token":"at","refresh_token":"rt","expires_in":3600}"#,
        )])
        .await;
        let http = client();
        let token = exchange_code_at(&http, &base, &CLIENT, "code123", "verifier", "http://x/cb")
            .await
            .unwrap();
        assert_eq!(token, "rt");
        let seen = requests.await.unwrap();
        assert!(seen[0].body.contains("grant_type=authorization_code"));
        assert!(seen[0].body.contains("code_verifier=verifier"));
    }

    #[tokio::test]
    async fn exchange_without_a_refresh_token_is_an_error() {
        let (base, _requests) = serve(vec![Route::json(
            "POST /token",
            200,
            r#"{"access_token":"at","expires_in":3600}"#,
        )])
        .await;
        let http = client();
        let err = exchange_code_at(&http, &base, &CLIENT, "c", "v", "http://x/cb")
            .await
            .unwrap_err();
        assert!(err.contains("refresh token"));
    }

    #[tokio::test]
    async fn refresh_returns_an_access_token() {
        let (base, requests) = serve(vec![Route::json(
            "POST /token",
            200,
            r#"{"access_token":"fresh-at","expires_in":3600}"#,
        )])
        .await;
        let http = client();
        let token = refresh_access_token_at(&http, &base, &CLIENT, "rt")
            .await
            .unwrap();
        assert_eq!(token, "fresh-at");
        assert!(requests.await.unwrap()[0]
            .body
            .contains("grant_type=refresh_token"));
    }
}
