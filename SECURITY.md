# Politique de sécurité

Merci de contribuer à la sécurité de `carbon-fr`. Ce document décrit **comment signaler une faille** et **ce à quoi t'attendre** en retour.

## Signaler une vulnérabilité

**Ne pas** ouvrir d'*issue* publique, de *pull request* ni de discussion pour une faille de sécurité : un rapport public expose les utilisateurs avant qu'un correctif existe.

Privilégier le **signalement privé via GitHub** :

1. Onglet **Security** du dépôt → **Report a vulnerability** ([*Private vulnerability reporting*](https://docs.github.com/en/code-security/security-advisories/guidance-on-reporting-and-writing-information-about-vulnerabilities/privately-reporting-a-security-vulnerability)).
2. Décrire le problème, son impact, et les étapes de reproduction.

Le canal est privé : seuls les mainteneurs y ont accès, et la divulgation reste coordonnée jusqu'au correctif.

### Un bon rapport contient

- le **type** de faille (injection, SSRF, fuite de données, contournement d'authentification…) ;
- les **composants** touchés (endpoint `/v1/…`, adapter, SDK, image Docker) et la **version** (tag `vX.Y.Z` ou image GHCR) ;
- une **reproduction** pas à pas, idéalement une requête `curl` minimale ;
- l'**impact** que tu anticipes.

## Délais

Projet maintenu en *best effort* (open source, mainteneur unique pour l'instant) :

| Étape | Délai visé |
| --- | --- |
| Accusé de réception | **72 h** |
| Évaluation initiale (sévérité, recevabilité) | **7 jours** |
| Correctif ou plan de remédiation | selon la sévérité, on tient au courant |

Après correction, une **GitHub Security Advisory** est publiée ; le mérite t'est attribué si tu le souhaites (sinon, signalement anonyme respecté).

## Versions supportées

Avant la `1.0`, seule la **dernière release** reçoit les correctifs de sécurité. En production, **épingler une version exacte** de l'image (`ghcr.io/kovelt/carbon-fr:X.Y.Z`) et suivre les releases ; un correctif de sécurité = nouvelle version à redéployer (cf. [GOUVERNANCE.md](GOUVERNANCE.md), ADR-0019).

| Version | Supportée |
| --- | --- |
| Dernière release (`latest`) | ✅ |
| Versions antérieures | ❌ |

## Périmètre

**Dans le périmètre** : le service API (`bin/server`, endpoints `/v1`), les adapters, le SDK officiel [`@carbon-fr/sdk`](sdk/typescript/), l'image Docker publiée.

**Hors périmètre** : la disponibilité de l'instance hébergée ([carbon-fr-api.kovelt.fr](https://carbon-fr-api.kovelt.fr)), les sources de données amont (RTE/éCO2mix/ODRÉ, ENTSO-E, Open-Meteo), et toute instance auto-hébergée par un tiers. Les rapports de **déni de service** purement volumétriques contre l'instance hébergée ne sont pas recevables.

> Note de souveraineté des données : `carbon-fr` ne stocke **aucune IP en clair** (empreintes SHA-256 salées) et l'authentification est **optionnelle** (anonyme par défaut). Une faille touchant ces garanties est considérée comme **sévère**.
