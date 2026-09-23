-- Désactivation automatique des abonnements webhook après N livraisons échouées
-- consécutives (ADR-0016, addendum 2026-09-23 — point laissé ouvert par l'ADR
-- initial) : un endpoint mort n'est plus martelé à chaque franchissement de
-- seuil. Le compteur est remis à zéro par toute livraison réussie ;
-- `disabled_at` non nul = abonnement désactivé (exclu du watcher, toujours
-- listé pour son propriétaire, qui le recrée pour le réactiver).
ALTER TABLE webhook_subscription
    ADD COLUMN IF NOT EXISTS consecutive_failures INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS disabled_at TIMESTAMPTZ;
