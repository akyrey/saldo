//! Moving a card transaction to the next month and back (decision 12),
//! including from the month before the plan into its first month.

use saldo_core::card_cycle::Cycle;
use saldo_core::{AccountId, SplitId};
use sqlx::SqliteConnection;

use super::CreditCardService;
use super::cycles::Card;
use super::repo;
use super::view::CardMoveView;
use crate::clock::format_timestamp;
use crate::ctx::{HouseholdCtx, Origin};
use crate::error::{AppError, parse_stored};
use crate::events::{NewEvent, append_event};
use crate::identity::today_in;

const MOVE_ENTITY: &str = "credit_card_move";

impl CreditCardService {
    /// Counts the split in the month after its date. Moving a split that is
    /// already moved changes nothing.
    pub async fn move_split(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        account_id: AccountId,
        split_id: SplitId,
    ) -> Result<(), AppError> {
        self.set_moved(ctx, origin, account_id, split_id, true)
            .await
    }

    /// Counts the split in its own month again.
    pub async fn unmove_split(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        account_id: AccountId,
        split_id: SplitId,
    ) -> Result<(), AppError> {
        self.set_moved(ctx, origin, account_id, split_id, false)
            .await
    }

    async fn set_moved(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        account_id: AccountId,
        split_id: SplitId,
        moved: bool,
    ) -> Result<(), AppError> {
        let mut tx = crate::db::begin(&self.pool).await?;
        let household_id = ctx.household_id.to_string();
        let card = Card::load(&mut tx, &household_id, &account_id.to_string()).await?;
        let split = repo::card_split(
            &mut tx,
            &household_id,
            &card.facts.id,
            &split_id.to_string(),
        )
        .await?
        .ok_or_else(|| AppError::invalid("split_id", "is not a purchase or refund on this card"))?;
        if split.moved == moved {
            return Ok(());
        }
        let own = Cycle::of(parse_stored(&split.date)?);
        let today = today_in(&mut tx, ctx, &*self.clock).await?;
        // The month just before the plan has no budget or payment, so a
        // move out of it into the plan's first month only needs that month
        // to be open.
        let affected = if own.next() == card.starts {
            vec![own.next()]
        } else {
            vec![own, own.next()]
        };
        for cycle in affected {
            if card.is_closed(cycle, today)? {
                return Err(AppError::Conflict(format!(
                    "{cycle} is closed: it is before the plan or its payment is booked"
                )));
            }
        }
        if moved {
            let now = format_timestamp(self.clock.now());
            repo::insert_move(&mut tx, &household_id, &card.facts.id, &split.id, &now).await?;
        } else {
            repo::delete_move(&mut tx, &household_id, &split.id).await?;
        }
        let (from, to) = if moved {
            (own, own.next())
        } else {
            (own.next(), own)
        };
        let view = CardMoveView {
            account_id: card.facts.id.clone(),
            split_id: split.id.clone(),
            from_cycle: from.to_string(),
            to_cycle: to.to_string(),
        };
        let verb = if moved { "created" } else { "deleted" };
        self.emit_move(&mut tx, ctx, origin, &view, verb).await?;
        tx.commit().await?;
        Ok(())
    }

    pub(super) async fn emit_move(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        view: &CardMoveView,
        verb: &str,
    ) -> Result<(), AppError> {
        let id = view.split_id.clone();
        let event = NewEvent::new(&*self.clock, MOVE_ENTITY, verb, id, view)?;
        append_event(conn, ctx, origin, event).await?;
        Ok(())
    }
}
