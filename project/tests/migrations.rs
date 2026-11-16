//! The schema change itself: a variant stored before migration 0002 must
//! read back with a stock of 0 after it.
use std::time::Duration;

use shoes::products::ProductService;
use shoes::reviews::ReviewsClient;
use sqlx::PgPool;
use sqlx::migrate::Migrator;
use uuid::Uuid;

/// The files in migrations/, embedded at compile time, like in main.rs.
static MIGRATOR: Migrator = sqlx::migrate!();

/// The last version before the change: 20261115000000_create_catalog.sql
const BEFORE_STOCK: i64 = 20261115000000;

// `migrations = false`: the test starts with an empty database and applies
// the migrations itself, in two steps.
#[sqlx::test(migrations = false)]
async fn existing_variants_get_a_stock_of_zero(pool: PgPool) {
    MIGRATOR.run_to(BEFORE_STOCK, &pool).await.unwrap(); // the old schema
    let product_id: Uuid = sqlx::query_scalar(
        "INSERT INTO products (name, brand) VALUES ('Old Shoe', 'Stride') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO variants (product_id, sku, price_cents) VALUES ($1, 'OLD-42', 5000)")
        .bind(product_id)
        .execute(&pool)
        .await
        .unwrap();

    MIGRATOR.run(&pool).await.unwrap(); // applies 0002 to a table with data
    let reviews = ReviewsClient::new("http://127.0.0.1:9", Duration::from_secs(5)).unwrap(); // not used
    let product = ProductService::new(pool, reviews)
        .get(product_id)
        .await
        .unwrap();
    assert_eq!(product.variants[0].sku, "OLD-42");
    assert_eq!(product.variants[0].stock, 0);
}

#[sqlx::test(migrations = false)]
async fn sqlx_records_every_applied_migration(pool: PgPool) {
    MIGRATOR.run(&pool).await.unwrap();
    let versions: Vec<i64> =
        sqlx::query_scalar("SELECT version FROM _sqlx_migrations WHERE success ORDER BY version")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(versions, [BEFORE_STOCK, 20261115120000]);
    MIGRATOR.run(&pool).await.unwrap(); // nothing left to do: running again is fine
}
