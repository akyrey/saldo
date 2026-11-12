//! Accounts: repository, view and service.

pub(crate) mod repo;
mod view;

pub use view::AccountView;
pub(crate) use view::view_of;
