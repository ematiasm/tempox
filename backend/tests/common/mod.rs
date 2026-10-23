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
use sqlx::ConnectOptions;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
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

    /// A partial update. `PATCH` with `Some(body)`, because an empty body is itself
    /// a case worth measuring: v3's account and method routes refuse it with 400
    /// rather than returning a 200 that changed nothing.
    pub async fn patch(&self, uri: &str, body: serde_json::Value) -> TestResponse {
        self.send(Method::PATCH, uri, Some(body)).await
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

/// A spawned backend process, terminated when the guard is dropped.
pub struct SpawnedBackend {
    child: Child,
    base_url: String,
}

impl SpawnedBackend {
    /// The real socket address reported by the spawned backend.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

impl Drop for SpawnedBackend {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

/// Start the actual backend binary against the database owned by the test pool.
pub async fn spawn_backend(pool: &sqlx::PgPool) -> SpawnedBackend {
    let (current_database,): (String,) = sqlx::query_as("SELECT current_database()")
        .fetch_one(pool)
        .await
        .expect("read current database");
    let connect_options = pool.connect_options();
    let configured_database = connect_options
        .get_database()
        .expect("test pool connect options name their database");
    assert_eq!(
        configured_database, current_database,
        "the spawned backend URL must point at the test pool's database"
    );
    // PgConnectOptions implements ConnectOptions::to_url_lossy(), not Display;
    // its generated URL preserves database credentials and the database name.
    let database_url = connect_options.to_url_lossy().to_string();

    let mut child = Command::new(env!("CARGO_BIN_EXE_tempox"))
        .env("DATABASE_URL", database_url)
        .env("BACKEND_PORT", "0")
        .env("RUST_LOG", "info")
        .stdout(Stdio::piped())
        // Piped, not discarded: a boot failure (unreachable database, bad config) is
        // reported on stderr, and `null` would leave the panic below saying only
        // "timed out" with no cause. The line is drained once the address appears,
        // so a chatty stderr cannot deadlock the pipe.
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn backend binary");
    let stdout = child.stdout.take().expect("piped backend stdout");
    let mut stderr = child.stderr.take().expect("piped backend stderr");
    let mut lines = BufReader::new(stdout).lines();
    let mut collected = Vec::new();
    let readiness = tokio::time::timeout(Duration::from_secs(30), async {
        while let Some(line) = lines.next_line().await.expect("read backend stdout") {
            collected.push(line.clone());
            if let Some((_, address)) = line.split_once("backend listening on http://") {
                return format!(
                    "http://{}",
                    address
                        .split_whitespace()
                        .next()
                        .expect("backend log line contains its bound address")
                );
            }
        }
        // The child is gone, so its stderr is at EOF and this read returns at once.
        let mut err = String::new();
        let _ = tokio::io::AsyncReadExt::read_to_string(&mut stderr, &mut err).await;
        panic!(
            "backend exited before reporting its listening address\nstdout: {collected:#?}\nstderr: {err}"
        );
    })
    .await
    .unwrap_or_else(|_| {
        // The timeout path cannot await inside, so the exit cause is read here. A
        // boot that cannot reach its database says so on stderr, and that message is
        // the whole value of the test when it fails.
        panic!("timed out waiting for backend readiness; output: {collected:#?}")
    });

    SpawnedBackend {
        child,
        base_url: readiness,
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
