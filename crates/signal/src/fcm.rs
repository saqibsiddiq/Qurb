//! Waking an Android device through Firebase Cloud Messaging.
//!
//! The only way to reach a sleeping Android app from outside, and on iOS the
//! equivalent (APNs) is the only way at all — both platforms reserve it to
//! themselves. See
//! [decision 0028](../../docs/decisions/0028-waking-a-sleeping-device.md) for
//! what that costs and why it is accepted.
//!
//! **What is sent is a poke and nothing else.** No filenames, no sizes, no
//! counts, not even which peer has something. A woken device syncs with the
//! peers it already knows, so there is nothing useful to put in the message
//! and every reason not to: Google sees the message, and qurb's promise is
//! that nobody outside the devices learns what is being synced. Google learns
//! that a device was poked and when, which is metadata this cannot avoid and
//! should not pretend to.
//!
//! # Not verified against the live service
//!
//! This is written from the HTTP v1 specification and has not been run against
//! Firebase, because doing so needs a project, a service account and a real
//! device. The shape is covered by tests — the request that would be sent, and
//! the token cache — but the first real send is unproven. Said here rather
//! than discovered later.

use crate::wake::{WakeToken, Waker};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// What Google issues for a service account, as the JSON file gives it.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ServiceAccount {
    pub project_id: String,
    pub client_email: String,
    pub private_key: String,
}

impl ServiceAccount {
    /// Read the JSON file Firebase downloads for a service account.
    pub fn from_file(path: &std::path::Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("reading {}: {e}", path.display()))?;
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
    }
}

/// An access token and when it stops being usable.
struct Access {
    token: String,
    until: Instant,
}

/// Sends pokes to Firebase.
pub struct Fcm {
    account: ServiceAccount,
    http: reqwest::Client,
    access: Arc<Mutex<Option<Access>>>,
}

impl Fcm {
    pub fn new(account: ServiceAccount) -> Self {
        Self {
            account,
            http: reqwest::Client::new(),
            access: Arc::new(Mutex::new(None)),
        }
    }

    /// The URL a message is posted to.
    fn endpoint(&self) -> String {
        format!(
            "https://fcm.googleapis.com/v1/projects/{}/messages:send",
            self.account.project_id
        )
    }

    /// Exchange the service account for an access token, or reuse the one held.
    ///
    /// Google's tokens last an hour. Asking for one per push would triple the
    /// round trips and rate-limit a busy service, so it is cached with a
    /// margin — a token about to expire mid-request is worse than an early
    /// refresh.
    async fn access_token(&self) -> Result<String, String> {
        const MARGIN: Duration = Duration::from_secs(120);

        if let Some(held) = self.access.lock().expect("access").as_ref() {
            if held.until > Instant::now() + MARGIN {
                return Ok(held.token.clone());
            }
        }

        let assertion = self.assertion()?;
        let response = self
            .http
            .post("https://oauth2.googleapis.com/token")
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
                ("assertion", &assertion),
            ])
            .send()
            .await
            .map_err(|e| format!("asking for an access token: {e}"))?;

        #[derive(serde::Deserialize)]
        struct Granted {
            access_token: String,
            expires_in: u64,
        }

        let granted: Granted = response
            .json()
            .await
            .map_err(|e| format!("reading the access token: {e}"))?;

        let token = granted.access_token.clone();
        *self.access.lock().expect("access") = Some(Access {
            token: granted.access_token,
            until: Instant::now() + Duration::from_secs(granted.expires_in),
        });
        Ok(token)
    }

    /// The signed claim that buys an access token.
    fn assertion(&self) -> Result<String, String> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| format!("the clock is before 1970: {e}"))?
            .as_secs();

        #[derive(serde::Serialize)]
        struct Claims<'a> {
            iss: &'a str,
            scope: &'a str,
            aud: &'a str,
            iat: u64,
            exp: u64,
        }

        let claims = Claims {
            iss: &self.account.client_email,
            scope: "https://www.googleapis.com/auth/firebase.messaging",
            aud: "https://oauth2.googleapis.com/token",
            iat: now,
            exp: now + 3600,
        };

        let key = jsonwebtoken::EncodingKey::from_rsa_pem(self.account.private_key.as_bytes())
            .map_err(|e| format!("the service account's private key: {e}"))?;
        jsonwebtoken::encode(
            &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256),
            &claims,
            &key,
        )
        .map_err(|e| format!("signing the assertion: {e}"))
    }

    /// The message body for a poke.
    ///
    /// Data-only and high priority. Data-only because a notification message
    /// would put something on the person's screen, and this is housekeeping
    /// they did not ask to be told about. High priority because a normal one
    /// is held until the phone next wakes on its own, which is the delay this
    /// exists to remove.
    fn body(token: &str) -> serde_json::Value {
        serde_json::json!({
            "message": {
                "token": token,
                "data": { "qurb": "sync" },
                "android": { "priority": "high" },
            }
        })
    }

    async fn poke(&self, token: WakeToken) -> Result<(), String> {
        let access = self.access_token().await?;
        let response = self
            .http
            .post(self.endpoint())
            .bearer_auth(access)
            .json(&Self::body(&token))
            .send()
            .await
            .map_err(|e| format!("sending: {e}"))?;

        match response.status().is_success() {
            true => Ok(()),
            false => Err(format!(
                "firebase refused it: {} {}",
                response.status(),
                response.text().await.unwrap_or_default()
            )),
        }
    }
}

impl Waker for Fcm {
    fn wake(&self, token: &WakeToken) {
        // Spawned rather than awaited. `wake` is called while the directory's
        // lock is in reach and a push is a network round trip; blocking here
        // would stall every other device's signalling behind Google's latency.
        let token = token.clone();
        let account = self.account.clone();
        let http = self.http.clone();
        let access = Arc::clone(&self.access);
        tokio::spawn(async move {
            let sender = Fcm { account, http, access };
            if let Err(e) = sender.poke(token).await {
                // A device that is not woken syncs on its own schedule, which
                // is what happened before any of this existed. Not worth
                // failing anything over.
                tracing::debug!(error = %e, "could not wake a device");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The message says nothing about what is being synced. This is the
    /// promise the whole feature is judged against.
    #[test]
    fn the_message_carries_no_information_about_anybody_files() {
        let body = Fcm::body("a-device-token").to_string();
        for leak in ["path", "file", "name", "size", "peer", "count", "bytes"] {
            assert!(!body.contains(leak), "the push body mentions {leak}: {body}");
        }
        assert!(body.contains("a-device-token"), "the body must name the device to wake");
    }

    /// Data-only and high priority, for the reasons in `body`'s comment: no
    /// notification on the person's screen, and not held until the phone
    /// happens to wake by itself.
    #[test]
    fn the_message_is_a_silent_high_priority_poke() {
        let body = Fcm::body("t");
        assert!(body["message"]["notification"].is_null(), "it would show a notification");
        assert_eq!(body["message"]["android"]["priority"], "high");
        assert!(body["message"]["data"].is_object());
    }
}
