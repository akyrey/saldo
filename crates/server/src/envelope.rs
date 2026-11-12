//! Response envelopes: `{ "data": ... }`, and `meta` on paginated lists.

use saldo_app::page::Page;
use serde::Serialize;
use utoipa::ToSchema;

/// A single result.
#[derive(Debug, Serialize, ToSchema)]
pub struct Data<T> {
    pub data: T,
}

/// A page of results.
#[derive(Debug, Serialize, ToSchema)]
pub struct List<T> {
    pub data: Vec<T>,
    pub meta: ListMeta,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ListMeta {
    pub page: u32,
    pub per_page: u32,
    pub total: i64,
}

impl<T> Data<T> {
    pub fn new(data: T) -> Self {
        Self { data }
    }
}

/// `From` lets `List::from(page)` or `page.into()` convert a service page.
impl<T> From<Page<T>> for List<T> {
    fn from(page: Page<T>) -> Self {
        Self {
            data: page.items,
            meta: ListMeta {
                page: page.page,
                per_page: page.per_page,
                total: page.total,
            },
        }
    }
}
