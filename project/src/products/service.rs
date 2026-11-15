//! The product use cases: everything the API can do with products. The
//! service validates, decides, and calls the repository. It works with
//! entities and knows nothing about HTTP or JSON.
//!
//! It also owns the transactions. A write is several repository calls (the
//! product, its variants, its versions), so the service begins a
//! transaction, passes it to each call, and commits at the end. If any call
//! fails, `?` returns early, the transaction is dropped without a commit,
//! and SQLx rolls it back.
mod repo; // private: only the service can use the repository

use sqlx::{PgConnection, PgPool};
use tracing::info;

use super::entity::{Image, ProductEntity, ProductInput};
use super::model::ProductFilter;
use super::validate::validate;
use crate::error::CatalogError;
use crate::reviews::{Review, ReviewsClient};

/// The handle the routes hold. `PgPool` is a pool of database connections:
/// each use borrows one and gives it back. Like the reviews client, it's an
/// `Arc` inside, so cloning the service is cheap.
#[derive(Clone)]
pub struct ProductService {
    pool: PgPool,
    reviews: ReviewsClient,
}

impl ProductService {
    pub fn new(pool: PgPool, reviews: ReviewsClient) -> ProductService {
        ProductService { pool, reviews }
    }

    pub async fn list(&self, filter: &ProductFilter) -> Result<Vec<ProductEntity>, CatalogError> {
        let mut conn = self.pool.acquire().await?;
        let ids = repo::find_ids(&mut conn, filter).await?;
        repo::load(&mut conn, &ids).await
    }

    pub async fn get(&self, id: i64) -> Result<ProductEntity, CatalogError> {
        let mut conn = self.pool.acquire().await?;
        load_one(&mut conn, id).await
    }

    pub async fn create(&self, input: ProductInput) -> Result<ProductEntity, CatalogError> {
        validate(&input)?;
        let mut tx = self.pool.begin().await?;
        let id = repo::insert_product(&mut tx, &input).await?;
        repo::save_variants(&mut tx, id, &input.variants).await?;
        repo::replace_versions(&mut tx, id, &input.versions).await?;
        let product = load_one(&mut tx, id).await?;
        tx.commit().await?;
        info!(id, name = %product.name, "product created");
        Ok(product)
    }

    /// Replaces the product. A variant whose SKU was there before keeps its
    /// row, so its id and image; versions are rebuilt.
    pub async fn update(
        &self,
        id: i64,
        input: ProductInput,
    ) -> Result<ProductEntity, CatalogError> {
        validate(&input)?;
        let mut tx = self.pool.begin().await?;
        if !repo::update_product(&mut tx, id, &input).await? {
            return Err(CatalogError::NotFound(id)); // dropping `tx` rolls it back
        }
        repo::save_variants(&mut tx, id, &input.variants).await?;
        repo::replace_versions(&mut tx, id, &input.versions).await?;
        let product = load_one(&mut tx, id).await?;
        tx.commit().await?;
        info!(id, variants = product.variants.len(), "product updated");
        Ok(product)
    }

    pub async fn delete(&self, id: i64) -> Result<(), CatalogError> {
        let mut conn = self.pool.acquire().await?;
        if !repo::delete_product(&mut conn, id).await? {
            return Err(CatalogError::NotFound(id));
        }
        info!(id, "product deleted");
        Ok(())
    }

    /// The product's reviews, from the reviews service. An unknown product
    /// is "not found" without a call to that service.
    pub async fn reviews(&self, id: i64) -> Result<Vec<Review>, CatalogError> {
        self.get(id).await?;
        Ok(self.reviews.for_product(id).await?)
    }

    pub async fn set_product_image(&self, id: i64, image: Image) -> Result<(), CatalogError> {
        let mut conn = self.pool.acquire().await?;
        if !repo::set_product_image(&mut conn, id, &image).await? {
            return Err(CatalogError::NotFound(id));
        }
        let (kind, size) = (image.content_type, image.bytes.len());
        info!(id, %kind, size, "product image stored");
        Ok(())
    }

    pub async fn product_image(&self, id: i64) -> Result<Image, CatalogError> {
        let mut conn = self.pool.acquire().await?;
        repo::product_image(&mut conn, id).await
    }

    pub async fn set_variant_image(
        &self,
        id: i64,
        sku: &str,
        image: Image,
    ) -> Result<(), CatalogError> {
        let mut conn = self.pool.acquire().await?;
        repo::set_variant_image(&mut conn, id, sku, &image).await?;
        let (kind, size) = (image.content_type, image.bytes.len());
        info!(id, sku, %kind, size, "variant image stored");
        Ok(())
    }

    pub async fn variant_image(&self, id: i64, sku: &str) -> Result<Image, CatalogError> {
        let mut conn = self.pool.acquire().await?;
        repo::variant_image(&mut conn, id, sku).await
    }
}

/// One product, on whatever connection the caller has: inside a write's
/// transaction, it sees that transaction's uncommitted rows.
async fn load_one(conn: &mut PgConnection, id: i64) -> Result<ProductEntity, CatalogError> {
    let mut products = repo::load(conn, &[id]).await?;
    products.pop().ok_or(CatalogError::NotFound(id))
}
