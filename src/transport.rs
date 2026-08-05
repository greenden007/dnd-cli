use crate::config::{load_server_profile, ServerProfile};
use anyhow::Result;
use reqwest::{Client, Method, RequestBuilder, Response};
use std::time::Duration;
use tokio::time::sleep;

#[derive(Clone)]
pub struct ApiClient {
    http: Client,
    profile: ServerProfile,
}

impl ApiClient {
    pub fn new(profile: ServerProfile) -> Result<Self> {
        Ok(Self {
            http: Client::builder().timeout(Duration::from_secs(30)).build()?,
            profile,
        })
    }

    pub fn from_saved_profile() -> Result<Self> {
        Self::new(load_server_profile()?)
    }

    pub fn url(&self, path: &str) -> String {
        self.profile.url(path)
    }

    pub fn request(&self, method: Method, path: &str) -> RequestBuilder {
        self.http.request(method, self.url(path))
    }

    pub fn profile(&self) -> &ServerProfile {
        &self.profile
    }

    pub async fn send_json_with_retry(
        &self,
        method: Method,
        path: &str,
        bearer_token: Option<&str>,
        body: Option<&str>,
    ) -> Result<Response> {
        let max_attempts = 3;
        for attempt in 0..max_attempts {
            let mut request = self.request(method.clone(), path);
            if let Some(token) = bearer_token { request = request.bearer_auth(token); }
            if let Some(raw_body) = body {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(raw_body) {
                    request = request.json(&json);
                } else {
                    request = request.body(raw_body.to_string());
                }
            }

            match request.send().await {
                Ok(response) if should_retry(&method, response.status(), attempt, max_attempts) => {
                    sleep(Duration::from_millis(250 * (attempt as u64 + 1))).await;
                }
                Ok(response) => return Ok(response),
                Err(error) if is_retryable_method(&method) && attempt + 1 < max_attempts => {
                    sleep(Duration::from_millis(250 * (attempt as u64 + 1))).await;
                    let _ = error;
                }
                Err(error) => return Err(error.into()),
            }
        }
        unreachable!("retry loop must return a response or error")
    }
}

fn is_retryable_method(method: &Method) -> bool {
    matches!(method, &Method::GET | &Method::HEAD | &Method::PUT | &Method::DELETE)
}

fn should_retry(method: &Method, status: reqwest::StatusCode, attempt: usize, max_attempts: usize) -> bool {
    is_retryable_method(method)
        && (status.is_server_error() || status.as_u16() == 429)
        && attempt + 1 < max_attempts
}

#[cfg(test)]
mod tests {
    use super::{is_retryable_method, should_retry, ApiClient};
    use crate::config::ServerProfile;
    use reqwest::Method;

    #[test]
    fn resolves_relative_and_absolute_urls() {
        let client = ApiClient::new(ServerProfile::new("https://example.test/api").unwrap()).unwrap();
        assert_eq!(client.url("/health"), "https://example.test/api/health");
        assert_eq!(client.url("https://other.test/health"), "https://other.test/health");
    }

    #[test]
    fn retries_only_transient_failures_for_idempotent_methods() {
        for method in [Method::GET, Method::HEAD, Method::PUT, Method::DELETE] {
            assert!(is_retryable_method(&method));
            assert!(should_retry(&method, reqwest::StatusCode::SERVICE_UNAVAILABLE, 0, 3));
            assert!(should_retry(&method, reqwest::StatusCode::TOO_MANY_REQUESTS, 1, 3));
            assert!(!should_retry(&method, reqwest::StatusCode::OK, 0, 3));
            assert!(!should_retry(&method, reqwest::StatusCode::SERVICE_UNAVAILABLE, 2, 3));
        }
    }

    #[test]
    fn never_retries_non_idempotent_post() {
        assert!(!is_retryable_method(&Method::POST));
        assert!(!should_retry(&Method::POST, reqwest::StatusCode::SERVICE_UNAVAILABLE, 0, 3));
    }
}
