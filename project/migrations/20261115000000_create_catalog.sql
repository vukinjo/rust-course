-- Products, their variants and versions, and which variants each version offers.
-- Deleting a product deletes its variants and versions (ON DELETE CASCADE), and
-- deleting either end of a version_variants row deletes the row.

CREATE TABLE products (
    id          BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    brand       TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    image       BYTEA,
    image_type  TEXT CHECK (image_type IN ('image/jpeg', 'image/png', 'image/webp')),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((image IS NULL) = (image_type IS NULL))
);

CREATE TABLE variants (
    id          BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    product_id  BIGINT NOT NULL REFERENCES products (id) ON DELETE CASCADE,
    sku         TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL DEFAULT '',
    price_cents BIGINT NOT NULL CHECK (price_cents > 0),
    image       BYTEA,
    image_type  TEXT CHECK (image_type IN ('image/jpeg', 'image/png', 'image/webp')),
    CHECK ((image IS NULL) = (image_type IS NULL))
);
CREATE INDEX variants_product_id_idx ON variants (product_id);

CREATE TABLE versions (
    id          BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    product_id  BIGINT NOT NULL REFERENCES products (id) ON DELETE CASCADE,
    number      INTEGER NOT NULL CHECK (number > 0),
    notes       TEXT NOT NULL DEFAULT '',
    released_on DATE NOT NULL,
    UNIQUE (product_id, number)
);

CREATE TABLE version_variants (
    version_id  BIGINT NOT NULL REFERENCES versions (id) ON DELETE CASCADE,
    variant_id  BIGINT NOT NULL REFERENCES variants (id) ON DELETE CASCADE,
    PRIMARY KEY (version_id, variant_id)
);
CREATE INDEX version_variants_variant_id_idx ON version_variants (variant_id);
