-- Web Push: the browsers an account subscribed, and the server's own key
-- pair for it (VAPID, RFC 8292).
--
-- A subscription is a browser's, and one endpoint is of one account at a
-- time: the one that subscribed it last. It goes with the account (ON
-- DELETE CASCADE), and when its push service says it is gone.

CREATE TABLE push_subscriptions (
    -- The address of the push service the browser was given.
    endpoint    text COLLATE "C" PRIMARY KEY,
    account_id  text COLLATE "C" NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    -- The browser's P-256 public key and authentication secret, base64url
    -- as it gave them.
    p256dh      text NOT NULL,
    auth        text NOT NULL,
    created_at  timestamptz NOT NULL
);

-- An account's subscriptions, the oldest first.
CREATE INDEX push_subscriptions_account_idx ON push_subscriptions (account_id, created_at);

-- The server's private key, made when it first started with this
-- database. One row. A secret: it signs what the server sends to the push
-- services, and the browsers' subscriptions are bound to its public half,
-- so a new key ends every subscription.
CREATE TABLE push_key (
    one         boolean PRIMARY KEY DEFAULT true CHECK (one),
    private_key text NOT NULL,
    created_at  timestamptz NOT NULL
);
