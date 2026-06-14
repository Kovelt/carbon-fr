-- Compteur de consultations (visiteurs).
--
-- Aucune IP en clair : `visitor_hash` est un haché salé calculé par l'adapter
-- HTTP. Déduplication unique par (visiteur, jour) via la clé primaire.
CREATE TABLE IF NOT EXISTS visit (
    visitor_hash TEXT NOT NULL,
    day          DATE NOT NULL,
    PRIMARY KEY (visitor_hash, day)
);
