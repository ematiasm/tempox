use axum::Router;

use crate::AppState;

pub mod accounts;
pub mod collections;
pub mod customers;
pub mod day;
pub mod expenses;
pub mod health;
pub mod hello;
pub mod ledger;
pub mod notes;
pub mod parties;
pub mod payables;
pub mod payments;
pub mod products;
pub mod purchases;
pub mod sales;
pub mod suppliers;

pub fn router() -> Router<AppState> {
    Router::new().merge(health::routes()).nest(
        "/api",
        hello::routes()
            .merge(notes::routes())
            .merge(sales::routes())
            .merge(products::routes())
            .merge(customers::routes())
            .merge(suppliers::routes())
            .merge(purchases::routes())
            .merge(day::routes())
            .merge(expenses::routes())
            .merge(accounts::routes())
            .merge(ledger::routes())
            .merge(payments::routes())
            .merge(payables::routes())
            .merge(parties::routes())
            .merge(collections::routes()),
    )
}
