//! `GET /system` through the whole app. The test runs in the same process as
//! the app, so the process fields must describe the test itself.
mod common;

use common::{app, call};
use sqlx::PgPool;

#[sqlx::test]
async fn describes_the_running_process(pool: PgPool) {
    let app = app(pool);
    let (status, body) = call(&app, "GET", "/system", None).await;
    assert_eq!(status, 200);
    assert_eq!(body["build"]["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(body["build"]["debugAssertions"], cfg!(debug_assertions));
    assert_eq!(body["process"]["pid"], std::process::id());
    let threads = body["resources"]["threads"]
        .as_u64()
        .expect("Linux has /proc");
    assert!(
        threads >= 2,
        "the test thread and at least one Tokio worker: {threads}"
    );

    // The names of the settings variables, never their values
    let variables = body["environment"]["variables"].as_array().unwrap();
    assert!(variables.iter().any(|name| name == "APP_DATABASE__URL"));
    assert!(!body.to_string().contains("catalog:catalog@"));
}
