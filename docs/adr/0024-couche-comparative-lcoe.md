# ADR-0024 — Couche comparative LCOE (coût de production) : cadre de neutralité

- **Statut :** Accepté (ratifié le 2026-06-20) — **GATE de neutralité franchi le 2026-06-20** (revue datée : [`0024-revue-neutralite.md`](0024-revue-neutralite.md), évaluation adversariale pro/anti-nucléaire + audits structurels). *Publication ferme* conditionnée à deux pré-conditions de gouvernance (confirmation des licences formelles CdC/RTE ; multi-source par filière) — voir la revue §5. Couche **servie** avec mention « valeurs et licences à confirmer ».
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

> **État livré (revue 2026-06-20) :** chaque filière est aujourd'hui **mono-source** ; la fourchette restitue donc la **dispersion publiée par la source citée**, et **non** un éventail inter-sources. Le disclaimer servi le dit explicitement et ne revendique plus de « dispersion entre experts ». La **cible** reste le multi-sources par filière (au moins une contre-source pour le nucléaire neuf et pour le PV/éolien) : c'est un objectif de gouvernance, pas une propriété déjà tenue. Tant qu'on est mono-source, le garde-fou de neutralité repose sur la **dispersion publiée affichée honnêtement** + le **critère d'inclusion uniforme** (licence), pas sur une pluralité revendiquée à tort.

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

**Sources retenues — triptyque public français complémentaire (vetting du 2026-06-20) :**

| Source | Périmètre couvert | Licence / réutilisation |
|---|---|---|
| **ADEME** (*Coûts des EnR&R en France*) | Renouvelables (PV, éolien, hydro, biomasse, géothermie) | Jeu de données **Licence Ouverte / Etalab** — réutilisation commerciale permise avec attribution, compatible CC-BY |
| **Cour des comptes** (coûts nucléaire / système électrique) | Nucléaire **parc existant** (coût courant économique) | Document public (CRPA) — chiffres réutilisables ; licence formelle du rapport à confirmer |
| **RTE** (*Futurs énergétiques 2050*) | **Nouveau** nucléaire + prospectif toutes filières | Données RTE largement Licence Ouverte (déjà intégrées via ODRÉ) ; termes du rapport à confirmer |

**Critère d'inclusion / exclusion — uniforme et indépendant du résultat (précisé après la revue de neutralité du 2026-06-20).** Le critère **unique et disqualifiant** est la **licence** : on ré-encode des *chiffres-faits* publiés tant que la licence **n'interdit pas explicitement** la réutilisation commerciale. ADEME = Licence Ouverte (jeu réutilisable). Cour des comptes / RTE = rapports d'institutions publiques, sans clause d'interdiction (chiffres réutilisables comme faits ; licence formelle des rapports à confirmer avant publication ferme). Le même raisonnement « chiffres-faits ré-encodables » est appliqué de façon **identique** à toutes les sources : ne sont écartées que celles dont la licence **interdit** la réutilisation.

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
1. **Licences — vetting réalisé (2026-06-20) :** triptyque ADEME / Cour des comptes / RTE retenu (cf. Décision §5) ; AIE et Lazard écartés pour incompatibilité de licence. **Précaution résiduelle :** pour la CdC et RTE (rapports PDF), n'ingérer que les *chiffres* (faits réutilisables) en les ré-encodant dans une structure propre avec attribution — **jamais** reproduire tableaux/graphiques tels quels. La licence formelle de ces deux rapports reste à confirmer avant mise en production (analogue à la vérification Open-Meteo).
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
