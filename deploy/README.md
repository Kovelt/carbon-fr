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
docker pull ghcr.io/kovelt/carbon-fr:0.3.2
docker run --rm ghcr.io/kovelt/carbon-fr:0.3.2 --version   # → carbonfr-server 0.3.2
```

**Toujours épingler une version exacte en prod** (pas `latest`) : on sait quel build répond, et le rollback = redéployer le tag précédent. Le binaire logue sa version au démarrage (`info … version=…`) et répond à `--version`.

Cadrer une release : `git tag v0.3.3 && git push origin v0.3.3` (le workflow vérifie que le tag correspond à la version du workspace, puis construit et pousse l'image).

> **Build local** plutôt que tirer l'image : possible via le [`Dockerfile`](../Dockerfile) (`docker build -t carbon-fr .`) — utile pour un fork ou un patch non publié.

## 1. Self-hosting générique (exemples fournis)

- **Image** — `ghcr.io/kovelt/carbon-fr:X.Y.Z` (ci-dessus) ou build local via le [`Dockerfile`](../Dockerfile) (multi-stage, non-root, cache de build).
- **[`Caddyfile`](Caddyfile)** — reverse proxy TLS (Let's Encrypt auto, en-têtes de sécurité, sonde `/health/ready`). `caddy run --config deploy/Caddyfile`.
- **[`carbonfr.service`](carbonfr.service)** — unité systemd bare-metal (durcie : `NoNewPrivileges`, `ProtectSystem=strict`, arrêt gracieux SIGTERM).

Dans tous les cas : **API derrière un reverse proxy TLS** + `CARBONFR_TRUST_PROXY=1` (pour lire l'IP réelle du client via le **dernier segment** de `X-Forwarded-For`, que le proxy ajoute). `X-Real-Ip` n'est **pas** lu par défaut (audit 2026-08 : beaucoup de proxys — dont Caddy sans `header_up` — relaient l'en-tête client tel quel, donc spoofable) : pour l'utiliser, définir `CARBONFR_REAL_IP_HEADER=x-real-ip` **et** s'assurer que le proxy écrase cet en-tête (cf. le `header_up` du [`Caddyfile`](Caddyfile)). Sans proxy de confiance, laisser `CARBONFR_TRUST_PROXY=0` (l'en-tête est spoofable). Cf. [`.env.example`](../.env.example).

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

## 3. Supervision & alertes

Deux couches complémentaires, **à relier à un canal de notification** (sans lui, une alerte déclenchée ne prévient personne) :

1. **Prometheus** scrute `/metrics` sur le réseau interne (cf. ci-dessus) et évalue les règles de [`prometheus/alerts.yml`](prometheus/alerts.yml) (ADR-0022) :
   - `CarbonfrIngestionStale` — **alerte phare** : aucun cycle de poll réussi depuis plus de 2 × l'intervalle (`time() - carbonfr_poller_last_success_timestamp_seconds > 1800` pour le défaut de 900 s ; à ajuster si `CARBONFR_POLL_SECS` change) ;
   - `CarbonfrDown` — scrape en échec depuis 5 min ;
   - `CarbonfrIngestionErrors` — plus de 10 échecs d'ingestion en 15 min ;
   - `CarbonfrDataStale` — dernière mesure nationale connue de plus de 2 h (poller qui « réussit » en boucle sans rien de neuf, ou source ODRÉ en panne) ;
   - `CarbonfrOdreQuotaLow` / `CarbonfrOdreQuotaExhausted` — quota **réel** ODRÉ (par jeu de données) sous 10 % / épuisé (ADR-0022 addendum 2026-09-26, PROD-3).

   Ces deux dernières règles lisent les jauges `carbonfr_odre_quota_{limit,remaining,reset_timestamp_seconds,observed_timestamp_seconds}`, labellisées `dataset="…"` — une par jeu de données ODRÉ interrogé (national temps réel, régional, export…). Elles reflètent les en-têtes de quota renvoyés par ODRÉ lui-même, **pas** un comptage d'appels initiés : le quota est remis à zéro le 1er du mois, et il est compté **par client (IP)** — ces jauges donnent donc le quota de l'instance qui scrape `/metrics` (celui d'un déploiement self-hosted sur une autre IP est indépendant).

   ```yaml
   # prometheus.yml (extrait)
   rule_files:
     - /etc/prometheus/alerts.yml
   scrape_configs:
     - job_name: carbon-fr            # le nom de job est utilisé par CarbonfrDown
       metrics_path: /metrics
       static_configs:
         - targets: ["carbonfr:8080"] # nom du service sur le réseau Docker
   ```

   Le routage vers une notification passe par **Alertmanager** (non fourni ici).

2. **Sonde externe** (ex. Uptime Kuma), qui voit l'API comme un client :
   - `GET /health` — mot-clé `ok` ;
   - **fraîcheur de la donnée servie** — `GET /v1/intensity/now`, requête JSON (JSONata) `$toMillis(timestamp) > ($millis() - 3600000)`, valeur attendue `true` (le point le plus récent a moins d'1 h ; la publication éCO2mix arrive d'ordinaire 20 à 30 min après l'heure du point).

   Là encore, **attacher un canal de notification** (e-mail, messagerie…) à chaque sonde.

## 4. Sauvegarde & restauration

**Principe** : un `pg_dump` quotidien de la base, archivé **hors du serveur** et chiffré ; plus un dump ponctuel **juste avant chaque déploiement** (droits `600` : il contient les empreintes de clés API et les secrets de webhooks). Ce que la base contient d'irremplaçable : clés API, abonnements webhook, compteur de visites, historique des millésimes. Les mesures, elles, se re-backfillent (export de masse ODRÉ, ADR-0003), mais au prix de plusieurs heures.

**Restauration** (procédure testée le 2026-09-23 sur l'instance Kovelt) :

1. Récupérer l'archive du jour voulu depuis le stockage distant, la déchiffrer, en extraire le dump SQL de carbon-fr.
2. Restaurer dans un PostgreSQL **de même version majeure** (17), sur une base vide :
   ```bash
   psql -U carbonfr -d carbonfr -v ON_ERROR_STOP=1 -q < carbon-fr.sql
   ```
3. Démarrer carbon-fr sur cette base : les migrations manquantes (si le dump précède une release) s'appliquent au démarrage.
4. Vérifier : `GET /health/ready`, comptages de `measurement`, `api_key`, `webhook_subscription` contre les attendus, puis fraîcheur de `/v1/intensity/now` après un cycle de poll (le poller rattrape seul l'écart depuis l'heure du dump).

**Mesures du test** (~536 000 mesures, dump SQL de 128 Mo) : téléchargement de l'archive ~4 min, restauration **5 s**, sans aucune erreur ; **RPO = 24 h** (dump quotidien), **RTO ≈ 5 min** pour la base, hors redéploiement. **Refaire le test une fois par trimestre** : une sauvegarde jamais restaurée n'est pas une sauvegarde (celles de l'instance Kovelt ont échoué en silence du 2026-06-21 au 2026-09-23 ; le script alerte désormais aussi en cas d'échec).

## 5. Rattraper un trou de données

Un trou dans l'historique (panne de la collecte, retard d'un import) se comble par la
sous-commande `backfill`, qui télécharge l'**export de masse** d'ODRÉ par tranches (jamais
l'API paginée, qui consommerait le quota — ADR-0003), réécrit les mesures avec l'upsert
conditionnel au millésime (ADR-0006, sans effet sur une valeur de meilleur millésime),
reconstruit les séries agrégées, puis rattrape la charge et la météo archivée de la
période.

1. **Mesurer le trou** (lecture seule) : jours sans mesure nationale `rte-direct` sur la
   période suspecte.
2. **Choisir la source** : le jeu **consolidé** (`CARBONFR_BACKFILL_SOURCE=consolidated`,
   défaut) s'il couvre déjà la période — RTE le publie avec environ trois mois de retard ;
   sinon le jeu **temps réel** (`realtime`), dont les valeurs seront remplacées d'elles-mêmes
   par les consolidées lors d'un rattrapage ultérieur.
3. **Répéter en local** sur une base PostgreSQL 17 jetable, avec les mêmes paramètres, et
   contrôler le nombre de mesures par jour (48 au pas de 30 min pour le consolidé, 96 au pas
   de 15 min pour le temps réel) et le millésime.
4. **En production** : dump de la base juste avant (droits `600`), puis
   `docker exec -e CARBONFR_BACKFILL_FROM=… -e CARBONFR_BACKFILL_TO=… -e CARBONFR_BACKFILL_WINDOW_DAYS=30 [-e CARBONFR_BACKFILL_SOURCE=realtime] <conteneur> carbonfr-server backfill`
   (le binaire reprend `DATABASE_URL` du conteneur ; le service continue de tourner).
5. **Vérifier** : plus aucun jour vide sur la période, puis `/v1/intensity/date` et
   `/v1/intensity/stats` sur ces dates.

Mesure du 2026-09-25 : 150 jours rattrapés depuis le consolidé en ~1 min 15 s (5 exports),
8 jours depuis le temps réel en quelques secondes. La reconstruction complète des séries
horaires (~280 000 seaux) prend ~4 s et journalise un avertissement « slow statement »
attendu.
