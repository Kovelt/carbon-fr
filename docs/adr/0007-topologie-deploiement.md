# ADR-0007 — Topologie de déploiement (API sur VPS, site statique, sous-domaine Kovelt)

- **Statut** : Accepté
- **Date** : 2026-06-14

## Contexte

Le projet comprend deux composants aux besoins d'hébergement **radicalement différents** :

- un **site** explicatif + documentation + console/playground pour consommer les données ;
- l'**API** elle-même : un binaire Rust persistant + PostgreSQL + un poller.

Contraintes : souveraineté (FR/EU), auto-hébergement, coût maîtrisé, et le fait que **l'URL de l'API est un contrat public** (des intégrations la coderont en dur).

## Décision

1. **Site / doc / playground** : **statique** (HTML/CSS/JS, généré par un SSG — typiquement Zola ou mdBook, cohérents avec l'écosystème Rust), hébergé sur l'**hébergement mutualisé o2switch** existant. Redéplaçable sans impact.
2. **API** : service déployé sur un **VPS (FR/EU) géré par Kovelt**. PostgreSQL **co-localisé sur le même VPS** (volume ~1 Go, aucune raison de l'externaliser). **Reverse proxy + TLS** devant (Caddy pour l'HTTPS automatique, ou nginx + certbot).
3. **DNS** : **sous-domaine Kovelt** (ex. `carbon-fr.kovelt.fr` pour le site ; l'API sous le même host ou un `api.` dédié). Le VPS a une **IP fixe** → simple enregistrement A/AAAA. **Pas de DNS dynamique** (DuckDNS reste réservé aux IP résidentielles type PC-Dada).
4. **Contrat d'URL** : l'API est **versionnée dans le chemin** (`/v1/…`) dès le départ.

## Conséquences

- Coût marginal quasi nul pour le site (o2switch déjà payé) ; un petit VPS suffit pour l'API.
- Séparation nette : le **site** se redéménage sans douleur ; l'**API** garde une URL stable.
- Le mutualisé o2switch **ne peut pas** héberger l'API (pas de binaire persistant + PostgreSQL + poller) — assumé.
- Le versionnage `/v1` permet une **migration ultérieure** (sous-domaine Kovelt → domaine dédié façon `carbonintensity.org.uk`) ou une évolution de l'API **sans casser** les intégrations existantes.
- Topologie reflétée dans le workspace : `bin/server` (et éventuellement `bin/poller`), plus une doc de déploiement en phase 4.

## Questions ouvertes (phase 4 — packaging ; n'impactent pas le `core`)

- **Forme du poller** : tâche de fond intégrée au process `server`, **ou** binaire séparé `bin/poller` déclenché par un **timer systemd** (one-shot). *Recommandation : binaire séparé + timer systemd* — cohérent avec l'usage systemd existant, et plus robuste (isolation, redémarrage propre, indépendant de l'API).
- **Packaging** : `docker-compose` (server + postgres + proxy, reproductible) **vs** binaire + systemd + PostgreSQL système (plus léger, « bare-metal souverain »).

## Alternatives envisagées

- **Tout sur o2switch** : impossible pour l'API (mutualisé PHP/cPanel, pas de service persistant).
- **API sur PC-Dada (auto-hébergement résidentiel + DuckDNS)** : écarté — IP dynamique, disponibilité moindre, inadapté à une API publique. Le VPS à IP fixe est le bon support.
- **PostgreSQL managé externe** : superflu au volume, et s'éloigne de l'auto-hébergement souverain.
- **Domaine dédié dès maintenant** : reporté. Le sous-domaine Kovelt sert de vitrine, reste réversible, et un domaine dédié pourra venir si la communauté le justifie.

## Addendum (2026-06-20) — résolution des questions ouvertes

Les choix de packaging laissés ouverts ont été tranchés à l'usage, parfois à l'inverse de la recommandation initiale :

- **Forme du poller** : finalement **intégré** au process `server` (tâche de fond, pas de `bin/poller` séparé). Le flux live SSE passe par un canal mémoire `tokio::broadcast` ; une variante `bin/poller` + `LISTEN`/`NOTIFY` reste documentée pour un futur multi-instances (ADR-0014).
- **Packaging** : **image Docker publiée sur GHCR** (`ghcr.io/kovelt/carbon-fr`, ADR-0019), déployée en conteneur sur le VPS Kovelt derrière **Traefik** + PostgreSQL dédié. Les exemples self-host (`deploy/Caddyfile`, `deploy/carbonfr.service` systemd) restent fournis.
- **Reverse proxy** : la prod utilise **Traefik** (Caddy/nginx ne sont que des exemples self-host).
- **Site / doc / playground** : le SSG sur o2switch n'est pas encore déployé ; la doc vit dans le repo (README, `docs/`, OpenAPI `/docs`). Sous-domaine d'API retenu : `carbon-fr-api.kovelt.fr`.

Cf. ARCHITECTURE §9 et `deploy/README.md`.

## Addendum (2026-11-02) — site vitrine en ligne depuis le 2026-06-16, sans SSG : l'item « site / doc / playground » est clos

L'addendum du 2026-06-20 disait le SSG « pas encore déployé » sans mentionner le site, et l'ADR-0029 a ensuite dit ce site « non concrétisé » : le site vitrine était pourtant **en ligne depuis le 2026-06-16** sur `carbon-fr.kovelt.fr` (sous-domaine Kovelt de la décision 3), sur l'**hébergement mutualisé o2switch** de la décision 1 — seul le SSG n'a jamais existé. Le CORS ouvert de l'API, ajouté le même jour, le cite déjà. Constat fait le 2026-11-01 sur le site en production et sur son dépôt (GitLab Kovelt, distinct de celui-ci) :

- **Statique, sans SSG ni étape de build** : pages HTML/CSS/JS servies telles quelles, publiées par un script de synchronisation (`rsync`) vers o2switch. La décision 1 est tenue sur ce qu'elle cherchait — site statique, hébergement o2switch, redéployable sans impact — et abandonnée sur « généré par un SSG » : ni Zola ni mdBook.
- **Documentation rendue depuis les sources** : la référence de l'API lit `GET /v1/openapi.json`, le registre des ADR et le journal des versions lisent `docs/adr/` et `CHANGELOG.md` sur la branche `main` de GitHub, le playground exécute de vraies requêtes `GET` sur l'API de production. Référence, registre et journal gardent chacun un instantané de repli et un pré-rendu statique (indexation sans JavaScript), régénérés à chaque déploiement du site depuis ces mêmes sources.
- **La carte `/hydrogene` reste servie par l'API** (ADR-0029) : le site l'embarque en iframe depuis la 0.6.1 (CSP `frame-ancestors`).
- **Rafraîchi et redéployé le 2026-11-02 pour l'API 0.11.1** : le site était figé depuis le 2026-07-04 (API 0.6.1). Le playground connaît désormais `/v1/intensity/now/all` et `/v1/regions` (0.10.0), `/v1/intensity/index` et `?format=csv` (0.11.0) ; le quickstart décrit les crates publiées (`carbonfr-sdk`, `carbonfr-core`, `carbonfr-eligibility`, ADR-0030/0031) à la place d'une crate `carbonfr-client` qui n'a jamais existé ; la référence donne les horodatages en UTC (0.11.1). Le contenu du site suit l'API sans en dépendre : un retard de contenu n'est pas un écart de décision, et il se corrige hors de ce dépôt.

La décision de topologie — site statique sur o2switch, API sur le VPS Kovelt, sous-domaines `carbon-fr.kovelt.fr` et `carbon-fr-api.kovelt.fr`, `/v1` dans le chemin — est **appliquée** ; l'item « site / doc / playground » est **clos**. Aucun changement de code ni de contrat `/v1`.
