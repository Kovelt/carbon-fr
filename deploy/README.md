# Déploiement de carbon-fr

Deux voies, selon le contexte.

## 1. Self-hosting générique (exemples fournis)

- **[`Dockerfile`](../Dockerfile)** — image de prod multi-stage, non-root, cache de build.
- **[`Caddyfile`](Caddyfile)** — reverse proxy TLS (Let's Encrypt auto, en-têtes de sécurité, sonde `/health/ready`). `caddy run --config deploy/Caddyfile`.
- **[`carbonfr.service`](carbonfr.service)** — unité systemd bare-metal (durcie : `NoNewPrivileges`, `ProtectSystem=strict`, arrêt gracieux SIGTERM).

Dans tous les cas : **API derrière un reverse proxy TLS** + `CARBONFR_TRUST_PROXY=1` (pour lire l'IP réelle du client via `X-Real-Ip` / `X-Forwarded-For`). Sans proxy de confiance, laisser `CARBONFR_TRUST_PROXY=0` (l'en-tête est spoofable). Cf. [`.env.example`](../.env.example).

## 2. Production Kovelt — derrière Traefik (org)

L'instance hébergée (`carbon-fr-api.kovelt.fr`) tourne **comme un service de la stack Kovelt** (Traefik d'organisation, PostgreSQL dédié en conteneur). Caddy/systemd ci-dessus ne sont **pas** utilisés là : Traefik fait le TLS et pose `X-Forwarded-For`/`X-Real-Ip`.

Labels Traefik du service (compose) :

```yaml
labels:
  - "traefik.enable=true"
  # HTTP → HTTPS
  - "traefik.http.routers.carbonfr-http.entrypoints=web"
  - "traefik.http.routers.carbonfr-http.rule=Host(`carbon-fr-api.${DOMAIN}`)"
  - "traefik.http.routers.carbonfr-http.middlewares=carbonfr-https-redirect"
  - "traefik.http.middlewares.carbonfr-https-redirect.redirectscheme.scheme=https"
  # HTTPS
  - "traefik.http.routers.carbonfr.entrypoints=websecure"
  - "traefik.http.routers.carbonfr.rule=Host(`carbon-fr-api.${DOMAIN}`)"
  - "traefik.http.routers.carbonfr.tls=true"
  - "traefik.http.routers.carbonfr.tls.certresolver=letsencrypt"
  - "traefik.http.routers.carbonfr.middlewares=secure-headers@file"
  - "traefik.http.services.carbonfr.loadbalancer.server.port=8080"
```

Avec, côté service, **`CARBONFR_TRUST_PROXY=1`** (Traefik est le proxy de confiance) et un **`CARBONFR_VISIT_SALT`** secret (sinon le serveur refuse de démarrer en mode proxy). Les migrations s'appliquent au démarrage ; sondes `GET /health` (liveness) et `GET /health/ready` (vérifie la base).
