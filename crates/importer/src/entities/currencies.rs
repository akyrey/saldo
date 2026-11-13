//! `/v1/currencies` → `currencies` (enabled ones only).

use saldo_app::currencies::{self, CurrencyView, Ensured};

use crate::engine::{ImportCx, ImportError};
use crate::firefly::models::Currency;
use crate::report::ImportReport;

const ENTITY: &str = "currencies";

pub async fn import_currencies(
    cx: &ImportCx,
    report: &mut ImportReport,
) -> Result<(), ImportError> {
    let currencies = cx.firefly.list::<Currency>("/v1/currencies", &[]).await?;
    let mut state = cx.state.lock().await;
    // Iterator pipeline: keep the enabled ones. Like a Laravel collection's
    // `->filter()`, or a Go `for range` with an `if`.
    for currency in currencies
        .into_iter()
        .map(|c| c.attributes)
        .filter(|c| c.enabled)
    {
        let view = CurrencyView {
            code: currency.code.clone(),
            name: currency.name,
            symbol: currency.symbol,
            decimal_places: currency.decimal_places,
        };
        match currencies::ensure(&mut state.tx, &view).await {
            Ok(Ensured::Created) => report.created(ENTITY),
            Ok(Ensured::Unchanged) => report.unchanged(ENTITY),
            Err(error) => report.skipped(
                ENTITY,
                format!("{}: {}", currency.code, super::skippable(error)?),
            ),
        }
    }
    Ok(())
}
