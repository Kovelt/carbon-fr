-- Chaque abonnement webhook appartient à une clé API **existante** (ADR-0015,
-- addendum 2026-09-23 : révocation). L'invariant « pas d'abonnement orphelin »
-- est porté par la base, et non par l'ordre des requêtes applicatives : un
-- `POST /v1/webhooks` concurrent d'un `revoke-key` ne peut plus laisser un
-- abonnement dont la clé a disparu (toujours livré, mais plus listable ni
-- supprimable par personne — revue adversariale de `revoke-key`).
--
-- Purge préalable des éventuels orphelins (clé supprimée à la main en SQL avant
-- l'existence de `revoke-key`) : sans elle, la contrainte ne pourrait pas être
-- posée, et ces abonnements ne sont de toute façon plus gérables.
DELETE FROM webhook_subscription s
 WHERE NOT EXISTS (SELECT 1 FROM api_key k WHERE k.key_hash = s.owner_key_hash);

-- ON DELETE CASCADE : supprimer une clé emporte ses abonnements dans la même
-- instruction. Une création concurrente pose un verrou KEY SHARE sur la ligne
-- de la clé (contrôle de FK) : elle attend la révocation puis échoue, ou la
-- précède et son abonnement est emporté par la cascade.
ALTER TABLE webhook_subscription
    ADD CONSTRAINT webhook_subscription_owner_fk
    FOREIGN KEY (owner_key_hash) REFERENCES api_key (key_hash) ON DELETE CASCADE;
