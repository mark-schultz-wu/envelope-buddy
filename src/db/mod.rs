pub mod connection;
pub mod envelopes;
pub mod products;
pub(crate) mod schema;
pub mod system_state;
pub(crate) mod test_utils;
pub mod transactions;

pub use connection::{DbPool, init_db};
pub use envelopes::{
    CreateUpdateEnvelopeArgs, create_or_reenable_envelope_flexible, get_all_active_envelopes,
    get_all_unique_active_envelope_names, get_envelope_by_id, get_user_or_shared_envelope,
    seed_initial_envelopes, soft_delete_envelope, update_envelope_balance,
};
#[allow(unused_imports)]
pub use products::{
    add_product, delete_product_by_name, get_all_product_names, get_product_by_id,
    get_product_by_name, list_all_products, suggest_product_names, update_product_price,
};
pub use system_state::{get_system_state_value, set_system_state_value};
pub use transactions::{
    create_transaction, get_actual_spending_this_month, prune_old_transactions,
};
