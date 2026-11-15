# shoes

The shoe catalog service as it stands at the end of the course (Chapter 27): a REST API with axum, PostgreSQL
through SQLx with migrations, a reqwest client for reviews, tracing logs, layered settings, and bearer-token
authentication.

```
src/main.rs            start-up: tracing, settings, pool, migrations, server, graceful shutdown
src/lib.rs             app(): the router, the token check (route_layer) and the TraceLayer
src/auth.rs            bearer-token middleware and AuthError (401)
src/settings.rs        config/base.yaml → config/{APP_ENV}.yaml → APP_<SECTION>__<KEY> env vars
src/error.rs           CatalogError and its HTTP responses
src/reviews.rs         ReviewsClient (jsonplaceholder comments → reviews)
src/products/          the products feature: model (DTOs), entity, mapper, validate, routes, service, service/repo
migrations/            0001 create catalog, 0002 add variants.stock
tests/                 API tests (#[sqlx::test]), authentication tests, reviews client tests, migration test
.sqlx/                 saved query metadata, so it builds without a database (SQLX_OFFLINE=true)
assets/                sample files for image uploads (two PNGs and a text file that is not an image)
integration-tests/     Bruno collection: integration tests for the running service
```

## Run it

You need Rust 1.100+ and the PostgreSQL 18 container from Chapter 25:

```bash
podman run -d --name catalog-db \
  -e POSTGRES_USER=catalog -e POSTGRES_PASSWORD=catalog -e POSTGRES_DB=catalog \
  -p 127.0.0.1:5432:5432 -v catalog-pgdata:/var/lib/postgresql \
  --health-cmd 'pg_isready -U catalog -d catalog' --health-interval 5s \
  docker.io/library/postgres:18
podman exec catalog-db createdb -U catalog shoes   # once

cargo run                                         # applies the migrations, listens on 127.0.0.1:3000
```

If the container already exists, run `podman start catalog-db` instead. `config/local.yaml` holds the database
URL; override any setting with an environment variable, for example
`APP_DATABASE__URL=postgres://… APP_SERVER__ADDR=0.0.0.0:8080 cargo run`. `RUST_LOG` and `LOG_FORMAT=json`
control the logs.

## Test it

```bash
DATABASE_URL=postgres://catalog:catalog@localhost:5432/shoes cargo test   # 39 tests; each #[sqlx::test] gets its own database
cargo clippy --all-targets -- -D warnings
```

## API

Every route needs `Authorization: Bearer <token>`, where the token is any 10 characters (a placeholder for real
token verification, see Chapter 27). Without a valid token the answer is 401 with `WWW-Authenticate: Bearer`;
an unknown path is still 404.

```bash
curl -H 'Authorization: Bearer abcdefghij' localhost:3000/products
```

| Method and path | Result |
|---|---|
| `GET /products?name=&brand=&minPrice=&maxPrice=` | 200, the matching products |
| `POST /products` | 201 and the product; 409 for a taken name or SKU, 422 for invalid data |
| `GET /products/{id}` | 200, or 404 |
| `PUT /products/{id}` | 200; replaces the product, variants are matched by SKU and keep their id and image |
| `DELETE /products/{id}` | 204, or 404 |
| `PUT /products/{id}/image` | 204; the raw body is the image (JPEG, PNG or WebP, at most 1 MiB), else 415 or 413 |
| `GET /products/{id}/image` | 200 with the image, or 404 |
| `PUT/GET /products/{id}/variants/{sku}/image` | the same for a variant |
| `GET /products/{id}/reviews` | 200, the reviews from jsonplaceholder; 502 when that service fails |

Errors are JSON, such as `{"error": "invalid product: the name is empty"}`. Malformed JSON and unknown fields
or query parameters are rejected by axum's extractors, with a plain-text message.

## Integration tests with Bruno

`integration-tests/` is a [Bruno](https://www.usebruno.com/) collection with 37 requests and 78 tests. The five folders run
in order: they create a product, read it, filter it, update it, attach images, fetch its reviews and delete it,
and then check that requests without a valid token get 401. The collection sends `Authorization: Bearer {{token}}`
and every request inherits it (`auth: inherit`).
Each run puts a fresh suffix in names and SKUs, so you can run it as often as you like, even on a database that
holds other data. The reviews requests need internet access.

With the service running:

- **Bruno app:** *Open Collection* → choose the `integration-tests/` folder → select the `local` environment → run the
  collection (*Run* on the collection's menu).
- **Command line:**

  ```bash
  npm install -g @usebruno/cli
  cd integration-tests && bru run -r --env local
  ```

`environments/local.bru` points at `http://127.0.0.1:3000` and sets `token` to `test-token`; add another
environment file for another address, or override a variable with `--env-var baseUrl=http://…`.

Last verified on 2026-11-15 with Bruno CLI 4.3.0 against a fresh database.
