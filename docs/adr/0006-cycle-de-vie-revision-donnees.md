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
