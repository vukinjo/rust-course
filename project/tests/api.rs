//! API tests: requests go straight into the router with `oneshot`, so no
//! port is opened. `#[sqlx::test]` creates a fresh database for each test,
//! applies the migrations, passes in a pool, and drops the database when
//! the test passes, so the tests run in parallel without seeing each other.
mod common;

use axum::body::Body;
use axum::http::StatusCode;
use common::{PNG, app, call, send};
use serde_json::{Value, json};
use shoes::products::MAX_BODY_BYTES;
use sqlx::PgPool;
use uuid::Uuid;

fn trail_runner() -> Value {
    json!({
        "name": "Trail Runner X",
        "brand": "Stride",
        "variants": [
            {"sku": "TRX-BLK-42", "description": "Black, EU 42", "priceCents": 12999},
            {"sku": "TRX-VOLT-42", "description": "Volt, EU 42", "priceCents": 13999}
        ],
        "versions": [
            {"number": 1, "releasedOn": "2025-03-01", "variantSkus": ["TRX-BLK-42"]},
            {"number": 2, "releasedOn": "2026-03-01", "variantSkus": ["TRX-BLK-42", "TRX-VOLT-42"]}
        ]
    })
}

#[sqlx::test]
async fn create_then_get(pool: PgPool) {
    let app = app(pool);
    let (status, created) = call(&app, "POST", "/products", Some(trail_runner())).await;
    assert_eq!(status, StatusCode::CREATED);
    let id = created["id"].as_str().unwrap();
    assert_eq!(Uuid::parse_str(id).unwrap().get_version_num(), 7);
    assert_ne!(created["variants"][0]["id"], created["variants"][1]["id"]);
    assert_eq!(
        created["versions"][1]["variantSkus"],
        json!(["TRX-BLK-42", "TRX-VOLT-42"])
    );

    let (status, fetched) = call(&app, "GET", &format!("/products/{id}"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(fetched, created);
}

#[sqlx::test]
async fn errors_have_a_status_and_a_message(pool: PgPool) {
    let app = app(pool);
    let missing = Uuid::now_v7();
    let (status, body) = call(&app, "GET", &format!("/products/{missing}"), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(
        body,
        json!({"error": format!("product {missing} not found")})
    );
    let (status, _) = call(&app, "GET", "/products/7", None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST); // not a UUID

    let mut invalid = trail_runner();
    invalid["variants"][0]["priceCents"] = json!(0);
    let (status, body) = call(&app, "POST", "/products", Some(invalid)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        body["error"],
        "invalid product: the price of TRX-BLK-42 must be positive, got 0 cents"
    );

    call(&app, "POST", "/products", Some(trail_runner())).await;
    let (status, _) = call(&app, "POST", "/products", Some(trail_runner())).await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[sqlx::test]
async fn json_the_server_cant_use_is_rejected_by_axum(pool: PgPool) {
    let app = app(pool);
    let (status, _) = call(&app, "POST", "/products", Some(json!({"name": "X"}))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY); // valid JSON, missing `brand`
    let (status, _, _) = send(&app, "POST", "/products", Body::from("{not json")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[sqlx::test]
async fn list_filters_by_query_parameters(pool: PgPool) {
    let app = app(pool);
    call(&app, "POST", "/products", Some(trail_runner())).await;
    let walker = json!({"name": "City Walker", "brand": "Urbano",
                        "variants": [{"sku": "CW-41", "priceCents": 8999}]});
    call(&app, "POST", "/products", Some(walker)).await;

    let names = |body: Value| -> Vec<String> {
        body.as_array()
            .unwrap()
            .iter()
            .map(|p| p["name"].as_str().unwrap().to_string())
            .collect()
    };
    let (_, all) = call(&app, "GET", "/products", None).await;
    assert_eq!(names(all), ["Trail Runner X", "City Walker"]);
    let (_, cheap) = call(&app, "GET", "/products?maxPrice=10000", None).await;
    assert_eq!(names(cheap), ["City Walker"]);
    let (_, stride) = call(&app, "GET", "/products?brand=stride&name=runner", None).await;
    assert_eq!(names(stride), ["Trail Runner X"]);
    let (status, _) = call(&app, "GET", "/products?max_price=1", None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST); // unknown parameter
}

#[sqlx::test]
async fn update_replaces_the_product_and_keeps_known_variants(pool: PgPool) {
    let app = app(pool);
    let (_, created) = call(&app, "POST", "/products", Some(trail_runner())).await;
    let uri = format!("/products/{}", created["id"].as_str().unwrap());
    let volt_42 = &created["variants"][1]["id"];
    let mut changed = trail_runner();
    changed["variants"] = json!([
        {"sku": "TRX-VOLT-42", "description": "Volt, EU 42", "priceCents": 11999},
        {"sku": "TRX-VOLT-43", "description": "Volt, EU 43", "priceCents": 11999}
    ]);
    changed["versions"] = json!([]);
    let (status, updated) = call(&app, "PUT", &uri, Some(changed.clone())).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(&updated["variants"][0]["id"], volt_42); // TRX-VOLT-42 kept its id
    assert_eq!(updated["variants"][0]["priceCents"], 11999);
    assert_ne!(&updated["variants"][1]["id"], volt_42); // a new row
    assert_eq!(updated["versions"], json!([]));

    let missing = format!("/products/{}", Uuid::now_v7());
    let (status, _) = call(&app, "PUT", &missing, Some(changed)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn delete_answers_204_then_404(pool: PgPool) {
    let app = app(pool);
    let (_, created) = call(&app, "POST", "/products", Some(trail_runner())).await;
    let uri = format!("/products/{}", created["id"].as_str().unwrap());
    let (status, body) = call(&app, "DELETE", &uri, None).await;
    assert_eq!((status, body), (StatusCode::NO_CONTENT, Value::Null));
    let (status, _) = call(&app, "DELETE", &uri, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn images_upload_and_download_as_raw_bytes(pool: PgPool) {
    let app = app(pool);
    let (_, created) = call(&app, "POST", "/products", Some(trail_runner())).await;
    let product_uri = format!("/products/{}", created["id"].as_str().unwrap());
    let image_uri = format!("{product_uri}/image");
    let (status, _, _) = send(&app, "GET", &image_uri, Body::empty()).await;
    assert_eq!(status, StatusCode::NOT_FOUND); // nothing uploaded yet

    let (status, _, _) = send(&app, "PUT", &image_uri, Body::from(PNG)).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, content_type, bytes) = send(&app, "GET", &image_uri, Body::empty()).await;
    assert_eq!(
        (status, content_type.as_str(), bytes.as_slice()),
        (StatusCode::OK, "image/png", PNG)
    );
    let (_, product) = call(&app, "GET", &product_uri, None).await;
    assert_eq!(
        product["image"],
        json!({"contentType": "image/png", "size": PNG.len()})
    );

    let uri = format!("{product_uri}/variants/TRX-VOLT-42/image");
    let (status, _, _) = send(&app, "PUT", &uri, Body::from(PNG)).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, _, bytes) = send(&app, "GET", &uri, Body::empty()).await;
    assert_eq!(bytes, PNG);
}

#[sqlx::test]
async fn bad_uploads_are_rejected(pool: PgPool) {
    let app = app(pool);
    let (_, created) = call(&app, "POST", "/products", Some(trail_runner())).await;
    let product_uri = format!("/products/{}", created["id"].as_str().unwrap());
    let image_uri = format!("{product_uri}/image");
    let (status, _, _) = send(&app, "PUT", &image_uri, Body::from("hello")).await;
    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    let uri = format!("{product_uri}/variants/NOPE/image");
    let (status, _, _) = send(&app, "PUT", &uri, Body::from(PNG)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let too_big = vec![0u8; MAX_BODY_BYTES + 1];
    let (status, _, _) = send(&app, "PUT", &image_uri, Body::from(too_big)).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
}

#[sqlx::test]
async fn of_two_concurrent_creates_with_one_name_one_wins(pool: PgPool) {
    let app = app(pool);
    let (first, second) = tokio::join!(
        call(&app, "POST", "/products", Some(trail_runner())),
        call(&app, "POST", "/products", Some(trail_runner())),
    );
    let mut statuses = [first.0, second.0];
    statuses.sort();
    assert_eq!(statuses, [StatusCode::CREATED, StatusCode::CONFLICT]);
}

#[sqlx::test]
async fn a_sku_belongs_to_one_product(pool: PgPool) {
    let app = app(pool);
    call(&app, "POST", "/products", Some(trail_runner())).await;
    let thief = json!({"name": "Copy", "brand": "Other",
                       "variants": [{"sku": "TRX-BLK-42", "priceCents": 100}]});
    let (status, body) = call(&app, "POST", "/products", Some(thief)).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"], "SKU TRX-BLK-42 belongs to another product");
    // The transaction was rolled back: the product row isn't there either
    let (_, all) = call(&app, "GET", "/products", None).await;
    assert_eq!(all.as_array().unwrap().len(), 1);
}

#[sqlx::test]
async fn delete_cascades_to_variants_versions_and_links(pool: PgPool) {
    let app = app(pool.clone());
    let (_, created) = call(&app, "POST", "/products", Some(trail_runner())).await;
    let uri = format!("/products/{}", created["id"].as_str().unwrap());
    // sqlx 0.9 accepts only literal SQL (`&'static str`), a guard against
    // SQL injection, so each count is its own literal
    let count = |sql: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, i64>(sql)
                .fetch_one(&pool)
                .await
                .unwrap()
        }
    };
    let links = "SELECT count(*) FROM version_variants";
    assert_eq!(count(links).await, 3); // v1 offers 1 variant, v2 offers 2
    call(&app, "DELETE", &uri, None).await;
    for sql in [
        "SELECT count(*) FROM products",
        "SELECT count(*) FROM variants",
        "SELECT count(*) FROM versions",
        links,
    ] {
        assert_eq!(count(sql).await, 0, "{sql}");
    }
}

#[sqlx::test]
async fn timestamps_come_from_the_database(pool: PgPool) {
    let app = app(pool);
    let (_, created) = call(&app, "POST", "/products", Some(trail_runner())).await;
    assert_eq!(created["createdAt"], created["updatedAt"]);
    let uri = format!("/products/{}", created["id"].as_str().unwrap());
    let (_, updated) = call(&app, "PUT", &uri, Some(trail_runner())).await;
    assert_eq!(updated["createdAt"], created["createdAt"]);
    assert_ne!(updated["updatedAt"], created["updatedAt"]);
}

#[sqlx::test]
async fn an_update_keeps_the_image_of_a_kept_variant(pool: PgPool) {
    let app = app(pool);
    let (_, created) = call(&app, "POST", "/products", Some(trail_runner())).await;
    let product_uri = format!("/products/{}", created["id"].as_str().unwrap());
    let uri = format!("{product_uri}/variants/TRX-VOLT-42/image");
    send(&app, "PUT", &uri, Body::from(PNG)).await;
    let (status, _) = call(&app, "PUT", &product_uri, Some(trail_runner())).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _, bytes) = send(&app, "GET", &uri, Body::empty()).await;
    assert_eq!((status, bytes.as_slice()), (StatusCode::OK, PNG));
}

#[sqlx::test]
async fn stock_is_saved_defaults_to_zero_and_cant_be_negative(pool: PgPool) {
    let app = app(pool);
    let mut product = trail_runner();
    product["variants"][0]["stock"] = json!(12); // the second variant leaves it out
    let (status, created) = call(&app, "POST", "/products", Some(product.clone())).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["variants"][0]["stock"], 12);
    assert_eq!(created["variants"][1]["stock"], 0);

    product["variants"][0]["stock"] = json!(-1);
    let uri = format!("/products/{}", created["id"].as_str().unwrap());
    let (status, body) = call(&app, "PUT", &uri, Some(product)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        body["error"],
        "invalid product: the stock of TRX-BLK-42 can't be negative, got -1"
    );
}
