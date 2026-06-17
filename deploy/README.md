# Déploiement de carbon-fr

Deux voies, selon le contexte.

## Image (source recommandée)

Chaque tag git `vX.Y.Z` publie une image de prod sur **GHCR** (workflow [`release.yml`](../.github/workflows/release.yml), ADR-0019) :

```
ghcr.io/kovelt/carbon-fr:X.Y.Z   ← épingler la version exacte en prod
ghcr.io/kovelt/carbon-fr:X.Y     ← suit les patchs de la mineure
ghcr.io/kovelt/carbon-fr:latest  ← dernier tag publié
```

L'image est **publique** : `docker pull` sans authentification.

```bash
docker pull ghcr.io/kovelt/carbon-fr:0.1.0
docker run --rm ghcr.io/kovelt/carbon-fr:0.1.0 --version   # → carbonfr-server 0.1.0
```

**Toujours épingler une version exacte en prod** (pas `latest`) : on sait quel build répond, et le rollback = redéployer le tag précédent. Le binaire logue sa version au démarrage (`info … version=…`) et répond à `--version`.

Cadrer une release : `git tag v0.2.0 && git push origin v0.2.0` (le workflow vérifie que le tag correspond à la version du workspace, puis construit et pousse l'image).

> **Build local** plutôt que tirer l'image : possible via le [`Dockerfile`](../Dockerfile) (`docker build -t carbon-fr .`) — utile pour un fork ou un patch non publié.

## 1. Self-hosting générique (exemples fournis)

- **Image** — `ghcr.io/kovelt/carbon-fr:X.Y.Z` (ci-dessus) ou build local via le [`Dockerfile`](../Dockerfile) (multi-stage, non-root, cache de build).
- **[`Caddyfile`](Caddyfile)** — reverse proxy TLS (Let's Encrypt auto, en-têtes de sécurité, sonde `/health/ready`). `caddy run --config deploy/Caddyfile`.
- **[`carbonfr.service`](carbonfr.service)** — unité systemd bare-metal (durcie : `NoNewPrivileges`, `ProtectSystem=strict`, arrêt gracieux SIGTERM).

Dans tous les cas : **API derrière un reverse proxy TLS** + `CARBONFR_TRUST_PROXY=1` (pour lire l'IP réelle du client via `X-Real-Ip` / `X-Forwarded-For`). Sans proxy de confiance, laisser `CARBONFR_TRUST_PROXY=0` (l'en-tête est spoofable). Cf. [`.env.example`](../.env.example).

## 2. Production Kovelt — derrière Traefik (org)

L'instance hébergée (`carbon-fr-api.kovelt.fr`) tourne **comme un service de la stack Kovelt** (Traefik d'organisation, PostgreSQL dédié en conteneur). Caddy/systemd ci-dessus ne sont **pas** utilisés là : Traefik fait le TLS et pose `X-Forwarded-For`/`X-Real-Ip`.

Le service compose tire l'**image taguée** depuis GHCR (`image: ghcr.io/kovelt/carbon-fr:X.Y.Z`, version épinglée — cf. section *Image*), pas un build sur place. Déployer une nouvelle version = bumper le tag de l'image et redéployer.

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

### Restreindre `/metrics` (exploitation, non public)

`GET /metrics` (Prometheus, ADR-0022) est un endpoint **d'exploitation** : il n'expose aucun secret, mais n'a pas vocation à être public. Deux niveaux, à combiner :

1. **Scraper en interne, sans passer par Traefik** (recommandé) : si Prometheus tourne dans la même stack, il scrute directement le service sur le réseau Docker — `http://carbonfr:8080/metrics` — et `/metrics` n'a alors **aucune** raison d'être routé publiquement.

2. **Bloquer `/metrics` sur l'entrée publique** (défense en profondeur, même si la DNS pointe sur l'hôte). Routeur **dédié et prioritaire** pour le préfixe `/metrics`, derrière une *allow-list* d'IP internes (middleware `ipAllowList`, Traefik v3) — un client public tombe sur `403` :

```yaml
  # /metrics : routeur dédié, prioritaire, restreint aux IP internes.
  - "traefik.http.routers.carbonfr-metrics.entrypoints=websecure"
  - "traefik.http.routers.carbonfr-metrics.rule=Host(`carbon-fr-api.${DOMAIN}`) && PathPrefix(`/metrics`)"
  - "traefik.http.routers.carbonfr-metrics.priority=100"   # > routeur principal → gagne sur /metrics
  - "traefik.http.routers.carbonfr-metrics.tls=true"
  - "traefik.http.routers.carbonfr-metrics.tls.certresolver=letsencrypt"
  - "traefik.http.routers.carbonfr-metrics.service=carbonfr"
  - "traefik.http.routers.carbonfr-metrics.middlewares=carbonfr-metrics-allow"
  # Plages privées RFC 1918 (réseau Docker / hôte de supervision) — ajuster au besoin.
  - "traefik.http.middlewares.carbonfr-metrics-allow.ipallowlist.sourcerange=10.0.0.0/8,172.16.0.0/12,192.168.0.0/16"
```

> ⚠️ `ipAllowList` filtre sur l'**IP source vue par Traefik**. Si Traefik est lui-même derrière un autre balanceur, régler `ipallowlist.ipstrategy.depth` pour lire la bonne IP dans `X-Forwarded-For` (sinon l'allow-list verrait l'IP du balanceur, pas celle du client). Le routeur principal `carbonfr` (rule `Host(...)` seule, priorité = longueur de règle) reste plus bas que `priority=100` : `/metrics` part donc bien sur le routeur restreint, tout le reste sur le routeur public.
