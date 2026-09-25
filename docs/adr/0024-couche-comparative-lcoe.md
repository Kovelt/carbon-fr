# ADR-0024 — Couche comparative LCOE (coût de production) : cadre de neutralité

- **Statut :** Accepté (ratifié le 2026-06-20) — **GATE de neutralité franchi le 2026-06-20** (revue datée : [`0024-revue-neutralite.md`](0024-revue-neutralite.md), évaluation adversariale pro/anti-nucléaire + audits structurels). **Licences confirmées (recherche 2026-06-20) : pré-condition levée sous conditions** — réutilisation des chiffres-faits fondée sur Licence Ouverte (ADEME), CRPA + absence de clause NC (Cour des comptes), et non-protection des faits + extraction non substantielle (RTE, dont les mentions légales du rapport sont restrictives) ; voir §5 et la revue §licences. *Conditions* : ne ré-encoder que des valeurs (jamais tableaux/figures), attribution nominative, et — pour un **palier payant** s'appuyant sur la donnée **RTE** — demande de confirmation écrite à RTE recommandée. **Multi-sources atteint depuis 2026-06-20** (6 des 7 filières : ADEME + IRENA pour les renouvelables, Cour des comptes + CRE pour le nucléaire existant). Reste ouvert (gouvernance, non bloquant) : une 2e source licence-compatible pour le nucléaire **nouveau** (encore mono-source RTE) et une contre-source **France** pour les renouvelables.
- **Date :** 2026-06-20
- **Décideur :** Morgan (Kovelt / carbon-fr)
- **Lié à :** ADR-0023 (décomposition du prix ancrée TRV), ADR-0005 (méthodologie carbone), ADR-0006 (cycle de vie / millésime des données)

> **Note de numérotation (2026-06-20)** : rédigé initialement sous le numéro provisoire « ADR-0016 », réattribué **ADR-0024** à l'intégration (0015 et 0016 étant déjà pris par le tier hébergé et les webhooks).

> **Cet ADR ne décrit pas une fonctionnalité, il décrit des garde-fous.** L'objet livré (un comparatif coût de production / prix de marché) est secondaire ; ce qui est décidé ici, c'est *à quelles conditions strictes* carbon-fr peut l'exposer sans cesser d'être un instrument de mesure neutre.

---

## Contexte

L'ADR-0023 a écarté le LCOE (coût de production par source) comme ancrage du « prix réel de l'énergie », au profit de la composante énergie spot (factuelle). Le LCOE reste néanmoins demandé par la communauté comme **comparatif pédagogique** : « combien coûte réellement à produire » en regard de « combien se vend l'énergie sur le marché ».

C'est la zone la plus sensible du projet. Le risque n'est **pas** la donnée — c'est le **cadrage**. Trois pièges spécifiques au LCOE :

1. **Le LCOE n'est pas un fait, c'est une estimation sous hypothèses.** Le résultat dépend du taux d'actualisation (WACC), de la durée de vie retenue, du facteur de charge, du périmètre (coûts plateau seuls / coûts système d'intégration / externalités / démantèlement / stockage des déchets), et surtout — pour le nucléaire français — du choix **parc existant amorti** vs **nouveau (EPR)**. Deux estimations « sérieuses » peuvent différer d'un facteur 2 à 3.

2. **Choisir une source ou un chiffre, c'est prendre parti.** Annoncer « nucléaire 0,05 €/kWh » (parc existant, vision basse) plutôt que « 0,10–0,15 » (nouveau nucléaire) *est* une position dans le débat énergétique français, même sans phrase. L'inverse aussi.

3. **Le LCOE n'est pas le coût marginal.** Les centrales sont appelées sur leur coût *marginal*, pas sur leur LCOE. Mettre « coût de production » et « prix de marché marginal » en différence revient à comparer deux grandeurs de nature différente et à suggérer que leur écart est anormal — alors qu'un marché à tarification marginale les *fait* diverger par construction.

**Principe directeur (hérité de l'ADR-0023) :** la neutralité ne se déclare pas, elle se construit dans la structure de donnée. Ici, elle repose sur la **dispersion assumée** et la **transparence de méthode**, pas sur un chiffre.

---

## Décision

### Principe 0 — La neutralité prime sur la fonctionnalité

Cette couche est un **bonus pédagogique, jamais porteur de la mission**. Le cœur (prix payé décomposé, ADR-0023) suffit déjà à l'objectif. **Si la présentation neutre ne peut être garantie, le défaut correct est de ne pas livrer cette couche.** Aucune pression d'usage ne justifie d'assouplir un seul des garde-fous ci-dessous.

### 1 — Jamais un chiffre unique : toujours une fourchette

On n'expose **aucun** LCOE ponctuel : toujours une **fourchette** (min / médiane / max) qui affiche la **dispersion**. La dispersion **est** l'information.

> **État livré (mis à jour 2026-06-20, re-jeu n°3 du GATE) :** la couche est désormais **multi-sources** sur 6 des 7 filières — **ADEME + IRENA** (LCOE mondiaux) pour les renouvelables, **Cour des comptes + CRE** pour le nucléaire existant ; la fourchette mêle alors dispersion **intra-source ET inter-sources**. Le **nucléaire nouveau reste mono-source (RTE)** : aucune 2e source primaire licence-compatible (IPCC/NEA/IEA écartés pour clause NC — asymétrie de *disponibilité de licence*, content-blind, pas idéologique : IRENA, la source la plus pro-EnR, est *incluse*). Transparence : chaque entrée expose `geography` (`france`/`monde`) et `technology_source_count`, pour que l'asymétrie de couverture soit lisible par machine (et qu'un plancher mondial IRENA ne se lise pas comme un coût français).
>
> **Contre-source FRANÇAISE pour les renouvelables — recherchée puis ÉCARTÉE (2026-06-20, re-jeu n°4 du GATE → ROUGE).** On a recherché et vérifié une 2e source française pour les renouvelables (Cour des comptes « Le soutien aux EnR via les CSPE » mars 2026 + prix d'appels d'offres CRE), licence-compatible (CRPA, sans clause NC). Mais l'ajout a fait **échouer le GATE sur deux blocs** : (1) **commensurabilité** — co-lister la grande hydro amortie (ADEME 15) et la petite hydro sous soutien (Cour 125) sous une seule filière reproduit la non-commensurabilité que l'ADR a justement corrigée pour le nucléaire en *scindant* la techno ; (2) **test aveugle** — enrichir les seuls renouvelables porte leur `technology_source_count` à 3 vs 1-2 pour le nucléaire, rendant la famille devinable à la seule structure (le neuf mono-source, le plus cher, devient un « phare »). Or le rééquilibrage (ajouter des sources au nucléaire) est **bloqué par les licences NC** (IPCC/NEA/IEA). Conformément au **Principe 0** (« si la neutralité ne peut être garantie, le défaut correct est de ne pas livrer »), le changement a été **annulé** ; l'état VERT (re-jeu n°3, v0.3.2) est conservé. L'asymétrie géographique FR/monde des renouvelables est donc **assumée** (déjà jugée *content-blind* au re-jeu n°3). Détail dans la revue de neutralité, addendum re-jeu n°4. **Cible de gouvernance restante (subordonnée à une 2e source nucléaire licence-compatible)** : ce n'est qu'une fois le nucléaire multi-sourçable que l'enrichissement des renouvelables redeviendrait neutre.

### 2 — Méthode et périmètre comme dimensions de première classe

Sur le modèle de la dimension méthodologie déjà en place (`acv-ademe`, `rte-direct`), chaque estimation est clé par **`source × méthodologie × périmètre × millésime`** et porte explicitement :

- le **périmètre** (plateau seul / coûts système / externalités incluses ou non / démantèlement / déchets) ;
- pour le nucléaire, la **catégorie de parc obligatoire** : *existant amorti* (source : Cour des comptes) vs *nouveau / construction* (source : RTE, Futurs énergétiques 2050) — **jamais fusionnés** ;
- les **hypothèses clés** disponibles (taux d'actualisation, durée de vie, facteur de charge) ;
- le **millésime** de l'estimation (stratégie de version, cf. ADR-0006).

### 3 — Aucune soustraction, aucun « écart », aucun verdict

LCOE et prix de marché sont présentés comme **grandeurs distinctes de nature différente**, **jamais mis en différence**, jamais agrégés en un « écart ». carbon-fr ne calcule ni n'affiche de gap, de « surcoût », de « prix juste ».

Une **note explicative éditorialement neutre** accompagne obligatoirement la couche : elle rappelle que le LCOE mesure un *coût moyen de production sur la durée de vie sous hypothèses*, que le prix de marché est un *prix marginal de compensation horaire*, et que **les deux ne sont pas censés être égaux** dans un marché à tarification marginale. Objectif : neutraliser la lecture naïve « scandale » *par l'explication du mécanisme*, sans désigner de responsable ni de camp.

### 4 — Statut « estimation » systématique et séparé du « mesure »

Toute valeur de cette couche est étiquetée **« estimation »**, avec provenance et millésime, et **n'est jamais présentée au même niveau de statut** que la donnée live (mesure). Le vocabulaire d'interface distingue explicitement *estimé* de *mesuré*.

### 5 — Pluralité et provenance des sources

Sélection **multi-sources par défaut**, en privilégiant la **diversité méthodologique** et la **traçabilité de provenance**. **Aucune source n'est privilégiée par défaut ;** l'équilibre méthodologique prime sur la commodité.

**Sources retenues (vetting licences 2026-06-20) — multi-sources par filière :**

| Source | Périmètre couvert | Fondement de réutilisation (recherche licences 2026-06-20) |
|---|---|---|
| **ADEME** (*Coûts des EnR&R en France*) | Renouvelables (PV, éolien, hydro, biomasse), **France** | Jeu de données **Licence Ouverte / Etalab 2.0** — réutilisation commerciale **explicitement permise** avec attribution. *Confiance haute.* |
| **IRENA** (*Renewable Power Generation Costs in 2024*) | Renouvelables, **mondial** (2e source EnR) | Licence IRENA **permissive maison** (« may be freely used … with acknowledgement », **sans clause NC**, pas du Creative Commons) — réutilisation y compris commerciale. *Confiance haute.* LCOE mondiaux (souvent < France : dispersion réelle). |
| **Cour des comptes** (coûts nucléaire) | Nucléaire **parc existant** (coût courant économique) | Pas de licence ouverte nommée sur `ccomptes.fr` ; conditions du site **sans clause NC** + **CRPA art. L321-1**. *Confiance moyenne.* |
| **CRE** (coûts du nucléaire existant) | Nucléaire **existant** (coût complet, 2e source) | Autorité administrative ; **CRPA art. L321-1** (réutilisation des informations publiques, sans clause NC). *Confiance haute.* |
| **RTE** (*Futurs énergétiques 2050*) | **Nouveau** nucléaire + prospectif (mono-source) | ⚠️ Mentions légales du **rapport restrictives** ; la valeur EPR2 vient du **rapport**, **pas** d'un jeu sous Licence Ouverte. Réutilisation des **chiffres-faits** fondée sur **non-protection des faits** (CPI L112-1) + **extraction non substantielle** (CPI L341-1/L342-3). *Confiance moyenne, risque résiduel réel.* |

> **Multi-sources atteint** sur le nucléaire existant (CdC + CRE) et les 5 renouvelables (ADEME + IRENA). Le **nucléaire nouveau reste mono-source (RTE)** : aucune 2e source primaire licence-compatible (IPCC/NEA/IEA écartés pour clause NC). Asymétrie **content-blind** (cf. revue de neutralité, addendum re-jeu n°3). Sources toujours écartées pour licence : **AIE/IEA**, **GIEC/IPCC AR6**, **Fraunhofer ISE** (CC BY-NC / clause NC), **NEA/OCDE** (restrictive), **Lazard** (propriétaire).

**Critère d'inclusion / exclusion — uniforme et indépendant du résultat (confirmé par la recherche licences du 2026-06-20).** On ne réutilise que des **chiffres-faits** (non protégés par le droit d'auteur, CPI L112-1), **ré-encodés** dans une structure propre — jamais tableaux/figures/texte — et en **petit nombre** par filière (≠ extraction substantielle, CPI L341-1). Sur ce socle, la réutilisation, y compris commerciale, est défendable pour les cinq sources retenues (ADEME, IRENA, Cour des comptes, CRE, RTE). Sont **écartées** les sources dont la **licence interdit** le commercial (AIE, CC BY-NC) ou entièrement propriétaires (Lazard) — motif *licence*, identique pour toutes, indépendant du résultat. **Ce n'est pas un avis juridique** : recherche best-effort ; le détail et les risques résiduels par source sont consignés dans la revue de neutralité (`0024-revue-neutralite.md` §licences).

**Sources écartées (motif : interdiction de licence, pas le résultat) :** AIE — licence **CC BY-NC** (non commercial) qui **interdit explicitement** la réutilisation commerciale du jeu, incompatible avec les paliers payants ; Lazard — rapport entièrement propriétaire, aucune licence de réutilisation. L'exclusion ne tient **pas** à leurs chiffres ni à leur géographie : c'est l'interdiction de licence, appliquée comme pour toute source.

**Sources différées (ni retenues, ni écartées) :** GIEC et Fraunhofer ISE — utiles comme contexte ; licences à vérifier si un jour intégrées.

> **Souveraineté = préférence de contexte, jamais critère disqualifiant.** La nature française/publique des sources retenues est un *bonus de contexte* (outil France-first), **pas** la raison de l'exclusion d'AIE/Lazard — laquelle repose uniquement sur la licence. On ne fusionne pas les deux critères pour consolider une exclusion (correctif de la revue 2026-06-20).
>
> ⚠️ **Limite assumée :** depuis 2026-06-20, 6 des 7 filières sont **multi-sources** (la fourchette mêle dispersion intra-source ET inter-sources) ; seul le **nucléaire nouveau reste mono-source** (RTE), sa fourchette étant la dispersion *publiée par la source*, **pas** un désaccord inter-sources. Le disclaimer le dit explicitement ; une 2e source licence-compatible pour le neuf et une contre-source France pour les renouvelables restent des objectifs de gouvernance.

### 6 — Forme d'exposition : ressource de référence découplée

Exposition via une **ressource de référence dédiée** (p. ex. `/cost-reference`), **statique et versionnée**, **physiquement découplée de `/price`** (ADR-0023) et de `/mix`. La séparation matérielle renforce la distinction de statut estimation/mesure et empêche toute fusion accidentelle en une comparaison live.

---

## Conséquences

### Positives
- Répond à la demande pédagogique sans déroger à la posture d'instrument neutre.
- Restitue honnêtement l'**incertitude** (fourchette) là où les autres acteurs assènent un chiffre orienté — différenciant et crédible.
- Rend lisible la distinction LCOE / coût marginal / prix payé, souvent confondue.

### Coûts / charges
- **Charge de gouvernance élevée et continue** : veille, re-vérification et re-millésimage des estimations ; vetting des sources.
- Harmonisation d'unités (LCOE souvent en €/MWh ; décomposition prix en €/kWh).
- Surface de référence supplémentaire à spécifier, documenter et couvrir (SDK / `/docs`).

### Risques / points ouverts (actions avant implémentation)
1. **Licences — confirmé (recherche 2026-06-20, sources primaires).** ADEME = **Licence Ouverte / Etalab 2.0** (commercial permis, confiance haute). Cour des comptes = pas de licence ouverte nommée mais **CRPA art. L321-1** + conditions de site **sans clause NC** (confiance moyenne ; vérifier au cas par cas qu'un chiffre repris n'est pas crédité à un **tiers** dans le rapport). RTE = mentions légales du **rapport restrictives** → réutilisation des chiffres fondée sur la **non-protection des faits** (CPI L112-1) + **extraction non substantielle** (CPI L341-1/L342-3), **pas** sur une Licence Ouverte du rapport (confiance moyenne, **risque résiduel réel**). **Conditions impératives :** ne ré-encoder que des **valeurs** (jamais tableaux/figures/texte), peu de valeurs par filière, attribution nominative + millésime, **lien externe** vers le rapport plutôt que reproduction. **Pour un palier payant s'appuyant sur la donnée RTE :** demande de **confirmation écrite à RTE** recommandée (prévue par leurs mentions légales) — peu coûteux, lève l'incertitude. ⚠️ Recherche best-effort, **pas un avis juridique**. AIE (CC BY-NC) et Lazard restent écartés pour licence.
2. **Critère d'acceptation de neutralité (GATE bloquant) :** opérationnalisé en checklist vérifiable — voir section **« GATE de neutralité »** ci-dessous.
3. **Définition de l'agrégat de dispersion** (min/médiane/max vs enveloppe), à figer.
4. **Harmonisation d'unités :** sources en €/MWh, décomposition prix (ADR-0023) en €/kWh — conversion à acter dans le modèle.

---

## Alternatives considérées

- **A — Source unique « de référence ».** Rejetée : choisir la source *est* le parti pris ; masque l'incertitude réelle.
- **B — Comparaison calculée (« écart », « surcoût »).** Rejetée : produit un verdict ; compare des grandeurs de nature différente.
- **C — Fusion dans `/price` live.** Rejetée : confère le statut de mesure à une estimation ; rend la comparaison implicite inévitable.
- **D — Ne pas livrer cette couche.** **Conservée comme défaut légitime.** Le cœur de mission (ADR-0023) n'en dépend pas ; en cas de doute sur la neutralité, c'est l'option correcte.

---

## GATE de neutralité (critère d'acceptation opérationnel)

Le principe « critiquable par les deux camps » est transformé ici en **procédure pass/fail vérifiable**. La couche ne démarre pas en implémentation, et ne passe pas en production, tant que **tous** les blocs ne sont pas au vert. Échec d'un seul item → correction, ou repli sur la non-livraison (Alternative D).

> **Piège prioritaire — la symétrie de périmètre.** C'est le vecteur de biais le plus courant des comparatifs LCOE : inclure une dimension de coût (externalités, démantèlement, coûts système, back-up de l'intermittence) pour une filière et pas pour les autres penche, même avec des sources impeccables et zéro mot de jugement. **Le même jeu de dimensions de périmètre est exposé pour toutes les filières, ou pour aucune.**

**Bloc 1 — Symétrie (structurel)**
- [ ] Nucléaire *existant* **et** *nouveau* tous deux présents : même proéminence, même niveau de source, mêmes réserves.
- [ ] Dimensions de périmètre identiques pour toutes les filières (externalités, coûts système, démantèlement, intermittence/back-up) — jamais à géométrie variable.
- [ ] Dispersion affichée pour chaque filière, jamais un point unique.

**Bloc 2 — Non-verdict**
- [ ] Aucune différence / écart calculé entre LCOE et prix de marché.
- [ ] Aucun tri ni classement par défaut suggérant un gagnant.
- [ ] Lexique évaluatif banni, y compris « compétitif », « bon marché », « cher », « vrai prix ».

**Bloc 3 — Provenance**
- [ ] Inclusion/exclusion de chaque source justifiée par une raison *non liée au résultat* (licence, géographie, méthode), documentée. Aucune source écartée parce que ses chiffres dérangent.

**Bloc 4 — Tests qualitatifs (jugement)**
- [ ] *Test adverse :* rédiger la critique « carbon-fr penche pro-nucléaire » la plus forte **et** la critique « penche anti-nucléaire » la plus forte qu'un lecteur pourrait tirer de la sortie réelle. **Passage :** les deux ne se répondent *que* par « on montre la fourchette complète et le périmètre, on ne conclut pas ». Si l'une mord sur un *choix de conception* (défaut, omission, formulation) → FAIL.
- [ ] *Test aveugle :* libellés et attributions retirés, un lecteur neutre ne peut pas deviner de quel côté l'outil penche.

**Modalité :** auto-évaluation documentée par défaut ; relecteur externe de chaque bord en renfort différé (optionnel, renforçant).

**Enregistrement :** franchir le GATE produit une **revue de neutralité datée et signée** (annexe de cet ADR ou document lié), **re-jouée à chaque modification** de sources, de lexique ou d'agrégation. Tant qu'elle n'est pas intégralement au vert : non-livraison.

---

## Suite

Sous réserve de ratification et de la levée des points ouverts (licences + GATE de neutralité), préparation d'un brief d'implémentation Claude Code : modèle `source × méthodologie × périmètre × millésime`, schéma `/cost-reference`, agrégat de dispersion, note explicative neutre, étiquetage estimation. **Tant que le GATE de neutralité n'est pas intégralement au vert, l'implémentation ne démarre pas.**

---

## Addendum (2026-09-25) — Cadence de revue, prochaine échéance et versionnement des mises à jour

**Objet.** Cet ADR (§2, §5) ancre chaque estimation à un `source × technologie × périmètre × millésime` mais ne fixait **aucune cadence** de revue de ces millésimes — un point resté ouvert, listé dans [`docs/plan-iterations.md` §I7](../plan-iterations.md). Itération **décisionnelle** (aucune ligne de code) : cet addendum fixe la cadence, la prochaine date et la règle de versionnement. Il ne touche ni le GATE ni son verdict (`0024-revue-neutralite.md`, non modifié).

### État constaté — millésimes actuellement servis (`crates/core/src/domain/cost.rs`, lu le 2026-09-25)

| Source | Technologie(s) | Millésime servi | Ligne(s) |
|---|---|---|---|
| Cour des comptes | Nucléaire existant | **2021** | `cost.rs:411` |
| RTE | Nucléaire nouveau | **2021** | `cost.rs:427` |
| ADEME | Solaire PV, éolien terrestre, éolien mer, hydraulique, biomasse | **2024** | `cost.rs:443,458,473,488,503` |
| CRE | Nucléaire existant (2ᵉ source) | **2023** | `cost.rs:528` |
| IRENA | Les 5 renouvelables (2ᵉ source) | **2024** | `cost.rs:539,550,561,575,586` |

### Recherche — prochaines éditions (consultée le 2026-09-25)

| Source | Cadence observée | Dernière édition **déjà publiée** | Prochaine édition |
|---|---|---|---|
| **IRENA** — *Renewable Power Generation Costs* | **Annuelle, chaque juillet** (édition « in 2024 » parue juillet 2025 ; c'est celle actuellement ré-encodée) | ⚠️ **« Renewable Power Generation Costs in 2025 »**, parue **juillet 2026** — [PDF IRENA](https://www.irena.org/-/media/Files/IRENA/Agency/Publication/2026/Jul/IRENA_TEC_RPGC_2025_Executive_summary_2026.pdf), pas encore ré-encodée | « in 2026 », attendue juillet 2027 |
| **CRE** — coût complet du nucléaire existant | **Légale depuis la LF 2025** (fin de l'ARENH au 31/12/2025) : publication **« au moins tous les trois ans »** | ⚠️ Évaluation **2026-2028**, publiée **30/09/2025**, coût retenu 60,3 €₂₀₂₆/MWh (≈61,5 €courants/MWh) — [communiqué CRE](https://www.cre.fr/actualites/toute-lactualite/la-commission-de-regulation-de-lenergie-publie-son-evaluation-des-couts-complets-de-production-de-lelectricite-au-moyen-des-centrales-electronucleaires-historiques-pour-la-periode-2026-2028.html), pas encore ré-encodée | Prochaine échéance légale : au plus tard 2028 |
| **RTE** — *Futurs énergétiques 2050* | Pas annuelle — grande réactualisation ponctuelle (~5 ans après l'édition 2021) | Travaux de réactualisation lancés début 2025 ; consultation publique 03/04–15/05/2026 ([synthèse RTE](https://assets.rte-france.com/prod/public/2026-04/RTE-Reactualisation-FE-2050-consultation-publique-2026-synthese.pdf)) | Résultats attendus **fin 2026** (date précise non publiée) |
| **ADEME** — *Coûts des EnR&R en France* | ~Tous les 2-3 ans depuis 2016 (éditions ≈2016, ≈2019, 2022, 2025) | 4ᵉ édition (« Évolution … entre 2012 et 2022 »), publiée **30/01/2025** — [CIBE, copie du rapport ADEME](https://cibe.fr/documents/2025-01-30-ademe-evolution-cout-energies-renouvelables-et-recuperation-entre-2012-2022-rapport-final/) | ~2027 (aucune date officielle annoncée) |
| **Cour des comptes** — coûts du nucléaire | Irrégulière, aucune cadence légale identifiée (éditions 2012, 2014, 2018, **2021**) | Dernière édition = celle déjà servie (**« L'analyse des coûts du système électrique en France »**, publiée **13/12/2021** — [PDF Cour des comptes](https://www.ccomptes.fr/sites/default/files/2022-01/20211213-S2021-2052-analyse-couts-systeme-production-electrique-France-rep-MTE.pdf)) ; aucune édition plus récente trouvée au 2026-09-25 | Aucune date annoncée |

**Constat central : le déclencheur « nouvelle édition » est déjà actif pour 2 des 5 sources** (IRENA, CRE) sans qu'aucune revue n'ait encore eu lieu pour les traiter.

### Décision 1 — Cadence de revue : double déclenchement

1. **Calendaire** : une revue au moins une fois par an.
2. **Événementiel, prioritaire sur le calendaire** : dès qu'une des 5 sources retenues (§5) publie une nouvelle édition, la revue est **due immédiatement** — sans attendre l'échéance annuelle suivante. C'est le cas ci-dessus décrit.

### Décision 2 — Prochaine revue : **2026-12-15**

Choisie pour traiter en une seule passe les deux déclencheurs événementiels déjà actifs (IRENA « in 2025 », CRE 2026-2028) et pour tomber après la fenêtre où RTE annonce ses résultats (« fin 2026 ») — si RTE publie à temps, sa mise à jour est incluse dans la même revue ; sinon, elle déclenche sa **propre** revue dès sa parution (Décision 1, point 2), indépendamment de cette date. ADEME et Cour des comptes n'ont pas de nouvelle édition connue à ce jour : pas de déclencheur événementiel pour elles, elles sont simplement vérifiées à cette même échéance. **La revue suivante après celle-ci a lieu 12 mois après la date où elle est effectivement menée** (cadence glissante, pas une date calendaire fixe d'une année sur l'autre), sauf déclenchement événementiel anticipé par une nouvelle édition d'une source.

### Décision 3 — Règle de versionnement d'une mise à jour

- Une mise à jour de source **ajoute une nouvelle entrée** `CostReferenceKey` avec un **nouveau `vintage`** ; elle ne mute **jamais** en place les chiffres d'une entrée déjà servie — même logique que le reste de la donnée carbon-fr (millésime porté par la donnée, ADR-0006), appliquée ici à un catalogue constant plutôt qu'à une table Postgres.
- **Jamais de mutation silencieuse** : toute mise à jour de valeur passe par une PR dédiée qui cite la nouvelle source (URL + date de consultation), le nouveau millésime, et référence le nouveau re-jeu du GATE (point suivant). Un bump de millésime **est** une « modification de source » au sens de la règle déjà posée par cet ADR (§ GATE, « Enregistrement ») et par `0024-revue-neutralite.md` — cette règle n'était pas ambiguë, cet addendum la rappelle explicitement pour le cas récurrent « cadence », pas seulement pour un changement structurel (ajout/retrait de source).
- **Re-jeu du GATE de neutralité obligatoire** (`0024-revue-neutralite.md`, non modifié par cet addendum) dès que la présentation change : nouvelles valeurs, nouveau millésime, nouvelle source, formulation du disclaimer ou de l'agrégation. Le re-jeu est numéroté à la suite des re-jeux n°3/n°4 existants (`0024-revue-neutralite.md`). **Chaque nouvelle édition d'une source doit aussi revérifier sa licence** (une nouvelle édition peut changer ses conditions de réutilisation par rapport au vetting du 2026-06-20, §5/§licences) avant tout ré-encodage.
- Une entrée dont la source cesse de publier une nouvelle édition **n'est pas retirée** du catalogue tant qu'elle reste la meilleure donnée disponible ; elle est seulement signalée comme datée si son millésime dépasse significativement les autres (`COST_REFERENCE_DISCLAIMER`, déjà chargé de signaler l'hétérogénéité des millésimes).

---

> ### Points à confirmer par Morgan
>
> 1. **Date du 2026-12-15** proposée pour la prochaine revue : elle tombe pendant l'itération I7/I8 (sqlx 0.9, autres addenda) — à recaler si la charge réelle ne le permet pas. Recommandation : la garder, car elle rattrape déjà 2 sources en retard (IRENA, CRE) plutôt que de laisser le retard grossir.
> 2. **Ancrage « cadence glissante » (12 mois après la dernière revue effective)** plutôt qu'une date calendaire fixe (ex. 20 juin, anniversaire du GATE) : recommandation = garder le glissant, plus robuste si une revue est décalée, mais c'est un choix de gouvernance discutable.
> 3. **Portée du re-jeu à chaque bump de millésime** : cet addendum exige le GATE complet (4 blocs, tests adverses des deux bords) à **toute** mise à jour de valeur, y compris un simple bump sans changement de structure — cohérent avec la règle déjà écrite dans l'ADR, mais c'est une charge de gouvernance réelle (déjà notée en « Coûts / charges » de cet ADR). Une alternative plus légère (rejouer seulement Blocs 1+3 si la structure et la licence de la source n'ont pas changé) a été **écartée** ici par prudence — à trancher explicitement si la charge s'avère trop lourde en pratique.
> 4. **Écart de millésime ADEME** (catalogue : `vintage: 2024` ; édition la plus proche identifiée par cette recherche publiée le 30/01/2025) : peut être un simple effet du délai de publication habituel « fin d'année → janvier suivant » (comme pour le TRV, ADR-0023 addendum 2026-09-23) et non une erreur — **hors périmètre décisionnel de cet addendum** (aucun code touché ici), remonté pour que la revue du 2026-12-15 le vérifie sur pièce.
>
> Recherche best-effort (WebSearch/WebFetch, sources primaires citées ci-dessus), **pas un avis juridique ni une garantie d'exhaustivité** — dans le même esprit que le vetting licences du 2026-06-20 (§5).
