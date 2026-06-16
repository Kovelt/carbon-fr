# ADR-0018 — Dérivation renouvelable : météo → production éolien/solaire

- **Statut** : Accepté, **engagé** (fondation livrée : calculateur + backtest ; exposition à suivre)
- **Date** : 2026-06-16
- **S'appuie sur** : ADR-0002 (hexagonal), ADR-0009 (gardé par backtest), ADR-0012 (store météo)

## Contexte

La météo (vent à 100 m, irradiance) est déjà ingérée (ADR-0012) mais n'alimentait qu'un GBDT bout-en-bout `météo → intensité` qui **ne battait pas la climatologie** (les arbres n'extrapolent pas). Le levier différenciant — *dev-first* FR/EU, que personne n'expose — est la **couche de dérivation métier** : décomposer le problème en un **intermédiaire physique explicable**, plutôt qu'un modèle opaque.

```
vent à 100 m  ──(courbe de puissance agrégée)──► éolien estimé (MW)
irradiance    ──(modèle PV linéaire)───────────► solaire estimé (MW)
                                   └──► part renouvelable → intensité carbone
```

## Décision

Un **calculateur de domaine pur** (`RenewableModel`, `core`, zéro IO), **versionné** :

- **Éolien** : facteur de charge = **sigmoïde du vent** (la courbe de puissance individuelle, avec cut-in/cut-out, se lisse à l'échelle de la flotte nationale) ; production = capacité effective × facteur de charge.
- **Solaire** : **linéaire en irradiance** (réf. STC 1000 W/m²), nul la nuit.
- **Capacités effectives calibrées** par moindres carrés à l'origine sur l'historique (le parc installé croît dans le temps → on cale, on ne suppose pas). Régression **sans constante** : pas de ressource → pas de production.

La qualité est **mesurée par backtest** (cas d'usage `BacktestRenewable`, sous-commande `backtest-renewable`), **jamais supposée** — même discipline que la prévision (ADR-0009). Protocole **anti-surapprentissage** : calibration sur les 70 % anciens, mesure d'erreur sur les 30 % récents, comparée à un **baseline naïf** (production moyenne). Un modèle qui ne bat pas le baseline n'est pas publié.

## Évidence mesurée (2024 S1, national, 2621 points de test)

| Filière | Modèle (RMSE) | Baseline (RMSE) | Capacité calibrée | Réel installé FR 2024 |
|---|---|---|---|---|
| Éolien  | **1747 MW** | 4262 MW | 22 085 MW | ~22 GW ✓ |
| Solaire | **1387 MW** | 4727 MW | 18 334 MW | ~18 GW ✓ |

Le modèle bat le baseline d'un facteur **2,4× (éolien)** et **3,4× (solaire)**, et les capacités calibrées **retrouvent le parc réellement installé** — validation que la décomposition est physiquement juste, pas un *fit* opportuniste. **Le moat est réel et chiffré.**

## Conséquences

- **Défendable** : tout le monde peut relire Open-Meteo ; calibrer et **prouver** la dérivation sur la production RTE française, non. C'est notre IP (le calcul est nôtre, on cite les sources de méthode : courbe de puissance, modèle PV).
- **Honnêteté** : moyenne nationale + relation **contemporaine** (météo de l'heure → production de l'heure) — on prouve d'abord le **lien physique**.
- **À suivre** (engagé, derrière le même contrat versionné) :
  1. **Prévision** : météo *prévue* (anti-fuite `run_at ≤ valid_at − h`) → production prévue → intensité prévue (`forecast@N`, gardé par backtest face à `climatology@1`).
  2. **Attribution carbone** : part renouvelable estimée → contribution à l'intensité (le produit final).
  3. **Exposition** : `/v1/weather` (substrat, **attribution Open-Meteo + vérification de licence avant publication**) puis un endpoint de potentiel renouvelable.
  4. **Raffinement** : paramètres de courbe (médiane/raideur) calés par balayage ; capacité variable dans le temps (parc croissant) plutôt que constante par fenêtre.
