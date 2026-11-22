-- The authorization server: a pilot agrees, on the server's own page, that
-- a program (an assistant, above all) may read for them, and the program
-- gets tokens without the pilot copying a secret.
--
-- Three things are kept. The programs that registered themselves to ask;
-- the codes an agreement is turned into, each exchanged once and within
-- minutes; and the agreement itself, which is a row of access_tokens, so
-- that it stands in the pilot's list of tokens and is withdrawn there like
-- any other. As everywhere, no secret is stored, only its SHA-256 hash.

-- A program that registered itself. Its name is its own claim.
CREATE TABLE oauth_clients (
    id            text COLLATE "C" PRIMARY KEY,
    name          text NOT NULL,
    -- Where a pilot may be sent back to after agreeing or refusing.
    redirect_uris text[] NOT NULL CHECK (cardinality(redirect_uris) >= 1),
    created_at    timestamptz NOT NULL,
    -- When a pilot last agreed to it, or it last exchanged a code or a
    -- refresh token. NULL for a program no pilot ever agreed to: the one
    -- of those that registered longest ago makes room when the table is
    -- full.
    last_used_at  timestamptz
);

CREATE INDEX oauth_clients_never_used_idx ON oauth_clients (created_at, id)
    WHERE last_used_at IS NULL;

-- A token that came of an agreement names the program, the address of
-- what it may be used at, and the end of the access token in use. Its
-- token_hash is that access token's, and changes with every refresh; its
-- expires_at is the end of the agreement, which each refresh moves on.
-- All three are NULL for a token a pilot made by hand.
ALTER TABLE access_tokens
    ADD COLUMN client_id text COLLATE "C" REFERENCES oauth_clients (id) ON DELETE CASCADE;
ALTER TABLE access_tokens ADD COLUMN resource text;
ALTER TABLE access_tokens ADD COLUMN access_until timestamptz;
ALTER TABLE access_tokens ADD CONSTRAINT access_tokens_agreement_is_whole CHECK (
    (client_id IS NULL AND resource IS NULL AND access_until IS NULL)
    OR (client_id IS NOT NULL AND resource IS NOT NULL AND access_until IS NOT NULL
        AND expires_at IS NOT NULL)
);

-- One agreement of an account with a program: agreeing again replaces it.
CREATE UNIQUE INDEX access_tokens_one_per_client_idx ON access_tokens (account_id, client_id)
    WHERE client_id IS NOT NULL;
CREATE INDEX access_tokens_client_idx ON access_tokens (client_id) WHERE client_id IS NOT NULL;

-- The code an agreement is turned into. A code that was exchanged stays
-- until its end, with the token that came of it, so that a second
-- exchange is recognised and withdraws that token.
CREATE TABLE oauth_codes (
    code_hash    bytea PRIMARY KEY,
    client_id    text COLLATE "C" NOT NULL REFERENCES oauth_clients (id) ON DELETE CASCADE,
    account_id   text COLLATE "C" NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    redirect_uri text NOT NULL,
    scopes       text[] NOT NULL CHECK (cardinality(scopes) >= 1),
    resource     text NOT NULL,
    -- The program's challenge (RFC 7636, S256).
    challenge    text NOT NULL,
    created_at   timestamptz NOT NULL,
    expires_at   timestamptz NOT NULL,
    used_at      timestamptz,
    -- The token the exchange made. Text, not a reference: the token may
    -- be gone before the code is.
    token_id     text COLLATE "C"
);

CREATE INDEX oauth_codes_expiry_idx ON oauth_codes (expires_at);

-- The refresh tokens of an agreement: the one that is good, and the ones
-- that were used, kept until their end so that a second use is recognised
-- and ends the agreement.
CREATE TABLE oauth_refresh_tokens (
    token_hash bytea PRIMARY KEY,
    token_id   text COLLATE "C" NOT NULL REFERENCES access_tokens (id) ON DELETE CASCADE,
    created_at timestamptz NOT NULL,
    expires_at timestamptz NOT NULL,
    spent_at   timestamptz
);

CREATE INDEX oauth_refresh_tokens_token_idx ON oauth_refresh_tokens (token_id);
CREATE INDEX oauth_refresh_tokens_expiry_idx ON oauth_refresh_tokens (expires_at);
