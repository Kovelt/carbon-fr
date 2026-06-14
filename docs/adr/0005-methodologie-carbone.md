# ADR-0005 — Méthodologie carbone

- **Statut** : Accepté
- **Date** : 2026-06-14

## Contexte

L'intensité carbone n'est pas un chiffre absolu : elle dépend de choix méthodologiques explicites. Deux projets peuvent annoncer des valeurs très différentes pour le même réseau selon qu'ils :

- comptent les **émissions directes** (combustion) ou le **cycle de vie complet** (ACV — construction, combustible, démantèlement) ;
- **incluent ou non les imports/exports** d'électricité aux interconnexions ;
- raisonnent sur la **production** ou sur la **consommation** d'un périmètre géographique donné.

Pour référence : l'estimation publiée par RTE porte sur les émissions de la **production d'électricité en France**. Le modèle britannique, lui, intègre la génération **plus** les imports par interconnexions **et** les pertes de transport/distribution. Sans méthodologie affichée, un chiffre n'est pas comparable.

## Décision

1. **Base MVP** : reprendre l'estimation RTE telle quelle (émissions de la production en France), ce qui rend les valeurs **directement comparables à éCO2mix**.
2. **La méthodologie est un attribut de premier ordre, versionné et sélectionnable**, dès le MVP : chaque réponse de l'API porte un identifiant de méthode (p. ex. `methodology = "rte-direct"` + une version), même s'il n'existe qu'une seule valeur au lancement. Le **domaine** porte également cet identifiant sur chaque mesure.
3. **Enrichissement engagé** : fournir une **méthodologie enrichie** — cycle de vie via la **Base Carbone ADEME** et prise en compte des **imports interconnexions** (à la manière du modèle UK) — comme **méthode additionnelle versionnée** (p. ex. `acv-ademe`), **coexistant** avec la méthode RTE. Ce n'est pas un remplacement : les deux méthodes restent disponibles et comparables.

## Conséquences

- Démarrage rapide, chiffres crédibles et comparables à la référence officielle française.
- L'API **et le domaine** portent un **identifiant de méthodologie dès le MVP** → l'enrichissement s'ajoute **sans casser le contrat** : les clients voient et/ou choisissent la méthode, et aucune valeur existante ne change de sens en silence.
- Transparence : chaque réponse dit quelle méthode l'a produite ; plusieurs méthodes peuvent coexister et être comparées côte à côte.
- **Engagement de gouvernance** : toute nouvelle méthode = **nouvelle version + nouvel ADR** dédié (spécification des facteurs ACV, traitement des imports, sources). Jamais de modification silencieuse d'une méthode publiée.
- Conséquence de modélisation à intégrer avant la phase 1 : le type `Measurement` du domaine porte un champ `methodology` (identifiant + version).

## Alternatives envisagées

- **Cycle de vie complet (ACV) comme seule méthode dès le départ** : plus juste sur le plan environnemental, mais plus lourd (intégration de facteurs externes, calibration) et moins directement comparable à éCO2mix au lancement. Écarté comme méthode *unique* — mais **retenu comme méthode additionnelle** (point 3).
- **Méthode unique non versionnée** : rejetée — elle interdit d'enrichir la méthodologie sans rupture pour les clients.

## Suite

La spécification de la méthode enrichie (`acv-ademe` : facteurs ADEME retenus, périmètre des imports, traitement des pertes) fera l'objet d'un **ADR dédié** au moment de sa conception, en phase ultérieure.
