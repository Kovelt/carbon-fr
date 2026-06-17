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
