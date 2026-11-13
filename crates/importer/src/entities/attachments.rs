//! `/v1/attachments` → `attachments` (filled in by the attachments work package).

use crate::engine::{ImportCx, ImportError};
use crate::report::ImportReport;

pub async fn import_attachments(
    _cx: &ImportCx,
    _report: &mut ImportReport,
) -> Result<(), ImportError> {
    Ok(())
}
