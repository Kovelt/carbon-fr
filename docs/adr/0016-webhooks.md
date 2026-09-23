# ADR-0016 — Webhooks : notification sortante signée, gardée par l'ownership de clé

- **Statut** : Accepté (**implémenté** — fondation de sécurité, store, livraison, watcher, endpoints)
- **Date** : 2026-06-16
- **Débloqué par** : ADR-0015 (clés API → *ownership* d'abonnement) ; **spécifie** le report de l'ADR-0014 §3
- **S'appuie sur** : ADR-0002 (hexagonal), ADR-0004 (Postgres natif), ADR-0014 (flux d'événements `IntensityUpdate`)

## État d'implémentation (2026-06-16)

**Livré de bout en bout, sécurité d'abord.**

**Tranche A — fondation pure (`core`)** : `should_fire` (*edge-triggered*),
`validate_webhook_url`/`is_public_ip` (anti-SSRF, HTTPS + deny-list IPv4/IPv6),
`hmac_sha256_hex` (validé RFC 4231), modèle `Subscription` ; ports
`SubscriptionRepository`/`Notifier`. **Le code dangereux, testé à froid.**

**Tranche B — le réseau, par-dessus la fondation** :

- **Store** : table `webhook_subscription` (migration `0009`) + impl
  `SubscriptionRepository` (CRUD **scopé au propriétaire**, validé Postgres réel) ;
- **Livraison** : crate `carbonfr-adapter-webhook` (`HttpNotifier`) — **re-validation
  SSRF à la résolution DNS** (refus si l'hôte résout vers une IP non publique,
  parade TOCTOU), **pas de redirection** (réouvrirait la faille), **retries** à
  *backoff* exponentiel borné, timeout court ;
- **Watcher** : tâche de fond branchée sur le flux `IntensityUpdate` (ADR-0014),
  détecte les franchissements, signe (HMAC) et délègue la livraison ;
- **Endpoints** `POST` / `GET` `/v1/webhooks` + `DELETE /v1/webhooks/{id}`, **clé
  API requise** (le secret n'est affiché qu'à la création).

(Désactivation après N échecs consécutifs et **purge des abonnements
désactivés** : **livrées**, cf. addendum 2026-09-23.)
(Quota par clé livré : `MAX_WEBHOOKS_PER_KEY = 50` abonnements, refusé au-delà.)

## Contexte

Le SSE (ADR-0014) livre du **push client-initié** : aucun état, aucune URL stockée, aucune surface SSRF. Les **webhooks** sont l'inverse — une **action sortante initiée par le serveur** vers une URL fournie par l'utilisateur. Cela force tout ce que l'ADR-0014 §3 listait comme bloquant : **état par-utilisateur** (abonnements), **ownership/auth** (qui possède un abonnement ?), **filtrage SSRF**, **anti-amplification**, **fiabilité de livraison** (retries, signatures). L'ADR-0014 a donc reporté les webhooks « faute de propriétaire d'abonnement ».

L'ADR-0015 a livré ce propriétaire : une **clé API** identifie un porteur. Le blocage est levé ; cet ADR spécifie les webhooks.

Le risque dominant n'est **pas** fonctionnel, il est **sécuritaire** : un webhook est un moteur de requêtes sortantes piloté par l'utilisateur — un vecteur **SSRF** classique (atteindre `169.254.169.254`, des services internes, `localhost`), et un vecteur d'**amplification** (DDoS réfléchi via l'instance). La fondation à poser en premier est donc la **sécurité**, pas la plomberie.

## Décision

### 1. Un webhook = un **abonnement possédé par une clé**

Pas de webhook anonyme : créer un abonnement **exige une clé API** (ADR-0015). La clé est le propriétaire ; elle borne les quotas (nombre d'abonnements, débit de livraison) et autorise la gestion (lister / supprimer **ses** abonnements). Anonyme ⇒ `401`.

### 2. Déclencheur = **franchissement de seuil**, *edge-triggered*

Un abonnement décrit une condition sur l'intensité d'une région : « **passe sous / au-dessus** de `X` gCO₂eq/kWh ». La notification part au **franchissement** (transition), **pas** à chaque cycle du poller tant que la condition reste vraie — sinon on inonde l'endpoint toutes les 15 min. L'évaluation est une **fonction pure du domaine** `(état précédent, nouvelle mesure, condition) → faut-il notifier ?`, branchée sur le **flux d'événements existant** (`IntensityUpdate`, ADR-0014) — **aucune nouvelle source**.

### 3. Anti-SSRF : liste de refus **stricte**, validée à l'inscription **et** à la livraison

L'URL de rappel est validée par une **fonction pure** (`core`) :

- **HTTPS uniquement** (pas de `http`, `file`, `gopher`…) ;
- hôte **non résolu** vers une IP **privée / loopback / link-local / réservée** (`127.0.0.0/8`, `10/8`, `172.16/12`, `192.168/16`, `169.254/16`, `::1`, `fc00::/7`, `fe80::/10`, …), ni `localhost` ;
- re-vérifiée **au moment de la livraison** (TOCTOU : le DNS peut changer entre l'inscription et l'appel ; l'adapter re-résout et re-valide l'IP avant d'émettre).

C'est le cœur de l'ADR ; il est **pur et testé en isolation** avant tout réseau.

### 4. Authenticité : **signature HMAC-SHA256**

Chaque livraison porte un en-tête `X-Carbonfr-Signature: sha256=<hex>` = HMAC-SHA256 du corps avec un **secret par abonnement** (généré à la création, non ré-affiché). Le récepteur vérifie la signature → il sait que l'appel vient bien de carbon-fr et que le corps n'a pas été altéré. HMAC implémenté **tout-Rust** (sur `sha2`, sans nouvelle dépendance), testé contre des vecteurs connus.

### 5. Fiabilité : retries bornés, *backoff*, abandon

Livraison **best-effort fiable** : timeout court, **retries** à *backoff* exponentiel borné (p. ex. 3 tentatives), puis **abandon** (pas de dead-letter complexe en v1 — un compteur d'échecs par abonnement, désactivation après N échecs consécutifs pour ne pas marteler un endpoint mort). La livraison est **hors du chemin de requête** : une tâche de fond consomme le flux d'événements.

### 6. `core` minimal, sécurité dans le domaine, réseau au bord

- **Domaine (`core`, pur)** : le **modèle d'abonnement**, l'**évaluation de seuil** *edge-triggered*, la **validation d'URL anti-SSRF**, la **signature HMAC**. Tout ce qui est dangereux est pur et testable sans réseau.
- **Ports sortants** : `SubscriptionRepository` (CRUD, Postgres) et `Notifier` (émission d'une livraison signée).
- **Adapters** : store Postgres ; adapter de livraison HTTP (re-validation SSRF à la résolution, retries) ; **watcher** (tâche de fond branchée sur le flux `IntensityUpdate`). **Endpoints** `/v1/webhooks` (création/liste/suppression, **auth requise**).

### 7. Périmètre v1

Seuil d'intensité par région (`rte-direct`). Pas de webhook sur la prévision, le scheduling ou `acv-ademe` en v1 (extensions derrière le même contrat). Surface de gestion : endpoints `/v1/webhooks` (un portail sur le site statique est une option ultérieure, ADR-0007).

## Conséquences

- **Webhooks enfin possibles** sans casser la posture : anonyme **inchangé** (SSE reste l'option sans état) ; les webhooks sont un **opt-in authentifié**.
- **`core`** : nouveaux types `Subscription` / évaluation / validation SSRF / HMAC — **purs**, zéro IO. Le `core` ne fait toujours **aucune requête sortante** (c'est le rôle de l'adapter `Notifier`).
- **Infra** : table `webhook_subscription`, adapter de livraison, watcher de fond, endpoints de gestion. Réutilise le flux `IntensityUpdate` (ADR-0014) et l'auth (ADR-0015).
- **Surface de risque assumée et cantonnée** : SSRF (deny-list double), amplification (quotas par clé), spam (edge-trigger + désactivation sur échecs). Tous adressés, et la partie dangereuse est **testée à froid**.
- **RGPD** : on stocke une URL + un secret par clé ; minimisation et suppression suivent le régime de l'ADR-0015.

## Alternatives envisagées

- **Webhooks anonymes** : impossibles à gouverner (pas d'ownership → pas de quota crédible, abus trivial). Écarté — c'est précisément ce que l'ADR-0015 débloque.
- **Pas de re-validation SSRF à la livraison** : plus simple, mais **faille TOCTOU** (DNS rebinding). Écarté — la double validation est non négociable.
- **Niveau d'alerte à chaque cycle** (*level-triggered*) plutôt qu'au franchissement : inonde l'endpoint. Écarté au profit de l'*edge-trigger*.
- **File d'attente / dead-letter dédiée (Redis, broker)** : surdimensionné au volume v1 ; un compteur d'échecs + désactivation suffit, réversible derrière `Notifier`. Écarté pour l'instant.
- **Dépendance `hmac`/`url`/`ssrf` tierce** : on garde la surface minimale — HMAC sur `sha2` (déjà présent), validation d'URL maison testée. Réversible si besoin.

## Questions ouvertes (implémentation — n'impactent pas le principe)

- Politique exacte de retries (nombre, *backoff*) — le seuil de désactivation est fixé par l'addendum 2026-09-23.
- Re-résolution DNS à la livraison : implémentation (résoudre puis *bind* sur l'IP validée) vs validation best-effort.
- Quotas chiffrés par clé (nb d'abonnements, livraisons/min).
- Rétention et purge des abonnements désactivés — le délai est fixé par
  l'addendum 2026-09-23 (« Purge des abonnements désactivés »).

## Addendum (2026-09-23) — Désactivation automatique après N échecs consécutifs

Point laissé ouvert par l'état d'implémentation initial, **livré** :

- **Compteur par abonnement** (migration `0014` : `consecutive_failures`, `disabled_at`). Une **livraison** = un appel au `Notifier` avec ses retries (3 tentatives à *backoff* borné) ; son issue est enregistrée par le watcher via `SubscriptionRepository::record_delivery`. Un succès remet le compteur à zéro ; un échec l'incrémente et, au seuil, **désactive** l'abonnement — en **une seule instruction SQL** (deux échecs concurrents du même abonnement ne peuvent pas se marcher dessus). Une livraison sautée faute de permis (saturation) ne compte pas.
- **Seuil** : `CARBONFR_WEBHOOK_MAX_FAILURES`, défaut **10** (`DEFAULT_WEBHOOK_MAX_CONSECUTIVE_FAILURES`), strictement positif — la désactivation n'est pas débrayable. Les franchissements de seuil étant rares (quelques-uns par jour au plus), 10 échecs d'affilée signalent un endpoint durablement mort, pas un incident passager. Un refus SSRF à la livraison (hôte résolu vers une IP non publique) compte comme un échec.
- **Désactivé ≠ supprimé** : l'abonnement sort du watcher (`active` filtre `disabled_at IS NULL`) mais reste **listé** pour son propriétaire — `GET /v1/webhooks` expose `status` (`active` | `disabled`) et `disabled_at` — champs **additifs**, non cassants pour le contrat `/v1` (aucun champ existant retiré ni modifié). Un succès tardif (livraison en vol) ne le réactive pas. **Réactivation** : le supprimer puis le recréer (nouveau secret) — pas d'endpoint dédié en v1.

## Addendum (2026-09-23) — Purge des abonnements désactivés

Point laissé ouvert par l'addendum précédent, **livré** :

- **Tâche de fond dédiée**, branchée sur `SubscriptionRepository::purge_disabled` : au **démarrage** puis **toutes les 6 h**, supprime les abonnements dont `disabled_at` est antérieur à `CARBONFR_WEBHOOK_PURGE_DAYS` jours (défaut **30**, `DEFAULT_WEBHOOK_PURGE_DAYS`, strictement positif — parsée/validée comme `CARBONFR_WEBHOOK_MAX_FAILURES`). Un abonnement **actif** (`disabled_at IS NULL`), quelle que soit son ancienneté, n'est jamais purgé — seule la désactivation automatique (addendum précédent) ou une future désactivation manuelle y expose.
- **Pourquoi 30 jours** : `GET /v1/webhooks` expose `status`/`disabled_at` (addendum précédent) — 30 jours laissent le temps à un propriétaire qui ne surveille pas activement son intégration de constater la désactivation (par consultation périodique, ou en retrouvant que ses notifications se sont arrêtées) et de la recréer s'il le souhaite, avant que la ligne ne disparaisse. Plus court risquerait de purger avant que quiconque ait eu l'occasion de regarder ; plus long laisserait des abonnements morts occuper le quota (50/clé) inutilement longtemps.
- **Supervision** : même patron que le poller et le watcher — tâche `tokio::spawn` surveillée dans le `select!` d'arrêt du serveur (fail-fast : sa mort inattendue arrête le processus, relancé par le superviseur systemd), `abort()` à l'arrêt gracieux. Une erreur de purge est journalisée (`warn`) sans jamais interrompre la boucle ni paniquer ; un succès qui purge au moins une ligne est journalisé (`info`, nombre purgé).
- **Pas de migration** : la colonne `disabled_at` existe depuis la migration `0014` (addendum précédent) ; la purge est une suppression conditionnelle dessus.

