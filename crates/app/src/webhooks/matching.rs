//! Which endpoints receive which events (ARCHITECTURE.md 8.2). Pure rules,
//! tested without a database.

use crate::error::AppError;

/// At most this many patterns per endpoint.
const MAX_PATTERNS: usize = 50;

/// The event type that only the endpoint it names receives.
pub const TEST_EVENT: &str = "webhook.test";

/// What the dispatcher knows about an endpoint when matching.
#[derive(Debug, Clone)]
pub struct EndpointFilter {
    pub id: String,
    pub household_id: String,
    pub event_types: Vec<String>,
    pub include_import_events: bool,
    /// Events from before the endpoint existed are never sent to it.
    pub created_at: String,
}

/// What the dispatcher knows about an event when matching.
#[derive(Debug, Clone)]
pub struct EventSummary {
    pub seq: i64,
    pub household_id: String,
    pub event_type: String,
    pub entity_id: String,
    pub origin: String,
    pub occurred_at: String,
}

/// Does `pattern` select `event_type`? `*` selects everything, `prefix.*`
/// every type starting with `prefix.`, anything else only itself.
pub fn pattern_matches(pattern: &str, event_type: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    match pattern.strip_suffix(".*") {
        Some(prefix) => event_type
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with('.')),
        None => pattern == event_type,
    }
}

/// Should `endpoint` receive `event`?
pub fn endpoint_wants(endpoint: &EndpointFilter, event: &EventSummary) -> bool {
    if endpoint.household_id != event.household_id || event.occurred_at < endpoint.created_at {
        return false;
    }
    // A test event goes to the endpoint it was made for, whatever its filter.
    if event.event_type == TEST_EVENT {
        return event.entity_id == endpoint.id;
    }
    if event.origin == "import" && !endpoint.include_import_events {
        return false;
    }
    endpoint
        .event_types
        .iter()
        .any(|pattern| pattern_matches(pattern, &event.event_type))
}

/// Checks and normalises an endpoint's `event_types` input.
pub fn check_event_types(patterns: &[String]) -> Result<Vec<String>, AppError> {
    let field = "event_types";
    if patterns.is_empty() {
        return Err(AppError::invalid(
            field,
            "must list at least one event type",
        ));
    }
    if patterns.len() > MAX_PATTERNS {
        return Err(AppError::invalid(
            field,
            format!("at most {MAX_PATTERNS} entries"),
        ));
    }
    let mut checked: Vec<String> = Vec::new();
    for pattern in patterns {
        let pattern = pattern.trim();
        if !valid_pattern(pattern) {
            return Err(AppError::invalid(
                field,
                format!("{pattern:?} is not `*`, `<entity>.*` or `<entity>.<verb>`"),
            ));
        }
        if !checked.iter().any(|p| p == pattern) {
            checked.push(pattern.to_string());
        }
    }
    Ok(checked)
}

fn valid_pattern(pattern: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    let parts: Vec<&str> = pattern.split('.').collect();
    let word = |p: &&str| {
        !p.is_empty()
            && p.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    };
    match parts.as_slice() {
        [entity, "*"] => word(entity),
        [entity, verb] => word(entity) && word(verb),
        _ => false,
    }
}

/// Checks a target URL: `http` or `https` with a host. Private addresses are
/// allowed on purpose (Home Assistant, n8n on the LAN).
pub fn check_url(url: &str) -> Result<String, AppError> {
    let url = url.trim();
    let parsed = reqwest::Url::parse(url)
        .map_err(|_| AppError::invalid("url", "must be an absolute http or https URL"))?;
    let scheme_ok = matches!(parsed.scheme(), "http" | "https");
    if !scheme_ok || parsed.host_str().is_none_or(str::is_empty) {
        return Err(AppError::invalid(
            "url",
            "must be an absolute http or https URL",
        ));
    }
    Ok(url.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn endpoint(types: &[&str], imports: bool) -> EndpointFilter {
        EndpointFilter {
            id: "w1".into(),
            household_id: "h1".into(),
            event_types: types.iter().map(|t| t.to_string()).collect(),
            include_import_events: imports,
            created_at: "2026-11-13T09:00:00.000Z".into(),
        }
    }

    fn event(event_type: &str, origin: &str) -> EventSummary {
        EventSummary {
            seq: 1,
            household_id: "h1".into(),
            event_type: event_type.into(),
            entity_id: "e1".into(),
            origin: origin.into(),
            occurred_at: "2026-11-13T10:00:00.000Z".into(),
        }
    }

    #[test]
    fn patterns_match_exact_prefix_and_star() {
        assert!(pattern_matches("*", "transaction.created"));
        assert!(pattern_matches("transaction.*", "transaction.created"));
        assert!(pattern_matches(
            "transaction.created",
            "transaction.created"
        ));
        assert!(!pattern_matches(
            "transaction.updated",
            "transaction.created"
        ));
        assert!(!pattern_matches("transaction.*", "transactions.created"));
        assert!(!pattern_matches("bill.*", "bill"));
        assert!(!pattern_matches("account.*", "transaction.created"));
    }

    #[test]
    fn endpoints_get_matching_events_of_their_household_only() {
        let hook = endpoint(&["transaction.*", "bill.overdue"], false);
        assert!(endpoint_wants(&hook, &event("transaction.deleted", "api")));
        assert!(endpoint_wants(&hook, &event("bill.overdue", "scheduler")));
        assert!(!endpoint_wants(&hook, &event("bill.due_soon", "scheduler")));
        let mut other = event("transaction.created", "api");
        other.household_id = "h2".into();
        assert!(!endpoint_wants(&hook, &other));
    }

    #[test]
    fn import_events_need_the_opt_in() {
        let all = event("transaction.created", "import");
        assert!(!endpoint_wants(&endpoint(&["*"], false), &all));
        assert!(endpoint_wants(&endpoint(&["*"], true), &all));
    }

    #[test]
    fn events_from_before_the_endpoint_are_skipped() {
        let mut old = event("transaction.created", "api");
        old.occurred_at = "2026-11-12T10:00:00.000Z".into();
        assert!(!endpoint_wants(&endpoint(&["*"], false), &old));
    }

    #[test]
    fn test_events_go_only_to_their_endpoint_whatever_the_filter() {
        let hook = endpoint(&["bill.*"], false);
        let mut test = event(TEST_EVENT, "api");
        test.entity_id = "w1".into();
        assert!(endpoint_wants(&hook, &test));
        test.entity_id = "w2".into();
        assert!(!endpoint_wants(&endpoint(&["*"], false), &test));
    }

    #[test]
    fn event_type_input_is_checked_and_deduplicated() {
        let input = vec![
            " transaction.* ".to_string(),
            "transaction.*".into(),
            "*".into(),
        ];
        assert_eq!(check_event_types(&input).unwrap(), ["transaction.*", "*"]);
        for bad in [
            "",
            "Transaction.created",
            "a.b.c",
            "*.created",
            ".x",
            "a.",
            "a b.c",
        ] {
            assert!(check_event_types(&[bad.to_string()]).is_err(), "{bad}");
        }
        assert!(check_event_types(&[]).is_err());
        let many: Vec<String> = (0..51).map(|i| format!("e{i}.created")).collect();
        assert!(check_event_types(&many).is_err());
    }

    #[test]
    fn urls_must_be_http_or_https() {
        assert_eq!(
            check_url(" http://192.168.1.5:8123/api/webhook/x ").unwrap(),
            "http://192.168.1.5:8123/api/webhook/x"
        );
        assert!(check_url("https://example.com/hook").is_ok());
        for bad in [
            "ftp://example.com",
            "example.com/hook",
            "",
            "file:///etc/passwd",
            "http://",
        ] {
            assert!(check_url(bad).is_err(), "{bad}");
        }
    }
}
