-- A request for a pilot's agreement that waits for the pilot's answer.
--
-- The authorization endpoint checks a request (the program, the address
-- the pilot is sent back to, the challenge, what is asked for) and, when
-- everything is good, keeps it here under the hash of an identifier nobody
-- can guess. The web app, where the pilot is signed in already, reads it
-- by that identifier and answers it with the pilot's session: an account
-- that signs in with Google and has no password agrees this way.
--
-- A request is answered once. An answered one stays until its end, so that
-- a second answer is told "answered already"; nothing of it is read again.
-- As everywhere, the identifier is not stored, only its SHA-256 hash.
CREATE TABLE oauth_requests (
    request_hash bytea PRIMARY KEY,
    client_id    text COLLATE "C" NOT NULL REFERENCES oauth_clients (id) ON DELETE CASCADE,
    -- The address the pilot is sent back to: one the program registered.
    redirect_uri text NOT NULL,
    scopes       text[] NOT NULL CHECK (cardinality(scopes) >= 1),
    resource     text NOT NULL,
    -- The program's challenge (RFC 7636, S256).
    challenge    text NOT NULL,
    -- The program's own value, handed back unread. NULL if it sent none.
    state        text,
    created_at   timestamptz NOT NULL,
    expires_at   timestamptz NOT NULL,
    answered_at  timestamptz
);

CREATE INDEX oauth_requests_expiry_idx ON oauth_requests (expires_at);
