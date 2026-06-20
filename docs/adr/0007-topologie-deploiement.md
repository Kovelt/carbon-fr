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
