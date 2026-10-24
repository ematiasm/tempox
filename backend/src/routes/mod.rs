use axum::Router;

use crate::AppState;

pub mod accounts;
pub mod customers;
pub mod day;
pub mod health;
pub mod hello;
pub mod ledger;
pub mod notes;
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
            .merge(accounts::routes())
            .merge(ledger::routes()),
    )
}
