# shoes

The shoe catalog service as it stands at the end of the course (Chapter 28): a REST API with axum, PostgreSQL
through SQLx with migrations, a reqwest client for reviews, tracing logs, settings from environment variables, bearer-token
authentication, an endpoint that shows what the process sees of the operating system, and a Containerfile that packages
it as a 35 MB image.

```
src/main.rs            start-up: tracing, settings, pool, migrations, server, graceful shutdown
src/lib.rs             app(): the router, the token check (route_layer) and the TraceLayer
src/auth.rs            bearer-token middleware, AuthError (401), CurrentUser and GET /me
src/settings.rs        Settings from APP_* environment variables only (APP_ENVIRONMENT, APP_DATABASE__URL, …)
src/system.rs          GET /system: build, system, process, resources and environment, mostly read from /proc
src/error.rs           CatalogError and its HTTP responses
src/reviews.rs         ReviewsClient (jsonplaceholder comments → reviews)
src/products/          the products feature: model (DTOs), entity, mapper, validate, routes, service, service/repo
migrations/            0001 create catalog, 0002 add variants.stock
tests/                 API tests (#[sqlx::test]), authentication, reviews client, migration and /system tests
.sqlx/                 saved query metadata, so it builds without a database (SQLX_OFFLINE=true)
assets/                sample files for image uploads (two PNGs and a text file that is not an image)
integration-tests/     Bruno collection: integration tests for the running service
.cargo/config.toml     the local settings: Cargo sets them as APP_* variables for cargo run and cargo test
docs/architecture.svg  diagram: start-up wiring, request flow, errors, comparison with Spring Boot
docs/schema.svg        diagram: the database schema (crow's foot notation), constraints and indexes
Containerfile          two-stage image build: compile with --profile small, run on distroless (Chapter 28)
.containerignore       keeps the build context to the files the build needs
```

The diagram in [docs/architecture.svg](docs/architecture.svg) shows how `main.rs` builds each component and how a
request travels through the layers, the handler, the service and the repository. [docs/schema.svg](docs/schema.svg) shows the
tables after both migrations, their relations, constraints and indexes, and the `_sqlx_migrations` table that SQLx keeps.

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

If the container already exists, run `podman start catalog-db` instead. `RUST_LOG` and `LOG_FORMAT=json`
control the logs.

The service reads its settings only from environment variables, in every environment, and has no defaults in
the code; a missing one stops the start-up with its name. On your machine, `.cargo/config.toml` holds every
local value and Cargo sets them for `cargo run` and `cargo test` (the binary itself never reads that file). A
variable that is already set wins, so one run can override a value, for example
`APP_DATABASE__URL=postgres://… APP_SERVER__ADDR=0.0.0.0:8080 cargo run`. Running `target/debug/shoes`
directly needs the variables set some other way.

| Variable | Example |
|---|---|
| `APP_ENVIRONMENT` | `local`, `development`, `staging`, `production` or `release` |
| `APP_SERVER__ADDR` | `0.0.0.0:3000` |
| `APP_DATABASE__URL` | `postgres://user:password@host:5432/shoes` (a secret) |
| `APP_DATABASE__MAX_CONNECTIONS` | `20` |
| `APP_REVIEWS__URL` | `https://jsonplaceholder.typicode.com` |
| `APP_REVIEWS__TIMEOUT_MS` | `5000` |

For deployment, `cargo build --release` writes `target/release/shoes` (15 MB). `cargo build --profile small`
writes `target/small/shoes` (5.8 MB), optimized for size; see `[profile.small]` in `Cargo.toml`. Neither
needs anything but its environment variables.

## Run it in a container

The `Containerfile` compiles the binary in the Rust 1.100 image and copies only the binary into
`gcr.io/distroless/cc-debian13:nonroot`. The image holds no configuration: every setting, `APP_ENVIRONMENT`
included, comes from the container's environment, for example from an env file kept outside the repository
(it contains the database password, so `chmod 600` it):

```
APP_ENVIRONMENT=staging
APP_SERVER__ADDR=0.0.0.0:3000
APP_DATABASE__URL=postgres://catalog:catalog@catalog-db:5432/shoes
APP_DATABASE__MAX_CONNECTIONS=5
APP_REVIEWS__URL=https://jsonplaceholder.typicode.com
APP_REVIEWS__TIMEOUT_MS=5000
```

The build takes the Cargo profile from `CARGO_PROFILE` (default `small`): `CARGO_PROFILE=release podman build
--build-arg CARGO_PROFILE -t shoes .`. Both containers have to be on one Podman network, so the database
container is started on it (its data stays in the volume):

```bash
podman build -t shoes .                            # about a minute the first time; image 35 MB
podman network create catalog                      # once
podman stop catalog-db && podman rm catalog-db     # once, if it was started without --network
podman run -d --name catalog-db --network catalog \
  -e POSTGRES_USER=catalog -e POSTGRES_PASSWORD=catalog -e POSTGRES_DB=catalog \
  -p 127.0.0.1:5432:5432 -v catalog-pgdata:/var/lib/postgresql \
  --health-cmd 'pg_isready -U catalog -d catalog' --health-interval 5s \
  docker.io/library/postgres:18
podman run -d --name shoes --network catalog -p 127.0.0.1:3000:3000 \
  --env-file ~/shoes/staging.env shoes
```

The container and `cargo run` both use port 3000, so stop one before starting the other (`podman stop shoes`).

## Test it

```bash
DATABASE_URL=postgres://catalog:catalog@localhost:5432/shoes cargo test   # 46 tests; each #[sqlx::test] gets its own database
cargo lint                                                                # clippy --all-targets -- -D warnings
```

`.cargo/config.toml` defines four aliases: `cargo lint` (as above), `cargo fmt-check` (`fmt --check`, which changes
nothing and fails if a file is not formatted), `cargo small` (`build --profile small`) and
`cargo prepare` (`sqlx prepare -- --all-targets`, which updates `.sqlx/` after a query changes).

## API

Every route needs `Authorization: Bearer <token>`, where the token is any 10 characters (a placeholder for real
token verification, see Chapter 27). Without a valid token the answer is 401 with `WWW-Authenticate: Bearer`;
an unknown path is still 404. The token check also records the caller: every log line of an authenticated
request shows `user="<id>"`, and handlers can take a `CurrentUser` argument. For now the id is the token itself.

```bash
curl -H 'Authorization: Bearer abcdefghij' localhost:3000/products
```

Ids are UUIDv7 strings such as `01a1e42c-9b05-7222-92f8-b6e2aa85cf7b`: PostgreSQL makes them with `uuidv7()`, and
they sort in the order of creation. A path id that is not a UUID gets 400.

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
| `GET /me` | 200, `{"id": "<user id>"}`: who the service thinks is calling |
| `GET /system` | 200, what the process sees of the operating system: build, system, process, resources and the names of the `APP_*` variables |

Errors are JSON, such as `{"error": "invalid product: the name is empty"}`. Malformed JSON and unknown fields
or query parameters are rejected by axum's extractors, with a plain-text message.

## Integration tests with Bruno

`integration-tests/` is a [Bruno](https://www.usebruno.com/) collection with 41 requests and 87 tests. The six folders run
in order: they create a product, read it, filter it, update it, attach images, fetch its reviews and delete it,
and then check that requests without a valid token get 401 and that `GET /me` names the caller. The last folder reads
`GET /system`. The collection
sends `Authorization: Bearer {{token}}` and every request inherits it (`auth: inherit`).
Each run puts a fresh suffix in names and SKUs, so you can run it as often as you like, even on a database that
holds other data. The reviews requests need internet access.

With the service running:

- **Bruno app:** *Open Collection* → choose the `integration-tests/` folder → select the `local` environment → run the
  collection (*Run* on the collection's menu).
- **Command line:**

  ```bash
  cd integration-tests && bunx @usebruno/cli run -r --env local
  ```

  `bunx` downloads the CLI on first use and needs no global install.

`environments/local.bru` points at `http://127.0.0.1:3000` and sets `token` to `test-token`; add another
environment file for another address, or override a variable with `--env-var baseUrl=http://…`.

Last verified on 2026-11-16 with Bruno CLI 4.3.0 against the container and a fresh database.
