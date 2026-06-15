# Architecture Decision Records (ADR)

Chaque décision structurante est tracée ici au format **contexte → décision → conséquences**, avec ses alternatives. Une décision ne se modifie pas en place : on crée un nouvel ADR qui *supersède* l'ancien.

## Statuts

`Proposé` · `Accepté` · `Déprécié` · `Remplacé par ADR-XXXX`

## Liste

| N° | Titre | Statut |
| --- | --- | --- |
| [0001](0001-langage-rust.md) | Langage : Rust | Accepté |
| [0002](0002-architecture-hexagonale-et-workspace.md) | Architecture hexagonale + workspace | Accepté |
| [0003](0003-perimetre-donnees-et-source-odre.md) | Périmètre national+régional & source ODRÉ | Accepté |
| [0004](0004-stockage-postgresql-natif.md) | Stockage : PostgreSQL natif | Accepté |
| [0005](0005-methodologie-carbone.md) | Méthodologie carbone (RTE + enrichissement engagé) | Accepté |
| [0006](0006-cycle-de-vie-revision-donnees.md) | Cycle de vie & révision des données (millésime + upsert) | Accepté |
| [0007](0007-topologie-deploiement.md) | Topologie de déploiement (API VPS, site statique, sous-domaine Kovelt) | Accepté |
| [0008](0008-methodologie-acv-ademe-et-regional.md) | Méthodologie cycle de vie (`acv-ademe`) & intensité régionale | Accepté |
| [0009](0009-modele-prevision-climatologie.md) | Modèle de prévision d'intensité (`climatology@1`) | Accepté |
| [0010](0010-methodologie-acv-ademe-consumption.md) | `acv-ademe` *consumption-based* (imports ENTSO-E) — fait évoluer ADR-0008 | Proposé |
| [0011](0011-contrat-prevision-forecastpoint.md) | Contrat de prévision `ForecastPoint` (intervalles) — raffine ADR-0009 | Accepté (contrat) |
| [0012](0012-modele-prevision-ml-gbdt.md) | Modèle de prévision ML (`GbdtForecaster` tout-Rust + météo) | Proposé |
| [0013](0013-prevision-acv-ademe.md) | Prévision `acv-ademe` (prévoir les entrées → calculateur) | Proposé |
| [0014](0014-usage-scheduling-streaming.md) | Usage : primitives carbon-aware + livraison live (SSE) | Proposé |
| [0015](0015-tier-heberge-cles-api.md) | Tier hébergé : clés API en couche de bord, anonyme par défaut | Proposé |

## Gabarit

```markdown
# ADR-XXXX — Titre

- **Statut** : Proposé | Accepté | Déprécié | Remplacé par ADR-YYYY
- **Date** : AAAA-MM-JJ

## Contexte
Le problème, les forces en présence, les contraintes.

## Décision
Ce qu'on décide, formulé clairement.

## Conséquences
Positives, négatives, et ce que ça nous engage à faire ensuite.

## Alternatives envisagées
Les options écartées et pourquoi.
```
