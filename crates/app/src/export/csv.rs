//! CSV files in a zip: one file per entity, readable in any spreadsheet.

use std::collections::HashMap;
use std::io::{Cursor, Write};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use super::ExportDocument;
use crate::error::AppError;

/// One CSV row. A field with a comma, quote or line break is quoted, and
/// quotes inside are doubled (RFC 4180).
fn row(fields: &[&str]) -> String {
    let quoted: Vec<String> = fields
        .iter()
        .map(|f| {
            if f.contains([',', '"', '\n', '\r']) {
                format!("\"{}\"", f.replace('"', "\"\""))
            } else {
                (*f).to_string()
            }
        })
        .collect();
    quoted.join(",") + "\r\n"
}

pub(super) fn table(header: &[&str], rows: Vec<Vec<String>>) -> String {
    let mut out = row(header);
    for fields in rows {
        let borrowed: Vec<&str> = fields.iter().map(String::as_str).collect();
        out.push_str(&row(&borrowed));
    }
    out
}

pub(super) fn opt(value: &Option<String>) -> String {
    value.clone().unwrap_or_default()
}

/// The CSV files of a document, by file name.
pub(crate) fn files(doc: &ExportDocument) -> Vec<(&'static str, String)> {
    let account_names: HashMap<&str, &str> = doc
        .accounts
        .iter()
        .map(|a| (a.id.as_str(), a.name.as_str()))
        .collect();
    let category_names: HashMap<&str, &str> = doc
        .categories
        .iter()
        .map(|c| (c.id.as_str(), c.name.as_str()))
        .collect();
    let budget_names: HashMap<&str, &str> = doc
        .budgets
        .iter()
        .map(|b| (b.id.as_str(), b.name.as_str()))
        .collect();
    let bill_names: HashMap<&str, &str> = doc
        .bills
        .iter()
        .map(|b| (b.id.as_str(), b.name.as_str()))
        .collect();
    let tag_names: HashMap<&str, &str> = doc
        .tags
        .iter()
        .map(|t| (t.id.as_str(), t.name.as_str()))
        .collect();
    let name =
        |map: &HashMap<&str, &str>, id: &str| map.get(id).copied().unwrap_or_default().to_string();

    let accounts = table(
        &[
            "id",
            "name",
            "type",
            "equity_role",
            "currency",
            "owner_user_id",
            "asset_role",
            "liability_type",
            "iban",
            "active",
            "notes",
        ],
        doc.accounts
            .iter()
            .map(|a| {
                vec![
                    a.id.clone(),
                    a.name.clone(),
                    a.account_type.clone(),
                    opt(&a.equity_role),
                    a.currency.clone(),
                    opt(&a.owner_user_id),
                    opt(&a.asset_role),
                    opt(&a.liability_type),
                    opt(&a.iban),
                    a.active.to_string(),
                    opt(&a.notes),
                ]
            })
            .collect(),
    );
    let transactions = table(
        &[
            "group_id",
            "group_title",
            "split_id",
            "date",
            "kind",
            "description",
            "source_account",
            "destination_account",
            "amount",
            "currency",
            "foreign_amount",
            "foreign_currency",
            "category",
            "budget",
            "bill",
            "tags",
            "reconciled",
            "notes",
        ],
        doc.transactions
            .iter()
            .flat_map(|g| g.splits.iter().map(move |s| (g, s)))
            .map(|(g, s)| {
                let tags: Vec<String> = s.tag_ids.iter().map(|t| name(&tag_names, t)).collect();
                vec![
                    g.id.clone(),
                    opt(&g.title),
                    s.id.clone(),
                    s.date.clone(),
                    s.kind.clone(),
                    s.description.clone(),
                    name(&account_names, &s.source_account_id),
                    name(&account_names, &s.destination_account_id),
                    s.amount.clone(),
                    s.currency.clone(),
                    opt(&s.foreign_amount),
                    opt(&s.foreign_currency),
                    s.category_id
                        .as_deref()
                        .map(|c| name(&category_names, c))
                        .unwrap_or_default(),
                    s.budget_id
                        .as_deref()
                        .map(|b| name(&budget_names, b))
                        .unwrap_or_default(),
                    s.bill_id
                        .as_deref()
                        .map(|b| name(&bill_names, b))
                        .unwrap_or_default(),
                    tags.join("; "),
                    s.reconciled.to_string(),
                    opt(&s.notes),
                ]
            })
            .collect(),
    );
    let categories = table(
        &["id", "name", "notes", "color"],
        doc.categories
            .iter()
            .map(|c| vec![c.id.clone(), c.name.clone(), opt(&c.notes), opt(&c.color)])
            .collect(),
    );
    let tags = table(
        &["id", "name", "description"],
        doc.tags
            .iter()
            .map(|t| vec![t.id.clone(), t.name.clone(), opt(&t.description)])
            .collect(),
    );
    let attachments = table(
        &[
            "id",
            "attachable_type",
            "attachable_id",
            "filename",
            "title",
            "mime_type",
            "size_bytes",
            "sha256",
        ],
        doc.attachments
            .iter()
            .map(|a| {
                vec![
                    a.id.clone(),
                    a.attachable_type.clone(),
                    a.attachable_id.clone(),
                    a.filename.clone(),
                    opt(&a.title),
                    a.mime_type.clone(),
                    a.size_bytes.to_string(),
                    a.sha256.clone(),
                ]
            })
            .collect(),
    );
    let members = table(
        &["user_id", "email", "display_name", "role"],
        doc.members
            .iter()
            .map(|m| {
                vec![
                    m.user_id.clone(),
                    m.email.clone(),
                    m.display_name.clone(),
                    m.role.as_str().to_string(),
                ]
            })
            .collect(),
    );
    let currencies = table(
        &["code", "name", "symbol", "decimal_places"],
        doc.currencies
            .iter()
            .map(|c| {
                vec![
                    c.code.clone(),
                    c.name.clone(),
                    c.symbol.clone(),
                    c.decimal_places.to_string(),
                ]
            })
            .collect(),
    );
    let mut files = vec![
        ("accounts.csv", accounts),
        ("transactions.csv", transactions),
        ("categories.csv", categories),
        ("tags.csv", tags),
        ("attachments.csv", attachments),
        ("members.csv", members),
        ("currencies.csv", currencies),
    ];
    files.extend(super::planning_csv::files(doc));
    files.extend(super::savings_csv::files(doc));
    files.extend(super::recurrence_csv::files(doc));
    files.extend(super::templates_csv::files(doc));
    files.extend(super::views_csv::files(doc));
    files.extend(super::cards_csv::files(doc));
    files.extend(super::statements_csv::files(doc));
    files
}

/// The CSV files packed into one zip archive.
pub(crate) fn zip(doc: &ExportDocument) -> Result<Vec<u8>, AppError> {
    let failed = |e: &dyn std::fmt::Display| AppError::Internal(format!("cannot build zip: {e}"));
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (name, content) in files(doc) {
        writer.start_file(name, options).map_err(|e| failed(&e))?;
        writer
            .write_all(content.as_bytes())
            .map_err(|e| failed(&e))?;
    }
    Ok(writer.finish().map_err(|e| failed(&e))?.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_only_when_needed() {
        assert_eq!(row(&["a", "b c", ""]), "a,b c,\r\n");
        assert_eq!(
            row(&["x,y", "say \"hi\"", "two\nlines"]),
            "\"x,y\",\"say \"\"hi\"\"\",\"two\nlines\"\r\n"
        );
    }
}
