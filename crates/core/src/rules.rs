//! Rules: conditions on a split and the changes to make when they match
//! (ARCHITECTURE.md 4.3). Pure: evaluating and applying never touch storage.

use crate::currency::CurrencyCode;
use crate::ids::{AccountId, BillId, BudgetId, CategoryId, TagId};
use crate::kind::SplitKind;

/// An amount written in a rule, such as `700` or `12.50`, kept exact as
/// `units / 10^scale`. It is compared with split amounts in any currency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuleAmount {
    pub units: i64,
    pub scale: u8,
}

/// A condition on one split. Enum variants carry their data: Go would need an
/// interface with one struct per case, and PHP enums cannot hold per-case data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trigger {
    DescriptionContains(String),
    DescriptionStarts(String),
    DescriptionEnds(String),
    DescriptionIs(String),
    NotesContain(String),
    AmountLess(RuleAmount),
    AmountMore(RuleAmount),
    AmountExactly(RuleAmount),
    CurrencyIs(CurrencyCode),
    CategoryIs(CategoryId),
    HasNoCategory,
    HasAnyCategory,
    TagIs(TagId),
    HasNoTag,
    HasAnyTag,
    SourceAccountIs(AccountId),
    DestinationAccountIs(AccountId),
    KindIs(SplitKind),
}

/// A change to one split.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    SetCategory(CategoryId),
    ClearCategory,
    SetBudget(BudgetId),
    ClearBudget,
    AddTag(TagId),
    RemoveTag(TagId),
    RemoveAllTags,
    SetDescription(String),
    SetNotes(String),
    LinkToBill(BillId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleTrigger {
    pub trigger: Trigger,
    /// Match when the condition is *not* true.
    pub negate: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleAction {
    pub action: Action,
    /// Run no later actions of this rule.
    pub stop_processing: bool,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub triggers: Vec<RuleTrigger>,
    pub actions: Vec<RuleAction>,
    /// All triggers must match (`true`) or any one is enough (`false`).
    pub strict: bool,
}

/// What a rule sees of a split.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitView {
    pub description: String,
    pub notes: Option<String>,
    pub amount_minor: i64,
    pub currency: CurrencyCode,
    pub decimal_places: u8,
    pub kind: SplitKind,
    pub source: AccountId,
    pub destination: AccountId,
    pub category: Option<CategoryId>,
    pub budget: Option<BudgetId>,
    pub bill: Option<BillId>,
    pub tags: Vec<TagId>,
}

/// A trigger or action that could not be read from its stored form.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RuleError {
    #[error("unknown {0} kind {1:?}")]
    UnknownKind(&'static str, String),
    #[error("{kind}: bad value {value:?}")]
    BadValue { kind: String, value: String },
}

impl RuleAmount {
    /// Parses `"700"`, `"12.50"` or `"-3"`.
    pub fn parse(text: &str) -> Option<Self> {
        const MAX_SCALE: usize = 18;
        let (negative, digits) = match text.trim().strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, text.trim()),
        };
        let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
        let all_digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
        if whole.is_empty()
            || !all_digits(whole)
            || !all_digits(fraction)
            || fraction.len() > MAX_SCALE
        {
            return None;
        }
        let units: i64 = format!("{whole}{fraction}").parse().ok()?;
        let scale = u8::try_from(fraction.len()).ok()?;
        Some(Self {
            units: if negative { -units } else { units },
            scale,
        })
    }

    /// -1, 0 or 1 as `split_minor / 10^places` is below, equal or above this
    /// amount; cross-multiplied in `i128`, so no precision is lost.
    fn compare(&self, split_minor: i64, places: u8) -> std::cmp::Ordering {
        let split = i128::from(split_minor) * 10_i128.pow(u32::from(self.scale));
        let rule = i128::from(self.units) * 10_i128.pow(u32::from(places));
        split.cmp(&rule)
    }

    fn text(&self) -> String {
        let digits = self.units.unsigned_abs().to_string();
        let scale = usize::from(self.scale);
        let sign = if self.units < 0 { "-" } else { "" };
        if scale == 0 {
            return format!("{sign}{digits}");
        }
        let padded = format!("{digits:0>width$}", width = scale + 1);
        let (whole, fraction) = padded.split_at(padded.len() - scale);
        format!("{sign}{whole}.{fraction}")
    }
}

fn kind_name(kind: SplitKind) -> &'static str {
    match kind {
        SplitKind::Withdrawal => "withdrawal",
        SplitKind::Deposit => "deposit",
        SplitKind::Transfer => "transfer",
        SplitKind::OpeningBalance => "opening_balance",
        SplitKind::Reconciliation => "reconciliation",
    }
}

/// Saldo's names and Firefly's (`opening balance` with a space).
fn parse_kind(text: &str) -> Option<SplitKind> {
    match text.trim().to_lowercase().replace(' ', "_").as_str() {
        "withdrawal" => Some(SplitKind::Withdrawal),
        "deposit" => Some(SplitKind::Deposit),
        "transfer" => Some(SplitKind::Transfer),
        "opening_balance" => Some(SplitKind::OpeningBalance),
        "reconciliation" => Some(SplitKind::Reconciliation),
        _ => None,
    }
}

/// Reads one stored value. `F: FnOnce(&str) -> Option<T>` is a closure called
/// at most once (Go: a `func(string) (T, bool)` argument).
fn read<T, F: FnOnce(&str) -> Option<T>>(
    kind: &str,
    value: Option<&str>,
    parse: F,
) -> Result<T, RuleError> {
    let text = value.unwrap_or_default();
    parse(text).ok_or_else(|| RuleError::BadValue {
        kind: kind.to_string(),
        value: text.to_string(),
    })
}

fn non_empty(text: &str) -> Option<String> {
    (!text.trim().is_empty()).then(|| text.to_string())
}

impl Trigger {
    /// The stored `kind`, matching Firefly's keyword.
    pub fn kind(&self) -> &'static str {
        match self {
            Trigger::DescriptionContains(_) => "description_contains",
            Trigger::DescriptionStarts(_) => "description_starts",
            Trigger::DescriptionEnds(_) => "description_ends",
            Trigger::DescriptionIs(_) => "description_is",
            Trigger::NotesContain(_) => "notes_contains",
            Trigger::AmountLess(_) => "amount_less",
            Trigger::AmountMore(_) => "amount_more",
            Trigger::AmountExactly(_) => "amount_exactly",
            Trigger::CurrencyIs(_) => "currency_is",
            Trigger::CategoryIs(_) => "category_is",
            Trigger::HasNoCategory => "has_no_category",
            Trigger::HasAnyCategory => "has_any_category",
            Trigger::TagIs(_) => "tag_is",
            Trigger::HasNoTag => "has_no_tag",
            Trigger::HasAnyTag => "has_any_tag",
            Trigger::SourceAccountIs(_) => "source_account_is",
            Trigger::DestinationAccountIs(_) => "destination_account_is",
            Trigger::KindIs(_) => "transaction_type",
        }
    }

    /// The stored `value` (`None` for triggers without one).
    pub fn value(&self) -> Option<String> {
        // Several patterns share one arm with `|` when they bind the same
        // variable: PHP array destructuring with the conditions built in.
        match self {
            Trigger::DescriptionContains(text)
            | Trigger::DescriptionStarts(text)
            | Trigger::DescriptionEnds(text)
            | Trigger::DescriptionIs(text)
            | Trigger::NotesContain(text) => Some(text.clone()),
            Trigger::AmountLess(amount)
            | Trigger::AmountMore(amount)
            | Trigger::AmountExactly(amount) => Some(amount.text()),
            Trigger::CurrencyIs(code) => Some(code.to_string()),
            Trigger::CategoryIs(id) => Some(id.to_string()),
            Trigger::TagIs(id) => Some(id.to_string()),
            Trigger::SourceAccountIs(id) | Trigger::DestinationAccountIs(id) => {
                Some(id.to_string())
            }
            Trigger::KindIs(kind) => Some(kind_name(*kind).to_string()),
            Trigger::HasNoCategory
            | Trigger::HasAnyCategory
            | Trigger::HasNoTag
            | Trigger::HasAnyTag => None,
        }
    }

    pub fn parse(kind: &str, value: Option<&str>) -> Result<Self, RuleError> {
        let text = |v: Option<&str>| read(kind, v, non_empty);
        let amount = |v: Option<&str>| read(kind, v, RuleAmount::parse);
        let id = |v: Option<&str>| read(kind, v, |t| t.parse::<uuid::Uuid>().ok());
        Ok(match kind {
            "description_contains" => Trigger::DescriptionContains(text(value)?),
            "description_starts" => Trigger::DescriptionStarts(text(value)?),
            "description_ends" => Trigger::DescriptionEnds(text(value)?),
            "description_is" => Trigger::DescriptionIs(text(value)?),
            "notes_contains" => Trigger::NotesContain(text(value)?),
            "amount_less" => Trigger::AmountLess(amount(value)?),
            "amount_more" => Trigger::AmountMore(amount(value)?),
            "amount_exactly" => Trigger::AmountExactly(amount(value)?),
            "currency_is" => Trigger::CurrencyIs(read(kind, value, |t| CurrencyCode::new(t).ok())?),
            "category_is" => Trigger::CategoryIs(CategoryId(id(value)?)),
            "has_no_category" => Trigger::HasNoCategory,
            "has_any_category" => Trigger::HasAnyCategory,
            "tag_is" => Trigger::TagIs(TagId(id(value)?)),
            "has_no_tag" => Trigger::HasNoTag,
            "has_any_tag" => Trigger::HasAnyTag,
            "source_account_is" => Trigger::SourceAccountIs(AccountId(id(value)?)),
            "destination_account_is" => Trigger::DestinationAccountIs(AccountId(id(value)?)),
            "transaction_type" => Trigger::KindIs(read(kind, value, parse_kind)?),
            other => return Err(RuleError::UnknownKind("trigger", other.to_string())),
        })
    }

    fn matches(&self, split: &SplitView) -> bool {
        let description = split.description.to_lowercase();
        let lower = |text: &String| text.to_lowercase();
        let compare =
            |amount: &RuleAmount| amount.compare(split.amount_minor, split.decimal_places);
        match self {
            Trigger::DescriptionContains(text) => description.contains(&lower(text)),
            Trigger::DescriptionStarts(text) => description.starts_with(&lower(text)),
            Trigger::DescriptionEnds(text) => description.ends_with(&lower(text)),
            Trigger::DescriptionIs(text) => description == lower(text),
            Trigger::NotesContain(text) => split
                .notes
                .as_ref()
                .is_some_and(|n| n.to_lowercase().contains(&lower(text))),
            Trigger::AmountLess(amount) => compare(amount).is_lt(),
            Trigger::AmountMore(amount) => compare(amount).is_gt(),
            Trigger::AmountExactly(amount) => compare(amount).is_eq(),
            Trigger::CurrencyIs(code) => split.currency == *code,
            Trigger::CategoryIs(id) => split.category == Some(*id),
            Trigger::HasNoCategory => split.category.is_none(),
            Trigger::HasAnyCategory => split.category.is_some(),
            Trigger::TagIs(id) => split.tags.contains(id),
            Trigger::HasNoTag => split.tags.is_empty(),
            Trigger::HasAnyTag => !split.tags.is_empty(),
            Trigger::SourceAccountIs(id) => split.source == *id,
            Trigger::DestinationAccountIs(id) => split.destination == *id,
            Trigger::KindIs(kind) => split.kind == *kind,
        }
    }
}

impl Action {
    pub fn kind(&self) -> &'static str {
        match self {
            Action::SetCategory(_) => "set_category",
            Action::ClearCategory => "clear_category",
            Action::SetBudget(_) => "set_budget",
            Action::ClearBudget => "clear_budget",
            Action::AddTag(_) => "add_tag",
            Action::RemoveTag(_) => "remove_tag",
            Action::RemoveAllTags => "remove_all_tags",
            Action::SetDescription(_) => "set_description",
            Action::SetNotes(_) => "set_notes",
            Action::LinkToBill(_) => "link_to_bill",
        }
    }

    pub fn value(&self) -> Option<String> {
        match self {
            Action::SetCategory(id) => Some(id.to_string()),
            Action::SetBudget(id) => Some(id.to_string()),
            Action::AddTag(id) | Action::RemoveTag(id) => Some(id.to_string()),
            Action::SetDescription(text) | Action::SetNotes(text) => Some(text.clone()),
            Action::LinkToBill(id) => Some(id.to_string()),
            Action::ClearCategory | Action::ClearBudget | Action::RemoveAllTags => None,
        }
    }

    pub fn parse(kind: &str, value: Option<&str>) -> Result<Self, RuleError> {
        let id = |v: Option<&str>| read(kind, v, |t| t.parse::<uuid::Uuid>().ok());
        let text = |v: Option<&str>| read(kind, v, non_empty);
        Ok(match kind {
            "set_category" => Action::SetCategory(CategoryId(id(value)?)),
            "clear_category" => Action::ClearCategory,
            "set_budget" => Action::SetBudget(BudgetId(id(value)?)),
            "clear_budget" => Action::ClearBudget,
            "add_tag" => Action::AddTag(TagId(id(value)?)),
            "remove_tag" => Action::RemoveTag(TagId(id(value)?)),
            "remove_all_tags" => Action::RemoveAllTags,
            "set_description" => Action::SetDescription(text(value)?),
            "set_notes" => Action::SetNotes(text(value)?),
            "link_to_bill" => Action::LinkToBill(BillId(id(value)?)),
            other => return Err(RuleError::UnknownKind("action", other.to_string())),
        })
    }
}

/// Every trigger kind Saldo understands.
pub const TRIGGER_KINDS: [&str; 18] = [
    "description_contains",
    "description_starts",
    "description_ends",
    "description_is",
    "notes_contains",
    "amount_less",
    "amount_more",
    "amount_exactly",
    "currency_is",
    "category_is",
    "has_no_category",
    "has_any_category",
    "tag_is",
    "has_no_tag",
    "has_any_tag",
    "source_account_is",
    "destination_account_is",
    "transaction_type",
];

/// Every action kind Saldo understands.
pub const ACTION_KINDS: [&str; 10] = [
    "set_category",
    "clear_category",
    "set_budget",
    "clear_budget",
    "add_tag",
    "remove_tag",
    "remove_all_tags",
    "set_description",
    "set_notes",
    "link_to_bill",
];

/// Whether the rule's triggers match the split.
pub fn matches(rule: &Rule, split: &SplitView) -> bool {
    if rule.triggers.is_empty() {
        return false;
    }
    let hit = |t: &RuleTrigger| t.trigger.matches(split) != t.negate;
    if rule.strict {
        rule.triggers.iter().all(hit)
    } else {
        rule.triggers.iter().any(hit)
    }
}

/// The actions to run on `split`: none if the triggers do not match, else the
/// active actions in order up to the first that stops processing.
pub fn evaluate(rule: &Rule, split: &SplitView) -> Vec<Action> {
    if !matches(rule, split) {
        return Vec::new();
    }
    let mut actions = Vec::new();
    for step in rule.actions.iter().filter(|a| a.active) {
        actions.push(step.action.clone());
        if step.stop_processing {
            break;
        }
    }
    actions
}

/// A copy of `split` with `actions` applied; the input is left untouched.
pub fn apply(split: &SplitView, actions: &[Action]) -> SplitView {
    // `fold` threads a new copy through every action: the original is never
    // changed, like chaining immutable `with...()` calls on a PHP value object.
    actions
        .iter()
        .fold(split.clone(), |current, action| match action {
            Action::SetCategory(id) => SplitView {
                category: Some(*id),
                ..current
            },
            Action::ClearCategory => SplitView {
                category: None,
                ..current
            },
            Action::SetBudget(id) => SplitView {
                budget: Some(*id),
                ..current
            },
            Action::ClearBudget => SplitView {
                budget: None,
                ..current
            },
            Action::AddTag(id) if current.tags.contains(id) => current,
            Action::AddTag(id) => {
                let tags = current.tags.iter().copied().chain([*id]).collect();
                SplitView { tags, ..current }
            }
            Action::RemoveTag(id) => {
                let tags = current.tags.iter().copied().filter(|t| t != id).collect();
                SplitView { tags, ..current }
            }
            Action::RemoveAllTags => SplitView {
                tags: Vec::new(),
                ..current
            },
            Action::SetDescription(text) => SplitView {
                description: text.clone(),
                ..current
            },
            Action::SetNotes(text) => SplitView {
                notes: Some(text.clone()),
                ..current
            },
            Action::LinkToBill(id) => SplitView {
                bill: Some(*id),
                ..current
            },
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn eur() -> CurrencyCode {
        CurrencyCode::new("EUR").unwrap()
    }

    fn split() -> SplitView {
        SplitView {
            description: "Rent November".into(),
            notes: Some("paid by transfer".into()),
            amount_minor: 80_000,
            currency: eur(),
            decimal_places: 2,
            kind: SplitKind::Withdrawal,
            source: AccountId(Uuid::from_u128(1)),
            destination: AccountId(Uuid::from_u128(2)),
            category: None,
            budget: None,
            bill: None,
            tags: vec![TagId(Uuid::from_u128(30))],
        }
    }

    fn trigger(trigger: Trigger) -> RuleTrigger {
        RuleTrigger {
            trigger,
            negate: false,
        }
    }

    fn action(action: Action) -> RuleAction {
        RuleAction {
            action,
            stop_processing: false,
            active: true,
        }
    }

    fn amount(text: &str) -> RuleAmount {
        RuleAmount::parse(text).unwrap()
    }

    #[test]
    fn rule_amounts_parse_exactly() {
        assert_eq!(
            amount("700"),
            RuleAmount {
                units: 700,
                scale: 0
            }
        );
        assert_eq!(
            amount("12.50"),
            RuleAmount {
                units: 1250,
                scale: 2
            }
        );
        assert_eq!(
            amount("-3.1"),
            RuleAmount {
                units: -31,
                scale: 1
            }
        );
        for bad in ["", "abc", "1,5", "1e3", "."] {
            assert_eq!(RuleAmount::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn text_triggers_ignore_case() {
        let s = split();
        assert!(Trigger::DescriptionContains("RENT".into()).matches(&s));
        assert!(Trigger::DescriptionStarts("rent n".into()).matches(&s));
        assert!(Trigger::DescriptionEnds("NOVEMBER".into()).matches(&s));
        assert!(Trigger::DescriptionIs("rent november".into()).matches(&s));
        assert!(!Trigger::DescriptionIs("rent".into()).matches(&s));
        assert!(Trigger::NotesContain("Transfer".into()).matches(&s));
        assert!(!Trigger::NotesContain("x".into()).matches(&SplitView { notes: None, ..s }));
    }

    #[test]
    fn amount_triggers_compare_across_decimal_places() {
        let s = split(); // 800.00 EUR
        assert!(Trigger::AmountMore(amount("700")).matches(&s));
        assert!(!Trigger::AmountMore(amount("800")).matches(&s));
        assert!(Trigger::AmountLess(amount("800.01")).matches(&s));
        assert!(Trigger::AmountExactly(amount("800.000")).matches(&s));
        // A 0-decimal currency: 800 units.
        let tickets = SplitView {
            amount_minor: 800,
            decimal_places: 0,
            ..split()
        };
        assert!(Trigger::AmountExactly(amount("800")).matches(&tickets));
        assert!(Trigger::AmountLess(amount("800.5")).matches(&tickets));
    }

    #[test]
    fn reference_and_kind_triggers() {
        let s = split();
        let category = CategoryId(Uuid::from_u128(9));
        assert!(Trigger::CurrencyIs(eur()).matches(&s));
        assert!(Trigger::HasNoCategory.matches(&s));
        assert!(!Trigger::HasAnyCategory.matches(&s));
        let categorised = SplitView {
            category: Some(category),
            ..split()
        };
        assert!(Trigger::CategoryIs(category).matches(&categorised));
        assert!(Trigger::HasAnyCategory.matches(&categorised));
        assert!(Trigger::TagIs(TagId(Uuid::from_u128(30))).matches(&s));
        assert!(Trigger::HasAnyTag.matches(&s));
        assert!(!Trigger::HasNoTag.matches(&s));
        assert!(Trigger::SourceAccountIs(AccountId(Uuid::from_u128(1))).matches(&s));
        assert!(Trigger::DestinationAccountIs(AccountId(Uuid::from_u128(2))).matches(&s));
        assert!(Trigger::KindIs(SplitKind::Withdrawal).matches(&s));
        assert!(!Trigger::KindIs(SplitKind::Deposit).matches(&s));
    }

    #[test]
    fn strict_negate_and_any() {
        let s = split();
        let strict = Rule {
            triggers: vec![
                trigger(Trigger::DescriptionContains("rent".into())),
                trigger(Trigger::AmountMore(amount("900"))),
            ],
            actions: vec![],
            strict: true,
        };
        assert!(!matches(&strict, &s));
        assert!(matches(
            &Rule {
                strict: false,
                ..strict.clone()
            },
            &s
        ));
        let negated = Rule {
            triggers: vec![RuleTrigger {
                trigger: Trigger::AmountMore(amount("900")),
                negate: true,
            }],
            actions: vec![],
            strict: true,
        };
        assert!(matches(&negated, &s));
        let empty = Rule {
            triggers: vec![],
            actions: vec![],
            strict: true,
        };
        assert!(
            !matches(&empty, &s),
            "a rule without triggers never matches"
        );
    }

    #[test]
    fn evaluate_returns_active_actions_until_stop() {
        let bill = BillId(Uuid::from_u128(5));
        let rule = Rule {
            triggers: vec![trigger(Trigger::DescriptionContains("rent".into()))],
            actions: vec![
                action(Action::LinkToBill(bill)),
                RuleAction {
                    action: Action::ClearCategory,
                    stop_processing: false,
                    active: false,
                },
                RuleAction {
                    action: Action::RemoveAllTags,
                    stop_processing: true,
                    active: true,
                },
                action(Action::SetNotes("never".into())),
            ],
            strict: true,
        };
        assert_eq!(
            evaluate(&rule, &split()),
            vec![Action::LinkToBill(bill), Action::RemoveAllTags]
        );
        let other = SplitView {
            description: "Groceries".into(),
            ..split()
        };
        assert!(evaluate(&rule, &other).is_empty());
    }

    #[test]
    fn apply_returns_a_changed_copy() {
        let original = split();
        let category = CategoryId(Uuid::from_u128(9));
        let tag = TagId(Uuid::from_u128(31));
        let budget = BudgetId(Uuid::from_u128(40));
        let changed = apply(
            &original,
            &[
                Action::SetCategory(category),
                Action::AddTag(tag),
                Action::AddTag(tag),
                Action::RemoveTag(TagId(Uuid::from_u128(30))),
                Action::SetBudget(budget),
                Action::SetDescription("Rent".into()),
                Action::SetNotes("auto".into()),
                Action::LinkToBill(BillId(Uuid::from_u128(5))),
            ],
        );
        assert_eq!(changed.category, Some(category));
        assert_eq!(changed.tags, vec![tag], "no duplicates; removed tag gone");
        assert_eq!(changed.budget, Some(budget));
        assert_eq!(
            (changed.description.as_str(), changed.notes.as_deref()),
            ("Rent", Some("auto"))
        );
        assert_eq!(changed.bill, Some(BillId(Uuid::from_u128(5))));
        assert_eq!(original, split(), "the input is unchanged");
        let cleared = apply(
            &changed,
            &[
                Action::ClearCategory,
                Action::ClearBudget,
                Action::RemoveAllTags,
            ],
        );
        assert_eq!(
            (cleared.category, cleared.budget, cleared.tags.len()),
            (None, None, 0)
        );
    }

    #[test]
    fn every_trigger_and_action_round_trips_through_kind_and_value() {
        let id = Uuid::from_u128(7);
        let triggers = [
            Trigger::DescriptionContains("a".into()),
            Trigger::DescriptionStarts("a".into()),
            Trigger::DescriptionEnds("a".into()),
            Trigger::DescriptionIs("a".into()),
            Trigger::NotesContain("a".into()),
            Trigger::AmountLess(amount("1.5")),
            Trigger::AmountMore(amount("2")),
            Trigger::AmountExactly(amount("3.25")),
            Trigger::CurrencyIs(eur()),
            Trigger::CategoryIs(CategoryId(id)),
            Trigger::HasNoCategory,
            Trigger::HasAnyCategory,
            Trigger::TagIs(TagId(id)),
            Trigger::HasNoTag,
            Trigger::HasAnyTag,
            Trigger::SourceAccountIs(AccountId(id)),
            Trigger::DestinationAccountIs(AccountId(id)),
            Trigger::KindIs(SplitKind::Transfer),
        ];
        assert_eq!(triggers.len(), TRIGGER_KINDS.len());
        for t in triggers {
            assert!(TRIGGER_KINDS.contains(&t.kind()), "{}", t.kind());
            assert_eq!(
                Trigger::parse(t.kind(), t.value().as_deref()),
                Ok(t.clone())
            );
        }
        let actions = [
            Action::SetCategory(CategoryId(id)),
            Action::ClearCategory,
            Action::SetBudget(BudgetId(id)),
            Action::ClearBudget,
            Action::AddTag(TagId(id)),
            Action::RemoveTag(TagId(id)),
            Action::RemoveAllTags,
            Action::SetDescription("d".into()),
            Action::SetNotes("n".into()),
            Action::LinkToBill(BillId(id)),
        ];
        assert_eq!(actions.len(), ACTION_KINDS.len());
        for a in actions {
            assert!(ACTION_KINDS.contains(&a.kind()), "{}", a.kind());
            assert_eq!(Action::parse(a.kind(), a.value().as_deref()), Ok(a.clone()));
        }
    }

    #[test]
    fn unknown_kinds_and_bad_values_are_errors() {
        assert!(matches!(
            Trigger::parse("journal_id", Some("1")),
            Err(RuleError::UnknownKind(..))
        ));
        assert!(matches!(
            Trigger::parse("amount_more", Some("lots")),
            Err(RuleError::BadValue { .. })
        ));
        assert!(matches!(
            Trigger::parse("category_is", None),
            Err(RuleError::BadValue { .. })
        ));
        assert!(matches!(
            Trigger::parse("transaction_type", Some("refund")),
            Err(RuleError::BadValue { .. })
        ));
        assert!(matches!(
            Action::parse("delete_transaction", None),
            Err(RuleError::UnknownKind(..))
        ));
        assert!(matches!(
            Action::parse("add_tag", Some("not-a-uuid")),
            Err(RuleError::BadValue { .. })
        ));
        assert!(matches!(
            Trigger::parse("description_contains", Some("")),
            Err(RuleError::BadValue { .. })
        ));
    }
}
