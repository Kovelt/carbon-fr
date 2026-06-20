# ADR-0024 — Couche comparative LCOE (coût de production) : cadre de neutralité

- **Statut :** Accepté (ratifié le 2026-06-20) — **GATE de neutralité franchi le 2026-06-20** (revue datée : [`0024-revue-neutralite.md`](0024-revue-neutralite.md), évaluation adversariale pro/anti-nucléaire + audits structurels). **Licences confirmées (recherche 2026-06-20) : pré-condition levée sous conditions** — réutilisation des chiffres-faits fondée sur Licence Ouverte (ADEME), CRPA + absence de clause NC (Cour des comptes), et non-protection des faits + extraction non substantielle (RTE, dont les mentions légales du rapport sont restrictives) ; voir §5 et la revue §licences. *Conditions* : ne ré-encoder que des valeurs (jamais tableaux/figures), attribution nominative, et — pour un **palier payant** s'appuyant sur la donnée **RTE** — demande de confirmation écrite à RTE recommandée. Reste ouvert (gouvernance, non bloquant) : le multi-source par filière.
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

**Critère d'inclusion / exclusion — uniforme et indépendant du résultat (confirmé par la recherche licences du 2026-06-20).** On ne réutilise que des **chiffres-faits** (non protégés par le droit d'auteur, CPI L112-1), **ré-encodés** dans une structure propre — jamais tableaux/figures/texte — et en **petit nombre** par filière (≠ extraction substantielle, CPI L341-1). Sur ce socle, la réutilisation, y compris commerciale, est défendable pour les trois sources retenues. Sont **écartées** les sources dont la **licence interdit** le commercial (AIE, CC BY-NC) ou entièrement propriétaires (Lazard) — motif *licence*, identique pour toutes, indépendant du résultat. **Ce n'est pas un avis juridique** : recherche best-effort ; le détail et les risques résiduels par source sont consignés dans la revue de neutralité (`0024-revue-neutralite.md` §licences).

**Sources écartées (motif : interdiction de licence, pas le résultat) :** AIE — licence **CC BY-NC** (non commercial) qui **interdit explicitement** la réutilisation commerciale du jeu, incompatible avec les paliers payants ; Lazard — rapport entièrement propriétaire, aucune licence de réutilisation. L'exclusion ne tient **pas** à leurs chiffres ni à leur géographie : c'est l'interdiction de licence, appliquée comme pour toute source.

**Sources différées (ni retenues, ni écartées) :** GIEC et Fraunhofer ISE — utiles comme contexte ; licences à vérifier si un jour intégrées.

> **Souveraineté = préférence de contexte, jamais critère disqualifiant.** La nature française/publique des sources retenues est un *bonus de contexte* (outil France-first), **pas** la raison de l'exclusion d'AIE/Lazard — laquelle repose uniquement sur la licence. On ne fusionne pas les deux critères pour consolider une exclusion (correctif de la revue 2026-06-20).
>
> ⚠️ **Limite assumée :** chaque filière est aujourd'hui **mono-source** ; la fourchette affichée est la dispersion *publiée par la source*, **pas** un désaccord inter-sources. Le disclaimer le dit explicitement ; le multi-sources par filière reste un objectif de gouvernance (et non une propriété déjà tenue).

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
