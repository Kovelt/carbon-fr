# ADR-0006 — Cycle de vie & révision des données (millésime + upsert)

- **Statut** : Accepté
- **Date** : 2026-06-14

## Contexte

Les données éCO2mix ne sont pas figées. RTE les **révise** dans le temps :

- **temps réel** (`tr`) : publiées au fil de l'eau, issues de télémesures complétées par des estimations ;
- **consolidées** (`consolidated`) : vérifiées et complétées, livrées vers le milieu du mois M+1, qui **remplacent** le temps réel ;
- **définitives** (`definitive`) : livrées en A+1 une fois tous les comptages vérifiés.

Conséquence : notre stockage **n'est pas purement append-only**. Une même `(région, horodatage, méthodologie)` peut recevoir une valeur plus fiable plus tard. Sans stratégie explicite, on servirait des chiffres provisoires comme s'ils étaient définitifs, ou on dupliquerait les lignes.

## Décision

1. **Millésime** : chaque mesure porte un champ `vintage` ∈ { `tr`, `consolidated`, `definitive` }, avec un ordre de qualité `definitive > consolidated > tr`.
2. **Clé d'unicité** : `(région, horodatage, méthodologie)` — le millésime n'entre **pas** dans la clé.
3. **Ingestion par upsert** : à l'arrivée d'une donnée, on insère ou on met à jour la ligne existante **uniquement si le millésime entrant est de qualité supérieure ou égale** à celui stocké. Un `tr` n'écrase jamais un `consolidated`/`definitive`.
4. **Exposition** : l'API renvoie toujours la meilleure version disponible et **expose le millésime** dans la réponse, pour que le consommateur sache s'il lit du provisoire ou du définitif.
5. **Rollups** : toute révision touchant une période agrégée déclenche le rafraîchissement des vues matérialisées concernées.

## Conséquences

- Les consommateurs reçoivent toujours la donnée la plus fiable connue, et savent à quel point elle l'est.
- Le stockage reste compact (une ligne par mesure et par méthode), sans historique des versions intermédiaires.
- Le port `IntensityRepository` doit exposer une opération d'**upsert conditionnel au millésime** (pas un simple `insert`).
- L'index `BRIN` sur l'horodatage reste pertinent : les insertions restent ordonnées dans le temps, les révisions sont des `UPDATE` ciblés.

## Alternatives envisagées

- **Conserver tous les millésimes** (clé incluant `vintage`, donc plusieurs lignes par instant) : permet d'auditer l'écart temps réel vs définitif, mais multiplie le volume et complique les lectures « meilleure version ». Écarté pour le MVP ; réintroductible plus tard si un besoin d'audit apparaît (le port le permettrait).
- **Ignorer les révisions** (ne garder que le temps réel) : simple, mais on servirait des chiffres durablement faux par rapport aux données consolidées/définitives de RTE. Inacceptable pour une API qui se veut référence.

## Addendum (2026-06-20) — rollups en tables incrémentales

Le point 5 est conservé dans son intention (toute révision touchant une période agrégée recalcule les rollups concernés), mais l'implémentation est passée des **vues matérialisées** (migration `0002`) à des **tables de rollup incrémentales** rafraîchies par seau (migration `0010`). L'invariant fonctionnel est inchangé. L'index `BRIN` évoqué en Conséquences a été livré le 2026-08-15 (migration `0012`) — cf. addendum ADR-0004 ; seul le partitionnement reste reporté.

## Addendum (2026-09-25) — partitionnement de `measurement` (ADR-0004) : renvoi

Le partitionnement déclaratif de `measurement`, reconsidéré dans l'itération I7 du plan, **n'est pas retenu pour l'instant** — mesures, seuils de déclenchement et procédure de ré-évaluation dans l'addendum du 2026-09-25 de l'[ADR-0004](0004-stockage-postgresql-natif.md). Point de vigilance pour le jour où il sera adopté : le partitionnement par plage temporelle **ne remet pas en cause** l'upsert conditionnel au millésime (décision 3 ci-dessus). La clé de partition serait `at`, qui est **immuable** pour une ligne donnée : une révision `tr → consolidated → definitive` ne change que l'intensité, le millésime et le mix, jamais `at`. L'upsert d'une révision cible donc **structurellement** la même partition que la ligne d'origine, quel que soit le délai de révision (définitive en A+1 comprise) — aucun upsert inter-partitions n'est possible.
