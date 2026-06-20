# ADR-0025 — Extension au vecteur hydrogène : couche carbon-aware « électrolyseur » et cadres d'éligibilité RFNBO / bas-carbone

- **Statut** : Accepté — **couche A implémentée** (méthodologie détaillée : ADR-0026)
- **Date** : 2026-06-20
- **Décideurs** : Morgan (Kovelt / carbon-fr)
- **ADR liés** : ADR-0003 (périmètre de données), ADR-0005 (méthodologie carbone), ADR-0006 (cycle de vie des révisions de données), ADR-0014 (primitives de scheduling carbon-aware)
- **Suite** : ADR-0026 (méthodologie détaillée des overlays d'éligibilité) — **rédigée et acceptée**

---

## Contexte

Une idée issue de la communauté propose d'étendre carbon-fr aux vecteurs énergétiques alternatifs, l'hydrogène en tête, sur le modèle de la carte interactive production/consommation d'électricité.

L'analyse du paysage de données 2026 fait apparaître un constat structurant :

- **Pas de substrat temps réel pour l'hydrogène.** Le parc d'électrolyseurs est quasi inexistant (la France vise 6,5 GW installés en 2030, soit un objectif de construction). Aucun gestionnaire de réseau ne publie de flux H₂ horodatés comme RTE le fait pour l'électricité via éCO2mix. La donnée existante est structurelle (localisations, capacités estimées) et de cadence annuelle/trimestrielle : Observatoire européen de l'hydrogène (Clean Hydrogen Partnership), h2inframap (ENTSOG et opérateurs gaziers, trajectoire à 2040), exposés en XLSX/dashboards. Le seul flux temps réel hydrogène existant — la disponibilité des stations de recharge (E-HRS-AS) — est déjà fourni en open source par l'UE.
- **Trois archétypes de données** doivent être distingués : temporel (éCO2mix, ENTSO-E — le terrain de carbon-fr), structurel/référentiel (infra, capacités), projection/scénario (coûts, modèles réseau 2030-2050).
- **Le seul aspect hydrogène à la fois carbon-relevant et temps réel** est l'intensité carbone de l'électricité alimentant l'électrolyseur — donnée que carbon-fr possède déjà, à la maille régionale et horodatée.
- **Besoin réglementaire émergent.** Les règles RFNBO (Règlements délégués (UE) 2023/1184 et 2023/1185) imposent une corrélation temporelle entre électrolyse et électricité renouvelable, se resserrant vers une granularité horaire (échéance fixée au 1er janvier 2030, actuellement sous pression de report vers 2031-2033). Ce besoin de matching horaire correspond précisément aux primitives carbon-aware de carbon-fr (ADR-0014).
- **Concurrence.** Le terrain structurel/observatoire est tenu par un acteur institutionnel financé (l'Observatoire européen). Reproduire une API structurelle hydrogène reviendrait à concurrencer une institution sur une donnée statique de faible volume.

---

## Décision

1. **Périmètre.** L'opportunité hydrogène est servie comme **extension de la couche de décision carbon-aware existante** (ADR-0014), à l'intérieur de carbon-fr. **Pas de produit sœur.**

2. **Couche A — overlay « électrolyseur ».** Helper SDK / endpoint mince implémentant les primitives de corrélation **temporelle** et **géographique** au-dessus des données régionales horodatées existantes et de `/greenest-window`. Cette couche **ne calcule pas** de gCO₂eq/kgH₂ ni de certification d'hydrogène : hors périmètre, car cela exige une donnée au niveau du site de production que carbon-fr ne possède pas.

3. **Cadres d'éligibilité servis en binôme, explicitement étiquetés**, par cohérence avec la dualité `rte-direct` / `acv-ademe` (ADR-0005) :
   - `rfnbo` — vue renouvelable (Règlements délégués (UE) 2023/1184 et 2023/1185) : piliers additionnalité, corrélation temporelle, corrélation géographique.
   - `low-carbon` — vue bas-carbone inclusive (nucléaire et gaz + CCS), acte délégué adopté par la Commission le 8 juillet 2025.
   - **Neutralité.** carbon-fr expose l'éligibilité au regard de chaque cadre ; il ne tranche pas la « couleur » de l'hydrogène ni le débat renouvelable/nucléaire. La donnée est ouverte et vérifiable ; les conclusions appartiennent à l'utilisateur.

4. **Paramétrage versionné** (cohérent avec ADR-0006). Aucune règle réglementaire n'est codée en dur. Sont paramétrables et versionnés : la fenêtre de corrélation temporelle (mensuelle/horaire), la date de bascule (défaut `2030-01-01`, révisable en cas de report), le seuil de l'exemption « Article 4 » (défaut 90 % de renouvelables dans le mix national), l'exception de surplus (prix de l'électricité < 20 €/MWh).

5. **Couche B-light — visualisation.** Page carte « électrolyseurs FR/UE × intensité carbone live » fusionnant une donnée structurelle ingérée (registre national RTE sur data.gouv + datasets de l'Observatoire européen, cadence trimestrielle) et la donnée live de carbon-fr. C'est le différenciateur : ni l'Observatoire ni h2inframap n'offrent la couche carbone temps réel par-dessus l'infra. Réalisé en **page/visualisation**, sans API structurelle à maintenir.

6. **Couche B-full — observatoire hydrogène autonome (API structurelle).** **Rejeté pour l'instant** (voir alternatives). Documenté en réserve ; à réexaminer si un gap dev-first se confirme.

---

## Conséquences

**Positives**
- Positionnement précoce sur un besoin réglementaire daté (matching horaire 2030).
- Réutilisation du substrat temps réel existant : coût marginal de développement et de maintenance faible.
- Cohérence avec la thèse « instrument de mesure temps réel » et avec l'architecture hexagonale (les overlays sont des stratégies de domaine, comme les méthodologies carbone).
- Neutralité énergétique préservée et lisible (binôme `rfnbo` / `low-carbon`).
- Différenciateur réel (fusion infra structurelle × carbone live) inaccessible aux acteurs en place.

**Négatives / risques**
- Dépendance à une réglementation mouvante (échéance horaire, seuil Article 4) → mitigée par le paramétrage versionné de la décision 4.
- Surface produit élargie à documenter (page d'usage, page carte, méthodologie).
- Nécessite l'ingestion d'un dataset structurel à cadence trimestrielle (charge légère).

**Suite**
- Un ADR de méthodologie dédié (**ADR-0026** pressenti) détaillera la paramétrisation fine des overlays au moment de l'implémentation, à l'image des ADR-0008/0010 pour `acv-ademe`.
- Un **brief d'implémentation** de la couche A accompagne cet ADR : [`brief-couche-A-electrolyseur`](../brief-claude-code-couche-A-electrolyseur.md).

---

## Alternatives considérées

1. **Produit sœur « observatoire hydrogène » (API structurelle autonome).**
   *Rejeté* : terrain déjà tenu par l'Observatoire européen ; donnée statique, annuelle et de faible volume, pour laquelle l'« API-fication » apporte une valeur marginale ; dilution de la marque « mesure temps réel ».

2. **RFNBO seul, sans cadre bas-carbone.**
   *Rejeté* : exclurait l'essentiel de l'électricité bas-carbone française (nucléaire) et constituerait un alignement implicite sur la position allemande dans le débat européen — rupture du principe de neutralité, et perte de pertinence pour le marché français.

3. **Moteur de certification GHG hydrogène (CertifHy / IPHE / ISO 19870).**
   *Rejeté* : hors périmètre ; exige une donnée au niveau du site de production non disponible et que carbon-fr n'a pas vocation à collecter.

4. **Ne rien faire / différer entièrement le sujet.**
   *Rejeté* : perte du bénéfice de positionnement précoce sur un besoin réglementaire clairement identifié, alors que la couche A est réalisable à coût marginal sur la donnée existante.
