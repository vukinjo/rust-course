//! Customer reviews, fetched from another HTTP service. The demo uses
//! jsonplaceholder.typicode.com, a free fake API: its "comments" stand in
//! for reviews, and each product gets one of its posts (`post_for`).
//!
//! The other service's JSON stays in this module: `Comment` mirrors it, and
//! the rest of the program only sees `Review`. If the other service changes
//! its format, or is replaced, only this file changes.
use std::time::Duration;

use serde::Deserialize;
use uuid::Uuid;

/// A review, in this program's terms.
#[derive(Clone, Debug, PartialEq)]
pub struct Review {
    pub author: String,
    pub title: String,
    pub body: String,
}

/// One comment as jsonplaceholder sends it. Fields we don't declare
/// (`id`, `postId`) are ignored.
#[derive(Deserialize)]
struct Comment {
    email: String,
    name: String,
    body: String,
}

impl From<Comment> for Review {
    fn from(comment: Comment) -> Self {
        Review {
            author: comment.email,
            title: comment.name,
            body: comment.body,
        }
    }
}

/// A client for the reviews service. `reqwest::Client` keeps a pool of
/// connections and is cheap to clone (it's an `Arc` inside), so the service
/// creates one at start-up and every request shares it.
#[derive(Clone)]
pub struct ReviewsClient {
    http: reqwest::Client,
    base_url: String,
}

impl ReviewsClient {
    /// `timeout` covers the whole request, from connecting to the last byte.
    pub fn new(base_url: &str, timeout: Duration) -> Result<ReviewsClient, reqwest::Error> {
        let http = reqwest::Client::builder().timeout(timeout).build()?;
        Ok(ReviewsClient {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
        })
    }

    /// GET {base_url}/comments?postId=N for the post of the product, as
    /// reviews.
    pub async fn for_product(&self, product_id: Uuid) -> Result<Vec<Review>, reqwest::Error> {
        let comments: Vec<Comment> = self
            .http
            .get(format!("{}/comments", self.base_url))
            .query(&[("postId", post_for(product_id))])
            .send() // fails if there's no connection or no answer in time
            .await?
            .error_for_status()? // turns 4xx and 5xx answers into errors too
            .json() // reads the body and deserializes it
            .await?;
        Ok(comments.into_iter().map(Review::from).collect())
    }
}

/// The jsonplaceholder post that stands in for a product. It has posts 1 to
/// 100, and the last bits of a UUIDv7 are random, so the products spread
/// over all of them.
pub fn post_for(product_id: Uuid) -> u64 {
    (product_id.as_u128() % 100) as u64 + 1
}
