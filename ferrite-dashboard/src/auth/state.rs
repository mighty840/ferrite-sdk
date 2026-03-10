use serde::{Deserialize, Serialize};

/// Server's auth mode discovery response from `GET /auth/mode`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct AuthModeInfo {
    pub mode: String,
    pub authority: Option<String>,
    pub client_id: Option<String>,
}

/// Current user info after successful authentication.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserInfo {
    pub name: String,
    pub email: Option<String>,
}

/// Application-wide authentication state.
#[derive(Debug, Clone, PartialEq)]
pub enum AuthState {
    /// Initial state, checking auth status.
    Loading,
    /// User is authenticated.
    Authenticated { user: UserInfo, token: AuthToken },
    /// Not authenticated, login required.
    Unauthenticated { auth_mode: AuthModeInfo },
}

/// Token type matching the active auth mode.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AuthToken {
    Bearer(String),
    Basic(String),
}

impl AuthToken {
    /// Format as an HTTP Authorization header value.
    pub fn header_value(&self) -> String {
        match self {
            AuthToken::Bearer(t) => format!("Bearer {}", t),
            AuthToken::Basic(b) => format!("Basic {}", b),
        }
    }
}
