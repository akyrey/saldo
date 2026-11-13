-- Accounts at an external identity provider (Authentik over OpenID Connect)
-- linked to Saldo users. The first login matches a verified email to a user;
-- later logins find the user by (issuer, subject), which never changes.
CREATE TABLE user_identities (
    issuer      TEXT NOT NULL,
    subject     TEXT NOT NULL,
    user_id     TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at  TEXT NOT NULL,
    PRIMARY KEY (issuer, subject)
) STRICT;

CREATE INDEX user_identities_user ON user_identities (user_id);
