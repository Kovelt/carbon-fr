# ADR-0002 — Architecture hexagonale + workspace

- **Statut** : Accepté
- **Date** : 2026-06-14

## Contexte

Plusieurs forces propres à ce projet rendent le couplage à l'infrastructure dangereux :

- la **source** RTE peut changer, être plafonnée (quota) ou tomber ; on voudra des sources de secours (ENTSO-E, etc.) ;
- le **modèle de prévision** est amené à évoluer (statistique simple → ML) ;
- la **méthodologie carbone** évoluera (estimation RTE → cycle de vie) ;
- on veut **tester le cœur sans IO**, et réutiliser ce cœur comme bibliothèque embarquable.

Une architecture où le domaine dépendrait directement de RTE, de Postgres ou d'axum rendrait chacune de ces évolutions coûteuse.

## Décision

Adopter l'**architecture hexagonale** (ports & adapters) dans un **workspace Cargo multi-crates** :

- `core` : domaine + cas d'usage + **ports** (traits), **sans aucune IO** ;
- des crates **adapters** implémentant les ports (`adapter-odre`, `adapter-postgres`, `adapter-http`) ;
- un binaire `server` jouant le rôle de **composition root** (le seul à connaître les implémentations concrètes).

Règle d'or : **les dépendances pointent vers l'intérieur ; le domaine ne dépend de rien.**

## Conséquences

- Changer de source, de base ou de modèle = écrire un **nouvel adapter**, sans toucher au domaine ni à l'API.
- La **phase 3** (prévision) devient un simple adapter derrière le port `ForecastModel` : la roadmap est incrémentale « par construction ».
- Le `core` se teste avec des **fakes en mémoire** (pas de base ni de réseau en test).
- Le `core` est publiable comme bibliothèque réutilisable (angle SDK).
- Coût : **boilerplate de mapping** entre DTO d'adapter et types du domaine ; davantage de crates à orchestrer dans le workspace. Jugé acceptable au vu des bénéfices.

## Alternatives envisagées

- **Crate unique + modules** : plus léger au démarrage, mais s'appuie sur la discipline plutôt que sur le compilateur pour empêcher les fuites d'infrastructure dans le domaine. Le workspace rend ces frontières **mécaniquement** infranchissables (un crate `core` qui ne déclare pas `sqlx` ne peut pas l'utiliser).
- **Architecture en couches classique** : tend à laisser le domaine dépendre de la persistance ; précisément ce qu'on veut éviter ici.
