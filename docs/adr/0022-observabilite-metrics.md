# ADR-0022 — Observabilité : exposition Prometheus `/metrics`

- **Statut** : Accepté
- **Date** : 2026-06-17
- **S'appuie sur** : ADR-0007 (déploiement VPS), ADR-0019 (version exposée au démarrage)

## Contexte

L'API est **déployée et publique**. On avait les **logs** (`tracing`, format JSON en prod) et deux sondes (`/health`, `/health/ready`), mais **aucune métrique numérique** scrutable. Or les questions d'exploitation les plus importantes sont chiffrées :

- **L'ingestion est-elle vivante ?** Le poller alimente la base ; s'il échoue silencieusement, l'API sert de la donnée **gelée** sans rien casser. C'est la panne la plus sournoise : on veut une alerte sur la **fraîcheur**, pas un signalement d'utilisateur.
- **Où en est le quota amont ?** ODRÉ plafonne à 50 000 appels/mois ; on veut suivre la consommation (et celle d'Open-Meteo, ENTSO-E).
- **Combien de lignes ingérées, combien d'erreurs ?**

La latence/volume HTTP par route est déjà couverte par les logs (`TraceLayer`).

## Décision

**Exposer un endpoint `GET /metrics` au format texte Prometheus**, alimenté par un **registre fait maison, sans dépendance**.

### Pourquoi fait maison (pas `metrics`/`prometheus`)

Les métriques utiles ici sont des **compteurs et des jauges** — pas d'histogramme (la latence est dans les logs). Leur exposition en texte Prometheus est triviale. Un registre `Arc<{AtomicU64/AtomicI64…}>` + un `render()` couvre le besoin sans ajouter 2 crates (et leur surface `cargo deny`). C'est cohérent avec l'ethos du projet, qui a déjà du code maison là où une dépendance serait surdimensionnée (compteur de visiteurs, primitives de scheduling, exposition OpenAPI).

### Ce qui est exposé

| Métrique | Type | Usage |
| --- | --- | --- |
| `carbonfr_build_info{version}` | gauge (=1) | version du binaire en label (ADR-0019) |
| `carbonfr_poller_cycles_total` | counter | cycles de poll terminés |
| `carbonfr_poller_ingest_written_total` | counter | lignes de mesure écrites |
| `carbonfr_poller_ingest_errors_total` | counter | échecs d'ingestion (par région) |
| `carbonfr_upstream_requests_total{source}` | counter | appels amont (`odre`/`open-meteo`/`entsoe`) — **proxy de quota** |
| `carbonfr_poller_last_success_timestamp_seconds` | gauge | dernier cycle ayant écrit ≥ 1 ligne |
| `carbonfr_poller_last_measurement_timestamp_seconds` | gauge | horodatage de la dernière mesure nationale connue |
| `carbonfr_poller_last_price_timestamp_seconds` | gauge | horodatage de la dernière ingestion de prix spot (ADR-0023) |
| `carbonfr_poller_last_flows_timestamp_seconds` | gauge | horodatage de la dernière ingestion du contexte d'import transfrontalier (ADR-0010/0017) |

**Alerte phare** : `time() − carbonfr_poller_last_success_timestamp_seconds > 2 × intervalle de poll` ⇒ ingestion en panne.

### Placement

- **Hors du contrat `/v1`** (comme `/health`) : c'est un endpoint d'exploitation, en **texte** (pas du JSON versionné). Il n'apparaît donc **pas** dans l'OpenAPI et n'est pas soumis au garde-fou de contrat. Fusionné au routeur dans la composition root (`bin/server`), pas dans `adapter-http` (qui reste dédié au contrat public).
- **Le registre est alimenté par le poller**, lu par le handler — un `Arc` partagé, sans verrou (atomiques).
- **Accès** : non authentifié (ne révèle aucun secret — des compteurs et horodatages). En prod, **restreindre le scrape côté reverse proxy** (Traefik) si l'on ne veut pas l'exposer publiquement.

## Conséquences

- **Détection proactive** : la fraîcheur du poller et la consommation de quota deviennent des séries chiffrées, alertables (Prometheus/Alertmanager, ou un simple scrape + seuil).
- **Zéro dépendance ajoutée** ; `render()` est **testé unitairement** (présence et valeurs des métriques).
- **Engage** : alimenter les compteurs aux bons endroits du poller (déjà fait : ODRÉ par région + charge, Open-Meteo, ENTSO-E) ; tenir la liste à jour si une source amont s'ajoute.
- **Limite assumée** : `carbonfr_upstream_requests_total` compte les **appels initiés** (proxy), pas la facturation exacte côté fournisseur ; suffisant pour suivre une tendance de quota.

## Alternatives envisagées

- **Crates `metrics` + `metrics-exporter-prometheus`** — écarté : histogrammes gratuits mais 2 dépendances et plus de surface, pour un besoin couvert par des compteurs/jauges maison.
- **S'en tenir aux logs** — écarté : les logs répondent à « que s'est-il passé ? », pas à « depuis combien de temps la donnée est-elle gelée ? » sous forme alertable.
- **Exposer `/metrics` sous `/v1`** — écarté : ce n'est pas un contrat public versionné mais de l'exploitation ; le coupler à `/v1` brouillerait les deux.

## Addendum (2026-09-26) — quota ODRÉ réel

**Constat** : ODRÉ renvoie, sur **chaque** réponse (`records` comme `exports/json`), des en-têtes de quota **par jeu de données et par client (IP)** :

```
x-ratelimit-dataset-limit: 50000
x-ratelimit-dataset-remaining: 49977
x-ratelimit-dataset-reset: 2026-10-01 00:00:00+00:00
x-ratelimit-limit: 10000000            (quota API global, moins intéressant)
x-ratelimit-remaining: 9999959
x-ratelimit-reset: 2026-09-27 00:00:00+00:00
```

`carbonfr_upstream_requests_total{source="odre"}` (décision ci-dessus) n'en captait rien : c'est un **proxy** (appels initiés par le processus), pas ce qu'ODRÉ pense réellement du quota consommé sur cette IP. En prod, le jeu régional (`eco2mix-regional-tr`) était à ~55 % consommé à J26 du mois (22 422 restants sur 50 000) — cf. ADR-0003 addendum 2026-09-23. C'est le **prérequis explicite** posé par ce dernier addendum avant de densifier davantage le poll régional (plan I8, item PROD-3) : ne pas ajouter d'appel sans visibilité sur ce que le quota réel en dit.

### Décision

**Observer ces en-têtes de façon opportuniste**, sans appel supplémentaire, et les rendre en jauges Prometheus — en plus du proxy existant, pas à sa place.

**Placement (hexagonal)** : l'observation vit dans l'**adapter** (`crates/adapter-odre/src/quota.rs`, nouveau module `QuotaTracker` + `DatasetQuota`), pas dans `core`/`eligibility` (aucun nouveau port : ce n'est pas une donnée du domaine, seulement de l'exploitation d'un adapter concret) : `OdreClient` lit `resp.headers()` dans `fetch` et `fetch_export`, **avant** le contrôle du statut et avant de consommer le corps — à dessein, pour capter aussi les en-têtes d'un 429 (quota dépassé, le cas le plus probable pour porter `remaining: 0`) plutôt que de les perdre parce que la requête échoue — 0 appel HTTP en plus. Le **rendu** vit dans la composition root (`bin/server/src/metrics.rs`, `render_odre_quota` + `QuotaGauge`), qui reste indépendante des types de l'adapter (conversion faite dans `main.rs`). Un seul `QuotaTracker` est créé au démarrage du serveur et partagé (`with_quota_tracker`) entre les **deux** clients ODRÉ du processus qui exposent `/metrics` — le poller et l'auto-réparation quotidienne (ADR-0003 addendum 2026-09-25) — pas celui de la sous-commande `backfill`, un processus séparé sans `/metrics`.

**4 jauges ajoutées**, labellisées `dataset="…"` (une par jeu de données observé) :

| Métrique | Type | Usage |
| --- | --- | --- |
| `carbonfr_odre_quota_limit{dataset}` | gauge | plafond mensuel du jeu (`x-ratelimit-dataset-limit`) |
| `carbonfr_odre_quota_remaining{dataset}` | gauge | appels restants avant remise à zéro (`x-ratelimit-dataset-remaining`) |
| `carbonfr_odre_quota_reset_timestamp_seconds{dataset}` | gauge | prochaine remise à zéro (`x-ratelimit-dataset-reset`) ; **omise** pour un jeu sans cet en-tête (pas de `0` trompeur — `0` est un horodatage Unix valide) |
| `carbonfr_odre_quota_observed_timestamp_seconds{dataset}` | gauge | horodatage de la dernière observation pour ce jeu |

Observation **purement opportuniste** : si `limit` ou `remaining` est absent ou illisible, rien n'est enregistré (retour silencieux, jamais d'erreur — une observation manquée ne doit jamais faire échouer l'appel ODRÉ qui la porte).

**La « limite assumée » de la décision initiale est levée pour ODRÉ** : on ne dépend plus seulement du proxy `carbonfr_upstream_requests_total` pour ce fournisseur, on lit ce qu'il déclare lui-même. Le proxy **reste la seule visibilité pour Open-Meteo et ENTSO-E**, qui n'exposent pas ce genre d'en-tête de quota par jeu.

**Alertes** (`deploy/prometheus/alerts.yml`) :

- `CarbonfrOdreQuotaLow` — `carbonfr_odre_quota_remaining / carbonfr_odre_quota_limit < 0.10` pendant 30 min, `severity: warning`.
- `CarbonfrOdreQuotaExhausted` — `carbonfr_odre_quota_remaining == 0` pendant 15 min, `severity: critical`.

### Déclenchement

Prérequis explicite du **comblement régional** (PROD-1/PERF-3, plan `docs/plan-iterations.md` I8) : toute densification du poll sur `eco2mix-regional-tr` doit désormais pouvoir être vérifiée contre le quota **réel** (et alertée avant épuisement), pas seulement estimée par le proxy d'appels initiés.
