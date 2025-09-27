//! Entity module - Contains all SeaORM entity definitions for the database.
//! These entities represent the database tables and their relationships.
//! Each entity has a Model struct for data and an Entity struct for operations.

pub mod envelope;
pub mod product;
pub mod system_state;
pub mod transaction;

// Re-export specific types to avoid conflicts
pub use envelope::{Column as EnvelopeColumn, Entity as Envelope, Model as EnvelopeModel};
pub use product::{Column as ProductColumn, Entity as Product, Model as ProductModel};
pub use system_state::{
    Column as SystemStateColumn, Entity as SystemState, Model as SystemStateModel,
};
pub use transaction::{
    Column as TransactionColumn, Entity as Transaction, Model as TransactionModel,
};
