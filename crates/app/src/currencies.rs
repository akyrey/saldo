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
