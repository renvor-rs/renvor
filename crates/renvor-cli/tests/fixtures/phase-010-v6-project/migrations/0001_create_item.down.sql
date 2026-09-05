-- v6-project: reverse 0001.
--
-- Present because this migration DECLARES itself reversible. A migration with no `.down.sql` is
-- irreversible, and a rollback of one is refused before anything is modified rather than
-- discovered half-way through.
DROP TABLE IF EXISTS item;
