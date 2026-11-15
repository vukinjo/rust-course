-- How many pairs of each variant are in stock.
-- NOT NULL needs a value for the rows that already exist: DEFAULT 0 fills them in.
ALTER TABLE variants ADD COLUMN stock INTEGER NOT NULL DEFAULT 0 CHECK (stock >= 0);
