//! Automatic retries of requests that are safe to repeat.

use std::collections::hash_map::RandomState;
use std::error::Error as StdError;
use std::hash::{BuildHasher, Hasher};
use std::io;
use std::time::Duration;

use chrono::{DateTime, Utc};
use reqwest::StatusCode;
use reqwest::header::{HeaderMap, RETRY_AFTER};

/// When and how often failed requests are retried.
///
/// Only requests that are safe to repeat are retried: `GET` operations and
/// the token request. Operations with side effects (payments, Pix...) are
/// never repeated automatically.
///
/// A request is retried when the API answers `429 Too Many Requests`, `500`,
/// `502`, `503` or `504`, or when the connection cannot be established or
/// times out. Delays grow exponentially from
/// [`initial_delay`](Self::initial_delay), with random jitter, up to
/// [`max_delay`](Self::max_delay). A `Retry-After` header is honoured when it
/// does not exceed `max_delay`; when it does, the error is returned at once.
///
/// ```
/// use std::time::Duration;
/// use inter_pj::RetryPolicy;
///
/// let policy = RetryPolicy::new(5).initial_delay(Duration::from_millis(500));
/// assert_eq!(policy.max_attempts(), 5);
/// assert_eq!(RetryPolicy::disabled().max_attempts(), 1);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryPolicy {
    max_attempts: u32,
    initial_delay: Duration,
    max_delay: Duration,
}

impl RetryPolicy {
    /// Attempts made by default (the first one plus two retries).
    pub const DEFAULT_ATTEMPTS: u32 = 3;
    const DEFAULT_INITIAL_DELAY: Duration = Duration::from_secs(1);
    const DEFAULT_MAX_DELAY: Duration = Duration::from_secs(60);

    /// Makes up to `max_attempts` attempts in total (at least one).
    pub fn new(max_attempts: u32) -> Self {
        Self {
            max_attempts: max_attempts.max(1),
            initial_delay: Self::DEFAULT_INITIAL_DELAY,
            max_delay: Self::DEFAULT_MAX_DELAY,
        }
    }

    /// Never retries.
    pub fn disabled() -> Self {
        Self::new(1)
    }

    /// Delay before the first retry (default: 1 s); it doubles on each retry.
    #[must_use]
    pub fn initial_delay(mut self, delay: Duration) -> Self {
        self.initial_delay = delay;
        self
    }

    /// Longest delay between two attempts, including the one asked for by
    /// `Retry-After` (default: 60 s).
    #[must_use]
    pub fn max_delay(mut self, delay: Duration) -> Self {
        self.max_delay = delay;
        self
    }

    /// Total number of attempts, the first one included.
    pub fn max_attempts(&self) -> u32 {
        self.max_attempts
    }

    /// Delay before the attempt that follows failed attempt number `attempt`
    /// (1-based), or `None` when no further attempt should be made.
    ///
    /// `jitter` is a random number in `[0, 1)`: the exponential delay `d` is
    /// spread over `[d/2, d)` so that concurrent clients do not retry in step.
    pub(crate) fn delay(
        &self,
        attempt: u32,
        retry_after: Option<Duration>,
        jitter: f64,
    ) -> Option<Duration> {
        if attempt >= self.max_attempts {
            return None;
        }
        let exponential = self
            .initial_delay
            .saturating_mul(2u32.saturating_pow(attempt.saturating_sub(1)))
            .min(self.max_delay);
        let half = exponential / 2;
        let backoff = half + half.mul_f64(jitter.clamp(0.0, 1.0));
        match retry_after {
            Some(asked) if asked > self.max_delay => None,
            Some(asked) => Some(asked.max(backoff)),
            None => Some(backoff),
        }
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self::new(Self::DEFAULT_ATTEMPTS)
    }
}

/// Whether a request may be sent again after a transient failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RetryMode {
    /// Repeating the request has no further effect (`GET`, token request).
    Idempotent,
    /// The request changes server-side state (e.g. advances a scroll), so it
    /// is repeated only when it surely was not processed: rate limited
    /// (`429`) or the connection could not be established.
    WhenNotProcessed,
    /// Never repeated (payments and other operations with side effects).
    Never,
}

impl RetryMode {
    /// Whether a response with `status` should be retried.
    pub(crate) fn retries_status(self, status: StatusCode) -> bool {
        match self {
            Self::Idempotent => matches!(status.as_u16(), 429 | 500 | 502 | 503 | 504),
            Self::WhenNotProcessed => status == StatusCode::TOO_MANY_REQUESTS,
            Self::Never => false,
        }
    }

    /// Whether a transport error should be retried. TLS failures (rejected
    /// certificate, unknown CA...) never are, since repeating cannot help.
    pub(crate) fn retries_transport(self, err: &reqwest::Error) -> bool {
        let transient = match self {
            Self::Idempotent => err.is_connect() || err.is_timeout(),
            // A timeout may strike after the server processed the request.
            Self::WhenNotProcessed => err.is_connect(),
            Self::Never => false,
        };
        transient && !is_tls_failure(err)
    }
}

/// The TLS stack reports handshake failures as `io::ErrorKind::InvalidData`,
/// possibly wrapped in another `io::Error` (whose `source()` skips the inner
/// one, so wrapped errors are unwrapped with `get_ref`).
fn is_tls_failure(err: &(dyn StdError + 'static)) -> bool {
    let mut source = Some(err);
    while let Some(cause) = source {
        let mut io = cause.downcast_ref::<io::Error>();
        while let Some(error) = io {
            if error.kind() == io::ErrorKind::InvalidData {
                return true;
            }
            io = error
                .get_ref()
                .and_then(|inner| inner.downcast_ref::<io::Error>());
        }
        source = cause.source();
    }
    false
}

/// Parses `Retry-After`, given in seconds or as an HTTP date.
pub(crate) fn retry_after(headers: &HeaderMap, now: DateTime<Utc>) -> Option<Duration> {
    let value = headers.get(RETRY_AFTER)?.to_str().ok()?.trim();
    if let Ok(seconds) = value.parse::<f64>() {
        return (seconds.is_finite() && seconds >= 0.0)
            .then(|| Duration::try_from_secs_f64(seconds).ok())
            .flatten();
    }
    let date = DateTime::parse_from_rfc2822(value)
        .ok()?
        .with_timezone(&Utc);
    Some((date - now).to_std().unwrap_or(Duration::ZERO))
}

/// A random number in `[0, 1)`, good enough for jitter.
pub(crate) fn jitter() -> f64 {
    // Each `RandomState` is seeded with fresh random keys.
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u8(0);
    #[allow(clippy::cast_precision_loss)] // 53 bits fit exactly in an f64
    let fraction = (hasher.finish() >> 11) as f64 / (1u64 << 53) as f64;
    fraction
}

#[cfg(test)]
mod tests {
    use reqwest::header::HeaderValue;

    use super::*;

    const S: Duration = Duration::from_secs(1);

    #[test]
    fn delays_grow_exponentially_within_the_jitter_band() {
        let policy = RetryPolicy::new(10);
        for (attempt, full) in [(1, 1), (2, 2), (3, 4), (4, 8)] {
            let full = S * full;
            assert_eq!(policy.delay(attempt, None, 0.0), Some(full / 2));
            let high = policy.delay(attempt, None, 0.999).unwrap();
            assert!(high < full && high > full / 2, "{attempt}: {high:?}");
        }
    }

    #[test]
    fn delays_are_capped_by_max_delay() {
        let policy = RetryPolicy::new(50).max_delay(S * 10);
        assert_eq!(policy.delay(40, None, 0.0), Some(S * 5));
        assert!(policy.delay(40, None, 0.999).unwrap() < S * 10);
    }

    #[test]
    fn stops_after_max_attempts() {
        let policy = RetryPolicy::new(3);
        assert!(policy.delay(1, None, 0.5).is_some());
        assert!(policy.delay(2, None, 0.5).is_some());
        assert_eq!(policy.delay(3, None, 0.5), None);
        assert_eq!(RetryPolicy::disabled().delay(1, None, 0.5), None);
        assert_eq!(RetryPolicy::new(0).max_attempts(), 1);
    }

    #[test]
    fn honours_retry_after_up_to_max_delay() {
        let policy = RetryPolicy::new(3).max_delay(S * 30);
        assert_eq!(policy.delay(1, Some(S * 7), 0.5), Some(S * 7));
        // Never sooner than the backoff itself.
        assert_eq!(
            policy.delay(1, Some(Duration::ZERO), 0.0),
            Some(Duration::from_millis(500))
        );
        assert_eq!(policy.delay(1, Some(S * 31), 0.5), None);
    }

    #[test]
    fn retryable_statuses_depend_on_the_mode() {
        let status = |code| StatusCode::from_u16(code).unwrap();
        for code in [429, 500, 502, 503, 504] {
            assert!(RetryMode::Idempotent.retries_status(status(code)), "{code}");
            assert!(!RetryMode::Never.retries_status(status(code)), "{code}");
        }
        for code in [200, 400, 401, 403, 404, 409, 422, 501] {
            assert!(
                !RetryMode::Idempotent.retries_status(status(code)),
                "{code}"
            );
        }
        assert!(RetryMode::WhenNotProcessed.retries_status(status(429)));
        for code in [500, 502, 503, 504] {
            assert!(
                !RetryMode::WhenNotProcessed.retries_status(status(code)),
                "{code}"
            );
        }
    }

    #[test]
    fn parses_retry_after_seconds_and_dates() {
        let now = DateTime::parse_from_rfc3339("2026-01-02T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let header = |value: &str| {
            let mut headers = HeaderMap::new();
            headers.insert(RETRY_AFTER, HeaderValue::from_str(value).unwrap());
            retry_after(&headers, now)
        };
        assert_eq!(header("7"), Some(S * 7));
        assert_eq!(header(" 1.5 "), Some(Duration::from_millis(1500)));
        assert_eq!(header("Fri, 02 Jan 2026 10:00:30 GMT"), Some(S * 30));
        assert_eq!(
            header("Fri, 02 Jan 2026 09:00:00 GMT"),
            Some(Duration::ZERO)
        );
        assert_eq!(header("-3"), None);
        assert_eq!(header("amanhã"), None);
        assert_eq!(retry_after(&HeaderMap::new(), now), None);
    }

    #[test]
    fn tls_failures_are_recognised_in_the_error_chain() {
        #[derive(Debug)]
        struct Wrapper(io::Error);
        impl std::fmt::Display for Wrapper {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("falha de conexão")
            }
        }
        impl StdError for Wrapper {
            fn source(&self) -> Option<&(dyn StdError + 'static)> {
                Some(&self.0)
            }
        }
        let tls = Wrapper(io::Error::new(io::ErrorKind::InvalidData, "UnknownCA"));
        assert!(is_tls_failure(&tls));
        // How the HTTP stack actually reports it: an `Other` error wrapping
        // the `InvalidData` one.
        let wrapped = Wrapper(io::Error::other(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid peer certificate",
        )));
        assert!(is_tls_failure(&wrapped));
        let refused = Wrapper(io::Error::from(io::ErrorKind::ConnectionRefused));
        assert!(!is_tls_failure(&refused));
        let other = Wrapper(io::Error::other("conexão encerrada"));
        assert!(!is_tls_failure(&other));
    }

    #[test]
    fn jitter_is_a_fraction() {
        for _ in 0..1000 {
            let value = jitter();
            assert!((0.0..1.0).contains(&value), "{value}");
        }
    }
}
