# ADR-0029 — Carte « électrolyseurs × carbone live » (couche B-light)

- **Statut** : Accepté, implémenté
- **Date** : 2026-07-03
- **Décideurs** : Morgan (Kovelt / carbon-fr)
- **ADR liés** : ADR-0025 (parent — décision 5, couche B-light), ADR-0007 (topologie — site statique o2switch resté non concrétisé), ADR-0024 (discipline licences), ADR-0026/0028 (données d'éligibilité consommées)

---

## Contexte

L'ADR-0025 réserve la couche **B-light** : une page carte « électrolyseurs FR/UE × intensité carbone live », fusion d'une donnée **structurelle** à cadence lente et de la donnée **temps réel** de carbon-fr — présentée comme le différenciateur du produit (ni l'Observatoire européen ni h2inframap n'ont la couche carbone temps réel), **sans API structurelle à maintenir**. Chantier H6 de la roadmap hydrogène.

Enquête préalable (2026-07-03, sources et licences vérifiées sur pièces — fichiers téléchargés et inspectés, pages *legal notice* citées verbatim) :

| Source | Contenu | Licence | Verdict |
|---|---|---|---|
| **European Hydrogen Observatory** (Clean Hydrogen JU) — « Hydrogen production and consumption projects » | 238 sites UE (26 FR) : **lat/lon**, MWel, statut Operation/Construction, filière, année | « Reproduction is authorised, provided the source is acknowledged » (legal notice ; politique UE 2011/833) | **Retenu** (v1) |
| ADEME `hyd01-sites` (data.ademe.fr) | 98 sites FR aidés (lat/lon, MW, kg/j, statut, avancement), très frais | Champ licence **null** sur ce dataset précis (le portail est « quasi-totalité » Licence Ouverte) | **En attente** — couche v2 après confirmation écrite (`cdo@ademe.fr`), même réflexe que RTE en ADR-0024 |
| Vig'Hy (France Hydrogène) | Carte web de la filière | **Aucune licence publiée** (association privée, pas d'obligation d'ouverture) | **Écarté** — lien de renvoi seulement |
| GISCO/Eurostat NUTS (contours) | Frontières administratives UE | « © EuroGeographics », clause **usage commercial → accord séparé** (famille NC) | **Écarté** (discipline ADR-0024) |
| IGN Admin Express (contours régions FR) | 13 régions métropolitaines | **Licence Ouverte 2.0** | **Retenu** |
| Natural Earth (contexte pays) | Pays 1:110M | **Domaine public** (« no permission is needed ») | **Retenu** |
| Registre national RTE / ODRÉ | Production/stockage électricité | — | **Hors sujet** vérifié : les électrolyseurs sont des consommateurs, hors périmètre L142-9-1 |

## Décision

1. **Une page, pas un produit** : `GET /hydrogene` + trois jeux embarqués (`/hydrogene/sites.json`, `regions.geojson`, `pays.geojson`), servis par l'API **hors contrat `/v1`** (précédent `/docs`) — pas dans l'OpenAPI, pas versionnés. Rationale : le site statique o2switch (ADR-0007) n'a jamais été concrétisé ; servir la page depuis le binaire déjà déployé = zéro nouvelle infra, même origine (pas de CORS), parité self-hosting. La migration vers un site statique reste ouverte (le CORS de l'API est déjà permissif).

2. **Donnée structurelle = EHO seul en v1**, filtrée `Water electrolysis` (233 sites, 25 FR) : `name, city, country, mw, year, stage, lat, lon` + en-tête de **provenance embarquée** (source, URL, licence, périmètre, instantané, date de récupération). Pas de facette « technologie » (PEM/alcalin) : la source ne la fournit pas — on n'invente pas. **Cadence semestrielle** (mesurée sur 3 instantanés Dec2024/May2025/Dec2025) : rafraîchissement = re-télécharger le XLSX, rejouer la conversion (procédure §Rafraîchissement), committer. Le test `embedded_datasets_are_valid_json_with_provenance` garde le contrat du fichier.

3. **Page auto-contenue, zéro dépendance** : un seul HTML, SVG maison (projection équirectangulaire compensée), **aucun CDN, aucune tuile externe, aucune bibliothèque** — gardé par le test `page_is_self_contained`. Fond : régions IGN simplifiées (~88 Ko, coordonnées à 3 décimales), pays Natural Earth allégé (~29 Ko), sites (~35 Ko). Total < 200 Ko.

4. **Couche live = composition de l'API existante, rien de neuf** : choropleth des 12 régions par `GET /v1/intensity/now?region=&methodology=acv-ademe` (12 appels légers au chargement), bandeau national `rte-direct` rafraîchi par **SSE** (`/v1/intensity/stream`, repli dernière valeur), fenêtres `rfnbo`/`low-carbon` par `greenest-window?eligibility=` (réponse additive ADR-0026/0028). La **Corse** (présente dans le fond de carte, hors couverture éCO2mix régional) est rendue « sans donnée », légendée.

5. **Dataviz disciplinée** (skill dataviz, palettes **validées par script**) : rampe séquentielle bleue à 5 classes (`<25, 25-45, 45-70, 70-100, ≥100 gCO₂eq/kWh`), une par mode — en **sombre la rampe est retournée** (faible = proche de la surface, fort = saillant) ; marqueurs orange (contraste CVD ≥ 96 vs le fond bleu, vérifié), plein = exploitation / creux = construction (double encodage forme+remplissage), **surface ∝ capacité** ; tooltips par région et par site ; thème clair/sombre (`prefers-color-scheme`).

6. **Neutralité (cardinal, hérité ADR-0025)** : la page n'affiche **jamais** une éligibilité **par site** (donnée niveau site absente) — la couleur carbone est celle du **réseau**, les marqueurs un état structurel. Dit **aux endroits où la mélecture se produit** (passe de neutralité du 2026-07-03, constats C2/C5) : panneau des fenêtres, **légende**, **tooltip de chaque site** (« couleur régionale : signal réseau, pas une éligibilité du site »), encadré « Ce que montre cette page » (qui nomme le **nucléaire** — une intensité basse en France n'est pas synonyme de renouvelable) ; présence garantie par test de non-régression. Attribution complète en pied de page (contrat de réutilisation des sources).

## Rafraîchissement (semestriel, manuel)

1. Télécharger le dernier « Hydrogen production and consumption projects <MoisAnnée>.xlsx » depuis la [page datasets](https://observatory.clean-hydrogen.europa.eu/tools-reports/datasets) de l'Observatoire.
2. Rejouer la conversion (parsing XLSX → filtre `Water electrolysis` → champs `name/city/country/mw/year/stage/lat/lon` arrondis, tri pays+nom, en-tête de provenance avec le nouvel instantané et la date de récupération) — le format cible est celui de `crates/adapter-http/assets/hydrogene/sites.json`.
3. `cargo test -p carbonfr-adapter-http hydrogene` (le test de contrat valide le nouveau fichier), commit, release.

## Conséquences

**Positives** : le différenciateur d'ADR-0025 existe (personne d'autre ne croise l'infra H₂ et le carbone temps réel) ; zéro maintenance structurelle (un fichier semestriel) ; zéro dépendance externe au runtime ; licences toutes propres et attribuées ; self-hosting servi à l'identique.

**Négatives / limites (assumées)** :
- Couverture EHO : ≥ 0,5 MWel, **exploitation + construction seulement** (pas les annonces) — dit dans la page.
- Instantané figé entre deux rafraîchissements (« instantané Dec2025 » affiché) ; la cadence dépend de l'Observatoire.
- 12 appels au chargement (pas d'endpoint bulk) — négligeable, mais un endpoint « toutes régions » serait une optimisation future si la page devient très fréquentée.
- La page vit dans le binaire (≈ +170 Ko) — accepté, précédent `/docs`.

## Alternatives envisagées

- **Site statique o2switch d'abord** (lettre de l'ADR-0007) — *reporté* : aurait fait de B-light l'otage d'un chantier d'infra jamais commencé ; le CORS ouvert permet d'y migrer sans toucher l'API.
- **Tuiles OSM/OpenFreeMap** — *écarté* : dépendance réseau runtime + politique d'usage à respecter, contraire à l'auto-contenance ; le SVG suffit à cette échelle.
- **Leaflet/MapLibre/D3** — *écarté* : 13-207 Ko gzip pour ~250 marqueurs et 13 polygones ; la projection maison fait 30 lignes.
- **Inclure ADEME `hyd01-sites` tout de suite** — *reporté* : licence non taguée sur ce dataset précis ; demande écrite d'abord (gouvernance, précédent ADR-0024).
- **GISCO/NUTS pour les contours** — *écarté* : clause commerciale EuroGeographics (famille des clauses NC déjà refusées).
- **Éligibilité affichée par site** — *rejeté* : violerait le périmètre d'ADR-0025 (donnée niveau site absente) et la neutralité — c'est un garde-fou, pas une limitation technique.
