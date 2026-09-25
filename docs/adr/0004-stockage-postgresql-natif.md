# ADR-0004 — Stockage : PostgreSQL natif (sans extension)

- **Statut** : Accepté (partitionnement : mesuré le 2026-09-25, **non retenu pour l'instant** — seuils de déclenchement fixés, cf. addendum)
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
- **2026-08-15** : l'index `BRIN` sur `measurement(at)` a été livré (migration `0012`, audit perf) pour accélérer le rafraîchissement incrémental des rollups (fenêtre 7 j) — cf. CHANGELOG [0.7.0]. Seul le **partitionnement déclaratif** reste reporté.

## Addendum (2026-09-25) — partitionnement de measurement : mesuré, non retenu pour l'instant

Item I7 du plan ([docs/plan-iterations.md §I7](../plan-iterations.md)) : reconsidérer le partitionnement déclaratif « maintenant que l'historique complet est ingéré ». Mesures en production le 2026-09-25 (PostgreSQL 17.11, session en **lecture seule**, `BEGIN` / requêtes / `ROLLBACK` — aucune écriture).

### Mesures

**Taille et configuration.**

| | |
|---|---|
| `measurement` (table + index) | **148 Mo** total (91 Mo de heap + 57 Mo d'index : `measurement_pkey` 31 Mo, `measurement_region_methodology_at_idx` [migration 0001](../../crates/adapter-postgres/migrations/0001_measurement.sql) 26 Mo, `measurement_at_brin` [migration 0012](../../crates/adapter-postgres/migrations/0012_measurement_at_brin.sql) 24 **kilo**-octets) |
| Base entière (`carbonfr`) | 306 Mo |
| `shared_buffers` / `effective_cache_size` (`SHOW`) | 128 Mo / **4 Go** |
| Lignes exactes (`SELECT methodology_id, count(*) … GROUP BY`) | **537 282** (`acv-ademe` 281 859 sur 13 zones — national + 12 régions, ADR-0008 ; `rte-direct` 255 423 sur 1 zone national-only, addendum ADR-0003) — vs. 523 784 lignes estimées par `pg_class.reltuples` (dernier `ANALYZE`), écart ≈2,5 %, normal |
| Couverture temporelle (`min(at)`/`max(at)`) | `2012-01-01` → `2026-09-25`, soit **5 381 jours (≈14,7 ans)** |

**Latence (`EXPLAIN (ANALYZE, BUFFERS)`, production, 2026-09-25).**

| Requête | Plan | Temps mesuré |
|---|---|---|
| `GET /v1/intensity/date` national `rte-direct`, 366 j ([`handlers.rs:440`](../../crates/adapter-http/src/handlers.rs)) | Bitmap Heap Scan sur `measurement_region_methodology_at_idx` | **8,5 ms** |
| `GET /v1/intensity/date` régional `acv-ademe` (Bretagne, échantillon), 366 j | Index Scan Backward, même index | **4,3 ms** |
| Agrégat SQL équivalent à `/v1/intensity/stats` national `rte-direct`, 10 ans — **non atteignable via l'API publique** (voir note ⚠️ ci-dessous) | **Parallel Seq Scan** (2 workers) — aucun index ne sert un agrégat sur une plage `at` aussi large | **45,2 ms** |
| `GET /v1/intensity/stats` national `rte-direct`, 1 an ([`handlers.rs:537`](../../crates/adapter-http/src/handlers.rs)) | Bitmap Heap Scan sur `measurement_region_methodology_at_idx` | **4,5 ms** |
| Recalcul du rollup horaire, fenêtre glissante 7 j (`SELECT` du poller, [migration 0010](../../crates/adapter-postgres/migrations/0010_rollups_incremental.sql)) | Bitmap Heap Scan sur `measurement_at_brin` | **6,4 ms** |

### Interprétation

- ⚠️ **`/v1/intensity/date` et `/v1/intensity/stats` sont plafonnés à 366 jours par l'application**, pas seulement par convention : `MAX_HISTORY_SPAN = Duration::days(366)` ([`handlers.rs:40`](../../crates/adapter-http/src/handlers.rs)), vérifié à l'entrée des deux handlers ([`handlers.rs:469`](../../crates/adapter-http/src/handlers.rs) pour `/date`, [`handlers.rs:566`](../../crates/adapter-http/src/handlers.rs) pour `/stats`) — une requête `from`/`to` de 10 ans reçoit un **400** avant d'atteindre la base. La ligne « 10 ans » du tableau ci-dessus est donc un **agrégat SQL de contrôle** (même prédicat, exécuté hors du garde applicatif) servant de **proxy** au coût d'un scan complet — pas un appel reproductible via l'API publique. **En usage réel, aucune requête ne peut donc déclencher de seq scan** : toute fenêtre `/date` ou `/stats` valide (≤366 j) est couverte par l'index composite de la migration 0001.
- **Le BRIN (migration 0012) couvre déjà** l'unique point chaud identifié par l'audit perf 2026-08 (prédicat `at >= $1` seul du recalcul incrémental des rollups) : 6,4 ms mesurés, cf. addendum ADR-0006 du 2026-06-20. Rien à ajouter de ce côté.
- **L'index composite `(region, methodology_id, at DESC)`** de la migration 0001 couvre `/v1/intensity/date` et `/v1/intensity/stats` dès qu'un filtre `region`+`methodology` est posé : 4 à 9 ms, que la fenêtre fasse 366 j ou 1 an.
- **Le seul scénario mesuré en seq scan** (agrégat de contrôle sur 10 ans, national, non atteignable en pratique — voir note ⚠️ ci-dessus) reste rapide : **45 ms**, parallélisé sur 2 workers, ~172 000 lignes retenues par le prédicat sur les ~537 000 lignes parcourues (`rows=57468` + `Rows Removed by Filter=121626`, × 3 boucles : toute la table). C'est le scénario qu'un partitionnement par plage temporelle accélérerait le plus (élagage de partitions), **si** le plafond de 366 jours était un jour levé sur un usage interne (ex. export, tableau de bord d'exploitation) — non prévu à ce jour.
- **L'« historique complet ingéré » n'est pas une croissance organique** : l'essentiel des 537 282 lignes vient du **backfill en une fois** (~494 000 lignes, cf. [`CLAUDE.md:64`](../../CLAUDE.md) §`upsert_many`, depuis l'export de masse ODRÉ de l'ADR-0003), pas d'une accumulation lente depuis 2012. La moyenne brute « lignes totales ÷ 14,7 ans » (≈36 500 lignes/an) **mélange un événement ponctuel et la cadence réelle** — trompeuse comme proxy du rythme futur.
- **Rythme de croissance organique**, calculé depuis la répartition mesurée (lignes dont `at` tombe dans les 366 derniers jours — un bon proxy du rythme futur : que la ligne vienne du backfill ou du poller ne change rien à la densité par année de calendrier `at`) :
  - national `rte-direct` : 14 693 lignes/366 j → **≈14 660 lignes/an** ;
  - régional `acv-ademe` (échantillon **Bretagne uniquement**) : 2 196 lignes/366 j/zone → **≈2 190 lignes/an/zone**, extrapolé aux 13 zones `acv-ademe` : **≈28 490 lignes/an** ;
  - **total extrapolé ≈ 43 150 lignes/an** — extrapolation à partir d'une seule région échantillon, à confirmer si l'écart entre régions s'avère important lors de la prochaine mesure ;
  - **borne haute théorique** (pas quart d'heure complet, aucune lacune, 14 séries actuelles — cf. commentaire de la [migration 0001](../../crates/adapter-postgres/migrations/0001_measurement.sql), « volume national récent ~96 lignes/j ») : 96 × 365,25 × 14 ≈ **490 900 lignes/an**. Le rythme mesuré n'en représente qu'≈9 % ; écart **non investigué** dans le cadre de cette décision (lacunes de collecte ? autre cause ?) — retenu tel quel comme fourchette [mesuré, maximal théorique].
- **Délai avant un volume qui justifierait un partitionnement** (ordres de grandeur, depuis 537 282 lignes / 148 Mo) :

  | Cible | Au rythme mesuré (~43 150/an) | Au rythme maximal théorique (~490 900/an) |
  |---|---|---|
  | 2 millions de lignes | ~34 ans | **~3,0 ans** |
  | 5 millions de lignes | ~103 ans | **~9,1 ans** |
  | Taille totale ≈ `effective_cache_size` actuel (4 Go, extrapolation linéaire taille/lignes) | ~330 ans | **~29 ans** |

  Même dans l'hypothèse la plus rapide (rythme maximal théorique, aucune lacune de collecte), plusieurs années séparent la mesure du 2026-09-25 d'un volume qui justifierait l'opération.

### Décision

**Ne pas partitionner `measurement` maintenant.** Le partitionnement déclaratif reste une option **réversible** (port `IntensityRepository`, décision initiale ci-dessus), mais rien dans les mesures du 2026-09-25 ne le justifie : table et index tiennent largement dans `effective_cache_size` (148 Mo pour 4 Go) ; **aucune requête publique ne peut aujourd'hui déclencher de seq scan** (`/v1/intensity/date` et `/stats` plafonnés à 366 jours par `MAX_HISTORY_SPAN`) et le seul scénario de contrôle mesuré en seq scan (agrégat 10 ans) reste sous 50 ms ; le BRIN de la migration 0012 couvre déjà le point chaud identifié par l'audit perf 2026-08 ; la croissance organique mesurée (~43 150 lignes/an, jusqu'à ~490 900/an dans l'hypothèse la plus défavorable) laisse au minimum plusieurs années de marge avant tout seuil de volume raisonnable. Partitionner aujourd'hui ajouterait de la complexité opérationnelle (bornes de partitions à maintenir, risque de plan de requête dégradé si un prédicat ne couvre pas la clé de partition) sans bénéfice mesurable.

### Seuils de déclenchement d'une ré-évaluation

Rejouer la mesure dès que **l'un** des seuils suivants est franchi :

1. **Taille totale** de `measurement` (table + index, `pg_total_relation_size`) **≥ 4 Go** — aligné sur `effective_cache_size` actuel (`SHOW effective_cache_size`) : au-delà, un scan complet ne tient plus commodément dans le cache visé par le planificateur. Facteur ×27 par rapport aux 148 Mo mesurés.
2. **Volume** (`SELECT count(*) FROM measurement`) **≥ 5 millions de lignes**. Facteur ×9,3 par rapport aux 537 282 mesurées.
3. **Latence p95 en production**, mesurée sur une fenêtre glissante de 7 jours (si l'observabilité de l'ADR-0022 l'expose), d'une des requêtes nommées ci-dessus, dépassant durablement :
   - `GET /v1/intensity/date` ou `/stats`, fenêtre maximale autorisée (366 j), national : **100 ms p95** (baseline mesurée : 8,5 ms pour `/date`, 4,5 ms pour `/stats` sur 1 an — aucune requête publique ne dépasse 366 j, cf. `MAX_HISTORY_SPAN`) ;
   - recalcul de rollup horaire, fenêtre 7 j : **50 ms p95** (baseline 6,4 ms) — un dépassement dégraderait aussi la fraîcheur surveillée par l'alerte de l'ADR-0022 ;
   - si `MAX_HISTORY_SPAN` est un jour levé pour un usage interne : rejouer l'agrégat de contrôle sur 10 ans (baseline 45,2 ms) et lui fixer un budget **avant** de l'exposer.
4. **Rythme de croissance mesuré** (même méthode que ci-dessus : lignes dont `at` tombe dans les 366 derniers jours) **> 300 000 lignes/an** de façon soutenue (contre ~43 150/an mesurées) — signalerait une densification du poll ou une granularité/méthodologie supplémentaire, qui rapprocherait nettement les seuils 1 et 2.

**Procédure de ré-évaluation** : rejouer les 5 `EXPLAIN (ANALYZE, BUFFERS)` de ce relevé (`/date` national et régional 366 j, `/stats` 10 ans et 1 an, recalcul rollup 7 j), mesurer taille totale et volume exact, comparer aux baselines ci-dessus, documenter le nouveau relevé dans un **nouvel addendum daté sur cet ADR** (même format que celui-ci). Si le partitionnement est alors décidé : partitionnement déclaratif par plage temporelle, testé sur une **copie de prod** avec fenêtre de maintenance (item I7 du plan), en conservant le BRIN existant et en recréant l'index composite par partition — vérifier que le prédicat des requêtes chaudes (`region`, `methodology_id`, plage `at`) reste couvert par les index recréés, pour que l'élagage de partitions fonctionne.

> **Points à confirmer par Morgan**
> Les 4 valeurs numériques ci-dessus (4 Go, 5 millions de lignes, budgets de latence p95, 300 000 lignes/an) sont une **recommandation**, pas une contrainte technique dure : marge de ×5 à ×30 au-dessus des mesures du 2026-09-25, avec `effective_cache_size` comme ancre pour le seuil de taille. Si tu préfères des seuils plus serrés (ré-évaluation plus fréquente, moins de risque de surprise) ou plus larges (moins de bruit opérationnel), ajuste-les librement — ce sont des constantes d'exploitation, pas une décision d'architecture.
