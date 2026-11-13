//! Currencies (a fixed list seeded by migration).

use saldo_core::{Currency, CurrencyCode};
use serde::Serialize;
use sqlx::{SqliteConnection, SqlitePool};

use crate::error::AppError;

/// A currency as `GET /currencies` returns it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct CurrencyView {
    pub code: String,
    pub name: String,
    pub symbol: String,
    pub decimal_places: u8,
}

/// Lists currencies. Not household-specific, so no ctx.
#[derive(Clone)]
pub struct CurrencyService {
    pool: SqlitePool,
}

impl CurrencyService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn list(&self) -> Result<Vec<CurrencyView>, AppError> {
        let rows =
            sqlx::query!("SELECT code, name, symbol, decimal_places FROM currencies ORDER BY code")
                .fetch_all(&self.pool)
                .await?;
        rows.into_iter()
            .map(|row| {
                Ok(CurrencyView {
                    code: row.code,
                    name: row.name,
                    symbol: row.symbol,
                    decimal_places: places(row.decimal_places)?,
                })
            })
            .collect()
    }
}

fn places(stored: i64) -> Result<u8, AppError> {
    u8::try_from(stored).map_err(|_| AppError::Internal(format!("bad decimal places {stored}")))
}

/// Looks up a currency by code; `field` names the input for the error.
///
/// `&mut SqliteConnection` is an *exclusive* borrow of one connection: while
/// this function holds it, nobody else can use the connection. It works for a
/// pooled connection and for an open transaction alike.
pub(crate) async fn find(
    conn: &mut SqliteConnection,
    code: &str,
    field: &str,
) -> Result<Currency, AppError> {
    let unknown = || AppError::invalid(field, format!("unknown currency {code:?}"));
    let parsed = CurrencyCode::new(code).map_err(|_| unknown())?;
    let row = sqlx::query!("SELECT decimal_places FROM currencies WHERE code = ?", code)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or_else(unknown)?;
    Currency::new(parsed, places(row.decimal_places)?)
        .map_err(|e| AppError::Internal(e.to_string()))
}

/// Whether [`ensure`] added the currency or found it already present.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ensured {
    Created,
    Unchanged,
}

/// Adds a currency unless it exists. A different number of decimal places
/// for an existing code is a conflict: amounts would be misread.
pub async fn ensure(
    conn: &mut SqliteConnection,
    currency: &CurrencyView,
) -> Result<Ensured, AppError> {
    let existing = sqlx::query_scalar!(
        "SELECT decimal_places FROM currencies WHERE code = ?",
        currency.code
    )
    .fetch_optional(&mut *conn)
    .await?;
    match existing {
        Some(places) if places == i64::from(currency.decimal_places) => Ok(Ensured::Unchanged),
        Some(places) => Err(AppError::Conflict(format!(
            "currency {} has {places} decimal places here but {} in the source",
            currency.code, currency.decimal_places
        ))),
        None => {
            let code = CurrencyCode::new(&currency.code)
                .map_err(|e| AppError::invalid("code", e.to_string()))?;
            Currency::new(code, currency.decimal_places)
                .map_err(|e| AppError::invalid("decimal_places", e.to_string()))?;
            let places = i64::from(currency.decimal_places);
            sqlx::query!(
                "INSERT INTO currencies (code, name, symbol, decimal_places) VALUES (?, ?, ?, ?)",
                currency.code,
                currency.name,
                currency.symbol,
                places,
            )
            .execute(&mut *conn)
            .await?;
            Ok(Ensured::Created)
        }
    }
}

/// Every currency, inside the caller's transaction (sees currencies an
/// import added moments ago).
pub async fn all_in(conn: &mut SqliteConnection) -> Result<Vec<CurrencyView>, AppError> {
    let rows =
        sqlx::query!("SELECT code, name, symbol, decimal_places FROM currencies ORDER BY code")
            .fetch_all(&mut *conn)
            .await?;
    rows.into_iter()
        .map(|row| {
            Ok(CurrencyView {
                code: row.code,
                name: row.name,
                symbol: row.symbol,
                decimal_places: places(row.decimal_places)?,
            })
        })
        .collect()
}
