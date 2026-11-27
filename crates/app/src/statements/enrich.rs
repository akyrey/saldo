//! What stored Amazon and PayPal rows say about statement lines (Phase 13,
//! Track D; brief decisions 4, 8 and 9): a better description, the real
//! counterparty, notes, and for one Amazon charge its items.

use chrono::{NaiveDate, NaiveTime};
use saldo_core::matching::AmazonOrder;
use saldo_core::matching::{
    AmazonItem, AmazonMatchKind, LagWindow, Line, PayPalKind, PayPalRow, amazon_orders,
    match_amazon, match_paypal, pair_paypal_funding,
};
use saldo_core::{Currency, DateRange, Money};
use serde::{Deserialize, Serialize};

use super::parse::StatementLine;

/// Bank texts that name a source (L18): lowercase, matched as substrings.
const AMAZON_MARKERS: [&str; 2] = ["amazon", "amzn"];
const PAYPAL_MARKERS: [&str; 1] = ["paypal"];
/// How far a bank line may be from its source row (decision 10).
pub(super) const AMAZON_WINDOW: LagWindow = LagWindow {
    early_days: 2,
    late_days: 14,
};
pub(super) const PAYPAL_WINDOW: LagWindow = LagWindow {
    early_days: 2,
    late_days: 7,
};
/// Longest product name used in a description.
const MAX_PRODUCT_CHARS: usize = 80;

/// One stored Amazon item or PayPal row, as the matchers need it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SourceRowData {
    pub id: String,
    pub external_id: String,
    pub date: NaiveDate,
    pub time: Option<NaiveTime>,
    pub amount: Money,
    pub row_kind: String,
    /// Amazon: the order ID.
    pub group_key: Option<String>,
    pub counterparty: Option<String>,
    pub text: Option<String>,
    pub excluded: bool,
}

/// One item of an Amazon charge, for splitting it (decision 9).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ProposalItem {
    pub product: String,
    pub amount_minor: i64,
}

/// What a line's matched source proposes; stored as the line's `proposal`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Proposal {
    /// `amazon` or `paypal`.
    pub source: String,
    pub description: String,
    pub counterparty: String,
    pub notes: Vec<String>,
    /// The items of a single Amazon charge; empty otherwise.
    pub items: Vec<ProposalItem>,
}

/// A source row explaining a line, for `source_matches`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SourceMatch {
    pub row_id: String,
    /// `order_id`, `amount_date`, `approximate`, `funding_pair` or `amount_lag`.
    pub match_type: &'static str,
    pub amount_diff_minor: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Enrichment {
    pub proposal: Proposal,
    /// Amazon order dates: the ledger may carry the purchase on one (L9).
    pub order_dates: Vec<NaiveDate>,
    pub matches: Vec<SourceMatch>,
}

fn mentions(text: &str, markers: &[&str]) -> bool {
    let text = text.to_lowercase();
    markers.iter().any(|marker| text.contains(marker))
}

fn matcher_lines(lines: &[StatementLine]) -> Vec<Line> {
    lines
        .iter()
        .map(|line| Line {
            date: line.booked_date,
            other_dates: vec![],
            amount: line.amount,
            text: line.description.clone(),
            window: None,
        })
        .collect()
}

/// The proposal for each line, PayPal first when both match (decision 4).
pub(super) fn enrich(
    lines: &[StatementLine],
    amazon: &[SourceRowData],
    paypal: &[SourceRowData],
    range: DateRange,
    currency: &Currency,
) -> Vec<Option<Enrichment>> {
    let matcher = matcher_lines(lines);
    let mut found = amazon_enrichments(lines, &matcher, amazon, currency);
    for (index, enrichment) in paypal_enrichments(lines, &matcher, paypal, range)
        .into_iter()
        .enumerate()
    {
        if enrichment.is_some() {
            found[index] = enrichment;
        }
    }
    found
}

fn amazon_enrichments(
    lines: &[StatementLine],
    matcher: &[Line],
    rows: &[SourceRowData],
    currency: &Currency,
) -> Vec<Option<Enrichment>> {
    let mut found = vec![None; lines.len()];
    let items: Vec<AmazonItem> = rows
        .iter()
        .map(|row| AmazonItem {
            order_id: row.group_key.clone().unwrap_or_default(),
            date: row.date,
            amount: row.amount,
            product: row.text.clone().unwrap_or_default(),
            cancelled: row.excluded,
            gift_card_only: false,
        })
        .collect();
    let orders = amazon_orders(&items);
    let eligible: Vec<bool> = lines
        .iter()
        .map(|l| mentions(&l.description, &AMAZON_MARKERS))
        .collect();
    // The whole history: an order ID matches whatever its date.
    let all = DateRange::new(NaiveDate::MIN, NaiveDate::MAX);
    let Some(all) = all else { return found };
    let result = match_amazon(matcher, &eligible, &orders, AMAZON_WINDOW, 0, all);
    for m in result.matches {
        let order = &orders[m.order];
        let products: Vec<&AmazonItem> = order.items.iter().map(|&i| &items[i]).collect();
        // Splitting by item makes sense for one charge of one order.
        let proposal = amazon_proposal(
            order,
            &products,
            m.amount_diff,
            m.lines.len() == 1,
            currency,
        );
        let match_type = match m.kind {
            AmazonMatchKind::OrderId => "order_id",
            AmazonMatchKind::Exact => "amount_date",
            AmazonMatchKind::Approximate => "approximate",
        };
        let matches: Vec<SourceMatch> = order
            .items
            .iter()
            .map(|&i| SourceMatch {
                row_id: rows[i].id.clone(),
                match_type,
                amount_diff_minor: m.amount_diff.minor(),
            })
            .collect();
        for &line in &m.lines {
            found[line] = Some(Enrichment {
                proposal: proposal.clone(),
                order_dates: vec![order.date],
                matches: matches.clone(),
            });
        }
    }
    found
}

/// What an Amazon order proposes for a charge: its products, and with
/// `single` (one charge for the whole order) the items to split by.
fn amazon_proposal(
    order: &AmazonOrder,
    products: &[&AmazonItem],
    amount_diff: Money,
    single: bool,
    currency: &Currency,
) -> Proposal {
    let first = products
        .first()
        .map(|p| short(&p.product))
        .unwrap_or_default();
    let more = products.len().saturating_sub(1);
    let description = if more > 0 {
        format!("Amazon: {first} + {more} more")
    } else {
        format!("Amazon: {first}")
    };
    let names: Vec<&str> = products.iter().map(|p| p.product.as_str()).collect();
    let mut notes = vec![format!(
        "Amazon order {} ({}): {}",
        order.order_id,
        order.date,
        names.join("; ")
    )];
    if amount_diff.minor() != 0 {
        let size = Money::new(amount_diff.minor().saturating_abs(), currency.code())
            .to_decimal_string(currency);
        let way = if amount_diff.minor() > 0 {
            "more"
        } else {
            "less"
        };
        notes.push(format!(
            "Charged {size} {} {way} than the order total",
            currency.code()
        ));
    }
    let items = if single {
        products
            .iter()
            .map(|p| ProposalItem {
                product: short(&p.product),
                amount_minor: p.amount.minor(),
            })
            .collect()
    } else {
        vec![]
    };
    Proposal {
        source: "amazon".to_string(),
        description,
        counterparty: "Amazon".to_string(),
        notes,
        items,
    }
}

fn paypal_kind(name: &str) -> PayPalKind {
    match name {
        "funding" => PayPalKind::Funding,
        "withdrawal" => PayPalKind::Withdrawal,
        "payment" => PayPalKind::Payment,
        "refund" => PayPalKind::Refund,
        _ => PayPalKind::Other,
    }
}

fn paypal_enrichments(
    lines: &[StatementLine],
    matcher: &[Line],
    rows: &[SourceRowData],
    range: DateRange,
) -> Vec<Option<Enrichment>> {
    let mut found = vec![None; lines.len()];
    let paypal: Vec<PayPalRow> = rows
        .iter()
        .map(|row| PayPalRow {
            date: row.date,
            time: row.time,
            kind: paypal_kind(&row.row_kind),
            amount: row.amount,
            name: row.counterparty.clone().unwrap_or_default(),
        })
        .collect();
    let movements = pair_paypal_funding(&paypal);
    let eligible: Vec<bool> = lines
        .iter()
        .map(|l| mentions(&l.description, &PAYPAL_MARKERS))
        .collect();
    let result = match_paypal(
        matcher,
        &eligible,
        &paypal,
        &movements,
        PAYPAL_WINDOW,
        range,
    );
    for (line, m) in result.matched {
        let movement = movements[m];
        found[line] = Some(paypal_enrichment(
            &rows[movement.movement],
            &rows[movement.merchant],
            movement.funded(),
            &lines[line].description,
        ));
    }
    found
}

/// What a PayPal movement proposes: the merchant of the payment it paid for
/// (or of itself), and the rows that explain the line.
fn paypal_enrichment(
    moved: &SourceRowData,
    merchant: &SourceRowData,
    funded: bool,
    bank_text: &str,
) -> Enrichment {
    let name = merchant.counterparty.clone().unwrap_or_default();
    let mut matches = vec![SourceMatch {
        row_id: moved.id.clone(),
        match_type: if funded { "funding_pair" } else { "amount_lag" },
        amount_diff_minor: 0,
    }];
    if funded {
        matches.push(SourceMatch {
            row_id: merchant.id.clone(),
            match_type: "funding_pair",
            amount_diff_minor: 0,
        });
    }
    let (description, counterparty) = if name.is_empty() {
        (bank_text.to_string(), "PayPal".to_string())
    } else {
        (name.clone(), name.clone())
    };
    Enrichment {
        proposal: Proposal {
            source: "paypal".to_string(),
            description,
            counterparty,
            notes: vec![format!(
                "PayPal {} on {}: {}",
                merchant.external_id, merchant.date, name
            )],
            items: vec![],
        },
        order_dates: vec![],
        matches,
    }
}

fn short(text: &str) -> String {
    text.trim().chars().take(MAX_PRODUCT_CHARS).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use saldo_core::CurrencyCode;

    fn eur(minor: i64) -> Money {
        Money::new(minor, CurrencyCode::new("EUR").unwrap())
    }

    fn euro() -> Currency {
        Currency::new(CurrencyCode::new("EUR").unwrap(), 2).unwrap()
    }

    fn day(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, d).unwrap()
    }

    fn line(d: u32, minor: i64, text: &str) -> StatementLine {
        StatementLine {
            row: 1,
            accounting_date: day(d),
            value_date: day(d),
            booked_date: day(d),
            amount: eur(minor),
            description: text.to_string(),
            category: None,
            counterparty: None,
            counterparty_iban: None,
            external_id: None,
            notes: vec![],
            parts: vec![],
        }
    }

    fn row(
        id: &str,
        d: u32,
        minor: i64,
        kind: &str,
        group: Option<&str>,
        who: Option<&str>,
        text: Option<&str>,
    ) -> SourceRowData {
        SourceRowData {
            id: id.to_string(),
            external_id: format!("X{id}"),
            date: day(d),
            time: NaiveTime::from_hms_opt(10, 0, 0),
            amount: eur(minor),
            row_kind: kind.to_string(),
            group_key: group.map(str::to_string),
            counterparty: who.map(str::to_string),
            text: text.map(str::to_string),
            excluded: false,
        }
    }

    fn november() -> DateRange {
        DateRange::new(day(1), day(30)).unwrap()
    }

    const ORDER: &str = "405-1234567-1234567";

    #[test]
    fn an_amazon_order_id_proposes_its_products_and_items() {
        let amazon = [
            row("a1", 2, 1_500, "item", Some(ORDER), None, Some("Moka pot")),
            row("a2", 2, 500, "item", Some(ORDER), None, Some("Filter")),
        ];
        let lines = [line(20, -2_000, &format!("AMAZON EU {ORDER}"))];
        let found = enrich(&lines, &amazon, &[], november(), &euro());
        let e = found[0].as_ref().unwrap();
        assert_eq!(e.proposal.description, "Amazon: Moka pot + 1 more");
        assert_eq!(e.proposal.counterparty, "Amazon");
        assert_eq!(e.proposal.items.len(), 2);
        assert_eq!(e.order_dates, vec![day(2)]);
        assert_eq!(e.matches.len(), 2);
        assert_eq!(e.matches[0].match_type, "order_id");
        assert!(e.proposal.notes[0].contains(ORDER));
    }

    #[test]
    fn a_paypal_funding_names_the_merchant() {
        let paypal = [
            row("p1", 5, -1_299, "payment", None, Some("Netflix"), None),
            row("p2", 5, 1_299, "funding", None, None, None),
        ];
        let lines = [line(6, -1_299, "ADDEBITO SDD PAYPAL EUROPE")];
        let found = enrich(&lines, &[], &paypal, november(), &euro());
        let e = found[0].as_ref().unwrap();
        assert_eq!(
            (
                e.proposal.description.as_str(),
                e.proposal.counterparty.as_str()
            ),
            ("Netflix", "Netflix")
        );
        assert_eq!(e.matches.len(), 2);
        assert_eq!(e.matches[0].match_type, "funding_pair");
    }

    #[test]
    fn paypal_wins_when_both_match() {
        let amazon = [row("a1", 5, 1_299, "item", Some(ORDER), None, Some("Book"))];
        let paypal = [row(
            "p1",
            5,
            -1_299,
            "payment",
            None,
            Some("Amazon Payments"),
            None,
        )];
        let lines = [line(6, -1_299, "PAYPAL *AMAZON")];
        let found = enrich(&lines, &amazon, &paypal, november(), &euro());
        assert_eq!(found[0].as_ref().unwrap().proposal.source, "paypal");
    }

    #[test]
    fn an_order_charged_in_two_shipments_offers_no_split_by_item() {
        let amazon = [row(
            "a1",
            2,
            3_000,
            "item",
            Some(ORDER),
            None,
            Some("Shoes"),
        )];
        let lines = [
            line(10, -1_000, &format!("AMAZON {ORDER}")),
            line(15, -2_000, &format!("AMAZON {ORDER}")),
        ];
        let found = enrich(&lines, &amazon, &[], november(), &euro());
        assert!(
            found
                .iter()
                .all(|e| e.as_ref().unwrap().proposal.items.is_empty())
        );
    }

    #[test]
    fn a_line_no_source_explains_gets_nothing() {
        let lines = [line(10, -500, "POS BAR")];
        assert_eq!(enrich(&lines, &[], &[], november(), &euro()), vec![None]);
    }
}
