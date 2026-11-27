//! Reading a file with a mapping that is not saved yet, for the mapping
//! editor (Phase 13, Track B). Nothing is stored.

use serde::Serialize;

use super::StatementService;
use super::imports::{statement_currency, stored_format};
use super::parse::{Mapping, ParsedRow, parse_statement};
use super::summary::reason_text;
use crate::ctx::HouseholdCtx;
use crate::currencies;
use crate::error::AppError;
use crate::identity;

/// How many lines a preview shows.
const PREVIEW_LINES: usize = 20;

#[derive(Debug, Clone, PartialEq, Serialize, utoipa::ToSchema)]
pub struct PreviewLine {
    pub row: usize,
    pub accounting_date: String,
    pub value_date: String,
    pub booked_date: String,
    pub amount: String,
    pub description: String,
    pub category: Option<String>,
    pub counterparty: Option<String>,
    pub external_id: Option<String>,
    pub notes: Vec<String>,
    /// `"Meal Vouchers: -8.00"` for each part.
    pub parts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, utoipa::ToSchema)]
pub struct UnsupportedRow {
    pub row: usize,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, utoipa::ToSchema)]
pub struct PreviewView {
    pub currency: String,
    /// The first lines of the file.
    pub lines: Vec<PreviewLine>,
    pub total_lines: usize,
    /// Every row that would not be imported, with the reason.
    pub unsupported: Vec<UnsupportedRow>,
    pub opening_balance: Option<String>,
    pub closing_balance: Option<String>,
}

impl StatementService {
    /// Reads `bytes` with `mapping` in the currency of `account_id` (or the
    /// household's primary currency) and returns what an import would see.
    pub async fn preview(
        &self,
        ctx: &HouseholdCtx,
        format: &str,
        mapping: Mapping,
        account_id: Option<String>,
        bytes: &[u8],
    ) -> Result<PreviewView, AppError> {
        let format = stored_format(format)
            .ok_or_else(|| AppError::invalid("format", "must be csv or xlsx"))?;
        mapping
            .validate()
            .map_err(|error| AppError::invalid("mapping", error.to_string()))?;
        let mut conn = self.pool.acquire().await?;
        let currency = match account_id {
            Some(account) => {
                statement_currency(&mut conn, &ctx.household_id.to_string(), &account).await?
            }
            None => {
                let code = identity::primary_currency(&mut conn, ctx).await?;
                currencies::find(&mut conn, &code, "currency").await?
            }
        };
        let parsed = parse_statement(format, bytes, &mapping, &currency)
            .map_err(|error| AppError::invalid("file", error.to_string()))?;
        let money = |m: saldo_core::Money| m.to_decimal_string(&currency);
        let mut lines = Vec::new();
        let mut unsupported = Vec::new();
        let mut total_lines = 0;
        for row in parsed.rows {
            match row {
                ParsedRow::Line(line) => {
                    total_lines += 1;
                    if lines.len() < PREVIEW_LINES {
                        lines.push(PreviewLine {
                            row: line.row,
                            accounting_date: line.accounting_date.to_string(),
                            value_date: line.value_date.to_string(),
                            booked_date: line.booked_date.to_string(),
                            amount: money(line.amount),
                            description: line.description,
                            category: line.category,
                            counterparty: line.counterparty,
                            external_id: line.external_id,
                            notes: line.notes,
                            parts: line
                                .parts
                                .iter()
                                .map(|p| format!("{}: {}", p.column, money(p.amount)))
                                .collect(),
                        });
                    }
                }
                ParsedRow::Unsupported { row, reason } => unsupported.push(UnsupportedRow {
                    row,
                    reason: reason_text(&reason),
                }),
            }
        }
        Ok(PreviewView {
            currency: currency.code().to_string(),
            lines,
            total_lines,
            unsupported,
            opening_balance: parsed.opening_balance.map(money),
            closing_balance: parsed.closing_balance.map(money),
        })
    }
}
