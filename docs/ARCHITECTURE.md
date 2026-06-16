# Architecture — `carbon-fr`

Ce document décrit la conception du système : la vision, les contraintes, le découpage hexagonal, le modèle de données et la roadmap. Les décisions ponctuelles et leurs alternatives sont tracées dans les [ADR](adr/).

## 1. Vision & contraintes

`carbon-fr` rend la donnée carbone du réseau électrique français **directement consommable par des développeurs et des machines**, là où la source officielle reste brute et plafonnée.

Contraintes structurantes :

- **Souveraineté** : aucune dépendance propriétaire obligatoire, auto-hébergeable, licences OSI.
- **Dev-first** : API lisible, index humainement interprétable, SDK, OpenAPI.
- **Résilience au quota** : la source impose 50 000 appels/utilisateur/mois ; l'architecture ne doit pas exposer ce plafond aux clients.
- **Évolutivité méthodologique** : la façon de calculer le carbone va évoluer ; elle doit être isolée et versionnée.

### Non-objectifs

`carbon-fr` n'est **pas** :

- un outil de **pilotage réseau temps réel** ou de sûreté du système électrique ;
- un outil de **facturation, de comptage commercial ou de trading** de MWh ;
- une source couvrant les **territoires non métropolitains** (DOM-TOM) au lancement ;
- un **substitut à la source officielle** RTE : il la re-traite et la rend consommable, sans s'y substituer.

## 2. Le constat clé : la prévision n'existe pas dans la source

C'est le point le plus important du projet.

RTE/ODRÉ publie une **estimation des émissions** au pas quart d'heure, mais uniquement pour le passé et le présent. Les seules **prévisions** présentes dans le jeu de données temps réel sont des prévisions de **consommation** (J-1 et J), *pas* d'intensité carbone.

Conséquence sur le découpage de la valeur :

| Endpoint | Nature | Difficulté |
| --- | --- | --- |
| `/intensity/now`, `/mix`, `/intensity/date` | Repackaging propre de données existantes | Faible |
| `/intensity/forecast`, `/greenest-window` | **Valeur créée** : il faut modéliser | Élevée |

La prévision est donc une **responsabilité du service**, pas un simple proxy. Le modèle est branché derrière un port (`ForecastModel`) : on démarre avec un modèle statistique simple, on le remplace par du ML plus tard sans toucher au reste. À titre de référence, le modèle britannique repose sur du machine learning et de la modélisation réseau, avec une prévision à 96 h et plus.

## 3. Quota, ingestion & cache

Le quota (50 000 appels/utilisateur/mois) est un faux problème **à une condition** : un seul composant tape la source.

- Le national temps réel se rafraîchit toutes les 15 min → ~96 récupérations/jour.
- Le régional temps réel se rafraîchit toutes les heures, et **un seul appel renvoie les 12 régions** → ~24 récupérations/jour (pas ×12).
- Un **poller unique** (singleton) alimente la base ; l'API sert ensuite **tous** les clients depuis la base. En haute disponibilité, l'écriture reste assurée par un seul poller (élection de leader) pour éviter les appels en double.

Budget indicatif : ~96/jour (national) + ~24/jour (régional) ≈ **~120 appels/jour, soit ~3 700/mois — moins de 8 % du quota**. Le plafond ne concerne que les intégrateurs qui taperaient RTE en direct, précisément ce que `carbon-fr` leur évite. C'est aussi ce qui justifie l'existence du service par rapport à « tape l'open data toi-même ».

> La base joue le rôle de **read-model** (le « cache ») : pas de couche de cache séparée au départ. Une couche mémoire chaude pour `/intensity/now` pourra être ajoutée plus tard si le besoin se présente.

**Backfill historique** : le rapatriement de l'historique (2012→) ne passe **pas** par l'API paginée — cela brûlerait le quota — mais par l'**export en masse** du jeu de données ODRÉ (un téléchargement), réalisé une fois puis maintenu à jour par le poller.

## 4. Architecture hexagonale (ports & adapters)

```
              Adapters entrants                 Adapters sortants
              (driving)                          (driven)
        ┌─────────────────────┐          ┌────────────────────────────┐
        │  API HTTP (axum)     │          │  OdreClient  → Eco2mixSource│
        │  CLI (plus tard)     │          │  PgRepository→ IntensityRepo│
        └──────────┬──────────┘          │  StatForecaster→ForecastModel│
                   │                      └──────────────┬─────────────┘
                   │ appelle les cas d'usage             │ implémentent les ports
                   ▼                                     ▼
        ┌───────────────────────────────────────────────────────────┐
        │                        core (lib pure)                     │
        │   application/  cas d'usage (ports entrants)               │
        │   ports/        traits sortants (Eco2mixSource, …)         │
        │   domain/       CarbonIntensity, Index, GenerationMix,     │
        │                 Region, Measurement, greenest_window()     │
        │                  ── aucune IO, aucune dépendance infra ──   │
        └───────────────────────────────────────────────────────────┘
                   ▲
                   │ assemble tout
        ┌──────────┴──────────┐
        │  bin/server         │  ← composition root
        │  (le seul à connaître les implémentations concrètes)
        └─────────────────────┘
```

**Ports sortants** (le domaine *demande*, l'infra *fournit*) :

- `Eco2mixSource` — récupérer la donnée RTE (dernier point, plage).
- `IntensityRepository` — lire/écrire les mesures (chaud + historique).
- `ForecastModel` — produire une prévision *(phase 3)*.
- `Clock` — fournir l'instant courant (testabilité).

**Ports entrants** (cas d'usage exposés) : `GetCurrentIntensity`, `GetMix`, `GetIntensityHistory`, `IngestLatest` (le poller), `FindGreenestWindow`, `GetForecast`.

**Pourquoi ce pattern ici précisément** :

1. *Risque source* (RTE change, quota, panne) → ajouter un adapter de secours (ENTSO-E, voire Electricity Maps) derrière le même `Eco2mixSource`, sans toucher au domaine.
2. *Roadmap incrémentale* → la phase 3 (prévision) n'est qu'un nouvel adapter derrière `ForecastModel`. La séparation « données repackagées » / « valeur créée » est matérialisée dans le code.
3. *Testabilité* → le `core` se teste avec des fakes en mémoire, sans base ni réseau.

## 5. Modèle de données

Unité d'observation : une **mesure** = `{ horodatage, région, intensité (gCO₂eq/kWh), méthodologie (identifiant + version), millésime, mix de production (optionnel) }`, au pas quart d'heure. Les champs **méthodologie** (voir §6 et ADR-0005) et **millésime** (voir ci-dessous et ADR-0006) sont portés dès le départ.

**Clé d'unicité** : `(région, horodatage, méthodologie)`. La méthodologie fait partie de la clé car deux méthodes produisent deux valeurs distinctes pour le même instant (§6).

Périmètre : `National` + 12 régions métropolitaines (couverture éCO2mix régional). Historique : les jeux consolidés/définitifs RTE remontent à 2012 (national) / 2013 (régional) — base du modèle de prévision en phase 3.

### Cycle de vie de la donnée (révisions)

RTE **révise** ses données : le temps réel du mois M est remplacé par des données « consolidées » (M+1), puis « définitives » (A+1). La donnée n'est donc **pas purement append-only**. Chaque mesure porte un **millésime** (`tr` | `consolidated` | `definitive`) ; l'ingestion fait un **upsert** sur la clé d'unicité et n'écrase une valeur que par un millésime de qualité supérieure (`definitive` > `consolidated` > `tr`). L'API sert toujours la meilleure version disponible et expose le millésime. Détails et alternatives : **ADR-0006**.

**Stockage : PostgreSQL natif, sans extension** (voir ADR-0004).

- Partitionnement **déclaratif par plage temporelle** (mensuel), index `BRIN` sur l'horodatage, index sur `(region, horodatage)`. Le BRIN reste pertinent : les insertions arrivent ordonnées dans le temps ; les révisions sont des `UPDATE` ciblés (upsert), pas une remise en cause de l'ordre physique.
- **Rollups** (horaire/journalier) pour les statistiques et le modèle : **vues matérialisées** rafraîchies par le poller. Le rafraîchissement natif est complet (non incrémental) ; à ce volume (~5–6 M lignes, ~1 Go) c'est sans conséquence. Le rafraîchissement doit être déclenché après toute révision touchant la période agrégée.
- Choix **réversible** : le port `IntensityRepository` permet d'ajouter un adapter TimescaleDB plus tard si le volume ou l'ingestion l'exigent.

## 6. Méthodologie carbone

Un chiffre d'intensité n'est comparable que si sa méthode est explicite. Distinctions à exposer dans l'API :

- émissions **directes** (combustion) vs **cycle de vie** (ACV) ;
- imports/exports inclus ou non ;
- périmètre géographique.

**Décision actée (ADR-0005)** :

- **Base MVP** : reprendre l'estimation RTE telle quelle (émissions de la production en France), directement comparable à éCO2mix.
- **La méthodologie est un attribut de premier ordre, versionné et sélectionnable**, dès le MVP : chaque mesure (domaine) et chaque réponse (API) porte un identifiant (`methodology`, ex. `rte-direct` + version), même s'il n'existe qu'une valeur au lancement.
- **Enrichissement engagé** : une méthode additionnelle `acv-ademe` (cycle de vie via Base Carbone ADEME + imports interconnexions, façon modèle UK) **coexistera** avec `rte-direct`, sans rupture de contrat. Sa spécification fera l'objet d'un ADR dédié.

Conséquence : le type `Measurement` du domaine porte un champ `methodology` dès la phase 1, de sorte que l'ajout d'une méthode n'altère jamais le sens d'une donnée déjà publiée.

## 7. Découpage en crates

| Crate | Rôle | Dépendances notables |
| --- | --- | --- |
| `core` | domaine + cas d'usage + ports | aucune IO |
| `adapter-odre` | `Eco2mixSource` via HTTP | reqwest, serde |
| `adapter-postgres` | `IntensityRepository` via SQL | sqlx |
| `adapter-http` | API REST | axum, serde |
| `server` (bin) | composition root | toutes les précédentes |

## 8. Roadmap

1. **Socle** — `core` + poller (`IngestLatest`) + `/intensity/now` + `/mix`, national.
2. **Historique + régional** — backfill consolidé, `/intensity/date`, 12 régions, vues matérialisées de rollup.
3. **Prévision** — `StatForecaster` derrière `ForecastModel` → `/forecast` + `/greenest-window` ; ML ultérieurement.
4. **DX** — SDK (crate + client TS), OpenAPI, conteneur Docker, éventuel tier hébergé.
5. **Méthodologie enrichie** — méthode additionnelle `acv-ademe` (cycle de vie Base Carbone ADEME + imports interconnexions), coexistant avec `rte-direct` (livrée : production `@1` et consommation `@2`, ADR-0008/0010).

## 9. Déploiement

Deux composants aux besoins distincts (détails et alternatives : **ADR-0007**) :

- **Site + doc + playground** : statique (SSG type Zola/mdBook), hébergé sur l'**hébergement mutualisé o2switch**. Facilement redéplaçable.
- **API** : service (`carbonfr-server` + PostgreSQL co-localisé + poller + reverse proxy/TLS) sur un **VPS FR/EU** géré par Kovelt. IP fixe → enregistrement DNS simple, **pas de DNS dynamique**.
- **DNS** : sous-domaine Kovelt (ex. `carbon-fr.kovelt.fr`).
- **Contrat d'URL** : l'API est versionnée dans le chemin (`/v1/…`) dès le départ, pour migrer de domaine ou faire évoluer l'API sans casser les intégrations.

Le packaging (`docker-compose` vs binaire + systemd) et la forme du poller (intégré au `server` vs binaire `bin/poller` + timer systemd) sont tranchés en phase 4.

## 10. Sources

- RTE — éCO2mix, émissions de CO₂ : <https://www.rte-france.com/eco2mix/les-emissions-de-co2-par-kwh-produit-en-france>
- ODRÉ — éCO2mix national temps réel : <https://odre.opendatasoft.com/explore/dataset/eco2mix-national-tr/>
- ODRÉ — éCO2mix régional temps réel : <https://odre.opendatasoft.com/explore/dataset/eco2mix-regional-tr/>
- Modèle de référence (UK) : <https://carbonintensity.org.uk/> et <https://api.carbonintensity.org.uk/>
- Concurrent fermé : <https://www.electricitymaps.com/>
