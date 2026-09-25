# ADR-0008 — Méthodologie cycle de vie (`acv-ademe`) & intensité régionale

- **Statut** : Accepté
- **Date** : 2026-06-14

## Contexte

- L'ADR-0005 a **engagé** une méthode `acv-ademe` (analyse de cycle de vie via la
  Base Carbone ADEME), destinée à coexister avec `rte-direct` sans rupture.
- L'**addendum à l'ADR-0003** a acté que l'intensité carbone **régionale n'est
  pas publiée par la source** : le `taux_co2` n'existe qu'au national. Pour tenir
  la promesse de périmètre (« National + 12 régions »), l'intensité régionale
  doit être **dérivée par un modèle** appliqué au mix de production.
- Une méthode fondée sur des **facteurs d'émission par filière** répond aux deux
  besoins : elle s'applique au national (vue ACV comparable au `taux_co2` publié)
  comme au régional.

### Contrainte de données

Le mix de production **régional** (`eco2mix-regional-*`) agrège le thermique
fossile en **un seul champ `thermique`**, sans séparation gaz / charbon / fioul
(contrairement au national). Le modèle régional emploiera donc un **facteur
composite « thermique »** (voir §Décision, point régional).

## Décision

Définir la méthodologie **`acv-ademe@1`**, versionnée et portée par chaque
`Measurement` (champ `methodology`, ADR-0005), de clé d'unicité incluant la
méthodologie (ADR-0006). Elle **coexiste** avec `rte-direct` (aucune
modification de l'existant) ; le national peut exposer **les deux** (comparaison
ACV vs combustion directe publiée par RTE).

### Périmètre v1 (arbitrages actés)

1. **Référentiel unique** : facteurs **ADEME Base Carbone** (jeu cohérent,
   toutes filières). Une mise à jour vers la Base Empreinte ADEME courante
   (V23.6 [^empreinte]) fera l'objet d'`acv-ademe@2`.
2. **Basée production, sans imports** : `acv-ademe@1` calcule l'intensité du
   **mix de production**. La prise en compte des imports d'interconnexions
   (approche consommation, engagée par l'ADR-0005) relèvera d'`acv-ademe@2` —
   faute de facteur d'import ACV correctement sourçable à ce stade.
3. **Bioénergies** : valeur unique **24 gCO₂eq/kWh** (milieu de la fourchette
   ADEME 14–31 [^ademe]).
4. **Thermique régional** : à l'implémentation du régional, le champ agrégé
   `thermique` se verra appliquer le **facteur du gaz** en approximation v1
   (le charbon et le fioul sont quasi nuls dans le mix français [^rte]) ;
   raffinement possible en `acv-ademe@2`.

### Formule

Pour un mix de production (puissances en MW par filière) :

```
intensité (gCO₂eq/kWh) = Σ_filière (production_filière × FE_filière) / Σ_filière production_filière
```

- `production_filière` négatives ou nulles ignorées (bornées à 0).
- **Pompage** (consommation) et **échanges** exclus (ni production, ni imports en v1).
- Indéfinie (→ `None`) si la production totale est nulle.

### Table des facteurs — `acv-ademe@1` (ADEME Base Carbone [^ademe])

| Filière | FE (gCO₂eq/kWh) |
|---|---|
| Nucléaire | 6 |
| Gaz | 406 |
| Charbon | 1038 |
| Fioul | 778 |
| Hydraulique | 4 |
| Éolien | 7,3 |
| Solaire | 55 |
| Bioénergies | 24 |
| Thermique (composite régional) | 406 (= gaz, v1) |

> Repère de plausibilité : appliquée à un mix national très bas-carbone
> (nucléaire ≈ 38,8 GW, hydraulique ≈ 8,9 GW, éolien ≈ 2,6 GW…), la formule rend
> ≈ 12–13 gCO₂eq/kWh, cohérent avec une intensité **cycle de vie** française et
> inférieur au `taux_co2` combustion directe (≈ 15) du même instant — ce qui est
> attendu.

## Conséquences

- **Domaine (`core`)** : une table `EmissionFactors` **versionnée** (constante de
  domaine, pas une dépendance IO) et un calcul **pur** `acv_ademe_intensity(mix,
  factors)`. Aucun nouveau port sortant.
- **Ingestion** : à chaque mesure nationale portant un mix, on **dérive et stocke
  aussi** la mesure `acv-ademe` (même horodatage, même millésime). Le stockage et
  les rollups étant déjà indexés par méthodologie (ADR-0006), rien à changer côté
  schéma.
- **API** : sélection par paramètre `?methodology=` (défaut `rte-direct`).
- **Régional** (étape suivante) : exposer le mix régional (`eco2mix-regional-*`,
  aujourd'hui `NoData`), représenter le `thermique` agrégé dans le domaine, et
  dériver l'intensité `acv-ademe` régionale. `rte-direct` reste **national
  uniquement** (indicateur publié par RTE).
- **Versionnement** : toute évolution des facteurs ou du périmètre (imports,
  Base Empreinte, thermique fin) = nouvelle version (`acv-ademe@2`) + ADR ou
  addendum ; **jamais** de modification silencieuse (ADR-0005).

## Alternatives envisagées

- **`mix-factors` (combustion directe)** au lieu de l'ACV : plus proche de
  `rte-direct`, mais l'ADR-0005 a engagé l'enrichissement **cycle de vie** —
  écartée comme méthode principale.
- **Réutiliser le `taux_co2` national pour les régions** : faux par construction
  (le mix régional diffère du national) — écartée.
- **Ne pas dériver l'intensité régionale** (mix régional seul) : ne tient pas la
  promesse de périmètre — écartée.

## Sources

- [^ademe]: ADEME — Base Carbone (valeurs ACV électricité, éd. 2013), citées par Wikipédia, « Empreinte carbone de l'électricité » : nucléaire 6, charbon 1038, gaz 406, fioul 778, hydraulique (retenue) 4, photovoltaïque 55, éolien 7,3, biomasse 14–31 gCO₂eq/kWh. <https://fr.wikipedia.org/wiki/Empreinte_carbone_de_l%27%C3%A9lectricit%C3%A9>
- [^empreinte]: ADEME — Base Empreinte (ex-Base Carbone), version courante V23.6 (juillet 2025). <https://base-empreinte.ademe.fr/>
- [^rte]: RTE — Bilan électrique 2024, chapitre Émissions (part marginale du charbon/fioul ; facteurs ACV de référence). <https://analysesetdonnees.rte-france.com/bilan-electrique-2024/emissions>
- [^giec]: GIEC (IPCC) — AR5 WG3 (2014), Annexe III, médianes ACV par source (comparaison). <https://www.ipcc.ch/report/ar5/wg3/>
- RTE / ODRÉ — éCO2mix régional (champ `thermique` agrégé) : <https://odre.opendatasoft.com/explore/dataset/eco2mix-regional-tr/>

## Addendum (2026-06-20) — régional livré & évolution `@2`

Le point « Régional (étape suivante) » des Conséquences est **réalisé** : les 12 régions métropolitaines sont servies en `acv-ademe@1` (mix régional `eco2mix-regional-*`, `thermique` agrégé → facteur gaz), dérivées et stockées à l'ingestion ; `rte-direct` reste national. Cet ADR a par ailleurs été **fait évoluer** par l'**ADR-0010** (méthodologie consommation `acv-ademe@2` : imports valorisés à l'intensité du voisin + pertes T&D), servie au national.

## Addendum (2026-09-25) — nom acté `acv-ademe@3` (nouvelle table de facteurs), calendrier et travail requis

**Contexte du malentendu.** Le §Périmètre v1 point 1 promettait qu'« une mise à jour vers la Base Empreinte ADEME courante (V23.6) fera l'objet d'`acv-ademe@2` ». Ce numéro a en réalité été consommé par l'**ADR-0010** pour un changement de **périmètre** (production → consommation + imports ENTSO-E + pertes T&D), sans changement de table de facteurs : `AcvAdemeConsumption` (`@2`) appelle **la même** constante `EmissionFactors::acv_ademe_v1()` que `AcvAdemeProduction` (`@1`) — vérifié : `crates/core/src/application/get_consumption.rs:58` pour `@2` ; `crates/core/src/domain/factors.rs:176` (`derive_acv_ademe`), appelée à l'ingestion par `crates/core/src/application/ingest_latest.rs` et `crates/core/src/application/backfill.rs:70`, pour `@1`. Autrement dit, deux axes de version distincts (table de facteurs *vs* périmètre de calcul) ont été confondus dans un seul entier `@N`. **Décision : le prochain numéro libre, `acv-ademe@3`, est acté pour la mise à jour de la table de facteurs elle-même**, en gardant `@1` et `@2` strictement inchangés (gouvernance ADR-0005 : jamais de modification silencieuse d'une version publiée). Ceci **n'affecte pas** le statut *Accepté* ni ce qui est actuellement servi (`@1`, `@2`) — aucun changement de code dans cet addendum.

### Licence — contexte exact vérifié (pas de blocage identifié)

Le plan d'itérations mentionne une « demande de licence en attente à `cdo@ademe.fr` » : vérification faite, elle concerne un **autre** jeu de données — `hyd01-sites` (sites électrolyseurs géolocalisés pour la couche carte H6 v2, `docs/roadmap-hydrogene.md:48`, `docs/plan-iterations.md:196` et `:208`), **pas** les facteurs d'émission électricité. Pour la **Base Empreinte/Base Carbone** elle-même :

- La revue de licences de l'**ADR-0024** avait déjà conclu « ADEME = Licence Ouverte / Etalab 2.0 (commercial explicitement permis + attribution), confiance Haute » (`docs/adr/0024-revue-neutralite.md:92`).
- Vérifié directement ce jour sur le jeu de données lui-même : `GET https://data.ademe.fr/data-fair/api/v1/datasets/base-carboner` renvoie `"licence": "Licence Ouverte / Open Licence"`, `"date_derniere_mise_a_jour": "2025-07-03T08:07:23.668Z"` (soit la **V23.6** déjà citée [^empreinte]), 18 616 lignes (consulté le 2026-09-25).

**Donc : aucun blocage de licence pour `acv-ademe@3`**, contrairement au chantier `hyd01-sites`.

### Version courante — ce qui est confirmé vs ce qui ne l'est pas

- **Confirmé par appel API direct** (source primaire, ci-dessus) : le jeu « Base Carbone® » miroir sur `data.ademe.fr` est à la **V23.6 (3 juillet 2025)** — c'est la version déjà citée par cet ADR [^empreinte], **inchangée** depuis.
- **Non confirmé dans le temps imparti** : des recherches web (résumés IA, non revérifiés sur une source primaire accessible à cet outil) évoquent des mises à jour ultérieures de la plateforme interactive `base-empreinte.ademe.fr` (ex. « v23.10 », « v23.11 », « Base Empreinte v1.2 ») courant 2026 — `base-empreinte.ademe.fr` est une application JS qui ne s'est pas laissée lire par l'outil de récupération utilisé (page rendue vide hors titre, testé le 2026-09-25). **À revérifier à l'implémentation** (recherche manuelle ou navigateur) plutôt qu'à tenir pour acquis.

### Écart mesuré (échantillon partiel, pas la table complète)

Deux facteurs interrogés directement via l'API `data.ademe.fr` (`GET .../datasets/base-carboner/lines?q=…`, consulté le 2026-09-25) contre les valeurs actuelles de `crates/core/src/domain/factors.rs:66-78` (`acv_ademe_v1`, éd. 2013 relayée par Wikipédia [^ademe]) :

| Filière | `acv_ademe_v1` (code actuel) | Entrée Base Carbone trouvée | Écart |
|---|---|---|---|
| Nucléaire | 6 gCO₂eq/kWh (éd. 2013) | **3,7 gCO₂eq/kWh** — « Centrale nucléaire », *Électricité > Moyen de production > Conventionnels*, source **« ACV kWh nucléaire France 2022 (EDF) »**, valide jusqu'au 30/06/2027 | **−38 %**. Une entrée *archivée* à 0,006 kgCO₂e/kWh (= 6, valide jusqu'à déc. 2021) correspond exactement à la valeur encore utilisée par le code : le facteur actuel est bien le millésime **2014/2013**, déjà remplacé côté ADEME par l'étude EDF 2022. |
| Gaz | 406 gCO₂eq/kWh | 418 gCO₂eq/kWh — « Centrale gaz », même catégorie, validité affichée « déc. 2021 » | +3 %, mais **fiabilité non confirmée** : cette entrée affiche elle-même une date de validité dépassée ; impossible de dire dans le temps imparti si une entrée plus récente existe et n'a pas été retrouvée par la recherche, ou si celle-ci reste la référence de fait. |
| Solaire (indicatif, non comparable directement) | 55 gCO₂eq/kWh (cycle complet) | 25,2 à 43,9 gCO₂eq/kWh selon pays de fabrication du panneau, validité juin 2024 — mais ce sont des facteurs de **fabrication seule**, pas un facteur de génération cycle complet équivalent à la ligne actuelle | Non comparable tel quel ; indique seulement une tendance probable à la baisse (amélioration industrielle du PV depuis 2013). |
| Charbon, fioul, hydraulique, éolien, bioénergies | — | Non interrogés (budget de recherche de cette session) | **Reste à faire** |

**Lecture** : l'écart n'est pas anecdotique sur le nucléaire (−38 % sur un facteur qui, combiné à sa part dominante dans le mix français, pèse lourd dans l'intensité ACV nationale — le repère de plausibilité de cet ADR, §Décision, en serait mécaniquement affecté à la baisse) ; il est faible mais incertain sur le gaz ; il est indicatif seulement sur le solaire ; il est **inconnu** sur les 5 filières restantes. Ce tableau est un **échantillon**, pas l'audit complet requis avant implémentation.

### Points à confirmer par Morgan

> - **Périmètre couvert par `@3`** : uniquement production (successeur direct de `@1`, même périmètre — c'était l'intention originelle du §Périmètre v1 point 1), ou aussi la consommation (`@2`) ? *Recommandation : `@3` = production seule, même périmètre que `@1`, nouvelle table. Coupler en plus un changement de périmètre consommation dans le même effort referait l'erreur de confusion d'axes qui a produit ce malentendu ; un futur bump séparé de la table côté consommation reste possible ensuite (nouveau numéro, à décider alors).*
> - **Ré-dérivation de l'historique** : recalculer `@3` rétroactivement depuis 2012 (gros job CPU/DB, ~494 k lignes nationales × 13 zones à terme, mais **sans nouvel appel ODRÉ/ENTSO-E** — le mix source est déjà stocké) ou démarrer `@3` uniquement à partir de son activation (comme le fait déjà le poller pour toute mesure future, §Conséquences) ? *Recommandation : démarrer à l'activation ; ne pas ré-dériver l'historique tant qu'aucun besoin produit concret (ex. backtest profond) ne le justifie — cohérent avec le principe 6 du plan (« un déclencheur reste un déclencheur »).*
> - **Revue de neutralité avant publication** : au vu de l'ampleur de l'écart mesuré sur le nucléaire (filière politiquement sensible, CLAUDE.md/ADR-0025/0026), une **GATE de neutralité** façon ADR-0024 (revue adversariale multi-source, disclaimer de millésime par filière, aucune communication présentant la baisse comme un jugement de valeur) semble nécessaire avant de servir `@3`. *Recommandation : oui, obligatoire, réutiliser le gabarit `docs/adr/0024-revue-neutralite.md`.*
> - **Nom de la constante Rust** : `EmissionFactors::acv_ademe_v1()` désigne aujourd'hui la table (éd. 2013), pas la version de méthode — nom déjà source de confusion. *Recommandation : nommer la nouvelle table par son millésime/sa source plutôt que par un numéro `@N` (ex. `EmissionFactors::base_empreinte_2026()`), pour dissocier clairement l'axe "table de facteurs" de l'axe "version de méthode `acv-ademe@N`" et ne pas reproduire le malentendu `@2` initial.*

### Travail requis (décidé ici, non implémenté)

1. **Audit complet de la table** : les 9 lignes de facteurs (dont le composite `thermique` régional) relevées avec la même rigueur que l'échantillon ci-dessus (source primaire, millésime, validité, URL, date de consultation) — actuellement seuls nucléaire/gaz/solaire ont un point de comparaison, partiel.
2. **Nouvelle table de facteurs versionnée** dans `crates/core/src/domain/factors.rs`, distincte d'`acv_ademe_v1()` (celle-ci reste **inchangée**, immuabilité de version — ADR-0005) ; nouvelle valeur de `Methodology` (`acv-ademe`, version 3).
3. **Re-dérivation historique** : à trancher (point ci-dessus) avant d'implémenter.
4. **Revalidation des backtests** — situation vérifiée dans le code, plus précise que ce qui était supposé :
   - **ADR-0012 (`gbdt@1`)** : **non concerné**. Le modèle GBDT est explicitement borné à `rte-direct`, national, et **découplé de l'axe méthodologie** (« la prévision `acv-ademe` … reste couplée à l'axe 1 → reportée », `docs/adr/0012-modele-prevision-ml-gbdt.md:104`) — aucun changement de facteurs `acv-ademe` ne touche son backtest.
   - **ADR-0009 (`climatology@1`)** : le modèle est générique sur `methodology_id` (`crates/adapter-forecast/src/lib.rs:156-165`, prend `methodology_id` en paramètre), donc techniquement réutilisable sur une série `acv-ademe@3` une fois celle-ci stockée — mais ses paramètres par défaut (`N=10`, `τ=2 semaines`) n'ont jamais été calibrés que sur « national `rte-direct`, 2024 » (`docs/adr/0009-modele-prevision-climatologie.md:193`) : ils ne sont donc **pas invalidés** par `@3` (ils ne couvraient déjà pas `acv-ademe`), mais **pas validés** pour autant. Servir une prévision `climatology` sur `@3` nécessiterait un `backtest-sweep` dédié, pas juste un rejeu.
   - **ADR-0013 (`backtest-acv`, non cité par le plan mais le plus directement concerné)** : `BacktestConsumptionForecast` (`crates/core/src/application/backtest.rs:256-289`) dérive **à la fois** la vérité `@2` (via `derive_consumption_series`) **et** implicitement toute prévision `acv-ademe` (le même calculateur `AcvAdeme`, ADR-0013 §1) à partir d'`EmissionFactors::acv_ademe_v1()`. Si `@3` est un jour branché derrière la prévision (au-delà de la simple table de facteurs décidée ici), ce backtest et ses bandes d'incertitude calibrées (`calibrate_bands`) devront être **rejoués et recalibrés** — c'est le point d'ancrage réel entre « nouvelle table de facteurs » et « qualité de prévision », plus que l'ADR-0009/0012 initialement cités par le plan.
5. **GATE de neutralité** (point « Confirmer par Morgan » ci-dessus) avant publication.

### Calendrier / déclencheurs

Pas de date ferme : l'effort est qualifié d'**important** par le plan, et cet addendum ne fait que décider le nom, le périmètre attendu et la liste de travaux — l'implémentation reste **hors de cette itération** (I7 est décisionnelle uniquement pour ce chantier). Condition avant de démarrer l'implémentation : l'audit complet du point 1 ci-dessus (les 9 lignes, pas l'échantillon de 3). Faire figurer ce chantier au backlog « en attente d'un déclencheur » du plan d'itérations relève de `docs/plan-iterations.md`, hors du périmètre d'écriture de cet addendum.

### Sources (addendum 2026-09-25)

- `https://data.ademe.fr/data-fair/api/v1/datasets/base-carboner` — métadonnées du jeu de données (version, licence, date de mise à jour), consulté le 2026-09-25.
- `https://data.ademe.fr/data-fair/api/v1/datasets/base-carboner/lines?q=...` — recherches ciblées « nucléaire », « gaz naturel », « électricité » (facteurs individuels cités ci-dessus), consulté le 2026-09-25.
- `docs/adr/0024-revue-neutralite.md:92` — confirmation de licence ADEME (recherche du 2026-06-20).
- `docs/roadmap-hydrogene.md:48`, `docs/plan-iterations.md:196` et `:208` — contexte exact de la demande `cdo@ademe.fr` (jeu `hyd01-sites`, sans lien avec la Base Empreinte).
