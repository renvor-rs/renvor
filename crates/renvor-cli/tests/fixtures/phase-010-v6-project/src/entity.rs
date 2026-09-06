//! The `item` entity for v6-project, in SeaORM 2.0 dense style.
//!
//! # This file is NOT compiled yet, and that is deliberate
//!
//! `src/main.rs` does not declare `mod entity;`, and `Cargo.toml` declares no dependencies. Both
//! follow from one property Renvor will not give up: **`renvor new` works offline.** Generation
//! runs this project's own `cargo fmt`, `clippy`, `build`, `test` and `run` before placing it, so a
//! real `sea-orm` dependency would make generating a project resolve and compile SeaORM and SQLx
//! from the registry — minutes of work, and a network connection, to scaffold a directory.
//!
//! So the code is real and complete rather than sketched, and one step is left to you. Add the
//! dependencies `Cargo.toml` names, then declare `mod entity;` and `mod repository;` in
//! `src/main.rs`. Nothing here changes when you do.
//!
//! # Dense style
//!
//! `DeriveEntityModel` generates `Entity`, `Column`, `PrimaryKey` and `ActiveModel` from this one
//! struct. The alternative — writing each by hand — is what SeaORM calls expanded style; it exists
//! for entities that need to override something, and this one does not.

use sea_orm::entity::prelude::*;

/// One row of `item`.
///
/// The table name is stated rather than inferred: SeaORM's default derives it from the struct
/// name, and a rename would then silently change the SQL.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "item")]
pub struct Model {
    /// The primary key. `auto_increment` matches the migration, which declares
    /// `GENERATED ALWAYS AS IDENTITY`.
    #[sea_orm(primary_key)]
    pub id: i64,
    /// The item's name. `VARCHAR(200)` in the migration; the length is declared here too so that
    /// an over-long value is refused by the database rather than truncated by it.
    #[sea_orm(column_type = "String(StringLen::N(200))")]
    pub name: String,
}

/// `item` has no relations yet. The enum is required by the derive and is deliberately empty
/// rather than absent — an empty set is a statement, a missing one is an omission.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
