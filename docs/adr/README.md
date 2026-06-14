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
