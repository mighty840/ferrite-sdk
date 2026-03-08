use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

/// Token set returned after successful OIDC authentication.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenSet {
    pub access_token: String,
    pub id_token: Option<String>,
    pub refresh_token: Option<String>,
    pub expires_in: u64,
    pub token_type: String,
}

/// OIDC client implementing the Authorization Code flow with PKCE.
pub struct OidcClient {
    pub client_id: String,
    pub authority: String,
    pub redirect_uri: String,
    code_verifier: Option<String>,
}

impl OidcClient {
    pub fn new(client_id: &str, authority: &str, redirect_uri: &str) -> Self {
        Self {
            client_id: client_id.to_string(),
            authority: authority.trim_end_matches('/').to_string(),
            redirect_uri: redirect_uri.to_string(),
            code_verifier: None,
        }
    }

    /// Generate a random string for PKCE code verifier.
    fn generate_verifier() -> String {
        let array = js_sys::Uint8Array::new_with_length(32);
        let crypto = web_sys::window()
            .expect("no window")
            .crypto()
            .expect("no crypto");
        crypto
            .get_random_values_with_array_buffer_view(&array)
            .expect("random values failed");
        let bytes: Vec<u8> = array.to_vec();
        base64url_encode(&bytes)
    }

    /// Compute SHA-256 hash for PKCE code challenge.
    async fn compute_challenge(verifier: &str) -> String {
        let window = web_sys::window().expect("no window");
        let crypto = window.crypto().expect("no crypto");
        let subtle = crypto.subtle();
        let data = js_sys::Uint8Array::from(verifier.as_bytes());
        let algorithm = js_sys::Object::new();
        js_sys::Reflect::set(
            &algorithm,
            &JsValue::from_str("name"),
            &JsValue::from_str("SHA-256"),
        )
        .unwrap();
        let promise = subtle
            .digest_with_object_and_buffer_source(&algorithm, &data)
            .expect("digest failed");
        let result = wasm_bindgen_futures::JsFuture::from(promise)
            .await
            .expect("digest promise failed");
        let hash = js_sys::Uint8Array::new(&result);
        base64url_encode(&hash.to_vec())
    }

    /// Start the OIDC login flow by redirecting to the authorization endpoint.
    pub async fn start_login(&mut self) {
        let verifier = Self::generate_verifier();
        let challenge = Self::compute_challenge(&verifier).await;
        self.code_verifier = Some(verifier.clone());

        // Store verifier in session storage for use after redirect
        if let Some(storage) = web_sys::window()
            .and_then(|w| w.session_storage().ok())
            .flatten()
        {
            let _ = storage.set_item("iotai_pkce_verifier", &verifier);
        }

        let auth_url = format!(
            "{}/authorize?response_type=code&client_id={}&redirect_uri={}&scope=openid%20profile&code_challenge={}&code_challenge_method=S256&state=iotai",
            self.authority, self.client_id, self.redirect_uri, challenge
        );

        if let Some(window) = web_sys::window() {
            let _ = window.location().set_href(&auth_url);
        }
    }

    /// Exchange the authorization code for tokens.
    pub async fn handle_callback(&self, code: &str) -> Result<TokenSet, String> {
        let verifier = if let Some(v) = &self.code_verifier {
            v.clone()
        } else if let Some(storage) = web_sys::window()
            .and_then(|w| w.session_storage().ok())
            .flatten()
        {
            storage
                .get_item("iotai_pkce_verifier")
                .ok()
                .flatten()
                .ok_or("No PKCE verifier found in session storage")?
        } else {
            return Err("No PKCE verifier available".to_string());
        };

        let token_url = format!("{}/token", self.authority);
        let params = [
            ("grant_type", "authorization_code"),
            ("client_id", &self.client_id),
            ("code", code),
            ("redirect_uri", &self.redirect_uri),
            ("code_verifier", &verifier),
        ];

        let client = reqwest::Client::new();
        let resp = client
            .post(&token_url)
            .form(&params)
            .send()
            .await
            .map_err(|e| format!("Token request failed: {}", e))?;

        if resp.status().is_success() {
            let token_set: TokenSet = resp
                .json()
                .await
                .map_err(|e| format!("Failed to parse token response: {}", e))?;
            Ok(token_set)
        } else {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            Err(format!("Token exchange failed ({}): {}", status, body))
        }
    }
}

fn base64url_encode(input: &[u8]) -> String {
    let encoded = base64_encode(input);
    encoded
        .replace('+', "-")
        .replace('/', "_")
        .trim_end_matches('=')
        .to_string()
}

fn base64_encode(input: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}
