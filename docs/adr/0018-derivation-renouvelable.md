# ADR-0018 — Dérivation renouvelable : météo → production éolien/solaire

- **Statut** : Accepté, **engagé** (calculateur + backtest + **exposition `/v1/renewable`** livrés ; prévision météo-pilotée à suivre)
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
- **Livré** : `/v1/weather` (substrat, attribution Open-Meteo CC-BY 4.0 vérifiée) et `/v1/renewable` (production estimée + facteur de charge, modèle auto-calibré au démarrage).
- **Écarté (mesuré)** : la **prévision d'intensité météo-pilotée** (étape A) — voir addendum.
- **À suivre éventuellement** : raffinement des paramètres de courbe (médiane/raideur calés par balayage) ; capacité variable dans le temps (parc croissant).

## Addendum (2026-06-16) — Étape A : prévision météo-pilotée **écartée** (gate non franchi)

Avant de construire un `forecast@N` (météo prévue → renouvelable prévu → intensité prévue), on a mesuré le **plafond** du gain : l'anomalie de renouvelable **réel** (borne supérieure, perfect-foresight) améliore-t-elle la climatologie d'intensité, hors échantillon ? (cas d'usage `AnalyzeRenewableSignal`, sous-commande `analyze-renewable-signal`).

**Mesuré (2024, national, 5270 pts test)** : RMSE intensité **12,0 → 11,5** (gain **0,48 gCO₂eq/kWh, ~4 %**), avec **β ≈ 0**. L'outil est **validé** (tests : détecte un signal synthétique fort, donne β≈0 sans lien) → le résultat est fiable, pas un artefact.

**Conclusion** : pour le réseau **français**, dominé par le **nucléaire** (déjà très bas carbone), les variations de renouvelable ne déplacent quasiment pas l'intensité **au-delà de ce que la climatologie capte déjà**. Avec du renouvelable *prévu* (erreur météo en plus), le gain serait encore plus marginal. **On ne construit donc pas `forecast@N` météo-piloté** — même discipline que l'ajustement de charge (ADR-0011 §4) et le GBDT bout-en-bout (ADR-0012), tous deux écartés au backtest.

La dérivation renouvelable garde toute sa valeur **comme produit** (`/v1/renewable` : production estimée, facteur de charge, « pourquoi le carbone est bas ») — mais **pas comme levier de précision de la prévision d'intensité** sur ce réseau.
