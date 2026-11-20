//! Pagination of list results.

use crate::error::AppError;

pub const DEFAULT_PER_PAGE: u32 = 50;
pub const MAX_PER_PAGE: u32 = 200;

/// Which page of a list to return. Pages start at 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageRequest {
    page: u32,
    per_page: u32,
}

impl PageRequest {
    /// Checks the values from a query string; `None` means the default.
    pub fn new(page: Option<u32>, per_page: Option<u32>) -> Result<Self, AppError> {
        let page = page.unwrap_or(1);
        let per_page = per_page.unwrap_or(DEFAULT_PER_PAGE);
        if page == 0 {
            return Err(AppError::invalid("page", "must be 1 or more"));
        }
        if per_page == 0 || per_page > MAX_PER_PAGE {
            return Err(AppError::invalid(
                "per_page",
                format!("must be between 1 and {MAX_PER_PAGE}"),
            ));
        }
        Ok(Self { page, per_page })
    }

    /// The first page at the largest size, for background tasks that read a
    /// household's (short) lists of budgets or limits.
    pub fn max() -> Self {
        Self {
            page: 1,
            per_page: MAX_PER_PAGE,
        }
    }

    pub fn page(&self) -> u32 {
        self.page
    }

    pub fn per_page(&self) -> u32 {
        self.per_page
    }

    pub(crate) fn limit(&self) -> i64 {
        i64::from(self.per_page)
    }

    pub(crate) fn offset(&self) -> i64 {
        i64::from(self.page - 1) * i64::from(self.per_page)
    }
}

impl Default for PageRequest {
    fn default() -> Self {
        Self {
            page: 1,
            per_page: DEFAULT_PER_PAGE,
        }
    }
}

/// One page of results plus the total number of matches.
///
/// `Page<T>` is generic: `T` is the item type, like `Page<AccountView>`. Go
/// generics or a PHP collection typed in a docblock.
#[derive(Debug, Clone, PartialEq)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub page: u32,
    pub per_page: u32,
    pub total: i64,
}

impl<T> Page<T> {
    pub fn new(items: Vec<T>, request: PageRequest, total: i64) -> Self {
        Self {
            items,
            page: request.page,
            per_page: request.per_page,
            total,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_first_page_of_fifty() {
        let request = PageRequest::new(None, None).unwrap();
        assert_eq!((request.page(), request.per_page()), (1, 50));
        assert_eq!((request.limit(), request.offset()), (50, 0));
        assert_eq!(PageRequest::default(), request);
    }

    #[test]
    fn offset_skips_earlier_pages() {
        let request = PageRequest::new(Some(3), Some(20)).unwrap();
        assert_eq!(request.offset(), 40);
    }

    #[test]
    fn rejects_out_of_range_values() {
        assert!(PageRequest::new(Some(0), None).is_err());
        assert!(PageRequest::new(None, Some(0)).is_err());
        assert!(PageRequest::new(None, Some(201)).is_err());
        assert!(PageRequest::new(None, Some(200)).is_ok());
    }
}
