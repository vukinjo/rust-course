//! The HTTP layer for products: the router, and one handler per endpoint.
//! A handler takes what the extractors parsed from the request, converts
//! request types into entities with the mapper, calls the service, and
//! converts the entities it gets back into responses.
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use uuid::Uuid;

use super::entity::Image;
use super::model::{
    ProductCreateRequest, ProductFilter, ProductResponse, ProductUpdateRequest, ReviewResponse,
};
use super::service::ProductService;
use crate::error::CatalogError;

/// The largest request body accepted, which in practice means the largest
/// image. Bigger bodies get 413 Payload Too Large.
pub const MAX_BODY_BYTES: usize = 1024 * 1024;

/// The product routes, with the service as their state. axum clones the
/// service for each request, which only clones an `Arc`.
pub fn router(service: ProductService) -> Router {
    Router::new()
        .route("/products", get(list_products).post(create_product))
        .route(
            "/products/{id}",
            get(get_product).put(update_product).delete(delete_product),
        )
        .route("/products/{id}/reviews", get(product_reviews))
        .route(
            "/products/{id}/image",
            get(get_product_image).put(put_product_image),
        )
        .route(
            "/products/{id}/variants/{sku}/image",
            get(get_variant_image).put(put_variant_image),
        )
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .with_state(service)
}

/// GET /products?name=&brand=&minPrice=&maxPrice=
async fn list_products(
    State(service): State<ProductService>,
    Query(filter): Query<ProductFilter>,
) -> Result<Json<Vec<ProductResponse>>, CatalogError> {
    let products = service.list(&filter).await?;
    Ok(Json(
        products.into_iter().map(ProductResponse::from).collect(),
    ))
}

/// GET /products/{id}
async fn get_product(
    State(service): State<ProductService>,
    Path(id): Path<Uuid>,
) -> Result<Json<ProductResponse>, CatalogError> {
    let product = service.get(id).await?;
    Ok(Json(product.into()))
}

/// POST /products: 201 Created and the product with its ids.
async fn create_product(
    State(service): State<ProductService>,
    Json(request): Json<ProductCreateRequest>,
) -> Result<(StatusCode, Json<ProductResponse>), CatalogError> {
    let product = service.create(request.into()).await?;
    Ok((StatusCode::CREATED, Json(product.into())))
}

/// PUT /products/{id} replaces the product, its variants and its versions.
async fn update_product(
    State(service): State<ProductService>,
    Path(id): Path<Uuid>,
    Json(request): Json<ProductUpdateRequest>,
) -> Result<Json<ProductResponse>, CatalogError> {
    let product = service.update(id, request.into()).await?;
    Ok(Json(product.into()))
}

/// DELETE /products/{id}: 204 No Content.
async fn delete_product(
    State(service): State<ProductService>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, CatalogError> {
    service.delete(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// GET /products/{id}/reviews: 502 Bad Gateway when the reviews service fails.
async fn product_reviews(
    State(service): State<ProductService>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<ReviewResponse>>, CatalogError> {
    let reviews = service.reviews(id).await?;
    Ok(Json(
        reviews.into_iter().map(ReviewResponse::from).collect(),
    ))
}

/// PUT /products/{id}/image with the file as the raw body. The bytes
/// decide the type; the request's Content-Type header is not trusted.
async fn put_product_image(
    State(service): State<ProductService>,
    Path(id): Path<Uuid>,
    body: Bytes,
) -> Result<StatusCode, CatalogError> {
    let image = Image::from_bytes(body.to_vec()).ok_or(CatalogError::NotAnImage)?;
    service.set_product_image(id, image).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// GET /products/{id}/image: the bytes, with the stored Content-Type.
async fn get_product_image(
    State(service): State<ProductService>,
    Path(id): Path<Uuid>,
) -> Result<Response, CatalogError> {
    let image = service.product_image(id).await?;
    Ok(image_response(image))
}

/// PUT /products/{id}/variants/{sku}/image
async fn put_variant_image(
    State(service): State<ProductService>,
    Path((id, sku)): Path<(Uuid, String)>,
    body: Bytes,
) -> Result<StatusCode, CatalogError> {
    let image = Image::from_bytes(body.to_vec()).ok_or(CatalogError::NotAnImage)?;
    service.set_variant_image(id, &sku, image).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// GET /products/{id}/variants/{sku}/image
async fn get_variant_image(
    State(service): State<ProductService>,
    Path((id, sku)): Path<(Uuid, String)>,
) -> Result<Response, CatalogError> {
    let image = service.variant_image(id, &sku).await?;
    Ok(image_response(image))
}

fn image_response(image: Image) -> Response {
    let content_type = [(header::CONTENT_TYPE, image.content_type.content_type())];
    (content_type, image.bytes).into_response()
}
