//! Goal condition: `goal:<id>` for an active goal reached and not yet
//! recorded as reached. Recording it (`reached_on`) ends the condition, so
//! a goal alerts once; changing its target lets it alert again.

use serde_json::to_value;

use super::Condition;
use crate::ctx::HouseholdCtx;
use crate::error::AppError;
use crate::goals::GoalService;

pub(super) const ENTITY: &str = "net_worth_goal";

pub(super) async fn conditions(
    goals: &GoalService,
    ctx: &HouseholdCtx,
) -> Result<Vec<Condition>, AppError> {
    let mut conditions = Vec::new();
    for goal in goals.list(ctx, Some(true)).await? {
        if !goal.progress.reached || goal.reached_on.is_some() {
            continue;
        }
        conditions.push(Condition {
            key: format!("goal:{}", goal.id),
            entity_type: ENTITY,
            action: "reached",
            entity_id: goal.id.clone(),
            data: to_value(&goal).map_err(|e| AppError::Internal(e.to_string()))?,
        });
    }
    Ok(conditions)
}
