use crate::config::AppConfig;
use crate::error::{MorrowError, Result};
use oauth2::{
    basic::BasicClient, AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken,
    PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, RefreshToken, Scope, TokenResponse,
    TokenUrl,
};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;

const GOOGLE_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const REDIRECT_URI: &str = "http://localhost:8085";
const TASKS_SCOPE: &str = "https://www.googleapis.com/auth/tasks";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub access_token: String,
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_at: Option<i64>,
}

impl Credentials {
    pub fn load() -> Result<Option<Self>> {
        let path = AppConfig::credentials_path();
        if !path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(&path)?;
        let creds: Credentials = serde_json::from_str(&content)?;
        Ok(Some(creds))
    }

    pub fn save(&self) -> Result<()> {
        let path = AppConfig::credentials_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    pub fn from_env() -> Option<Self> {
        std::env::var("MORROW_GOOGLE_REFRESH_TOKEN").ok().map(|rt| Credentials {
            access_token: String::new(),
            refresh_token: Some(rt),
            expires_at: None,
        })
    }
}

pub struct GoogleAuth {
    client: BasicClient,
}

impl GoogleAuth {
    pub fn new() -> Result<Self> {
        let client_id = std::env::var("MORROW_GOOGLE_CLIENT_ID")
            .map_err(|_| MorrowError::Auth("MORROW_GOOGLE_CLIENT_ID not set".to_string()))?;
        let client_secret = std::env::var("MORROW_GOOGLE_CLIENT_SECRET")
            .map_err(|_| MorrowError::Auth("MORROW_GOOGLE_CLIENT_SECRET not set".to_string()))?;

        let client = BasicClient::new(
            ClientId::new(client_id),
            Some(ClientSecret::new(client_secret)),
            AuthUrl::new(GOOGLE_AUTH_URL.to_string()).unwrap(),
            Some(TokenUrl::new(GOOGLE_TOKEN_URL.to_string()).unwrap()),
        )
        .set_redirect_uri(RedirectUrl::new(REDIRECT_URI.to_string()).unwrap());

        Ok(Self { client })
    }

    pub async fn authenticate(&self) -> Result<Credentials> {
        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

        let (auth_url, _csrf_token) = self
            .client
            .authorize_url(CsrfToken::new_random)
            .add_scope(Scope::new(TASKS_SCOPE.to_string()))
            .set_pkce_challenge(pkce_challenge)
            .add_extra_param("access_type", "offline")
            .add_extra_param("prompt", "consent")
            .url();

        println!("Open this URL in your browser to authorize:\n\n{}\n", auth_url);

        let code = self.wait_for_callback()?;
        let token = self.exchange_code(code, pkce_verifier).await?;

        Ok(token)
    }

    fn wait_for_callback(&self) -> Result<AuthorizationCode> {
        let listener = TcpListener::bind("127.0.0.1:8085")
            .map_err(|e| MorrowError::Auth(format!("Failed to bind to port 8085: {}", e)))?;

        println!("Waiting for authorization...");

        let (mut stream, _) = listener
            .accept()
            .map_err(|e| MorrowError::Auth(format!("Failed to accept connection: {}", e)))?;

        let mut reader = BufReader::new(&stream);
        let mut request_line = String::new();
        reader.read_line(&mut request_line)?;

        let code = request_line
            .split_whitespace()
            .nth(1)
            .and_then(|path| {
                url::Url::parse(&format!("http://localhost{}", path)).ok()
            })
            .and_then(|url| {
                url.query_pairs()
                    .find(|(k, _)| k == "code")
                    .map(|(_, v)| v.to_string())
            })
            .ok_or_else(|| MorrowError::Auth("No authorization code received".to_string()))?;

        let response = "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n<html><body><h1>Authorization successful!</h1><p>You can close this window.</p></body></html>";
        stream.write_all(response.as_bytes())?;

        Ok(AuthorizationCode::new(code))
    }

    async fn exchange_code(
        &self,
        code: AuthorizationCode,
        pkce_verifier: PkceCodeVerifier,
    ) -> Result<Credentials> {
        let token_result = self
            .client
            .exchange_code(code)
            .set_pkce_verifier(pkce_verifier)
            .request_async(oauth2::reqwest::async_http_client)
            .await
            .map_err(|e| MorrowError::Auth(format!("Token exchange failed: {}", e)))?;

        let expires_at = token_result.expires_in().map(|d| {
            chrono::Utc::now().timestamp() + d.as_secs() as i64
        });

        Ok(Credentials {
            access_token: token_result.access_token().secret().clone(),
            refresh_token: token_result.refresh_token().map(|t| t.secret().clone()),
            expires_at,
        })
    }

    pub async fn refresh_token(&self, refresh_token: &str) -> Result<Credentials> {
        let token_result = self
            .client
            .exchange_refresh_token(&RefreshToken::new(refresh_token.to_string()))
            .request_async(oauth2::reqwest::async_http_client)
            .await
            .map_err(|e| MorrowError::Auth(format!("Token refresh failed: {}", e)))?;

        let expires_at = token_result.expires_in().map(|d| {
            chrono::Utc::now().timestamp() + d.as_secs() as i64
        });

        Ok(Credentials {
            access_token: token_result.access_token().secret().clone(),
            refresh_token: token_result
                .refresh_token()
                .map(|t| t.secret().clone())
                .or_else(|| Some(refresh_token.to_string())),
            expires_at,
        })
    }

    pub async fn get_valid_credentials(&self) -> Result<Credentials> {
        let creds = Credentials::from_env()
            .or_else(|| Credentials::load().ok().flatten())
            .ok_or_else(|| MorrowError::Auth("No credentials found. Run 'morrow auth' first.".to_string()))?;

        if let Some(refresh_token) = &creds.refresh_token {
            let needs_refresh = creds.expires_at
                .map(|exp| chrono::Utc::now().timestamp() >= exp - 300) // Refresh 5 min early
                .unwrap_or(true);

            if needs_refresh || creds.access_token.is_empty() {
                println!("Refreshing access token...");
                let new_creds = self.refresh_token(refresh_token).await?;
                new_creds.save()?;
                return Ok(new_creds);
            }
        }

        Ok(creds)
    }
}
