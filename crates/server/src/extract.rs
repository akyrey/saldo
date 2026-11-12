//! Wrappers around Axum's `Json`, `Query` and `Path` whose failures use the
//! API error envelope instead of Axum's plain-text messages.
//!
//! An *extractor* is a handler argument that Axum fills from the request,
//! like a Laravel form request or route-model binding. These derives reuse
//! Axum's own extractors (`via`) and only change the error type.

use axum::extract::{FromRequest, FromRequestParts};

use crate::error::ApiError;

#[derive(FromRequest)]
#[from_request(via(axum::Json), rejection(ApiError))]
pub struct ApiJson<T>(pub T);

#[derive(FromRequestParts)]
#[from_request(via(axum::extract::Query), rejection(ApiError))]
pub struct ApiQuery<T>(pub T);

#[derive(FromRequestParts)]
#[from_request(via(axum::extract::Path), rejection(ApiError))]
pub struct ApiPath<T>(pub T);
