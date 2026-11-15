//! The repository: the only code that talks to the database. Each function
//! runs its SQL on the connection it's given, which may be a plain pooled
//! connection or an open transaction: the service decides, so it can run
//! several of these functions as one transaction. This is a private module
//! of `service`, so nothing else can call it.
//!
//! The queries use `sqlx::query!`, which checks the SQL and the Rust types
//! against the real schema at compile time.
use std::collections::HashMap;

use sqlx::PgConnection;

use crate::error::CatalogError;
use crate::products::entity::{
    Image, NewProduct, NewVariant, NewVersion, Product, Variant, Version,
};
use crate::products::model::ProductFilter;

// ------------------------------------------------------------------ reads

/// The ids of the products that match the filter, in id order. A NULL
/// parameter switches its condition off, so one query serves every
/// combination of filters.
pub async fn find_ids(
    conn: &mut PgConnection,
    filter: &ProductFilter,
) -> Result<Vec<i64>, CatalogError> {
    let ids = sqlx::query_scalar!(
        r#"
        SELECT p.id FROM products p
        WHERE ($1::text IS NULL OR strpos(lower(p.name), lower($1)) > 0)
          AND ($2::text IS NULL OR lower(p.brand) = lower($2))
          AND (($3::bigint IS NULL AND $4::bigint IS NULL) OR EXISTS (
                SELECT 1 FROM variants v
                WHERE v.product_id = p.id
                  AND ($3::bigint IS NULL OR v.price_cents >= $3)
                  AND ($4::bigint IS NULL OR v.price_cents <= $4)))
        ORDER BY p.id
        "#,
        filter.name,
        filter.brand,
        filter.min_price,
        filter.max_price,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(ids)
}

/// Fetches the products with these ids, with their variants and versions:
/// four queries in all, however many products there are, and the rows are
/// put together in Rust.
pub async fn load(conn: &mut PgConnection, ids: &[i64]) -> Result<Vec<Product>, CatalogError> {
    let products = sqlx::query!(
        "SELECT id, name, brand, description, image, image_type, created_at, updated_at
         FROM products WHERE id = ANY($1) ORDER BY id",
        ids,
    )
    .fetch_all(&mut *conn)
    .await?;
    let variant_rows = sqlx::query!(
        "SELECT id, product_id, sku, description, price_cents, stock, image, image_type
         FROM variants WHERE product_id = ANY($1) ORDER BY id",
        ids,
    )
    .fetch_all(&mut *conn)
    .await?;
    let version_rows = sqlx::query!(
        "SELECT id, product_id, number, notes, released_on
         FROM versions WHERE product_id = ANY($1) ORDER BY number",
        ids,
    )
    .fetch_all(&mut *conn)
    .await?;
    // The join turns variant ids into SKUs, which is how versions refer to variants
    let link_rows = sqlx::query!(
        "SELECT vv.version_id, v.sku
         FROM version_variants vv JOIN variants v ON v.id = vv.variant_id
         WHERE v.product_id = ANY($1) ORDER BY v.id",
        ids,
    )
    .fetch_all(&mut *conn)
    .await?;

    let mut skus: HashMap<i64, Vec<String>> = HashMap::new();
    for link in link_rows {
        skus.entry(link.version_id).or_default().push(link.sku);
    }
    let mut variants: HashMap<i64, Vec<Variant>> = HashMap::new();
    for row in variant_rows {
        variants.entry(row.product_id).or_default().push(Variant {
            id: row.id,
            sku: row.sku,
            description: row.description,
            price_cents: row.price_cents,
            stock: row.stock,
            image: image(row.image, row.image_type),
        });
    }
    let mut versions: HashMap<i64, Vec<Version>> = HashMap::new();
    for row in version_rows {
        versions.entry(row.product_id).or_default().push(Version {
            id: row.id,
            number: row.number,
            notes: row.notes,
            released_on: row.released_on,
            variant_skus: skus.remove(&row.id).unwrap_or_default(),
        });
    }
    Ok(products
        .into_iter()
        .map(|row| Product {
            id: row.id,
            name: row.name,
            brand: row.brand,
            description: row.description,
            image: image(row.image, row.image_type),
            variants: variants.remove(&row.id).unwrap_or_default(),
            versions: versions.remove(&row.id).unwrap_or_default(),
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
        .collect())
}

pub async fn product_image(conn: &mut PgConnection, id: i64) -> Result<Image, CatalogError> {
    let row = sqlx::query!("SELECT image, image_type FROM products WHERE id = $1", id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(CatalogError::NotFound(id))?;
    image(row.image, row.image_type).ok_or(CatalogError::NoImage)
}

pub async fn variant_image(
    conn: &mut PgConnection,
    id: i64,
    sku: &str,
) -> Result<Image, CatalogError> {
    let row = sqlx::query!(
        "SELECT image, image_type FROM variants WHERE product_id = $1 AND sku = $2",
        id,
        sku,
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or_else(|| variant_not_found(id, sku))?;
    image(row.image, row.image_type).ok_or(CatalogError::NoImage)
}

// ------------------------------------------------------------------ writes

/// Inserts the product row and returns its id. A taken name violates the
/// UNIQUE constraint, which becomes `DuplicateName`.
pub async fn insert_product(
    conn: &mut PgConnection,
    new: &NewProduct,
) -> Result<i64, CatalogError> {
    sqlx::query_scalar!(
        "INSERT INTO products (name, brand, description) VALUES ($1, $2, $3) RETURNING id",
        new.name,
        new.brand,
        new.description,
    )
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| conflict(e, CatalogError::DuplicateName(new.name.clone())))
}

/// Updates the product row. False if there's no product with this id.
pub async fn update_product(
    conn: &mut PgConnection,
    id: i64,
    new: &NewProduct,
) -> Result<bool, CatalogError> {
    let updated = sqlx::query!(
        "UPDATE products SET name = $2, brand = $3, description = $4, updated_at = now()
         WHERE id = $1",
        id,
        new.name,
        new.brand,
        new.description,
    )
    .execute(&mut *conn)
    .await
    .map_err(|e| conflict(e, CatalogError::DuplicateName(new.name.clone())))?;
    Ok(updated.rows_affected() == 1)
}

/// Deletes the product. The foreign keys' ON DELETE CASCADE removes its
/// variants, versions and version_variants rows. False if there was none.
pub async fn delete_product(conn: &mut PgConnection, id: i64) -> Result<bool, CatalogError> {
    let deleted = sqlx::query!("DELETE FROM products WHERE id = $1", id)
        .execute(&mut *conn)
        .await?;
    Ok(deleted.rows_affected() == 1)
}

/// Makes the product's variants match `variants`:
/// 1. delete variants whose SKU isn't in the list any more;
/// 2. insert or update the others ("upsert"), keyed by SKU, so a kept
///    variant keeps its row: its id and its image.
pub async fn save_variants(
    conn: &mut PgConnection,
    id: i64,
    variants: &[NewVariant],
) -> Result<(), CatalogError> {
    let skus: Vec<String> = variants.iter().map(|v| v.sku.clone()).collect();
    sqlx::query!(
        "DELETE FROM variants WHERE product_id = $1 AND sku <> ALL($2)",
        id,
        &skus,
    )
    .execute(&mut *conn)
    .await?;

    for variant in variants {
        // If the SKU exists, update it, but only if it belongs to this
        // product. Otherwise no row is returned: the SKU is someone else's.
        // (Each attempt takes a number from the id sequence, even when it ends
        // up updating, so ids can have gaps.)
        let saved = sqlx::query_scalar!(
            "INSERT INTO variants (product_id, sku, description, price_cents, stock)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (sku) DO UPDATE
                SET description = EXCLUDED.description, price_cents = EXCLUDED.price_cents,
                    stock = EXCLUDED.stock
                WHERE variants.product_id = EXCLUDED.product_id
             RETURNING id",
            id,
            variant.sku,
            variant.description,
            variant.price_cents,
            variant.stock,
        )
        .fetch_optional(&mut *conn)
        .await?;
        if saved.is_none() {
            return Err(CatalogError::DuplicateSku(variant.sku.clone()));
        }
    }
    Ok(())
}

/// Replaces all of the product's versions, and links each one to its
/// variants: the many-to-many relation, one row per (version, variant).
pub async fn replace_versions(
    conn: &mut PgConnection,
    id: i64,
    versions: &[NewVersion],
) -> Result<(), CatalogError> {
    // Deleting the versions also deletes their links (ON DELETE CASCADE)
    sqlx::query!("DELETE FROM versions WHERE product_id = $1", id)
        .execute(&mut *conn)
        .await?;
    for version in versions {
        let version_id = sqlx::query_scalar!(
            "INSERT INTO versions (product_id, number, notes, released_on)
             VALUES ($1, $2, $3, $4) RETURNING id",
            id,
            version.number,
            version.notes,
            version.released_on,
        )
        .fetch_one(&mut *conn)
        .await?;
        sqlx::query!(
            "INSERT INTO version_variants (version_id, variant_id)
             SELECT $1, v.id FROM variants v WHERE v.product_id = $2 AND v.sku = ANY($3)",
            version_id,
            id,
            &version.variant_skus,
        )
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// Stores or replaces the product's image. False if there's no such product.
pub async fn set_product_image(
    conn: &mut PgConnection,
    id: i64,
    image: &Image,
) -> Result<bool, CatalogError> {
    let updated = sqlx::query!(
        "UPDATE products SET image = $2, image_type = $3, updated_at = now() WHERE id = $1",
        id,
        image.bytes,
        image.content_type.content_type(),
    )
    .execute(&mut *conn)
    .await?;
    Ok(updated.rows_affected() == 1)
}

pub async fn set_variant_image(
    conn: &mut PgConnection,
    id: i64,
    sku: &str,
    image: &Image,
) -> Result<(), CatalogError> {
    let updated = sqlx::query!(
        "UPDATE variants SET image = $3, image_type = $4 WHERE product_id = $1 AND sku = $2",
        id,
        sku,
        image.bytes,
        image.content_type.content_type(),
    )
    .execute(&mut *conn)
    .await?;
    if updated.rows_affected() == 0 {
        return Err(variant_not_found(id, sku));
    }
    Ok(())
}

// ------------------------------------------------------------------ helpers

fn variant_not_found(id: i64, sku: &str) -> CatalogError {
    CatalogError::VariantNotFound {
        id,
        sku: sku.to_string(),
    }
}

/// Two nullable columns back to an `Image`. The table's CHECK constraints
/// allow only both-or-neither and known types, so `None` means "no image".
fn image(bytes: Option<Vec<u8>>, kind: Option<String>) -> Option<Image> {
    Some(Image {
        content_type: kind?.parse().ok()?,
        bytes: bytes?,
    })
}

/// A unique-constraint violation becomes `duplicate`; any other database
/// error stays a database error.
fn conflict(error: sqlx::Error, duplicate: CatalogError) -> CatalogError {
    match &error {
        sqlx::Error::Database(e) if e.is_unique_violation() => duplicate,
        _ => CatalogError::Database(error),
    }
}
