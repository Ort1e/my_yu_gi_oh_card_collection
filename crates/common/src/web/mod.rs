//! HTTP plumbing shared by every collection: errors, form parsing,
//! pagination, template filters, the login, and the advent calendars' day
//! form.

pub mod auth;
pub mod day_form;
pub mod error;
pub mod filters;
pub mod forms;
pub mod pagination;
