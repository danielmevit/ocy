//! The shared PostHog capture transport, extracted from `usage_stats` so the crash-report
//! sender (CR2a) posts through the exact same per-platform TLS stack, timeouts, endpoint
//! and key handling. `usage_stats` re-exports everything, so its public API is unchanged.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use serde::Serialize;

/// The PostHog project API key injected at build time. `None` disables the feature.
pub const BUILD_KEY: Option<&str> = option_env!("OCY_POSTHOG_KEY");

/// Optional build-time override of the capture endpoint. Defaults to [`DEFAULT_ENDPOINT`].
pub const BUILD_HOST: Option<&str> = option_env!("OCY_POSTHOG_HOST");

/// The default capture endpoint (PostHog EU cloud).
pub const DEFAULT_ENDPOINT: &str = "https://eu.i.posthog.com";

/// Delay from app start to the first send attempt.
pub const INITIAL_DELAY: Duration = Duration::from_secs(60);

/// Delay between send attempts while the app runs.
pub const RETRY_DELAY: Duration = Duration::from_secs(60 * 60);

/// Timeout for the whole HTTP request.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// Debug/test-only endpoint override (`OCY_STATS_ENDPOINT`); release builds ignore it
/// (`debug_assertions` is off, and this constant does not exist there). Tests pass
/// endpoints through [`SenderConfig`] instead, so parallel tests never race on one
/// environment variable.
#[cfg(debug_assertions)]
const ENDPOINT_OVERRIDE_VAR: &str = "OCY_STATS_ENDPOINT";

/// Returns the build-time PostHog project API key, or `None` when the build has none.
#[must_use]
pub fn build_key() -> Option<&'static str> {
    BUILD_KEY.filter(|key| !key.is_empty())
}

/// Returns the capture endpoint base URL for this build: the debug-only override when set,
/// else the build host, else [`DEFAULT_ENDPOINT`]. Host and key are kept in sync: without a
/// key nothing ever connects anyway.
#[must_use]
pub fn build_endpoint() -> String {
    #[cfg(debug_assertions)]
    if let Some(overridden) = std::env::var(ENDPOINT_OVERRIDE_VAR)
        .ok()
        .filter(|value| !value.trim().is_empty())
    {
        return overridden.trim().trim_end_matches('/').to_owned();
    }
    endpoint_from_host(build_key().is_some(), BUILD_HOST)
}

/// Pure core of [`build_endpoint`]: a build host that is empty or only whitespace counts as
/// unset (an empty `OCY_POSTHOG_HOST=""` in CI must not produce a `"/batch/"` URL), and a
/// trailing slash is stripped.
fn endpoint_from_host(has_key: bool, build_host: Option<&str>) -> String {
    let host = if has_key {
        build_host
            .map(str::trim)
            .filter(|host| !host.is_empty())
            .unwrap_or(DEFAULT_ENDPOINT)
    } else {
        DEFAULT_ENDPOINT
    };
    host.trim_end_matches('/').to_owned()
}

/// HTTP-client configuration for a batch sender.
#[derive(Debug, Clone)]
pub struct SenderConfig {
    /// PostHog project API key ("phc_..." shape). `None` disables the feature.
    pub api_key: Option<String>,
    /// Capture endpoint base, for example `https://eu.i.posthog.com`.
    pub endpoint: String,
    pub request_timeout: Duration,
    pub initial_delay: Duration,
    pub retry_delay: Duration,
}

impl SenderConfig {
    /// Test-friendly config: explicit key and endpoint, fast cadence, short timeouts.
    #[must_use]
    pub fn for_tests(endpoint: String) -> Self {
        Self {
            api_key: Some("phc_test".to_owned()),
            endpoint,
            request_timeout: Duration::from_secs(2),
            initial_delay: Duration::from_millis(200),
            retry_delay: Duration::from_millis(200),
        }
    }
}

impl Default for SenderConfig {
    fn default() -> Self {
        Self {
            api_key: build_key().map(str::to_owned),
            endpoint: build_endpoint(),
            request_timeout: REQUEST_TIMEOUT,
            initial_delay: INITIAL_DELAY,
            retry_delay: RETRY_DELAY,
        }
    }
}

/// PostHog batch capture body: `{"api_key": ..., "batch": [...]}`.
#[derive(Debug, Serialize)]
pub struct Batch<'a> {
    pub api_key: &'a str,
    pub batch: &'a [serde_json::Value],
}

/// Result of one batch POST.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmitOutcome {
    /// 2xx: the server accepted the batch.
    Accepted,
    /// 4xx: rejected; stop sending for this run.
    RejectedClient,
    /// Network error or 5xx: retry later.
    RetryLater,
}

/// The TLS backend, chosen with `cfg` at the single place the agent is built so it always
/// matches the per-platform `ureq` features in `Cargo.toml`:
/// - **Linux:** rustls (ring) with bundled webpki roots — pure Rust, so the binary does
///   not link `libssl.so.3`/`libcrypto.so.3` (R4 finding 4).
/// - **Windows/macOS:** the OS-native stack (SChannel / Security.framework) via the
///   `NativeTls` provider with the platform's own verifier and roots.
///
/// Certificate and hostname verification stay on everywhere. The provider must match the
/// compiled features exactly: ureq's default (`Rustls` + `WebPki`) panics at the first
/// `https://` request when the `rustls` feature is off, and `NativeTls` without
/// `native-tls-webpki-roots` panics unless the roots come from the platform verifier
/// (R4 finding 1).
fn tls_config() -> ureq::tls::TlsConfig {
    let builder = ureq::tls::TlsConfig::builder();
    if cfg!(target_os = "linux") {
        builder
            .provider(ureq::tls::TlsProvider::Rustls)
            .root_certs(ureq::tls::RootCerts::WebPki)
            .build()
    } else {
        builder
            .provider(ureq::tls::TlsProvider::NativeTls)
            .root_certs(ureq::tls::RootCerts::PlatformVerifier)
            .build()
    }
}

/// POSTs one batch to `{endpoint}/batch/` and classifies the outcome: 2xx accepted, 4xx
/// rejected (stop for this run), anything else (network error, 5xx) retry later.
pub fn post_batch(config: &SenderConfig, batch: &Batch<'_>) -> SubmitOutcome {
    let agent_config = ureq::config::Config::builder()
        .tls_config(tls_config())
        .timeout_global(Some(config.request_timeout))
        .build();
    let agent = ureq::Agent::new_with_config(agent_config);
    let url = format!("{}/batch/", config.endpoint.trim_end_matches('/'));
    match agent.post(&url).send_json(batch) {
        Ok(_) => SubmitOutcome::Accepted,
        Err(ureq::Error::StatusCode(code)) if (400..500).contains(&code) => {
            SubmitOutcome::RejectedClient
        }
        Err(_) => SubmitOutcome::RetryLater,
    }
}

/// Handle for a background sender thread. Dropping it signals the thread to stop.
#[derive(Debug)]
pub struct SenderHandle {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl SenderHandle {
    /// Assembles a handle from its parts (the sender loops build the thread themselves).
    pub(crate) fn new(stop: Arc<AtomicBool>, thread: Option<JoinHandle<()>>) -> Self {
        Self { stop, thread }
    }

    /// Wakes the sender thread early (used when work just became ready, so it does not
    /// wait for the next hourly slot).
    pub fn wake(&self) {
        if let Some(thread) = &self.thread {
            thread.thread().unpark();
        }
    }

    /// Signals the thread to stop (without joining; the app never blocks on exit).
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
        self.wake();
    }
}

impl Drop for SenderHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_core_treats_an_empty_host_as_unset() {
        assert_eq!(endpoint_from_host(true, None), DEFAULT_ENDPOINT);
        assert_eq!(endpoint_from_host(true, Some("")), DEFAULT_ENDPOINT);
        assert_eq!(endpoint_from_host(true, Some("   ")), DEFAULT_ENDPOINT);
        assert_eq!(
            endpoint_from_host(true, Some("https://eu.i.posthog.com")),
            "https://eu.i.posthog.com"
        );
        assert_eq!(
            endpoint_from_host(true, Some("  https://eu.i.posthog.com/  ")),
            "https://eu.i.posthog.com"
        );
        // Without a key the endpoint stays at the default: nothing ever connects anyway.
        assert_eq!(
            endpoint_from_host(false, Some("https://example.com")),
            DEFAULT_ENDPOINT
        );
    }
}
