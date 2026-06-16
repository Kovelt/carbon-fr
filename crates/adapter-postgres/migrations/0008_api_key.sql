-- Registre des clés API du tier hébergé (ADR-0015). On ne stocke **jamais** la
-- clé en clair : seulement son empreinte (SHA-256 hex), comme pour le compteur
-- de visiteurs. L'identité (email…) est minimale et hors de cette table en v1 :
-- une clé porte un tier et un libellé non-sensible.
CREATE TABLE IF NOT EXISTS api_key (
    key_hash    TEXT        NOT NULL PRIMARY KEY,
    tier        TEXT        NOT NULL,
    label       TEXT        NOT NULL DEFAULT '',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
