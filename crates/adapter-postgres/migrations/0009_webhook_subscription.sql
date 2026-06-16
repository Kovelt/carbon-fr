-- Abonnements webhook (ADR-0016). Possédés par une **empreinte de clé** API
-- (ADR-0015) ; le `secret` signe les livraisons (HMAC) et n'est jamais ré-exposé.
-- L'URL de rappel est validée anti-SSRF à l'inscription (domaine) ET re-validée
-- à la résolution DNS par l'adapter de livraison.
CREATE TABLE IF NOT EXISTS webhook_subscription (
    id             TEXT             NOT NULL PRIMARY KEY,
    owner_key_hash TEXT             NOT NULL,
    region         TEXT             NOT NULL,
    threshold      DOUBLE PRECISION NOT NULL,
    -- `below` | `above` (sens du franchissement).
    direction      TEXT             NOT NULL,
    callback_url   TEXT             NOT NULL,
    secret         TEXT             NOT NULL,
    created_at     TIMESTAMPTZ      NOT NULL DEFAULT now()
);

-- Liste par propriétaire (gestion) ; l'évaluation par le watcher lit tout.
CREATE INDEX IF NOT EXISTS webhook_subscription_owner
    ON webhook_subscription (owner_key_hash);
