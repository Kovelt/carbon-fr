# Changelog

Tous les changements notables de ce projet sont consignés dans ce fichier.

Le format s'inspire de [Keep a Changelog](https://keepachangelog.com/fr/1.1.0/),
et le projet suit le [versionnage sémantique](https://semver.org/lang/fr/). En
phase `0.x`, des ruptures d'API peuvent survenir en *minor* (cf. GOUVERNANCE §6).

## [Non publié]

### Ajouté

- **Socle hexagonal** : crate `core` (domaine, cas d'usage, ports, sans IO),
  adapters `odre` (ODRÉ/éCO2mix), `postgres` (PostgreSQL natif) et `http`
  (axum), et binaire `carbonfr-server` (composition root + poller unique).
- **API `/v1`** (couverture nationale) :
  - `GET /v1/intensity/now` — dernière intensité carbone (gCO₂eq/kWh) ;
  - `GET /v1/mix` — mix de production par filière (MW) ;
  - `GET /v1/intensity/date?from=&to=` — série historique sur un intervalle ;
  - `GET /v1/intensity/stats?from=&to=[&interval=hour|day]` — résumé
    (moyenne/min/max) et série agrégée depuis les rollups ;
  - `GET /health` — sonde de disponibilité.
- **Backfill historique** national par export de masse ODRÉ
  (`carbonfr-server backfill`), upsert conditionnel au millésime.
- **Rollups** : vues matérialisées horaires et journalières, rafraîchies par le
  poller et le backfill.
- **Méthodologie `acv-ademe@1`** (cycle de vie ADEME, basée production, ADR-0008)
  coexistant avec `rte-direct` : dérivée et stockée à l'ingestion, sélectionnable
  via `?methodology=` sur les endpoints `/v1`.
- **Couverture régionale** (12 régions métropolitaines) : le poller ingère le
  mix régional (éCO2mix régional, `thermique` agrégé) et en dérive l'intensité
  `acv-ademe`. `rte-direct` reste national (taux_co2 publié par RTE).
- **OpenAPI 3.1** dérivée du code (`utoipa`) sous `GET /v1/openapi.json` +
  **Swagger UI** sous `GET /docs`.
- **Collection Bruno** versionnée (`bruno/`) couvrant tous les endpoints
  (cas nominaux national/régional × `rte-direct`/`acv-ademe`, et erreurs 400/404).
- **Documentation & gouvernance** : ADR 0001–0007 (+ addendum ADR-0003),
  `ARCHITECTURE.md`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `GOUVERNANCE.md`,
  et intégration continue GitHub Actions (fmt, clippy, tests + PostgreSQL).

### Notes

- `acv-ademe@1` est **basée production** : pour une région importatrice,
  l'intensité reflète la production locale, pas la consommation (imports =
  version consommation, `acv-ademe@2`).
- La prévision (`/forecast`, `/greenest-window`) relève de la phase 3.
