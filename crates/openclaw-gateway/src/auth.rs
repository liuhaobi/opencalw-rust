//! Connect-time authentication and the resulting authorization grant.

use openclaw_protocol::{ConnectAuth, ErrorShape};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use crate::config::{AuthConfig, AuthMode};

pub const SCOPE_READ: &str = "operator.read";
pub const SCOPE_WRITE: &str = "operator.write";
pub const SCOPE_ADMIN: &str = "operator.admin";

#[derive(Debug, Clone)]
pub struct AuthGrant {
    pub method: String,
    pub role: String,
    pub scopes: Vec<String>,
}

impl AuthGrant {
    pub fn has_scope(&self, scope: &str) -> bool {
        self.scopes.iter().any(|s| s == scope)
    }

    pub fn can_write(&self) -> bool {
        self.has_scope(SCOPE_WRITE) || self.has_scope(SCOPE_ADMIN)
    }
}

#[derive(Debug, Clone)]
pub struct Authenticator {
    config: AuthConfig,
}

impl Authenticator {
    pub fn new(config: AuthConfig) -> Self {
        Self { config }
    }

    pub fn mode(&self) -> AuthMode {
        self.config.mode
    }

    pub fn verify(&self, auth: Option<&ConnectAuth>) -> Result<AuthGrant, ErrorShape> {
        match self.config.mode {
            AuthMode::None => Ok(self.grant("none")),
            AuthMode::Token => {
                let expected = self.config.token.as_deref().unwrap_or_default();
                let presented = auth.and_then(|a| a.token.as_deref()).or_else(|| {
                    // Node hosts present long-lived device tokens; accept them as
                    // gateway credentials when they match the configured token.
                    auth.and_then(|a| a.device_token.as_deref())
                });
                match presented {
                    Some(value) if secrets_equal(value, expected) => Ok(self.grant("token")),
                    _ => Err(ErrorShape::unauthorized("invalid gateway token")),
                }
            }
            AuthMode::Password => {
                let expected = self.config.password.as_deref().unwrap_or_default();
                match auth.and_then(|a| a.password.as_deref()) {
                    Some(value) if secrets_equal(value, expected) => Ok(self.grant("password")),
                    _ => Err(ErrorShape::unauthorized("invalid gateway password")),
                }
            }
        }
    }

    fn grant(&self, method: &str) -> AuthGrant {
        AuthGrant {
            method: method.to_string(),
            role: "operator".to_string(),
            scopes: vec![
                SCOPE_READ.to_string(),
                SCOPE_WRITE.to_string(),
                SCOPE_ADMIN.to_string(),
            ],
        }
    }
}

/// Constant-time comparison that also hides length differences.
fn secrets_equal(presented: &str, expected: &str) -> bool {
    let a = Sha256::digest(presented.as_bytes());
    let b = Sha256::digest(expected.as_bytes());
    a.ct_eq(&b).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token_auth(value: &str) -> ConnectAuth {
        ConnectAuth {
            token: Some(value.to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn accepts_matching_token() {
        let auth = Authenticator::new(AuthConfig {
            mode: AuthMode::Token,
            token: Some("s3cret".into()),
            password: None,
        });
        let grant = auth.verify(Some(&token_auth("s3cret"))).unwrap();
        assert_eq!(grant.method, "token");
        assert!(grant.can_write());
    }

    #[test]
    fn rejects_wrong_token() {
        let auth = Authenticator::new(AuthConfig {
            mode: AuthMode::Token,
            token: Some("s3cret".into()),
            password: None,
        });
        let err = auth.verify(Some(&token_auth("nope"))).unwrap_err();
        assert_eq!(err.code, "unauthorized");
    }

    #[test]
    fn rejects_missing_credentials() {
        let auth = Authenticator::new(AuthConfig {
            mode: AuthMode::Token,
            token: Some("s3cret".into()),
            password: None,
        });
        assert!(auth.verify(None).is_err());
    }
}
