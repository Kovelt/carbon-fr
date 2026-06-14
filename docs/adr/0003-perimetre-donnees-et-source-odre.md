# ADR-0003 — Périmètre national+régional & source ODRÉ

- **Statut** : Accepté
- **Date** : 2026-06-14

## Contexte

La donnée carbone française est publiée par RTE dans **éCO2mix**, exposée en open data via **ODRÉ** :

- national temps réel, rafraîchi **tous les quarts d'heure** ;
- régional temps réel, rafraîchi **toutes les heures** ;
- jeux **consolidés/définitifs** pour l'historique (national depuis 2012, régional depuis 2013) ;
- un **quota** anti-robots de **50 000 appels par utilisateur et par mois**.

Point critique : la donnée d'**émissions** est disponible pour le passé/présent, mais les seules **prévisions** du jeu temps réel sont des prévisions de **consommation**, pas d'intensité carbone.

## Décision

- **Périmètre** : `National` + 12 régions métropolitaines (couverture éCO2mix régional).
- **Source primaire** : ODRÉ / éCO2mix (temps réel + consolidé), derrière le port `Eco2mixSource`.
- **Stratégie d'accès** : un **poller unique** récupère la donnée et la met en cache/base ; l'API sert ensuite tous les clients depuis la base. Budget ≈ quelques milliers d'appels/mois (~6 % du quota).
- **Prévision** : assumée comme **responsabilité du service** (modèle interne), pas comme une donnée de la source.

## Conséquences

- Le quota ne concerne plus les clients : il est absorbé par construction.
- L'historique doit être **backfillé** depuis les jeux consolidés (phase 2) pour alimenter statistiques et modèle.
- `/forecast` et `/greenest-window` nécessitent un modèle (phase 3) — voir ARCHITECTURE §2.
- Le port `Eco2mixSource` autorise l'ajout ultérieur de **sources de secours** sans impact sur le domaine.

## Alternatives envisagées

- **Electricity Maps** : donnée polie mais **fermée et payante**, alors qu'elle repose sur les mêmes données publiques. Contraire au positionnement souverain/OSS.
- **ENTSO-E (Transparency Platform)** : paneuropéen, intéressant comme source secondaire/comparaison, mais granularité et modèle différents ; gardé comme adapter futur, pas comme source primaire.
- **DOM-TOM / périmètre non métropolitain** : hors couverture éCO2mix régional standard ; exclu du périmètre initial.

## Addendum — 2026-06-14 : l'intensité carbone régionale n'est pas une donnée de la source

L'implémentation de l'adapter ODRÉ (`carbonfr-adapter-odre`) a mis au jour une contrainte non explicitée ci-dessus : le champ `taux_co2` (intensité carbone, gCO₂eq/kWh) n'existe **qu'au niveau national**. Vérifié sur le dataset temps réel `eco2mix-regional-tr`, qui ne publie que la **production par filière** (et son détail `tco_*`/`tch_*`), **sans aucune intensité**. C'est cohérent avec la nature de l'indicateur RTE : `taux_co2` est par construction un agrégat national (émissions de la production FR rapportées à la production totale) ; RTE ne publie pas de facteur d'émission régional officiel.

**La décision n'est pas remplacée, elle est précisée.** Le périmètre « National + 12 régions » reste l'objectif ; mais la **provenance** de l'intensité diffère selon l'échelle :

- **National** : `taux_co2` lu directement (méthodologie `rte-direct`, ADR-0005). Disponible dès la **phase 1**.
- **Régional** : l'intensité doit être **dérivée par un modèle** (facteurs d'émission par filière appliqués au mix régional, déjà fourni par le dataset régional). C'est une **valeur créée** par le service — au même titre que la prévision (ARCHITECTURE §2) —, pas une donnée de la source. Reportée en **phase 2**.

Conséquences :

- En phase 1, l'adapter ODRÉ renvoie `SourceError::NoData(region)` pour toute région ≠ `National`.
- Le port `Eco2mixSource` et le modèle de domaine sont **inchangés** : la dérivation régionale sera un calcul du domaine alimenté par la production régionale, exposé via une **méthodologie versionnée dédiée** (champ `methodology`). Elle ne doit **pas** être confondue avec `rte-direct` (intensité nationale publiée par RTE) et fera l'objet de son **propre ADR** quand elle sera spécifiée.
- Piste de secours si une intensité régionale « officielle » devenait nécessaire : un adapter `Eco2mixSource` secondaire (ENTSO-E, Electricity Maps) — sans impact sur le domaine.
