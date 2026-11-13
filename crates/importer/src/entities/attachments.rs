//! `/v1/attachments` + download → `attachments` and files (ARCHITECTURE.md 9.1, row 10).

use saldo_app::attachments::{ALLOWED_MIME_TYPES, NewAttachment};
use saldo_app::ctx::Origin;
use saldo_core::AttachmentId;

use super::transactions::SPLIT_REF;
use crate::engine::{ImportCx, ImportError};
use crate::firefly::FireflyError;
use crate::firefly::models::Attachment;
use crate::report::ImportReport;

const ENTITY: &str = "attachments";
const REF: &str = "attachment";

pub async fn import_attachments(
    cx: &ImportCx,
    report: &mut ImportReport,
) -> Result<(), ImportError> {
    let attachments = cx
        .firefly
        .list::<Attachment>("/v1/attachments", &[])
        .await?;
    let mut guard = cx.state.lock().await;
    let state = &mut *guard;
    for item in attachments {
        let a = &item.attributes;
        if a.attachable_type != "TransactionJournal" {
            report.note(&format!(
                "attachment on a Firefly {} not imported (Phase 5)",
                a.attachable_type
            ));
            continue;
        }
        let Some(split_id) = state
            .refs
            .resolve(SPLIT_REF, &a.attachable_id)
            .map(str::to_string)
        else {
            report.skipped(
                ENTITY,
                format!("{:?}: its split was not imported", a.filename),
            );
            continue;
        };
        let known = state
            .refs
            .resolve(REF, &item.id)
            .and_then(|id| id.parse::<AttachmentId>().ok());
        // `if let ... &&`: a let-chain, the pattern and the condition in one `if`.
        if let Some(id) = known
            && cx
                .app
                .attachments
                .get_in(&mut state.tx, &cx.ctx, id)
                .await
                .is_ok()
        {
            report.unchanged(ENTITY);
            continue;
        }
        if cx.dry_run {
            // Nothing is saved in a dry run, so the file is not downloaded either.
            report.created(ENTITY);
            continue;
        }
        let path = format!("/v1/attachments/{}/download", item.id);
        let bytes = match cx.firefly.get_bytes(&path).await {
            Ok(bytes) => bytes,
            // Firefly answers 500 when the stored file is gone: skip this one
            // attachment instead of failing the whole import.
            Err(FireflyError::Status { status, .. }) => {
                report.skipped(
                    ENTITY,
                    format!(
                        "{:?}: Firefly could not serve the file (HTTP {status})",
                        a.filename
                    ),
                );
                continue;
            }
            Err(other) => return Err(other.into()),
        };
        let input = NewAttachment {
            attachable_type: "split".into(),
            attachable_id: split_id,
            filename: plain_name(&a.filename),
            title: a.title.clone().filter(|t| !t.trim().is_empty()),
            notes: a.notes.clone().filter(|n| !n.trim().is_empty()),
            mime_type: mime_type(a.mime.as_deref(), &a.filename),
            bytes,
        };
        match cx
            .app
            .attachments
            .upload_in(&mut state.tx, &cx.ctx, Origin::Import, input)
            .await
        {
            Ok(view) => {
                state
                    .refs
                    .upsert(&mut state.tx, REF, &item.id, &view.id, &cx.now())
                    .await?;
                report.created(ENTITY);
            }
            Err(error) => report.skipped(
                ENTITY,
                format!("{:?}: {}", a.filename, super::skippable(error)?),
            ),
        }
    }
    Ok(())
}

/// The last path component, so a file name can never point elsewhere.
fn plain_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or_default().trim();
    if base.is_empty() {
        "attachment".to_string()
    } else {
        base.to_string()
    }
}

/// Firefly's MIME type if Saldo accepts it, otherwise a guess from the file
/// extension (some Firefly attachments have an empty type).
fn mime_type(firefly: Option<&str>, filename: &str) -> String {
    if let Some(mime) = firefly.filter(|m| ALLOWED_MIME_TYPES.contains(m)) {
        return mime.to_string();
    }
    let extension = filename
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    let guess = match extension.as_str() {
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "heic" => "image/heic",
        "txt" => "text/plain",
        "csv" => "text/csv",
        "xml" => "application/xml",
        _ => "application/octet-stream",
    };
    guess.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_types_are_made_safe() {
        assert_eq!(plain_name("../../etc/passwd"), "passwd");
        assert_eq!(plain_name("C:\\docs\\bill.pdf"), "bill.pdf");
        assert_eq!(plain_name("/"), "attachment");
        assert_eq!(mime_type(Some("application/pdf"), "x"), "application/pdf");
        assert_eq!(mime_type(Some(""), "scan.JPG"), "image/jpeg");
        assert_eq!(mime_type(None, "notes"), "application/octet-stream");
        assert_eq!(
            mime_type(Some("text/html"), "page.html"),
            "application/octet-stream"
        );
    }
}
