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
