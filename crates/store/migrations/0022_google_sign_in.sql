-- Sign in with Google: a credential of the kind 'google', whose secret is
-- the Google account's subject (the `sub` of its ID tokens), which Google
-- never changes and never gives to another account. It is no secret in the
-- sense of a password: it proves nothing alone, only an ID token that
-- Google signed for it does.
--
-- The email is Google's at the linking, for the pilot's own account and
-- archive and for whoever runs the server; no answer that others read
-- carries it. Only a 'google' credential has one.
--
-- An account made with Google has no 'password' credential.

ALTER TABLE credentials ADD COLUMN email text;

-- One Venti account per Google account, and one Google account per Venti
-- account.
CREATE UNIQUE INDEX credentials_google_subject_idx
    ON credentials (secret) WHERE kind = 'google';
CREATE UNIQUE INDEX credentials_google_account_idx
    ON credentials (account_id) WHERE kind = 'google';
