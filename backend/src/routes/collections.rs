//! A collection, or a payment to a supplier: one document that moves money once and answers
//! as many debts as it can.
//!
//! `odd/tasks/v5-payments.md` S1-S8. Slice 1 (`routes::parties`) taught the shop to **read** a
//! party's account; this is the write that closes documents. Before it, applying 300 to three
//! sales was three calls to `POST /api/settlements`, each naming one obligation, and the
//! cashier had to work out the split. The owner described the shape in `L2` of
//! `v5-customer-account.md` ("a side panel with three boxes, the second one the balance and
//! there the button to record the payment") and chose the rule in `L2` of this feature's own
//! document: **the cashier allocates freely, and the rest goes to the oldest debt**.
//!
//! | document | what it is | what it writes |
//! |---|---|---|
//! | one payment | money moving on one rail | one ledger entry, signed by its direction |
//! | N settlements | which debts that money answered, and how much | nothing — the money already moved |
//!
//! **The settlement writing nothing is the distinction this module must not lose.** A
//! settlement that moved money would count the same coin twice; `routes::payments` and
//! `routes::payables` say the same sentence, and the repetition is deliberate — the three
//! modules must not drift apart.
//!
//! **Everything here happens in one transaction, and a refusal therefore writes nothing.** No
//! payment, no ledger entry, no settlement, no till movement. That matters more than usual
//! because the ledger is append-only: a payment recorded for a request the operator believes
//! was rejected cannot be taken back row by row.
//!
//! **What is deliberately not here**: an idempotency key (no route in this application has
//! one, and a double submit is two payments, recorded as such), a reversal (compensating rows,
//! and no route for it yet), and any floor on a supplier payment (the ceiling in
//! `routes::payments` guards a *customer's* balance, not the till — a pre-existing gap this
//! slice does not widen and does not close).

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::post,
};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::AppState;
use crate::db::conflict_or_500;
use crate::routes::parties::{self, AccountPayload, Party};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/customers/{id}/collections", post(collect))
        .route("/suppliers/{id}/payments", post(pay))
}

// ── The wire ────────────────────────────────────────────────────────────────

/// One amount the cashier decided, against one debt they named.
///
/// `debt_id` is the **`id` of an unpaid document in the party's account** — the obligation for
/// a client, the payable for a supplier (`L4`). Not `obligation_id`: one body serves both
/// mirrors, and a debt is a document on both sides.
#[derive(Debug, Deserialize)]
pub struct ManualAllocation {
    pub debt_id: Uuid,
    /// A decimal string, not a JSON number: a float would lose the cent.
    pub amount: Decimal,
}

#[derive(Debug, Deserialize)]
pub struct NewCollection {
    pub payment_method_id: Uuid,
    /// The money that arrived, or left. The manual lines are a *plan* for it: they cannot add
    /// up to more, and what they do not name the rule allocates.
    pub total: Decimal,
    /// The debts the cashier decided by hand. Empty is legitimate: the rule then allocates
    /// the whole total, which is "oldest first" with nothing named.
    #[serde(default)]
    pub manual: Vec<ManualAllocation>,
}

/// Why an applied row exists, and the only thing that distinguishes a decision from a rule.
///
/// `manual` is what the cashier typed; `oldest` is what the rule added. Without it the
/// response could not answer the question a cashier actually has — "I named one document, why
/// are two closed?" — and no test could assert the rule at all.
#[derive(Debug, Serialize, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Manual,
    Oldest,
}

/// One debt this collection answered, with the figure afterwards rather than before: a caller
/// applying a balance needs to know where it stands without asking again.
#[derive(Debug, Serialize)]
pub struct AppliedRow {
    /// The obligation (client) or payable (supplier) this row settled. The internal row.
    pub debt_id: Uuid,
    /// **The sale or purchase** the debt came from — the id a cashier can actually look up.
    /// Carried separately because the two are different rows, and a receipt that printed the
    /// debt's id for the sale's is the defect slice 1 found three times over. `routes::parties`
    /// makes the same distinction on the read side.
    pub source_document_id: Uuid,
    #[serde(serialize_with = "crate::money::serialize_amount")]
    pub amount: Decimal,
    pub source: Source,
    /// What this debt still owes. Zero for a debt this collection closed.
    #[serde(serialize_with = "crate::money::serialize_amount")]
    pub debt_outstanding: Decimal,
}

#[derive(Debug, Serialize)]
pub struct Collection {
    pub payment_id: Uuid,
    #[serde(serialize_with = "crate::money::serialize_amount")]
    pub total: Decimal,
    pub direction: String,
    /// The debts in the order they were answered: the manual lines first, in the order the
    /// cashier wrote them, then the rule's, oldest first. The order is part of the answer — a
    /// client rendering it shows the decision before its consequence.
    pub applied: Vec<AppliedRow>,
    /// **What no debt needed.** Money that arrived and is not spoken for: the party's credit
    /// balance. Derived, never stored, and returned because it is the one figure a caller
    /// cannot reconstruct from `applied` without knowing the ceilings it did not see.
    #[serde(serialize_with = "crate::money::serialize_amount")]
    pub unapplied: Decimal,
    /// **The party's account after this write, read in the same transaction** (`S6`). One
    /// snapshot, so the panel replaces its state instead of asking again: a second request is
    /// a second instant, and slice 1 paid for a panel that contradicted itself.
    pub account: AccountPayload,
}

// ── The one implementation of both mirrors ──────────────────────────────────

/// A debt as this module needs it while allocating: which row, and what it still owes.
///
/// **`occurred_at` is not a field, deliberately.** "Oldest" is the order of the query that
/// produces this list, so carrying the date would invite a caller to re-sort it — and a second
/// ordering of the same debts is how the rule and its implementation drift apart. The one
/// `ORDER BY` is where the definition lives.
struct Debt {
    id: Uuid,
    /// The sale or purchase that created it, for the receipt. Carried from the same query so
    /// the applied row never has to look it up again.
    source_document_id: Uuid,
    outstanding: Decimal,
}

impl Party {
    /// This side's debts: the client's obligations, or the supplier's payables, each with its
    /// outstanding already derived by the same subtraction `routes::parties` reports.
    ///
    /// **Resolved debts are included, with `outstanding` zero**, and that is not an oversight:
    /// it is what lets the layer tell *"that document is not yours to close"* (`400`) from
    /// *"there is nothing left on it"* (`409`). Excluding them here would collapse the two into
    /// one answer, and the second is the answer a cashier who typed the split by hand gets when
    /// a colleague collected the same debt a moment earlier.
    ///
    /// **Ordered here, and it is the rule's order**: ascending document date, tie-broken by id
    /// so the sequence is total even when two documents share a timestamp. `routes::parties`
    /// orders newest-first for the panel, which is the right order for reading a list and the
    /// wrong one for spending money — so this is a different query with a different `ORDER BY`,
    /// not the same rows re-sorted after the fact.
    async fn debts(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        party: Party,
        party_id: Uuid,
    ) -> Result<Vec<Debt>, sqlx::Error> {
        let rows: Vec<(Uuid, Uuid, Decimal)> = match party {
            Party::Customer => sqlx::query_as(
                "SELECT o.id, o.source_document_id,
                        o.amount
                          - COALESCE((SELECT SUM(a.amount) FROM obligations_adjustments a
                                       WHERE a.obligation_id = o.id), 0)
                          - COALESCE((SELECT SUM(s.amount) FROM settlements s
                                       WHERE s.obligation_id = o.id), 0) AS outstanding
                   FROM obligations o
                  WHERE o.party_kind = 'customer' AND o.party_id = $1
                    AND o.amount
                          - COALESCE((SELECT SUM(a.amount) FROM obligations_adjustments a
                                       WHERE a.obligation_id = o.id), 0)
                          - COALESCE((SELECT SUM(s.amount) FROM settlements s
                                       WHERE s.obligation_id = o.id), 0) >= 0
                   ORDER BY o.occurred_at, o.id",
            ),
            Party::Supplier => sqlx::query_as(
                "SELECT o.id, o.source_document_id,
                        o.amount
                          - COALESCE((SELECT SUM(a.amount) FROM payables_adjustments a
                                       WHERE a.payable_id = o.id), 0)
                          - COALESCE((SELECT SUM(s.amount) FROM payable_settlements s
                                       WHERE s.payable_id = o.id), 0) AS outstanding
                   FROM payables o
                  WHERE o.party_kind = 'supplier' AND o.party_id = $1
                    AND o.amount
                          - COALESCE((SELECT SUM(a.amount) FROM payables_adjustments a
                                       WHERE a.payable_id = o.id), 0)
                          - COALESCE((SELECT SUM(s.amount) FROM payable_settlements s
                                       WHERE s.payable_id = o.id), 0) >= 0
                   ORDER BY o.occurred_at, o.id",
            ),
        }
        .bind(party_id)
        .fetch_all(&mut **tx)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(id, source_document_id, outstanding)| Debt {
                id,
                source_document_id,
                outstanding,
            })
            .collect())
    }

    /// Locks this side's debts, in the rule's own order, before any ceiling is judged.
    ///
    /// **What this buys is ordering, not the ceiling** (`S8`, and `V6` records the measurement).
    /// It gives the plan a read consistent with the writes that follow, and it keeps every
    /// writer of a party's money in the one order `aa95665` established — the party's payment
    /// rows first (`payments`/`payables` do that), then the debts ascending by
    /// `(occurred_at, id)`.
    ///
    /// **What it does not buy**: two racing collections are kept off each other's toes by
    /// `0022`'s `settlement_within_both_ceilings`, which locks the payment and then the debt and
    /// refuses the second settlement. This function was deleted and its order inverted during
    /// verification and the race test stayed green — so a comment claiming it prevents the race
    /// would be a comment the code does not back.
    ///
    /// The rows locked are **every** debt this party has, resolved ones included, which is
    /// exactly the set [`Party::debts`] then reads: a lock that covered less than the read would
    /// let a concurrent collector settle a debt this request is about to judge.
    async fn lock_debts(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        party: Party,
        party_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        match party {
            Party::Customer => sqlx::query(
                "SELECT id FROM obligations
                  WHERE party_kind = 'customer' AND party_id = $1
                  ORDER BY occurred_at, id
                  FOR UPDATE",
            ),
            Party::Supplier => sqlx::query(
                "SELECT id FROM payables
                  WHERE party_kind = 'supplier' AND party_id = $1
                  ORDER BY occurred_at, id
                  FOR UPDATE",
            ),
        }
        .bind(party_id)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    /// Writes one settlement row for this side, and returns nothing.
    ///
    /// One function, two tables, chosen by a closed enum. The alternative — a caller passing a
    /// table name — is what `routes::payments` refuses on principle, and it is right to: the
    /// safety of the interpolation would rest on a check three branches away.
    async fn settle(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        party: Party,
        debt_id: Uuid,
        payment_id: Uuid,
        amount: Decimal,
    ) -> Result<(), sqlx::Error> {
        match party {
            Party::Customer => sqlx::query(
                "INSERT INTO settlements (obligation_id, payment_id, amount, created_by)
                 VALUES ($1, $2, $3, 'counter')",
            ),
            Party::Supplier => sqlx::query(
                "INSERT INTO payable_settlements (payable_id, payment_id, amount, created_by)
                 VALUES ($1, $2, $3, 'counter')",
            ),
        }
        .bind(debt_id)
        .bind(payment_id)
        .bind(amount)
        .execute(&mut **tx)
        .await?;

        // **No ledger entry here, and this is the line that must never be added.** The money
        // moved when the payment was recorded; a settlement only says which debt it answered.
        Ok(())
    }

    /// The direction the money moves for this side: a client pays us (`in`), we pay a supplier
    /// (`out`). Stated once, because the sign of the ledger entry and the ceiling that applies
    /// are both derived from it.
    fn direction(self) -> &'static str {
        match self {
            Party::Customer => "in",
            Party::Supplier => "out",
        }
    }
}

/// The allocation plan: which debt takes how much, and where each decision came from.
///
/// Split out from the handler deliberately. The rule is the part of this module worth
/// reviewing, it needs no database, and a pure function can be read — and, if it ever earns
/// one, tested — without a fixture.
fn plan(
    debts: &[Debt],
    manual: &[ManualAllocation],
    total: Decimal,
) -> Result<Vec<(usize, Decimal, Source)>, StatusCode> {
    // **The manual lines first, and each must name a debt this party actually has.** An index
    // rather than an id, so the walk below cannot mistake a named debt for an unnamed one by
    // comparing strings.
    //
    // A line naming a debt outside this list is a `400` and not a `404`: the request is
    // well-formed and refers to a debt that is not this party's, or not outstanding any more.
    // The distinction matters because it is the difference between "you sent the wrong shape"
    // and "that document is not yours to close".
    let mut used = vec![false; debts.len()];
    let mut applied: Vec<(usize, Decimal, Source)> = Vec::with_capacity(manual.len());
    for line in manual {
        // **Two different refusals, and the distinction is the point.** A debt this party does
        // not have is a `400` — the request names something that is not theirs, or not a debt
        // at all. A debt with nothing left on it is a `409` — the request is well-formed and the
        // money simply is not owed any more, which is what a cashier sees when a colleague
        // collected the same document first.
        let index = debts
            .iter()
            .position(|debt| debt.id == line.debt_id)
            .ok_or(StatusCode::BAD_REQUEST)?;
        if debts[index].outstanding.is_zero() {
            return Err(StatusCode::CONFLICT);
        }
        // **The same debt twice is one row written twice**, which the schema refuses with its
        // `UNIQUE (…, payment_id)` — a `500` if this did not ask first. Refusing by hand also
        // gives the caller the one thing SQL cannot: which line was the duplicate's shape.
        if used[index] {
            return Err(StatusCode::BAD_REQUEST);
        }
        used[index] = true;
        // The debt's own ceiling, in its own message class: `409`, because the request is
        // well-formed and the money simply is not owed.
        if line.amount > debts[index].outstanding {
            return Err(StatusCode::CONFLICT);
        }
        applied.push((index, line.amount, Source::Manual));
    }

    let mut left = total
        .checked_sub(applied.iter().map(|(_, amount, _)| *amount).sum())
        .ok_or(StatusCode::BAD_REQUEST)?;
    if left < Decimal::ZERO {
        // The manual lines add up to more than the money that arrived, which is not a
        // malformed request — it is money that does not exist. `409`.
        return Err(StatusCode::CONFLICT);
    }

    // **The remainder, oldest first, and never into a debt the cashier already named** (`L8`).
    // The schema allows one settlement per `(debt, payment)` and a row has one `source`, so
    // revisiting a named debt would need a row with two origins — and `applied`'s `source`
    // would stop being an answer. The visible consequence (name the oldest for a part and the
    // rest goes to the *next* oldest) is written down in the spec rather than left to be
    // discovered.
    //
    // `debts` arrives already ordered by the rule, so this walk is the rule.
    for (index, debt) in debts.iter().enumerate() {
        if left.is_zero() {
            break;
        }
        // A resolved debt is skipped for the same reason a named one is: it has nothing to
        // take, and writing a settlement of zero against it would be a row that says nothing.
        if used[index] || debt.outstanding.is_zero() {
            continue;
        }
        let take = left.min(debt.outstanding);
        applied.push((index, take, Source::Oldest));
        left -= take;
    }

    Ok(applied)
}

/// The whole write, for either side. The two handlers are two lines each over this.
///
/// The order of operations is the order of the guarantees: shape, then the method and the
/// party, then the locks, then the plan, then the writes — and the last of those is the only
/// one that can leave a trace.
async fn allocate(
    state: &AppState,
    party: Party,
    party_id: Uuid,
    body: NewCollection,
) -> Result<Collection, StatusCode> {
    // ── Shape, with nothing opened and nothing locked ──
    //
    // `is_whole_cents` is asked here for the reason `routes::payments` states: the database
    // refuses a sub-cent amount with a CHECK, and a `RAISE` reaches the caller as a `500`.
    // Every manual line is checked, because one bad line refuses the whole request.
    if body.total <= Decimal::ZERO || !crate::money::is_whole_cents(body.total) {
        return Err(StatusCode::BAD_REQUEST);
    }
    for line in &body.manual {
        if line.amount <= Decimal::ZERO || !crate::money::is_whole_cents(line.amount) {
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    // **`READ COMMITTED`, and the reason is a defect `REPEATABLE READ` caused** (`V8`, findings
    // `R3-001` and `R4-1` of review `review-9cff43c50976184c`).
    //
    // The ceiling of this endpoint is judged in two places: here, in the plan, and in `0022`'s
    // `settlement_within_both_ceilings`. **Both read `SUM(settlements)`, and both must read the
    // same, current, committed state.** A snapshot fixed before the row locks were taken breaks
    // exactly that: two collections overlapping on one debt each block in `lock_debts`, and when
    // the winner commits, the loser resumes and still evaluates both ceilings against the
    // snapshot it took *before* that commit. Measured with the interleaving forced: a debt of
    // 100.00 was settled **twice for 100.00 each, summing 200**, and the loser answered `201`
    // with `debt_outstanding: 0.00`.
    //
    // So this transaction runs at the default level, where each statement sees the latest
    // committed state. Read consistency comes from the locks instead (`S8`), not the snapshot.
    //
    // **What this costs, stated rather than hidden.** `account_of`'s four queries could be
    // interleaved by a concurrent *sale*, which does not take this party's payment locks. That
    // is the window slice 1 closed with `REPEATABLE READ READ ONLY` on the read-only `GET`, and
    // a write transaction cannot have both. The ceiling that protects money wins.
    let mut tx = state.db.begin().await.map_err(|e| {
        tracing::error!(?e, "collection tx begin failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // ── The method has to exist, and it has to name a real rail ──
    //
    // The same check `routes::payments` makes, and for the same reason: a collection through an
    // account that holds nothing would say money arrived where it did not. `0021`'s trigger on
    // the book would refuse it too; this is the `404` that tells the caller.
    let account_id: Option<Uuid> =
        sqlx::query_scalar("SELECT account_id FROM payment_methods WHERE id = $1")
            .bind(body.payment_method_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!(?e, "payment method lookup failed");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
    if account_id.is_none() {
        return Err(StatusCode::NOT_FOUND);
    }

    // ── The party has to exist ──
    //
    // Two statements, not one with a table name interpolated: `Party` is a closed enum, so the
    // interpolation would be safe, and sqlx refuses dynamic SQL on principle anyway. The
    // principle is worth more than the line it saves.
    let found: i64 = match party {
        Party::Customer => {
            sqlx::query_scalar("SELECT count(*) FROM customers WHERE id = $1")
                .bind(party_id)
                .fetch_one(&mut *tx)
                .await
        }
        Party::Supplier => {
            sqlx::query_scalar("SELECT count(*) FROM suppliers WHERE id = $1")
                .bind(party_id)
                .fetch_one(&mut *tx)
                .await
        }
    }
    .map_err(|e| {
        tracing::error!(?e, "party lookup failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if found == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    // ── The locks, in the one order every writer of a party's money agrees on ──
    //
    // The party's **payment rows** first, exactly as `lock_party_balance` and `lock_party_sent`
    // take them, because a concurrent refund or settlement of the same party changes the state
    // this request is about to read. Then the debts, oldest first.
    //
    // Taking them the other way round is what `aa95665` records as a deadlock this base could
    // not have had: the trigger inside the schema locks the payment and then the debt, so a
    // route that locked the debt first would cycle against it.
    let _payment_rows: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM payments
          WHERE party_kind = $1 AND party_id = $2
          FOR UPDATE",
    )
    .bind(party.kind())
    .bind(party_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(?e, "party payment lock failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Party::lock_debts(&mut tx, party, party_id)
        .await
        .map_err(|e| {
            tracing::error!(?e, "debt lock failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // ── The plan, judged against the locked state ──
    let debts = Party::debts(&mut tx, party, party_id).await.map_err(|e| {
        tracing::error!(?e, "debt read failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let planned = plan(&debts, &body.manual, body.total)?;

    // ── The writes: the payment, its one ledger entry, and the settlements ──
    //
    // **The payment row is written even when nothing is planned.** A client who owes nothing
    // and hands over money leaves it as their credit balance, which `S3.4` says is a real
    // state — and refusing it would mean a cashier cannot record an advance.
    let signed = if party.direction() == "in" {
        body.total
    } else {
        -body.total
    };
    let payment: (Uuid, DateTime<Utc>, DateTime<Utc>) = sqlx::query_as(
        "INSERT INTO payments
            (party_kind, party_id, payment_method_id, total, direction, occurred_at, created_by)
         VALUES ($1, $2, $3, $4, $5, NOW(), 'counter')
         RETURNING id, occurred_at, created_at",
    )
    .bind(party.kind())
    .bind(party_id)
    .bind(body.payment_method_id)
    .bind(body.total)
    .bind(party.direction())
    .fetch_one(&mut *tx)
    .await
    .map_err(conflict_or_500)?;

    sqlx::query(
        "INSERT INTO money_entries
            (account_id, payment_method_id, document_kind, document_id, amount, occurred_at,
             created_by)
         SELECT pm.account_id, $1, 'payment', $2, $3, $4, 'counter'
           FROM payment_methods pm WHERE pm.id = $1",
    )
    .bind(body.payment_method_id)
    .bind(payment.0)
    .bind(signed)
    .bind(payment.1)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(?e, "collection money entry failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut applied = Vec::with_capacity(planned.len());
    let mut remaining: Vec<Decimal> = debts.iter().map(|debt| debt.outstanding).collect();
    for (index, amount, source) in planned {
        Party::settle(&mut tx, party, debts[index].id, payment.0, amount)
            .await
            // The ceilings are already judged above; a refusal here is a `409` because it can
            // only be one of them (a concurrent writer took the room), never a `500`.
            .map_err(conflict_or_500)?;
        remaining[index] -= amount;
        applied.push(AppliedRow {
            debt_id: debts[index].id,
            source_document_id: debts[index].source_document_id,
            amount,
            source,
            debt_outstanding: remaining[index],
        });
    }

    // ── The account, read by the same function the GET uses, in this same transaction ──
    //
    // **Not a second request** (`S6`): the panel replaces its state with this, so what it shows
    // and what it just wrote cannot describe different instants.
    let (name, owed, in_favour, documents) = parties::account_of(&mut tx, party, party_id)
        .await
        .map_err(|e| {
            tracing::error!(?e, "account read after collection failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        // The party was found above and its rows are locked, so a `None` here would mean it was
        // deleted mid-transaction — which cannot happen (there is no delete) and is a `500`
        // rather than a `404` if it somehow does.
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    let account = match party {
        Party::Customer => AccountPayload::Customer(parties::CustomerAccount {
            id: party_id,
            name,
            owed,
            in_favour,
            unpaid_sales: documents,
        }),
        Party::Supplier => AccountPayload::Supplier(parties::SupplierAccount {
            id: party_id,
            name,
            owed_to_them: owed,
            in_our_favour: in_favour,
            unpaid_purchases: documents,
        }),
    };

    tx.commit().await.map_err(|e| {
        tracing::error!(?e, "collection commit failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // **`unapplied` is the total less what was applied**, derived here from the plan rather than
    // re-read: the plan *is* what was written in this transaction.
    let applied_total: Decimal = applied.iter().map(|row| row.amount).sum();
    Ok(Collection {
        payment_id: payment.0,
        total: body.total,
        direction: party.direction().to_string(),
        applied,
        unapplied: body.total - applied_total,
        account,
    })
}

async fn collect(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<NewCollection>,
) -> Result<(StatusCode, Json<Collection>), StatusCode> {
    allocate(&state, Party::Customer, id, body)
        .await
        .map(|collection| (StatusCode::CREATED, Json(collection)))
}

async fn pay(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<NewCollection>,
) -> Result<(StatusCode, Json<Collection>), StatusCode> {
    allocate(&state, Party::Supplier, id, body)
        .await
        .map(|collection| (StatusCode::CREATED, Json(collection)))
}
