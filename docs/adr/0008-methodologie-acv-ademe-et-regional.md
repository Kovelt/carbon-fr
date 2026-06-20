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
