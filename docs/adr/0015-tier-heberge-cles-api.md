# ADR-0015 — Tier hébergé : clés API en couche de bord, anonyme par défaut

- **Statut** : Accepté (mise en œuvre **engagée** — auth par clé + quota livrés ; métering persistant & webhooks à venir)
- **Date** : 2026-06-15
- **S'appuie sur** : ADR-0002 (hexagonal), ADR-0004 (Postgres natif), ADR-0007 (déploiement / tier hébergé), ADR-0014 (webhooks reportés, en attente d'un *ownership*)

## État d'implémentation (2026-06-16)

**Tranche A — clés API + quota au bord, opt-in : livrée.** `core` **strictement
intact** (aucun cas d'usage, aucun type carbone touché — le seul ajout est un
**port sortant** `ApiKeyRepository` + ses value objects `ApiTier`/`ApiKeyRecord`,
qui vivent **avec le port**, pas dans le domaine carbone).

- **Middleware de bord** (`adapter-http`, `enforce`) : résout un **principal**
  (anonyme par IP, ou clé `Authorization: Bearer …`), applique un **quota par
  minute** (fenêtre fixe en mémoire), renvoie `401` (clé inconnue), `429` (quota,
  + en-têtes `RateLimit-*` / `Retry-After`) ; **`core` ne voit jamais le principal**.
- **Opt-in** (§6) : appliqué **seulement si** `CARBONFR_RATELIMIT_ENABLED=1`. Par
  défaut, l'API reste **anonyme et sans limite** → parité self-hosting préservée.
  Limites surchargeables (`CARBONFR_RATELIMIT_ANON_PER_MIN`/`_FREE_PER_MIN`).
- **Store** : port `ApiKeyRepository` + table `api_key` (empreinte SHA-256, tier,
  libellé — **jamais la clé en clair**), impl Postgres (validée sur base réelle).
- **Délivrance** : sous-commande **`carbonfr-server mint-key`** (génère une clé
  `cfr_<hex>`, stocke son empreinte, l'affiche **une seule fois** ;
  `CARBONFR_KEY_LABEL`).

**À venir :** **`UsageMeter`** persistant (métering/analytics, §2) — le quota
actuel suffit à l'enforcement, le métering persistant viendra avec le payant ;
**webhooks** (ADR-0014 §3, désormais débloqués par l'*ownership* de la clé) ;
identité email + lien magique (§3) ; **payant** = adapter de facturation cantonné
à l'instance hébergée (§7).

## Contexte

Jusqu'ici `carbon-fr` est un read-model **anonyme, sans état, auto-hébergeable**. Plusieurs reports en dépendaient — au premier chef les **webhooks** (ADR-0014), bloqués faute de **propriétaire d'abonnement**. La question « tier hébergé » (laissée ouverte par l'ADR-0007) doit donc être tranchée.

C'est une décision **de produit** (combien de service opérer ?) avant d'être technique. Les trois postures sont **emboîtées** : anonyme ⊂ clé gratuite ⊂ payant. Le tier gratuit construit déjà **tout le mécanisme** (identité, clés, métering, quotas) que le payant réutiliserait — passer au payant plus tard n'ajoute qu'un adapter de facturation et un drapeau de tier, **sans rien jeter**.

Décision de cadrage retenue : **tier hébergé gratuit** ; **anonyme conservé par défaut** ; **payant = extension future non-bloquante**.

## Décision

### 1. Posture cible : tier gratuit (comptes + clés API)

Deux niveaux : **anonyme** (limité par IP) et **clé gratuite** (identifié, limites plus hautes). Le **payant** n'est **pas** construit maintenant, mais l'architecture est posée pour l'accueillir sans refonte.

### 2. L'identité est une préoccupation **de bord**, jamais du domaine

Invariant non négociable, dans la lignée de « la météo ne crée aucun type dans `core` » (ADR-0012) :

```
requête ─▶ [middleware: auth + quota] ──(principal | anonyme)──▶ cas d'usage (INCHANGÉ) ─▶ domaine
                 │ lit ApiKeyRepository                                                       │
                 └────────── après réponse : UsageMeter.record ◀──────────────────────────────┘
```

- vérification de clé = **middleware** dans `adapter-http` ;
- persistance = **nouveaux ports sortants** `ApiKeyRepository` et `UsageMeter`, sur Postgres (ADR-0004) ;
- le **`core` n'apprend jamais qui appelle** — les cas d'usage ne prennent **aucun principal**. La tarification/quota est de la **politique de bord (config)**, pas un concept carbone.

**Conséquence directe : la fonctionnalité tier ajoute *zéro* au `core`.**

### 3. Modèle d'auth : clés API, pas OAuth

Une **clé porteuse** (`Authorization: Bearer …`) — le standard *dev-first*. Pas de sessions, pas de mot de passe : délivrance **par email** (clé envoyée / lien magique), pour **minimiser la surface RGPD** (email + clé, **aucune donnée de paiement**) et l'exploitation. L'anonyme continue de fonctionner, à limites plus basses.

### 4. Le tiering débloque les webhooks (ADR-0014)

anonyme → clé gratuite (→ payant). Les webhooks ne s'ouvriront qu'aux tiers **authentifiés** : la clé fournit enfin le **propriétaire d'abonnement** qui manquait. Cet ADR **lève le blocage** ; les webhooks eux-mêmes restent un **ADR futur** (port `Notifier`, watcher, HMAC, deny-list SSRF, quotas).

### 5. Le quota, dans l'autre sens

Jusqu'ici « résilience au quota » protégeait `carbon-fr` du plafond RTE. Le métering par clé ajoute le sens inverse : **imposer ses propres quotas aux clients** pour protéger l'instance. Même mécanisme, autre direction.

### 6. Parité self-hosted : le code d'auth reste dans l'OSS

Le middleware + les ports d'auth/métering vivent **dans le logiciel OSS**, **désactivés par défaut** (mode anonyme). Un self-hoster peut donc **fermer sa propre instance**. Pas de fork propriétaire Kovelt.

### 7. Payant : extension future explicitement non-bloquante

Quand (et si) le payant arrive : un **adapter de facturation** (Stripe ou équivalent) + un drapeau de tier. Cette dépendance est **propriétaire mais cantonnée à l'instance hébergée** — **le logiciel OSS reste sans dépendance propriétaire**. Aucune refonte requise : c'est pourquoi on le diffère sans regret.

### 8. Direction, pas calendrier

Décider « cible = tier gratuit » **n'oblige pas à le construire tout de suite**. On peut livrer l'instance **anonyme d'abord** et brancher les clés à l'ouverture publique. Cet ADR enregistre la *direction*.

## Conséquences

- **`core` intact** — c'est le titre : aucun cas d'usage, aucun type de domaine modifié.
- **`adapter-http`** : middleware d'authentification + d'application de quota + hooks de métering + en-têtes de rate-limit (`429`, `401`).
- **Nouveaux ports** : `ApiKeyRepository`, `UsageMeter` (+ identité minimale : email + clés), sur Postgres.
- **Posture préservée** : anonyme par défaut, auth **opt-in**, auto-hébergeable.
- **Webhooks débloqués** (ADR-0014) — l'*ownership* existe ; reste à les spécifier (ADR futur).
- **Surface RGPD introduite** (email + clé) : base légale, suppression, minimisation — léger mais réel.
- **Coût d'exploitation** : flux d'inscription, gestion/révocation de clé, support, gestion d'abus.

## Alternatives envisagées

- **Pas de tier (anonyme pur)** : le plus souverain, zéro RGPD/exploitation — mais **ferme définitivement les webhooks** et ne laisse que le ban d'IP (faible : CGNAT, IP tournantes) contre l'abus. Écarté **comme cible**, mais reste le **mode par défaut** du logiciel.
- **Payant d'emblée** : impose un processeur de paiement (dépendance propriétaire), TVA, remboursements, SLA, plus de RGPD ; non réversible à bon compte, et **ajoutable plus tard quasi sans refonte**. Écarté maintenant, gardé comme extension explicite.
- **OAuth / comptes complets** : sessions, mots de passe, flux — plus lourd que ce qu'une API *dev-first* requiert. Les clés API sont le bon ajustement. Écarté.
- **Identité propagée au domaine** : polluerait le `core` avec une préoccupation non carbone. Écarté — l'identité reste au bord.
- **Métering dans un store dédié (Redis…)** : prématuré au volume actuel ; Postgres suffit (ADR-0004), et c'est réversible derrière le port `UsageMeter`. Écarté pour l'instant.

## Questions ouvertes (implémentation — n'impactent pas le principe)

- Délivrance de clé : lien magique vs clé par email simple ; politique de rotation/révocation.
- Limites chiffrées par niveau (anonyme vs clé) et fenêtres de rate-limit.
- Granularité et rétention du métering (`UsageMeter`).
- Surface de gestion : endpoint `/v1/keys` vs petit portail sur le site statique (o2switch, ADR-0007).
- Ordre des travaux (anonyme d'abord, clés ensuite) — la direction est posée, pas le calendrier.
