# ADR-0004 — Stockage : PostgreSQL natif (sans extension)

- **Statut** : Accepté
- **Date** : 2026-06-14

## Contexte

On stocke une série temporelle au pas quart d'heure, sur le national + 12 régions, avec un historique remontant à 2012/2013 — soit de l'ordre de **5 à 6 millions de lignes (~1 Go)**. Besoins : servir le temps réel et l'historique, et produire des **rollups** (horaire/journalier) pour les statistiques et le futur modèle de prévision. Le projet est OSS, souverain, auto-hébergeable, avec un éventuel tier hébergé.

TimescaleDB (extension PostgreSQL spécialisée séries temporelles) était le candidat naturel, mais son modèle de licence pose problème : son **cœur est sous Apache 2.0** (hypertables, compression), tandis que les **agrégats continus et les politiques de rétention** relèvent de l'édition Community sous **Timescale License (TSL)** — source-available, **non OSI**, avec une clause interdisant de revendre le logiciel en tant que service.

## Décision

Utiliser **PostgreSQL natif, sans extension** :

- partitionnement **déclaratif par plage temporelle** + index `BRIN` sur l'horodatage + index `(region, horodatage)` ;
- **rollups via vues matérialisées** rafraîchies par le poller ;
- choix encapsulé derrière le port `IntensityRepository`.

## Conséquences

- **Licence 100 % OSI** (PostgreSQL) → aucune zone grise dans un projet OSS souverain.
- Installation triviale partout (toute distribution, tout hébergeur), rien à épingler ni à suivre en compatibilité d'extension lors des montées de version.
- Performance largement suffisante au volume visé.
- Coût : les rollups se codent à la main (le rafraîchissement des vues matérialisées est complet, non incrémental) — négligeable à ce volume.
- **Réversible** : le port `IntensityRepository` permet d'ajouter un adapter TimescaleDB plus tard si l'ingestion ou le volume explosent, sans toucher au domaine.

## Alternatives envisagées

- **TimescaleDB Apache-2** : hypertables + compression en Apache, mais **sans agrégats continus** (TSL). On prendrait la contrainte d'une extension sans le bénéfice ergonomique qui la justifie — le pire compromis ici.
- **TimescaleDB Community (TSL)** : agrégats continus + rétention automatiques (ergonomie maximale), mais licence source-available **non OSI** et clause « pas de DBaaS ». Entorse directe au positionnement souverain/OSS, et visible dans un repo public.

## Addendum (2026-06-20) — implémentation des rollups

La décision (PostgreSQL natif, sans extension) reste valide. Deux points d'implémentation ont évolué depuis :

- **Rollups** : d'abord des **vues matérialisées** (migration `0002`), puis remplacées par de **vraies tables incrémentales** upsertées par seau (migration `0010_rollups_incremental.sql`). Le rafraîchissement n'est donc plus complet mais ciblé sur les seaux touchés ; la surface de lecture est inchangée.
- **Partitionnement déclaratif + index `BRIN`** : **reportés** (la table `measurement` reste simple, cf. commentaire de la migration `0001`). À reconsidérer maintenant que l'historique complet est ingéré ; le choix reste réversible via le port `IntensityRepository`.
