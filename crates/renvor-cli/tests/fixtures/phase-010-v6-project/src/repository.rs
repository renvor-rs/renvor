//! The `item` repository for v6-project.
//!
//! Like `src/entity.rs`, this file is generated in full and is **not compiled yet** — see that
//! file's header for why, and `Cargo.toml` for the two lines that change it.
//!
//! # Every function takes `&impl ConnectionTrait`, and that is the whole design
//!
//! SeaORM implements that trait for a pool, for a transaction, and — when the Renvor crates are
//! published — for `renvor_seaorm::SeaOrmUnitOfWork`. So the same repository serves a one-off read
//! and a call inside an explicit transaction boundary, with no second implementation and no way
//! for the two to drift.
//!
//! It is also what keeps this module honest about layering: it names no pool, opens no connection,
//! and begins no transaction. A transaction is begun by the caller, in application code, where
//! `PLAN.md` §12 requires the boundary to be visible.
//!
//! # What is NOT here
//!
//! Raw SQL. The escape hatch has three rungs and this file needs none of them:
//!
//! 1. entity and SeaQuery APIs — everything below;
//! 2. `Statement::from_sql_and_values`, when a query cannot be expressed above **and every value
//!    is still bound**;
//! 3. a database-specific module, when the two engines genuinely differ.
//!
//! Two things are NOT rungs, and both are easy to reach for:
//!
//! - `execute_unprepared` binds nothing, so a caller-controlled value reaching it is an injection.
//!   It also accepts **multiple statements separated by semicolons** on both PostgreSQL and MySQL,
//!   so the blast radius is arbitrary stacked DDL and DML rather than one rewritten query.
//! - `Statement::from_string` is the constructor that carries SQL with **no values at all**. It
//!   looks like rung 2 and behaves like `execute_unprepared`. If you have a value, use
//!   `Statement::from_sql_and_values`.

use sea_orm::{
    ActiveModelTrait as _, ActiveValue, ColumnTrait as _, ConnectionTrait, DbErr, EntityTrait as _,
    Order, QueryFilter as _, QueryOrder as _, QuerySelect as _,
};

// Aliased so the code below reads the way SeaORM's own examples do, where each
// entity lives in a module named after its table.
use crate::entity as item;
use crate::entity::Entity as Item;

/// Inserts one item and returns it, primary key included.
///
/// # Errors
///
/// [`DbErr`] when the database refuses the insert — a value longer than the column, or a
/// connection that has gone away.
pub async fn create<C: ConnectionTrait>(connection: &C, name: &str) -> Result<item::Model, DbErr> {
    item::ActiveModel {
        // `NotSet`, not `Set(0)`: the database assigns the key, and naming a value here would
        // fight the identity column rather than defer to it.
        id: ActiveValue::NotSet,
        name: ActiveValue::Set(name.to_owned()),
    }
    .insert(connection)
    .await
}

/// Reads one item by primary key, or `None`.
///
/// `None` is an ordinary outcome rather than an error: "there is no such item" is an answer.
///
/// # Errors
///
/// [`DbErr`] when the read itself failed.
pub async fn by_id<C: ConnectionTrait>(
    connection: &C,
    id: i64,
) -> Result<Option<item::Model>, DbErr> {
    Item::find_by_id(id).one(connection).await
}

/// Renames one item, returning the updated row.
///
/// # Errors
///
/// [`DbErr::RecordNotUpdated`] when no row matched — which is NOT the same as "no such row", and
/// is the signal an optimistic-concurrency caller needs.
pub async fn rename<C: ConnectionTrait>(
    connection: &C,
    id: i64,
    name: &str,
) -> Result<item::Model, DbErr> {
    item::ActiveModel {
        id: ActiveValue::Unchanged(id),
        name: ActiveValue::Set(name.to_owned()),
    }
    .update(connection)
    .await
}

/// Deletes one item, returning whether a row was removed.
///
/// # Errors
///
/// [`DbErr`] when the delete itself failed.
pub async fn delete<C: ConnectionTrait>(connection: &C, id: i64) -> Result<bool, DbErr> {
    Ok(Item::delete_by_id(id).exec(connection).await?.rows_affected > 0)
}

/// One page of items, ordered by a **keyset** rather than an offset.
///
/// # Why a keyset and not `OFFSET`
///
/// `OFFSET` re-reads and discards every row it skips, so page 500 costs 500 pages of work, and a
/// row inserted between two requests shifts every later page — silently skipping or repeating
/// rows. A keyset asks for "the next rows after this key", which is O(page) and stable under
/// concurrent writes.
///
/// The order is TOTAL because `id` is unique. Ordering by `name` alone would leave rows that share
/// a name in an order the database may change between calls, and a cursor built on an unstable
/// order is worse than no cursor.
///
/// # Errors
///
/// [`DbErr`] when the read failed.
pub async fn page_after<C: ConnectionTrait>(
    connection: &C,
    after: Option<i64>,
    limit: u64,
) -> Result<Vec<item::Model>, DbErr> {
    let mut query = Item::find().order_by(item::Column::Id, Order::Asc);
    if let Some(cursor) = after {
        // A BOUND value. `gt` builds a parameter; the column is a typed constant from the derive,
        // so neither half of this comparison is a string a caller could influence.
        query = query.filter(item::Column::Id.gt(cursor));
    }
    query.limit(limit).all(connection).await
}
