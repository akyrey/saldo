//! "Log in with Authentik": OpenID Connect's authorization code flow with
//! PKCE. Saldo redirects the browser to the provider, gets a code back,
//! exchanges it for a signed ID token, checks the token (signature, issuer,
//! audience, expiry, nonce) and maps the person to a member
//! (`IdentityService::login_external`). Like "Login with Google" in a Laravel
//! app using Socialite, but the provider is your own Authentik.

pub mod config;
mod es256_p384;
mod flow;
mod handoff;
mod http;
mod pending;

pub use config::OidcConfig;
pub use flow::{OidcError, finish, start};
pub use handoff::{DesktopHandoffs, DesktopReturn};
pub use pending::{Pending, PendingLogins};
