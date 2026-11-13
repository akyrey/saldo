//! What a client sends for a recurrence, and the checks that turn it into rows.

use chrono::NaiveDate;
use saldo_core::{
    AccountId, BillId, BudgetId, CategoryId, Pattern, PiggyBankId, RecurrenceSchedule, Repetition,
    TagId, ValidationError, WeekendPolicy,
};
use serde_json::Map;
use sqlx::SqliteConnection;

use super::repo::{RecurrenceRow, RepetitionRow, TemplateSplitRow};
use crate::clock::{Clock, new_id};
use crate::ctx::{HouseholdCtx, Origin};
use crate::error::{AppError, parse_stored};
use crate::identity::required;
use crate::piggy_banks::repo as piggy_repo;
use crate::transactions::recurring::validate_template;
use crate::transactions::{SplitInput, TransactionInput};

/// A whole recurrence: `POST` creates one, `PUT` replaces one.
#[derive(Debug, Clone)]
pub struct RecurrenceInput {
    pub title: String,
    pub description: Option<String>,
    pub first_date: NaiveDate,
    pub repeat_until: Option<NaiveDate>,
    pub max_occurrences: Option<u32>,
    /// Default true. Stored now; the rule engine (built later) honours it.
    pub apply_rules: Option<bool>,
    pub active: Option<bool>,
    pub notes: Option<String>,
    pub repetitions: Vec<RepetitionInput>,
    pub splits: Vec<TemplateSplitInput>,
}

/// When the recurrence fires; see `saldo_core::Pattern` for `kind`/`moment`.
#[derive(Debug, Clone, Default)]
pub struct RepetitionInput {
    pub kind: String,
    pub moment: String,
    pub skip: u32,
    /// `keep` (default), `skip`, `previous_friday` or `next_monday`.
    pub weekend_policy: Option<String>,
}

/// One split of the transaction each firing creates.
#[derive(Debug, Clone)]
pub struct TemplateSplitInput {
    pub description: String,
    pub source_id: AccountId,
    pub destination_id: AccountId,
    pub amount: String,
    pub currency: Option<String>,
    pub foreign_amount: Option<String>,
    pub foreign_currency: Option<String>,
    pub category_id: Option<CategoryId>,
    pub budget_id: Option<BudgetId>,
    pub bill_id: Option<BillId>,
    /// A transfer into (out of) a linked account adds to (removes from) it.
    pub piggy_bank_id: Option<PiggyBankId>,
    pub tag_ids: Vec<TagId>,
}

/// Only the schedule, for previewing dates before saving.
#[derive(Debug, Clone)]
pub struct ScheduleInput {
    pub first_date: NaiveDate,
    pub repeat_until: Option<NaiveDate>,
    pub max_occurrences: Option<u32>,
    pub repetitions: Vec<RepetitionInput>,
}

/// The checked rows of a recurrence, ready to write.
pub(super) struct Prepared {
    pub row: RecurrenceRow,
    pub repetitions: Vec<RepetitionRow>,
    pub splits: Vec<TemplateSplitRow>,
}

/// Checks `input` and builds its rows. `row` carries the ID, household and
/// timestamps; its other fields are replaced.
pub(super) async fn prepare(
    conn: &mut SqliteConnection,
    clock: &dyn Clock,
    ctx: &HouseholdCtx,
    origin: Origin,
    row: RecurrenceRow,
    input: RecurrenceInput,
) -> Result<Prepared, AppError> {
    let schedule = ScheduleInput {
        first_date: input.first_date,
        repeat_until: input.repeat_until,
        max_occurrences: input.max_occurrences,
        repetitions: input.repetitions.clone(),
    };
    let parsed = schedule_of(&schedule)?;
    let title = required("title", &input.title)?;
    let repetitions = parsed
        .repetitions
        .iter()
        .map(|r| RepetitionRow {
            id: new_id(clock).to_string(),
            kind: r.pattern.kind().to_string(),
            moment: r.pattern.moment(),
            skip: i64::from(r.skip),
            weekend_policy: r.weekend.as_str().to_string(),
        })
        .collect();
    let splits = template_rows(
        conn,
        clock,
        ctx,
        origin,
        &title,
        input.first_date,
        input.splits,
    )
    .await?;
    let row = RecurrenceRow {
        title,
        description: input.description.filter(|d| !d.trim().is_empty()),
        first_date: input.first_date.to_string(),
        repeat_until: input.repeat_until.map(|d| d.to_string()),
        max_occurrences: input.max_occurrences.map(i64::from),
        apply_rules: input.apply_rules.unwrap_or(true),
        active: input.active.unwrap_or(true),
        notes: input.notes,
        ..row
    };
    Ok(Prepared {
        row,
        repetitions,
        splits,
    })
}

/// Parses and checks a schedule; errors name the field, for example
/// `repetitions[0].moment`.
pub(super) fn schedule_of(input: &ScheduleInput) -> Result<RecurrenceSchedule, AppError> {
    if input
        .repeat_until
        .is_some_and(|until| until < input.first_date)
    {
        return Err(AppError::invalid(
            "repeat_until",
            "must not be before the first date",
        ));
    }
    if input.max_occurrences == Some(0) {
        return Err(AppError::invalid("max_occurrences", "must be at least 1"));
    }
    if input.repetitions.is_empty() {
        return Err(AppError::invalid(
            "repetitions",
            "must contain at least one repetition",
        ));
    }
    let mut repetitions = Vec::with_capacity(input.repetitions.len());
    for (index, r) in input.repetitions.iter().enumerate() {
        let field = |name: &str| format!("repetitions[{index}].{name}");
        let pattern = Pattern::parse(r.kind.trim(), &r.moment)
            .map_err(|e| AppError::invalid(field("moment"), e.to_string()))?;
        let weekend = WeekendPolicy::parse(r.weekend_policy.as_deref().unwrap_or("keep"))
            .map_err(|e| AppError::invalid(field("weekend_policy"), e.to_string()))?;
        repetitions.push(Repetition {
            pattern,
            skip: r.skip,
            weekend,
        });
    }
    Ok(RecurrenceSchedule {
        first_date: input.first_date,
        repeat_until: input.repeat_until,
        max_occurrences: input.max_occurrences,
        repetitions,
    })
}

/// The schedule of stored rows (a parse failure means a corrupt row).
pub(super) fn stored_schedule(
    row: &RecurrenceRow,
    repetitions: &[RepetitionRow],
) -> Result<RecurrenceSchedule, AppError> {
    let corrupt = |e: String| AppError::Internal(format!("corrupt recurrence {}: {e}", row.id));
    let mut parsed = Vec::with_capacity(repetitions.len());
    for r in repetitions {
        parsed.push(Repetition {
            pattern: Pattern::parse(&r.kind, &r.moment).map_err(|e| corrupt(e.to_string()))?,
            skip: u32::try_from(r.skip).map_err(|e| corrupt(e.to_string()))?,
            weekend: WeekendPolicy::parse(&r.weekend_policy).map_err(|e| corrupt(e.to_string()))?,
        });
    }
    Ok(RecurrenceSchedule {
        first_date: parse_stored(&row.first_date)?,
        repeat_until: row.repeat_until.as_deref().map(parse_stored).transpose()?,
        max_occurrences: row
            .max_occurrences
            .map(|m| u32::try_from(m).map_err(|e| corrupt(e.to_string())))
            .transpose()?,
        repetitions: parsed,
    })
}

/// Validates the template splits with the ledger rules (as a group dated on
/// the first date) and turns them into rows with their parsed amounts.
async fn template_rows(
    conn: &mut SqliteConnection,
    clock: &dyn Clock,
    ctx: &HouseholdCtx,
    origin: Origin,
    title: &str,
    first_date: NaiveDate,
    splits: Vec<TemplateSplitInput>,
) -> Result<Vec<TemplateSplitRow>, AppError> {
    if splits.is_empty() {
        return Err(AppError::Validation(vec![ValidationError::EmptyGroup]));
    }
    let household_id = ctx.household_id.to_string();
    for (index, split) in splits.iter().enumerate() {
        if let Some(piggy) = split.piggy_bank_id
            && !piggy_repo::exists(&mut *conn, &household_id, &piggy.to_string()).await?
        {
            return Err(AppError::invalid(
                format!("splits[{index}].piggy_bank_id"),
                "unknown piggy bank",
            ));
        }
    }
    let group = TransactionInput {
        title: Some(title.to_string()),
        splits: splits
            .iter()
            .map(|s| as_split_input(s, first_date))
            .collect(),
    };
    let validated = validate_template(&mut *conn, ctx, origin, group).await?;
    let mut rows = Vec::with_capacity(splits.len());
    // `zip` walks the two lists side by side, like a Go loop over two slices by index.
    for (position, (input, checked)) in splits.into_iter().zip(validated.splits()).enumerate() {
        let split = checked.split();
        let tags: Vec<String> = split.tags.iter().map(ToString::to_string).collect();
        rows.push(TemplateSplitRow {
            id: new_id(clock).to_string(),
            position: i64::try_from(position)
                .map_err(|_| AppError::Internal("too many splits".into()))?,
            description: split.description.clone(),
            source_account_id: split.source.to_string(),
            destination_account_id: split.destination.to_string(),
            amount_minor: split.amount.minor(),
            currency_code: split.amount.currency().to_string(),
            foreign_amount_minor: split.foreign_amount.map(|m| m.minor()),
            foreign_currency_code: split.foreign_amount.map(|m| m.currency().to_string()),
            category_id: split.category.map(|c| c.to_string()),
            budget_id: split.budget.map(|b| b.to_string()),
            bill_id: split.bill.map(|b| b.to_string()),
            piggy_bank_id: input.piggy_bank_id.map(|p| p.to_string()),
            tags: serde_json::to_string(&tags)
                .map_err(|e| AppError::Internal(format!("cannot encode tags: {e}")))?,
        });
    }
    Ok(rows)
}

fn as_split_input(split: &TemplateSplitInput, date: NaiveDate) -> SplitInput {
    SplitInput {
        date,
        description: split.description.clone(),
        source_id: split.source_id,
        destination_id: split.destination_id,
        amount: split.amount.clone(),
        currency: split.currency.clone(),
        foreign_amount: split.foreign_amount.clone(),
        foreign_currency: split.foreign_currency.clone(),
        category_id: split.category_id,
        budget_id: split.budget_id,
        bill_id: split.bill_id,
        tag_ids: split.tag_ids.clone(),
        reconciled: false,
        notes: None,
        meta: Map::new(),
    }
}
