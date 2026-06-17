# ADR-0019 — Politique de versionnement (quatre axes découplés)

- **Statut** : Accepté, **engagé** (axe applicatif livré : version de workspace + exposition au démarrage + image Docker taguée sur `v*`)
- **Date** : 2026-06-17
- **S'appuie sur** : ADR-0005/0006 (versions de méthodologie & millésime portés par la donnée), ADR-0007 (déploiement, API `/v1`), ADR-0011 (versions de modèle)

## Contexte

`carbon-fr` n'a pas *un* versionnement mais **plusieurs**, qui ne bougent pas au même rythme. Trois étaient déjà décidés implicitement, un manquait :

1. **Contrat d'API HTTP** — `/v1`, `/v2`… L'URL **est** le contrat (ADR-0007/0011) : on ne bump que sur **rupture** de contrat, et les versions **coexistent**.
2. **Méthodologies & modèles** — `rte-direct`, `acv-ademe@1`/`@2`, `climatology@1`, `gbdt@1`. Version **portée par la donnée**, **immuable une fois publiée** (ADR-0005/0006/0009) : jamais de modification silencieuse, toute évolution = nouvelle version + nouvel ADR.
3. **SDK TypeScript** — `@carbon-fr/sdk`, SemVer, publié par tag `sdk-v*` (cf. `release-sdk.yml`).
4. **Le service / binaire déployé** — *c'était le trou* : les crates étaient toutes en `version = "0.0.0"`, aucun tag git, et les déploiements partaient de `main` HEAD **sans version traçable**. Impossible de répondre à « quel build répond en prod ? ».

Par ailleurs (ADR sur la publication crates.io) : les crates `carbonfr-*` **ne sont pas publiées séparément** sur crates.io — le service se distribue en image Docker (et `cargo install --git`). Les versionner individuellement n'a donc aucun intérêt.

## Décision

**Quatre axes, explicitement découplés.** Aucun ne pilote les autres.

### Axe applicatif (le manque comblé) — SemVer unique de workspace

- **Une seule `version`** dans `[workspace.package]` du `Cargo.toml` racine, **héritée** par toutes les crates (`version.workspace = true`). Pas de version par crate : le workspace se release d'un bloc.
- **Pré-1.0 (`0.y.z`)** tant qu'on se réserve des ruptures internes ; `1.0.0` le jour où le service est déclaré stable.
- **Tag git `vX.Y.Z`** → déclenche la construction et la publication de l'**image Docker taguée à l'identique** (+ `latest`) sur le registre. C'est **cette image taguée** qu'on déploie — fin du « `main` HEAD » non traçable.
- Le binaire **expose sa version au démarrage** (log `tracing` + `--version`), via `env!("CARGO_PKG_VERSION")` → on sait exactement quel build tourne.
- Garde-fou CI : le tag `vX.Y.Z` doit **correspondre** à la version du workspace (sinon échec), comme le SDK vérifie `sdk-v*` ↔ `package.json`.

### Les trois axes déjà en place — confirmés et figés ici

- **API** : `/v1` stable ; `/v2` seulement sur rupture, coexistant. Indépendant de la version applicative (une refonte interne `0.4.x → 0.5.0` ne touche pas `/v1`).
- **Méthodologies/modèles** : versionnées dans la donnée, immuables (ADR-0005/0006). Indépendantes du code : servir `acv-ademe@2` n'impose pas de bump applicatif majeur.
- **SDK** : suit le **contrat d'API**, pas la version du serveur. Sa ligne `sdk-v*` reste autonome.

### Règle de découplage (l'invariant à retenir)

> `v0.4.2` (code) ≠ `/v1` (contrat) ≠ `acv-ademe@2` (donnée) ≠ `sdk-v0.1.0` (client).

Correspondances typiques : refonte interne → bump applicatif seul ; rupture d'API → `/v2` **et** très probablement bump **majeur** applicatif **et** nouveau SDK ; nouvelle méthodologie → version de donnée + ADR, sans bump applicatif obligatoire.

## Conséquences

- **Traçabilité prod** : chaque déploiement = une image taguée = un commit taggé `v*`. `--version` et le log de démarrage répondent « quel build ? » sans deviner.
- **Releases simples** : `git tag v0.2.0 && git push origin v0.2.0` construit et pousse l'image. Aucun secret (auth registre via le `GITHUB_TOKEN` du workflow, même esprit OIDC que le SDK).
- **Engage** : tenir la version de workspace à jour avant de taguer (le garde-fou CI le force) ; rédiger un nouvel ADR pour toute rupture d'API ou de méthodologie (inchangé).
- **Ne nous engage pas** à du SemVer par crate ni à de la publication crates.io (hors périmètre — les crates restent `publish = false`).

## Alternatives envisagées

- **Versionner chaque crate indépendamment** — écarté : les crates ne sont pas publiées séparément, ça multiplierait les contrats sans bénéfice.
- **Déployer `main` HEAD (statu quo)** — écarté : aucune traçabilité, rollback hasardeux.
- **Calquer la version applicative sur `/v1`** — écarté : couple deux rythmes différents (le code itère bien plus vite que le contrat) ; c'est précisément ce que le découplage évite.
- **`git describe`/hash de commit comme « version »** — écarté comme *source* de version (illisible, non SemVer) ; le hash reste un complément utile dans les labels d'image, pas le numéro de release.
