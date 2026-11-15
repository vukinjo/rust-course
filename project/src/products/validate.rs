//! The rules a product must follow, checked before it is stored.
use std::collections::HashSet;
use std::error::Error;
use std::fmt;
use std::hash::Hash;

use super::entity::NewProduct;

/// A product that breaks one of the rules. Each variant says which rule,
/// and carries the data needed to explain it.
#[derive(Debug, PartialEq)]
pub enum ValidationError {
    EmptyName,
    EmptyBrand,
    EmptySku,
    DuplicateSku(String),
    PriceNotPositive { sku: String, cents: i64 },
    NegativeStock { sku: String, stock: i32 },
    VersionNumberNotPositive(i32),
    DuplicateVersion(i32),
    UnknownSku { version: i32, sku: String },
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValidationError::EmptyName => write!(f, "the name is empty"),
            ValidationError::EmptyBrand => write!(f, "the brand is empty"),
            ValidationError::EmptySku => write!(f, "a variant has an empty SKU"),
            ValidationError::DuplicateSku(sku) => write!(f, "SKU {sku} appears twice"),
            ValidationError::PriceNotPositive { sku, cents } => {
                write!(f, "the price of {sku} must be positive, got {cents} cents")
            }
            ValidationError::NegativeStock { sku, stock } => {
                write!(f, "the stock of {sku} can't be negative, got {stock}")
            }
            ValidationError::VersionNumberNotPositive(n) => {
                write!(f, "version numbers start at 1, got {n}")
            }
            ValidationError::DuplicateVersion(n) => write!(f, "version {n} appears twice"),
            ValidationError::UnknownSku { version, sku } => {
                write!(f, "version {version} lists {sku}, which isn't a variant")
            }
        }
    }
}

impl Error for ValidationError {}

/// The first value that occurs twice, if any. `HashSet::insert` returns
/// false for a value it already holds, so this is one pass, O(n), where
/// comparing every pair was O(n²). It takes any iterator, so callers don't
/// need to collect into a Vec first. `Copy` lets the set keep one copy of
/// each item while `find` returns another: `&str` and `i32` are both `Copy`.
pub fn find_duplicate<T: Eq + Hash + Copy>(items: impl IntoIterator<Item = T>) -> Option<T> {
    let mut seen = HashSet::new();
    items.into_iter().find(|&item| !seen.insert(item))
}

/// Checks the rules that need only the product itself.
pub fn validate(new: &NewProduct) -> Result<(), ValidationError> {
    if new.name.trim().is_empty() {
        return Err(ValidationError::EmptyName);
    }
    if new.brand.trim().is_empty() {
        return Err(ValidationError::EmptyBrand);
    }
    if new.variants.iter().any(|v| v.sku.trim().is_empty()) {
        return Err(ValidationError::EmptySku);
    }
    if let Some(v) = new.variants.iter().find(|v| v.price_cents <= 0) {
        return Err(ValidationError::PriceNotPositive {
            sku: v.sku.clone(),
            cents: v.price_cents,
        });
    }
    if let Some(v) = new.variants.iter().find(|v| v.stock < 0) {
        return Err(ValidationError::NegativeStock {
            sku: v.sku.clone(),
            stock: v.stock,
        });
    }
    let skus = new.variants.iter().map(|v| v.sku.as_str());
    if let Some(sku) = find_duplicate(skus.clone()) {
        return Err(ValidationError::DuplicateSku(sku.to_string()));
    }
    let skus: HashSet<&str> = skus.collect();
    if let Some(v) = new.versions.iter().find(|v| v.number <= 0) {
        return Err(ValidationError::VersionNumberNotPositive(v.number));
    }
    for version in &new.versions {
        if let Some(sku) = version
            .variant_skus
            .iter()
            .find(|s| !skus.contains(&s.as_str()))
        {
            return Err(ValidationError::UnknownSku {
                version: version.number,
                sku: sku.clone(),
            });
        }
    }
    if let Some(number) = find_duplicate(new.versions.iter().map(|v| v.number)) {
        return Err(ValidationError::DuplicateVersion(number));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::products::entity::{NewVariant, NewVersion};

    /// A valid product with two variants and two versions.
    fn valid() -> NewProduct {
        let variant = |sku: &str| NewVariant {
            sku: sku.to_string(),
            description: String::new(),
            price_cents: 1000,
            stock: 5,
        };
        let version = |number, skus: &[&str]| NewVersion {
            number,
            notes: String::new(),
            released_on: "2026-01-01".parse().unwrap(),
            variant_skus: skus.iter().map(|s| s.to_string()).collect(),
        };
        NewProduct {
            name: "Shoe".to_string(),
            brand: "Brand".to_string(),
            description: String::new(),
            variants: vec![variant("A"), variant("B")],
            versions: vec![version(1, &["A"]), version(2, &["A", "B"])],
        }
    }

    #[test]
    fn a_valid_product_passes() {
        assert_eq!(validate(&valid()), Ok(()));
    }

    #[test]
    fn each_rule_has_its_own_error() {
        type Change = fn(&mut NewProduct);
        let cases: Vec<(Change, ValidationError)> = vec![
            (|p| p.name = "  ".into(), ValidationError::EmptyName),
            (|p| p.brand.clear(), ValidationError::EmptyBrand),
            (|p| p.variants[0].sku.clear(), ValidationError::EmptySku),
            (
                |p| p.variants[1].sku = "A".into(),
                ValidationError::DuplicateSku("A".into()),
            ),
            (
                |p| p.variants[1].price_cents = 0,
                ValidationError::PriceNotPositive {
                    sku: "B".into(),
                    cents: 0,
                },
            ),
            (
                |p| p.variants[0].stock = -1,
                ValidationError::NegativeStock {
                    sku: "A".into(),
                    stock: -1,
                },
            ),
            (
                |p| p.versions[0].number = 0,
                ValidationError::VersionNumberNotPositive(0),
            ),
            (
                |p| p.versions[1].number = 1,
                ValidationError::DuplicateVersion(1),
            ),
            (
                |p| p.versions[1].variant_skus.push("C".into()),
                ValidationError::UnknownSku {
                    version: 2,
                    sku: "C".into(),
                },
            ),
        ];
        for (change, expected) in cases {
            let mut product = valid();
            change(&mut product);
            assert_eq!(validate(&product), Err(expected));
        }
    }

    #[test]
    fn find_duplicate_returns_the_second_occurrence() {
        assert_eq!(find_duplicate([3, 1, 4, 1, 5]), Some(1));
        assert_eq!(find_duplicate(["a", "b"]), None);
        assert_eq!(find_duplicate(Vec::<i32>::new()), None);
    }
}
