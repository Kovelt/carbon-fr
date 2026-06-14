# ADR-0001 — Langage : Rust

- **Statut** : Accepté
- **Date** : 2026-06-14

## Contexte

`carbon-fr` doit être à la fois un **service** (API auto-hébergeable, robuste, économe) et une **bibliothèque embarquable** (le cœur réutilisable hors du service). Le projet vise explicitement un trou de l'écosystème : des wrappers de l'API carbone existent en Python, JS, Rust… côté britannique, mais **rien d'équivalent pour la France**. Le projet s'inscrit aussi dans la cohérence technique de Kovelt (souveraineté, contrôle, performance).

## Décision

Le projet est écrit en **Rust** (edition 2024), runtime asynchrone **tokio**. Le `core` est conçu pour être publiable sur crates.io comme bibliothèque indépendante.

## Conséquences

- Sûreté mémoire et typage fort → moins de classes entières de bugs en production.
- Binaire unique, léger, sans runtime lourd → auto-hébergement simple, on-brand.
- Écosystème async mature (tokio, axum, sqlx, reqwest) couvrant tous nos besoins.
- Coût : temps de compilation, courbe d'apprentissage pour d'éventuels contributeurs — atténué par une doc soignée et l'architecture hexagonale qui isole le domaine.

## Alternatives envisagées

- **.NET / C#** : maîtrisé par l'auteur, excellent outillage, mais ne répond pas à l'objectif « combler un trou de l'écosystème Rust » et s'éloigne du positionnement « binaire souverain léger ».
- **Go** : déploiement simple, mais typage moins expressif pour modéliser proprement le domaine, et moins aligné avec la trajectoire technique de Kovelt.
- **Python** : prototypage rapide, mais inadapté à un service durable et économe pensé pour l'auto-hébergement.
