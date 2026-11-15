//! Conversions between the API types (`model.rs`) and the entities
//! (`entity.rs`), as `From` implementations: requests become `ProductInput`,
//! and entities become responses. Routes call them with `.into()` and
//! `ProductResponse::from`.
//!
//! The conversions can't fail: a request that parsed is a complete
//! `ProductInput`, and the service validates it. A conversion that could fail
//! would implement `TryFrom` instead.
use super::entity::{
    Image, ProductEntity, ProductInput, VariantEntity, VariantInput, VersionEntity, VersionInput,
};
use super::model::{
    ImageResponse, ProductCreateRequest, ProductResponse, ProductUpdateRequest, ReviewResponse,
    VariantRequest, VariantResponse, VersionRequest, VersionResponse,
};
use crate::reviews::Review;

impl From<ProductCreateRequest> for ProductInput {
    fn from(request: ProductCreateRequest) -> Self {
        ProductInput {
            name: request.name,
            brand: request.brand,
            description: request.description,
            variants: request
                .variants
                .into_iter()
                .map(VariantInput::from)
                .collect(),
            versions: request
                .versions
                .into_iter()
                .map(VersionInput::from)
                .collect(),
        }
    }
}

impl From<ProductUpdateRequest> for ProductInput {
    fn from(request: ProductUpdateRequest) -> Self {
        ProductInput {
            name: request.name,
            brand: request.brand,
            description: request.description,
            variants: request
                .variants
                .into_iter()
                .map(VariantInput::from)
                .collect(),
            versions: request
                .versions
                .into_iter()
                .map(VersionInput::from)
                .collect(),
        }
    }
}

impl From<VariantRequest> for VariantInput {
    fn from(request: VariantRequest) -> Self {
        VariantInput {
            sku: request.sku,
            description: request.description,
            price_cents: request.price_cents,
            stock: request.stock,
        }
    }
}

impl From<VersionRequest> for VersionInput {
    fn from(request: VersionRequest) -> Self {
        VersionInput {
            number: request.number,
            notes: request.notes,
            released_on: request.released_on,
            variant_skus: request.variant_skus,
        }
    }
}

impl From<ProductEntity> for ProductResponse {
    fn from(product: ProductEntity) -> Self {
        ProductResponse {
            id: product.id,
            name: product.name,
            brand: product.brand,
            description: product.description,
            image: product.image.as_ref().map(ImageResponse::from),
            variants: product
                .variants
                .into_iter()
                .map(VariantResponse::from)
                .collect(),
            versions: product
                .versions
                .into_iter()
                .map(VersionResponse::from)
                .collect(),
            created_at: product.created_at,
            updated_at: product.updated_at,
        }
    }
}

impl From<VariantEntity> for VariantResponse {
    fn from(variant: VariantEntity) -> Self {
        VariantResponse {
            id: variant.id,
            sku: variant.sku,
            description: variant.description,
            price_cents: variant.price_cents,
            stock: variant.stock,
            image: variant.image.as_ref().map(ImageResponse::from),
        }
    }
}

impl From<VersionEntity> for VersionResponse {
    fn from(version: VersionEntity) -> Self {
        VersionResponse {
            id: version.id,
            number: version.number,
            notes: version.notes,
            released_on: version.released_on,
            variant_skus: version.variant_skus,
        }
    }
}

impl From<Review> for ReviewResponse {
    fn from(review: Review) -> Self {
        ReviewResponse {
            author: review.author,
            title: review.title,
            body: review.body,
        }
    }
}

/// Only the type and the size: the bytes stay out of the JSON.
impl From<&Image> for ImageResponse {
    fn from(image: &Image) -> Self {
        ImageResponse {
            content_type: image.content_type.content_type().to_string(),
            size: image.bytes.len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_response_describes_images_without_their_bytes() {
        let product = ProductEntity {
            id: 1,
            name: "A".into(),
            brand: "B".into(),
            description: String::new(),
            image: Image::from_bytes(b"\x89PNG\r\n\x1a\nrest".to_vec()),
            variants: Vec::new(),
            versions: Vec::new(),
            created_at: chrono::DateTime::UNIX_EPOCH,
            updated_at: chrono::DateTime::UNIX_EPOCH,
        };
        let json = serde_json::to_value(ProductResponse::from(product)).unwrap();
        assert_eq!(
            json["image"],
            serde_json::json!({"contentType": "image/png", "size": 12})
        );
    }

    #[test]
    fn a_create_request_becomes_a_new_product() {
        let request: ProductCreateRequest = serde_json::from_str(
            r#"{"name": "A", "brand": "B",
                "variants": [{"sku": "A-1", "priceCents": 999}],
                "versions": [{"number": 1, "releasedOn": "2026-01-31", "variantSkus": ["A-1"]}]}"#,
        )
        .unwrap();
        let input = ProductInput::from(request);
        assert_eq!(input.variants[0].price_cents, 999);
        assert_eq!(input.versions[0].released_on.to_string(), "2026-01-31");
    }
}
