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

## Addendum — 2026-09-23 : budget de quota réel avec le régional

Le budget « ~6 % du quota » de la décision valait pour le **seul national**. Depuis l'ingestion régionale au même cycle (un appel ODRÉ par région, ADR-0008), le poller fait **14 appels par cycle** (1 national + 12 régions + 1 charge) × 96 cycles/jour ≈ **1 344 appels/jour ≈ 40 300/mois, soit ~80 % du quota de 50 000** (recompté sur le code du poller, `bin/server`). La décision tient — le quota reste absorbé par construction et indépendant du nombre de clients — mais la **marge est fine** : ne pas densifier le poll ni ajouter de jeu ODRÉ sans recompter (ARCHITECTURE §3, métrique `upstream_requests_total{source="odre"}`, ADR-0022).

## Addendum — 2026-09-25 : fenêtre glissante (`IngestRecent`) — un point isolé par un retard de publication n'était jamais rattrapé

**Diagnostic** (Prometheus, code du poller avant ce correctif — `bin/server/src/main.rs` `spawn_poller`) : le poller (toutes les 900 s, national + 12 régions) n'ingérait que la **dernière** mesure par zone (`IngestLatest` → `Eco2mixSource::latest`, `order_by date_heure desc, limit 1`). Or le jeu `eco2mix-national-tr` est publié avec **~30 min de retard** et contient, entre deux publications, des lignes **futures pré-remplies à valeurs nulles** (grille horaire complète, `taux_co2`/mix pas encore connus à cet horodatage). Conséquence mesurée : 96 cycles/jour sans trou de *service* (le poller tourne bien), mais **~2 points nationaux perdus par jour en moyenne** (94/96 effectivement ingérés) — un point manqué à un cycle n'était jamais relu, faute de second passage sur ce créneau. En août, des indisponibilités amont prolongées ont ainsi laissé des **trous de plusieurs jours** que le poller lui-même ne rattrapait jamais (seul un backfill manuel les a comblés). ~13 erreurs d'ingestion/jour (~1 %) au global, cohérent avec ce taux de perte.

**Fix** : nouveau cas d'usage additif `IngestRecent` (`crates/core/src/application/ingest_recent.rs`) — `IngestLatest` reste disponible pour compatibilité SemVer (ADR-0030 §3) mais n'est plus appelé par le poller. À chaque cycle, `IngestRecent` relit une **fenêtre glissante** des `N` dernières heures (`CARBONFR_POLL_WINDOW_HOURS`, défaut 3 h) via le port **existant** `Eco2mixSource::range` — celui déjà prévu pour le « rattrapage de courts trous » (doc du trait, décision initiale ci-dessus) — puis upsert conditionnellement au millésime (ADR-0006 : réécrire un point déjà stocké au même millésime est un no-op fonctionnel, sans coût réel). Un point manqué à un cycle est ainsi relu automatiquement aux cycles suivants tant qu'il reste dans la fenêtre — **sans second appel ODRÉ** : toujours un seul appel `range()` par zone et par cycle (3 h ≈ 12 points au pas quart d'heure, tient largement dans une page, `PAGE_SIZE = 100`).

**Pourquoi ce n'est pas le « backfill via `range()` » proscrit (CLAUDE.md)** : cette règle vise le rapatriement de l'**historique** (mois/années, potentiellement des dizaines de milliers de points) — le plafond de pagination de l'API (`API_WINDOW = 10 000`) et le volume rendraient `range()` ruineux en appels, d'où l'export de masse (`Eco2mixArchive`, `BackfillHistory`). Une fenêtre de 3 h reste dans l'usage documenté de `range()` depuis l'origine : le rattrapage de courts trous. Aucun changement de périmètre du port, aucun appel supplémentaire.

**Vérifié dans l'adapter ODRÉ** (`crates/adapter-odre/src/lib.rs`) à cette occasion : `range()` national filtre déjà `taux_co2 is not null` (même filtre que `latest()`) et `range()` régional filtre déjà `consommation is not null` (même filtre que `latest_regional()`) — les lignes futures pré-remplies à valeurs nulles étaient donc **déjà exclues** des deux côtés ; aucune correction de filtre n'a été nécessaire.

**Impact quota** : nul. Le poller fait toujours exactement un appel `range()` par zone et par cycle, à la place d'un appel `latest()` — même cardinalité qu'avant (budget ~80 % du quota inchangé, addendum précédent).

**Pannes plus longues que la fenêtre : auto-réparation quotidienne.** Cause établie a posteriori par Prometheus (métriques conservées depuis juillet) : du 2026-08-25 au 2026-09-03, **100 % des appels ODRÉ échouaient** (1 248 erreurs/jour = 13 zones × 96 cycles) alors que le poller tournait sans interruption (96 cycles/jour) et que le service restait joignable ; retour à la normale sans intervention → cause **externe** (ODRÉ). Petites pannes du même type les 7-8 et 11-12 août. Aucune fenêtre de quelques heures ne rattrape une telle panne ; d'où une tâche de fond **une fois par jour** (10 min après le démarrage, puis toutes les 24 h) qui réimporte les `CARBONFR_SELF_HEAL_DAYS` derniers jours nationaux (défaut 7, 0 = désactivée, plafond 7 = fenêtre du recalcul incrémental des rollups) par **un export de masse** du jeu temps réel (`eco2mix-national-tr`, cf. `CARBONFR_BACKFILL_SOURCE=realtime`, v0.9.4) — la voie d'import prévue par cet ADR, pas l'API paginée. Upsert conditionnel au millésime : une valeur consolidée n'est jamais écrasée. **Coût : +1 appel/jour** sur le jeu national. Le régional reste couvert par la fenêtre glissante seule (pas d'export d'archive régional à ce jour).

**Quota, précisé par les en-têtes ODRÉ** (`x-ratelimit-dataset-*`, lus le 2026-09-25 depuis le serveur de production) : le plafond de 50 000 appels/mois est **par jeu de données** ; au 25 septembre, 45 480 restaient sur `eco2mix-national-tr` (≈ 190 appels/jour : mesure nationale + charge). Le poste le plus chargé est le jeu régional (12 appels par cycle).

**Détection.** Pendant la panne d'août, les alertes Prometheus existantes se sont bien déclenchées, mais aucun routage de notification n'existait : personne n'a été prévenu. Une règle sur l'âge de la donnée (`CarbonfrDataStale`, dernière mesure nationale > 2 h) complète `deploy/prometheus/alerts.yml` ; le routage vers un canal (Uptime Kuma, ou Alertmanager) reste le vrai prérequis (`deploy/README.md` §3).