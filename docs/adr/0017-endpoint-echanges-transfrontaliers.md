# ADR-0017 — Endpoint public des échanges transfrontaliers

- **Statut** : Accepté (**implémenté** — `GET /v1/exchanges` + `GET /v1/exchanges/date`)
- **Date** : 2026-06-16
- **S'appuie sur** : ADR-0002 (hexagonal), ADR-0007 (API versionnée `/v1`), ADR-0010 (contexte d'import ENTSO-E déjà ingéré)

## Contexte

Le poller ingère déjà, toutes les 15 min, le **contexte d'import transfrontalier** (ENTSO-E) : pour chacune des 6 frontières métropolitaines, le **flux net signé** FR↔voisin et l'**intensité carbone** (cycle de vie ADEME) du voisin, alignés au pas quart d'heure (ADR-0010, store `cross_border_flow`). Cette donnée n'alimentait jusqu'ici que le calcul `acv-ademe@2` *consumption-based*, **sans être exposée**.

Un besoin produit (carte des voisins du dashboard : flèches directionnelles import/export par pays + couleur selon le carbone du voisin) demande à **servir** cette donnée. `/v1/mix.echanges` ne donne que le **solde net agrégé** (un scalaire), pas la répartition par pays.

## Décision

Exposer la donnée déjà stockée via un nouvel endpoint **`GET /v1/exchanges`** (sous `/v1`, contrat versionné — ADR-0007), **sans nouvelle ingestion ni nouveau port** :

- cas d'usage pur `GetCrossBorderExchanges` (lit `CrossBorderRepository` + ancre l'horodatage sur la dernière mesure nationale, pour cohérence avec `/v1/intensity/now` et `/v1/mix`) ;
- DTO `ExchangesResponse` : solde net FR + totaux import/export + **détail par frontière** (`country`, `country_name`, `flow_mw` signé, `direction`, `intensity` du voisin).
- **Convention de signe** : `flow_mw > 0` = import vers la France, `< 0` = export. Documentée dans l'OpenAPI.

Le domaine et l'ingestion sont **inchangés** : c'est strictement une projection de lecture (cohérent ADR-0002). La donnée reste au pas quart d'heure et suit les révisions ENTSO-E par upsert.

## Conséquences

- **Pas de coût d'ingestion** : la donnée existe déjà ; l'endpoint est une exposition.
- **Honnêteté** : `gb` (Royaume-Uni) est indisponible côté ENTSO-E depuis le Brexit → simplement absent des frontières servies (pas d'entrée fictive).
- **Périmètre** : limité aux **6 frontières de la France** (ce qu'ENTSO-E fournit proprement pour RTE). Une éventuelle **matrice européenne pays↔pays** (toutes frontières) serait un chantier distinct (ingestion élargie + quota + ADR dédié).
- **Série historique** : `GET /v1/exchanges/date?from=&to=` (fenêtre ≤ 366 j) sert la série derrière le même DTO par snapshot (`flows_range`), pour des courbes/animations dans le temps.
