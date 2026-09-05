-- v6-project: create the example item table.
--
-- Applied by version order, not by filename order. The version is the numeric prefix.
CREATE TABLE IF NOT EXISTS item (
    id   BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    name VARCHAR(200) NOT NULL
);

