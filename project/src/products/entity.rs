//! The entities: products, their variants and versions, and images, as the
//! service stores them. They have no serde derives: the JSON clients see is
//! defined separately, in `model.rs`, and `mapper.rs` converts between the
//! two. The `*Entity` types are what has been stored, with the ids the
//! repository assigned; the `*Input` types are what it takes to create or
//! replace a product: everything but the ids.

use std::error::Error;
use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, NaiveDate, Utc};

/// The image formats the service accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageType {
    Jpeg,
    Png,
    Webp,
}

impl ImageType {
    /// Recognizes a format from the file's first bytes (its "magic number"),
    /// so a client can't label a text file `image/png`.
    pub fn detect(bytes: &[u8]) -> Option<ImageType> {
        match bytes {
            [0xFF, 0xD8, 0xFF, ..] => Some(ImageType::Jpeg),
            [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1A, b'\n', ..] => Some(ImageType::Png),
            [
                b'R',
                b'I',
                b'F',
                b'F',
                _,
                _,
                _,
                _,
                b'W',
                b'E',
                b'B',
                b'P',
                ..,
            ] => Some(ImageType::Webp),
            _ => None,
        }
    }

    /// The media type sent in the `Content-Type` header.
    pub fn content_type(self) -> &'static str {
        match self {
            ImageType::Jpeg => "image/jpeg",
            ImageType::Png => "image/png",
            ImageType::Webp => "image/webp",
        }
    }
}

/// "image/gif".parse::<ImageType>() fails with this error.
#[derive(Debug, PartialEq)]
pub struct UnsupportedImageType(pub String);

impl fmt::Display for UnsupportedImageType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} is not image/jpeg, image/png or image/webp", self.0)
    }
}

impl Error for UnsupportedImageType {}

/// Parses a media type such as "image/png", the inverse of `content_type`.
impl FromStr for ImageType {
    type Err = UnsupportedImageType;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "image/jpeg" => Ok(ImageType::Jpeg),
            "image/png" => Ok(ImageType::Png),
            "image/webp" => Ok(ImageType::Webp),
            _ => Err(UnsupportedImageType(text.to_string())),
        }
    }
}

impl fmt::Display for ImageType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.content_type())
    }
}

/// An uploaded file: its bytes and what kind of image they are.
#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    pub content_type: ImageType,
    pub bytes: Vec<u8>,
}

impl Image {
    /// None when the bytes aren't a JPEG, PNG or WebP image.
    pub fn from_bytes(bytes: Vec<u8>) -> Option<Image> {
        let content_type = ImageType::detect(&bytes)?;
        Some(Image {
            content_type,
            bytes,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct VariantEntity {
    pub id: i64,
    pub sku: String,
    pub description: String, // colour, print, size: anything that varies
    pub price_cents: i64,
    pub stock: i32, // added by migration 0002
    pub image: Option<Image>,
}

/// A release of the model. It offers some of the product's variants,
/// and a variant can be offered by several releases: many-to-many.
#[derive(Clone, Debug, PartialEq)]
pub struct VersionEntity {
    pub id: i64,
    pub number: i32,
    pub notes: String,
    pub released_on: NaiveDate, // a date without a time zone
    pub variant_skus: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProductEntity {
    pub id: i64,
    pub name: String,
    pub brand: String,
    pub description: String,
    pub image: Option<Image>,
    pub variants: Vec<VariantEntity>, // one-to-many
    pub versions: Vec<VersionEntity>, // one-to-many
    pub created_at: DateTime<Utc>,    // set by PostgreSQL
    pub updated_at: DateTime<Utc>,
}

/// A product to create, or the new state of one to replace.
#[derive(Clone, Debug, PartialEq)]
pub struct ProductInput {
    pub name: String,
    pub brand: String,
    pub description: String,
    pub variants: Vec<VariantInput>,
    pub versions: Vec<VersionInput>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VariantInput {
    pub sku: String,
    pub description: String,
    pub price_cents: i64,
    pub stock: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VersionInput {
    pub number: i32,
    pub notes: String,
    pub released_on: NaiveDate,
    pub variant_skus: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_images_by_their_first_bytes() {
        assert_eq!(
            ImageType::detect(&[0xFF, 0xD8, 0xFF, 0xE0]),
            Some(ImageType::Jpeg)
        );
        assert_eq!(
            ImageType::detect(b"\x89PNG\r\n\x1a\n...."),
            Some(ImageType::Png)
        );
        assert_eq!(
            ImageType::detect(b"RIFF\0\0\0\0WEBPVP8 "),
            Some(ImageType::Webp)
        );
        assert_eq!(ImageType::detect(b"GIF89a"), None);
        assert_eq!(ImageType::detect(b""), None);
    }

    #[test]
    fn content_types_parse_back() {
        for kind in [ImageType::Jpeg, ImageType::Png, ImageType::Webp] {
            assert_eq!(kind.content_type().parse(), Ok(kind));
        }
        assert!("image/gif".parse::<ImageType>().is_err());
    }
}
