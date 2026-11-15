//! The API's JSON: what clients send (requests) and what they get back
//! (responses). These types are the contract with clients, kept apart from
//! the entities in `entity.rs`, so the stored data can change without
//! breaking clients and the other way round. `mapper.rs` converts between
//! them.
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

/// The body of `POST /products`. Fields marked `default` may be left out;
/// an unknown field, such as a misspelled `pricecents`, is an error instead
/// of being ignored.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProductCreateRequest {
    pub name: String,
    pub brand: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub variants: Vec<VariantRequest>,
    #[serde(default)]
    pub versions: Vec<VersionRequest>,
}

/// The body of `PUT /products/{id}`: the product's new state. It has the
/// same fields as a create today, but it's its own type, so the two can
/// change independently (an update might later have to say which version
/// of the product the client last saw).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProductUpdateRequest {
    pub name: String,
    pub brand: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub variants: Vec<VariantRequest>,
    #[serde(default)]
    pub versions: Vec<VersionRequest>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VariantRequest {
    pub sku: String,
    #[serde(default)]
    pub description: String,
    pub price_cents: i64,
    /// Optional in JSON, so clients written before the change keep working
    #[serde(default)]
    pub stock: i32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VersionRequest {
    pub number: i32,
    #[serde(default)]
    pub notes: String,
    pub released_on: NaiveDate, // "2026-02-30" is rejected while parsing
    #[serde(default)]
    pub variant_skus: Vec<String>,
}

/// The query string of `GET /products?brand=stride&maxPrice=15000`. Every
/// field is optional, and `ProductFilter::default()` matches everything.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProductFilter {
    /// Part of the name, in any letter case
    pub name: Option<String>,
    /// The whole brand, in any letter case
    pub brand: Option<String>,
    /// At least one variant costs this much or more...
    pub min_price: Option<i64>,
    /// ...and this much or less
    pub max_price: Option<i64>,
}

/// A product as the API returns it. Field names are camelCase in JSON
/// (`priceCents`), as is usual for JSON APIs.
#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductResponse {
    pub id: i64,
    pub name: String,
    pub brand: String,
    pub description: String,
    pub image: Option<ImageResponse>,
    pub variants: Vec<VariantResponse>,
    pub versions: Vec<VersionResponse>,
    pub created_at: DateTime<Utc>, // "2026-11-15T11:43:52.217518Z" in JSON
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VariantResponse {
    pub id: i64,
    pub sku: String,
    pub description: String,
    pub price_cents: i64,
    pub stock: i32,
    pub image: Option<ImageResponse>,
}

#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionResponse {
    pub id: i64,
    pub number: i32,
    pub notes: String,
    pub released_on: NaiveDate, // "2026-03-01" in JSON
    pub variant_skus: Vec<String>,
}

/// One review in `GET /products/{id}/reviews`.
#[derive(Debug, PartialEq, Serialize)]
pub struct ReviewResponse {
    pub author: String,
    pub title: String,
    pub body: String,
}

/// An image is described, not included: the bytes have their own URL.
#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageResponse {
    pub content_type: String,
    pub size: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optional_fields_may_be_left_out() {
        let request: ProductCreateRequest =
            serde_json::from_str(r#"{"name": "A", "brand": "B"}"#).unwrap();
        assert_eq!(
            (request.description.as_str(), request.variants.len()),
            ("", 0)
        );
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let error = serde_json::from_str::<ProductUpdateRequest>(r#"{"name": "A", "brnad": "B"}"#)
            .unwrap_err();
        assert!(
            error.to_string().starts_with("unknown field `brnad`"),
            "{error}"
        );
    }
}
