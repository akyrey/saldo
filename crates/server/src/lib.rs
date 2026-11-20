//! The Saldo HTTP API: routes, authentication, OpenAPI and the CLI.

pub mod auth;
pub mod cli;
pub mod config;
pub mod envelope;
pub mod error;
pub mod extract;
pub mod hardening;
pub mod notify;
pub mod oidc;
pub mod openapi;
pub mod rate_limit;
pub mod routes;
pub mod server;
pub mod spa;
pub mod state;
pub mod workers;
