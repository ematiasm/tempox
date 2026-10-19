//! Test harness shared by the integration tests.
//!
//! Until F3 every HTTP test was two processes: the suite started a backend on
//! `:3099` (through `scripts/test-stack.sh`) and called it with `reqwest`, while
//! opening its own `PgPool` to check what happened. Nothing at runtime kept those
//! two pointed at the same database — they agreed only because the harness set both
//! from `TEST_DATABASE_URL`. That is a guard, not a property.
//!
//! [`client`] removes the second process. It builds the **real** router with
//! [`tempox::build_app`] over the pool `#[sqlx::test]` provides, and sends requests
//! through it in-process. What is exercised is the same surface as before — routing,
//! extractors, status codes, the JSON body — with nothing to desynchronize.
//!
//! What it does **not** cover, and what F4 is for: binding a socket, `serve()`'s
//! middleware, and migrations running at boot.

use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use http_body_util::BodyExt;
use tower::ServiceExt;

/// A response, drained into something a test can assert on.
#[derive(Debug)]
pub struct TestResponse {
    pub status: StatusCode,
    pub body: serde_json::Value,
    /// The raw text, for the few assertions that look at a message rather than a
    /// field.
    pub text: String,
}

impl TestResponse {
    #[track_caller]
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// The parsed body. Panics with the raw text when there is none, because a test
    /// asking for JSON from a non-JSON response wants to see what came back.
    #[track_caller]
    pub fn json(&self) -> &serde_json::Value {
        assert!(
            !self.body.is_null(),
            "expected a JSON body, got: {}",
            self.text
        );
        &self.body
    }

    #[track_caller]
    pub fn expect_status(&self, expected: StatusCode) -> &Self {
        assert_eq!(
            self.status, expected,
            "unexpected status; body was: {}",
            self.text
        );
        self
    }
}

/// A client bound to one app, in this process.
///
/// Every call clones the router, which is what `oneshot` requires: it consumes the
/// service. That costs an `Arc` bump, not a rebuild — `build_app` is called once,
/// in [`client`].
pub struct Client {
    app: axum::Router,
}

impl Client {
    pub async fn get(&self, uri: &str) -> TestResponse {
        self.send(Method::GET, uri, None).await
    }

    pub async fn post(&self, uri: &str, body: serde_json::Value) -> TestResponse {
        self.send(Method::POST, uri, Some(body)).await
    }

    // `delete` has no caller yet: no `v1` test deletes anything. The helper is
    // written once for all the domains, and the `v2`/`v1_5` slices will use it, so
    // removing it now would only mean re-adding it there. Allowed explicitly rather
    // than left as a warning nobody reads.
    #[allow(dead_code)]
    pub async fn delete(&self, uri: &str) -> TestResponse {
        self.send(Method::DELETE, uri, None).await
    }

    async fn send(
        &self,
        method: Method,
        uri: &str,
        body: Option<serde_json::Value>,
    ) -> TestResponse {
        let builder = Request::builder().method(method).uri(uri);

        let request = match body {
            Some(value) => builder
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(value.to_string()))
                .expect("build request with a JSON body"),
            None => builder.body(Body::empty()).expect("build request"),
        };

        let response = self
            .app
            .clone()
            .oneshot(request)
            .await
            .expect("the app is infallible: it has no middleware that returns an error");

        let status = response.status();
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("collect the body")
            .to_bytes();
        let text = String::from_utf8_lossy(&bytes).into_owned();
        // Not every response is JSON — a 204 has no body at all — so the parsed
        // value is `Null` rather than a panic.
        let body = serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);

        TestResponse { status, body, text }
    }
}

/// Build the app over the pool `#[sqlx::test]` hands a test, and return a client
/// for it.
///
/// The pool is moved in, not connected to: it is the same pool the test uses for its
/// own assertions, so "the handler wrote it" and "the test read it" are the same
/// database by construction, not by configuration.
pub fn client(pool: sqlx::PgPool) -> Client {
    let state = tempox::AppState {
        db: pool,
        // The shop's zone. Fixed rather than read from the environment, so a test's
        // result cannot depend on the machine it runs on — the same reasoning
        // `docs/en/testing.md` gives for `/api/day` not trusting the browser clock.
        shop_timezone: "America/Argentina/Buenos_Aires"
            .parse()
            .expect("a valid IANA zone"),
    };

    Client {
        app: tempox::build_app(state),
    }
}

/// A name unique to this run, for the artifacts a test creates.
///
/// Tests share a process but not a database, so this is about readability in a
/// failure message more than about collision. It is still run entropy rather than a
/// counter: two runs must not be distinguishable by nothing.
///
/// The result is safe to place in a query string unencoded. It is hex today, but the
/// guarantee belongs to the function rather than to the caller's knowledge of `uuid`:
/// a future prefix with `&` or `+` would silently split the query and the failure
/// would name the assertion, not the URL.
pub fn unique(prefix: &str) -> String {
    let suffix = uuid::Uuid::new_v4().simple();
    format!("{prefix}-{suffix}")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect()
}

/// Percent-encode a value for use in a query string.
///
/// Written out rather than added as a dependency: the rules needed here are the
/// unreserved set from RFC 3986, and a test helper should not pull a URL crate for
/// six lines.
pub fn encode_query(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}
