## Quoi

<!-- Décris le changement en une ou deux phrases. -->

## Pourquoi

<!-- Le problème résolu / la valeur ajoutée. Lie l'issue et l'ADR concernés. -->

- Issue liée :
- ADR concerné (si décision structurante) :

## Checklist

- [ ] `cargo fmt --all` et `cargo clippy --all-targets -- -D warnings` passent.
- [ ] `cargo test --workspace` passe (le `core` se teste sans IO, avec des *fakes*).
- [ ] `cargo deny check` passe (licences/avis/sources — toute licence inédite = décision explicite dans `deny.toml`).
- [ ] Architecture hexagonale respectée : aucune IO (`reqwest`/`sqlx`/`axum`/`serde`) introduite dans `core` (ADR-0002).
- [ ] Aucune méthodologie publiée modifiée silencieusement ; toute nouvelle méthode = nouvelle version + ADR (ADR-0005).
- [ ] Pour toute décision structurante : un ADR a été ajouté dans `docs/adr/`.
- [ ] Branche à jour avec `main` (rebase) ; un commit = une intention.

<!--
`main` est protégée (ADR-0027) : pas de push direct ; PR + CI verte (5/5) +
branche à jour + conversations résolues obligatoires ; fusion en squash/rebase
(historique linéaire). La relecture se fait via le diff de la PR.
-->
