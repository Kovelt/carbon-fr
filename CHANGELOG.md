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
- **Prévision d'intensité** (phase 3, ADR-0009) : modèle `climatology@1`
  (climatologie horaire-de-semaine glissante + correction de persistance
  décroissante), fonction de domaine pure + adapter `ClimatologyForecaster`
  (alimenté par l'historique stocké). Exposée sous
  `GET /v1/intensity/forecast?from=&horizon_hours=` (série prévue) et
  `GET /v1/intensity/greenest-window?from=&horizon_hours=&window_minutes=`
  (créneau le plus bas-carbone). Prévisions **non persistées** (calculées à la
  lecture) ; l'identité du modèle est exposée dans chaque réponse.
- **Compteur de consultation** : `GET /v1/stats` + `POST /v1/stats/visit`
  (port `VisitCounter`). IP **jamais stockée** — empreinte SHA-256 salée
  (`CARBONFR_VISIT_SALT`), déduplication unique par IP/jour ; IP lue via
  `X-Forwarded-For`/`X-Real-IP`.
- **Documentation & gouvernance** : ADR 0001–0009 (+ addendum ADR-0003),
  `ARCHITECTURE.md`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `GOUVERNANCE.md`,
  et intégration continue GitHub Actions (fmt, clippy, tests + PostgreSQL).
- **Chaîne d'approvisionnement** : politique `cargo-deny` (`deny.toml`) vérifiée
  en CI — licences permissives en liste blanche (compatibles MIT/Apache-2.0),
  avis de sécurité RustSec, et sources de confiance.

### Notes

- `acv-ademe@1` est **basée production** : pour une région importatrice,
  l'intensité reflète la production locale, pas la consommation (imports =
  version consommation, `acv-ademe@2`).
- La prévision (`/forecast`, `/greenest-window`) relève de la phase 3.
