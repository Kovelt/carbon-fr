# Revue de neutralité — Couche comparative LCOE (ADR-0024)

- **Date :** 2026-06-20
- **Objet :** franchissement du *GATE de neutralité* de l'ADR-0024 (`GET /v1/cost-reference`)
- **Évalué :** la **sortie réellement servie** (`crates/core/src/domain/cost.rs`, `crates/adapter-http/src/dto.rs`), pas la seule spécification
- **Méthode :** évaluation adversariale multi-agents — un critique militant de **chaque bord** (pro- et anti-nucléaire) + des auditeurs structurels par bloc du GATE, rejoués sur la sortie réelle. Deux passages (RED → correctifs → re-test).
- **Statut :** **neutralité confirmée (GREEN sur tous les blocs de neutralité)** ; *publication ferme* conditionnée à deux pré-conditions de gouvernance (licences, multi-source) — voir §5.

> Cette revue est l'artefact exigé par le GATE de l'ADR-0024 (« franchir le GATE produit une revue de neutralité datée et signée, re-jouée à chaque modification »). Elle est **re-jouable** : toute modification des sources, du lexique, de l'agrégation ou du périmètre impose de la rejouer.

---

## 1. Déroulé

**Passage 1 (RED).** Les deux tests adverses (pro/anti-nucléaire) passaient déjà sans mordre sur la conception — la *mécanique* de l'endpoint était saine. Mais trois auditeurs structurels ont relevé des défauts réels, **dans la table de référence et sa documentation**, pas dans l'API :

- **Bloc 3 (Provenance) — FAIL :** le disclaimer revendiquait une « dispersion réelle entre experts » alors que chaque filière est **mono-source** (la fourchette est interne à une étude) ; le critère d'exclusion (AIE/Lazard) mêlait licence et souveraineté de façon non uniforme ; la revue datée n'existait pas.
- **Asymétrie de périmètre (Blocs 1/2) :** le périmètre « plateau » était asserté uniforme mais **non documenté** — il n'explicitait pas qu'il exclut, des deux côtés, les coûts système (back-up, réseau, stockage) **et** le démantèlement / les déchets.
- **Fausse commensurabilité (Bloc 1) :** un **coût comptable amorti** (nucléaire existant) et un **LCOE prospectif** (moyens neufs) étaient rangés sous un libellé identique.

**Correctifs (P1–P7, 2026-06-20).** Voir §4.

**Passage 2 (re-test sur la sortie corrigée).** Les six sous-blocs ont été rejoués. Résultat : §2.

---

## 2. Tableau pass/fail par bloc (état après correctifs)

| Bloc | Verdict | Mord sur la conception |
|---|---|---|
| Bloc 1 — Symétrie | **PASS** | non |
| Bloc 2 — Non-verdict | **PASS** | non |
| Bloc 3 — Provenance | **PASS** (après création de la présente revue) | — |
| Bloc 4 — Test adverse (pro-nucléaire) | **PASS** | non |
| Bloc 4 — Test adverse (anti-nucléaire) | **PASS** | non |
| Bloc 4 — Test aveugle | **PASS** | non |

Au passage 2, le seul item encore au rouge était le **point (c) du Bloc 3 — l'existence même de cette revue datée**, citée par l'ADR et par `cost.rs` alors qu'elle n'était pas encore écrite. **Le présent document lève ce point.** Les deux incohérences de texte résiduelles (un commentaire `cost.rs` et le §1 de l'ADR revendiquant encore « multi-sources / entre experts ») ont été corrigées dans le même temps.

---

## 3. Les deux charges les plus fortes (test adverse) et leur réfutation

C'est le cœur du critère « critiquable par les deux camps » : le passage exige que les deux charges ne se répondent **que** par « on montre la fourchette complète et le périmètre, on ne conclut pas ».

### Charge anti-nucléaire (la plus mordante)

« carbon-fr blanchit le nucléaire : la **seule** entrée à base avantageuse (`accounting-amortized`, "parc existant amorti", médiane 60 €/MWh) est le nucléaire ; toutes les EnR sont servies en `prospective-lcoe` "neuf cher". Le périmètre `plateau` **exclut** démantèlement et déchets de long terme — exactement le poste qui plombe le vrai coût nucléaire. Structure, base et périmètre convergent : plaidoyer pro-nucléaire déguisé en JSON. »

**Réfutation.** (1) Le nucléaire **neuf** (`nucleaire-nouveau`, RTE, 100/120/150) est présent à même proéminence et porte la **médiane pilotable la plus élevée** du catalogue : la version chère est donnée. (2) La médiane du nucléaire existant (60) = celle du PV ; la médiane pilotable la plus basse est en réalité l'**hydraulique (50)** — aucun avantage chiffré ne désigne le nucléaire. (3) Le champ `basis` **étiquette** la non-commensurabilité au lieu de la cacher : l'outil dit lui-même que ce chiffre n'est pas un LCOE comparable. (4) L'exclusion démantèlement/déchets est **nommée et symétrique** ; l'inclure pour le seul nucléaire **violerait** le piège prioritaire du GATE (symétrie de périmètre) et ferait basculer l'outil dans l'anti-nucléaire. La charge attaque la position neutre elle-même.

### Charge pro-nucléaire (symétrique, la plus mordante)

« Le périmètre `plateau` exclut les coûts système (back-up, réseau, stockage) — précisément le surcoût d'intégration de l'intermittence (load factor PV 0,14, éolien 0,25) que la sortie affiche elle-même. Il retire au pilotable son seul avantage structurel et flatte les variables. Et le nucléaire neuf est figé sur l'estimation RTE haute, sans contre-source basse. »

**Réfutation.** (1) Le classement implicite 120>110>65>60 existe dans les **nombres** mais n'est jamais matérialisé : `filtered()` ne réordonne ni n'agrège, aucun champ `gap`/`rank`/`cheapest`, et `range.max` du nucléaire neuf (150) **chevauche** l'éolien en mer (140) — la dispersion publiée interdit tout verdict ponctuel. (2) Le périmètre est **explicitement** signalé comme excluant back-up/réseau/stockage **ET** démantèlement/déchets « de part et d'autre », et le disclaimer dit « PAS directement comparable entre filières pilotables et variables » : l'asymétrie d'effet est nommée, pas masquée. (3) Le mono-source est assumé et frappe **symétriquement** (ADEME seule côté EnR).

**Symétrie tenue.** Les deux charges se réclament mutuellement l'inverse et s'effondrent toutes deux sur les mêmes garde-fous. **Aucune ne mord sur un défaut de conception corrigeable.** Le seul angle résiduel des deux bords — le mono-source par filière — est assumé, signalé et frappe les deux camps de la même façon ; il est traité en pré-condition de gouvernance (§5), pas en biais directionnel.

---

## 4. État des correctifs P1–P7

| # | Correctif | État |
|---|---|---|
| P1 | Disclaimer : retrait de la revendication « dispersion entre experts » → « dispersion **publiée par la source** » + limite mono-source assumée | **Levé** (servi) |
| P2 | Critère d'inclusion/exclusion **uniformisé** sur la **licence seule** ; souveraineté = préférence de contexte, jamais disqualifiant | **Levé** |
| P3 | Sémantique `null = non publié par la source` exposée dans le disclaimer | **Levé** |
| P4 | Périmètre `plateau` **explicité bilatéralement** (back-up/intermittence **et** démantèlement/déchets) + « non comparable pilotable/variable » | **Levé** (lève l'asymétrie nominale) |
| P5 | Champ `basis`/`basis_label` distinguant **coût comptable amorti** vs **LCOE prospectif** | **Levé** |
| P6 | Hétérogénéité des millésimes (nucléaire 2021, renouvelables 2024) signalée dans le disclaimer | **Levé** |
| P7 | Cohérence des textes non servis (commentaire `cost.rs`, §1 de l'ADR) avec la réalité mono-source | **Levé** |

---

## 5. Pré-conditions de gouvernance restantes (avant *publication ferme*)

Ces points ne sont **pas** des défauts de neutralité (la sortie est honnête sur chacun) ; ce sont des engagements de gouvernance que l'ADR-0024 portait déjà :

1. ~~**Licences formelles à confirmer**~~ **Confirmé (recherche licences 2026-06-20) — pré-condition levée sous conditions.** Voir §licences ci-dessous. ADEME = Licence Ouverte ; CdC = CRPA + absence de clause NC ; RTE = non-protection des faits + extraction non substantielle (licence du rapport restrictive). Conditions : valeurs uniquement (jamais tableaux/figures), attribution nominative, lien externe ; pour un palier payant sur la donnée **RTE**, confirmation écrite RTE recommandée.
2. **Valeurs best-effort à sourcer** — les fourchettes et millésimes sont des estimations best-effort à re-vérifier dans les rapports cités (charge de gouvernance continue).
3. **Multi-source par filière (cible)** — viser au moins une contre-source pour le nucléaire neuf **et** pour le PV/éolien, pour passer d'une dispersion intra-source à une dispersion inter-sources. Non bloquant pour la neutralité de la sortie actuelle (mono-source assumé et déclaré).

---

## §licences — Réutilisation des chiffres LCOE (recherche du 2026-06-20)

Recherche multi-agents avec vérification adverse contre sources primaires. **Best-effort, pas un avis juridique** ; la décision finale appartient à l'éditeur (Kovelt).

| Source | Réutilisation commerciale | Fondement | Confiance | Risque résiduel |
|---|---|---|---|---|
| **ADEME** | Oui | **Licence Ouverte / Etalab 2.0** (commercial explicitement permis + attribution) | Haute | Quasi nul (citer l'édition ; ne pas reproduire les graphiques) |
| **Cour des comptes** | Oui | Pas de licence ouverte nommée (`ccomptes.fr`) mais **conditions de site sans clause NC** + **CRPA art. L321-1** (réutilisation des informations publiques, y c. commerciale) + faits non protégés | Moyenne | Page silencieuse sur le commercial (base = droit général) ; vérifier qu'un chiffre repris n'est pas crédité à un **tiers** dans le rapport |
| **RTE** | Incertain → défendable | Mentions légales du **rapport restrictives** ; fondement réel = **faits non protégés** (CPI L112-1) + **extraction non substantielle** (CPI L341-1/L342-3), **pas** une Licence Ouverte du rapport | Moyenne | **Réel** : si RTE qualifie une valeur de « contenu » (pas « data »), la permission du site ne joue pas ; frontière fait/forme appréciée *in concreto* |

**Conditions de réutilisation (impératives, déjà tenues dans `cost.rs`)** : ré-encoder uniquement des **valeurs** dans la structure propre `CostEstimate` (jamais tableaux/figures/texte/structure) ; **petit nombre** de valeurs par filière (≠ extraction substantielle, CPI L342-2) ; **attribution nominative** + millésime + sens conservé ; **lien externe** vers le rapport plutôt que reproduction. **Pour un palier payant s'appuyant sur la donnée RTE** : demande de **confirmation écrite à RTE** recommandée (prévue par leurs mentions légales) — peu coûteux, lève l'incertitude résiduelle.

**Critère d'inclusion uniforme tenu** : ADEME (Licence Ouverte), CdC (sans clause NC), RTE (pas d'interdiction du *fait*, la restriction porte sur la forme) ; AIE (CC BY-NC) et Lazard restent écartés pour **interdiction de licence** — même critère, indépendant du résultat.

Le disclaimer servi continue de signaler les valeurs comme estimations best-effort à confirmer (millésimes).

---

## 6. Verdict

**Le GATE de neutralité est franchi.** Les six blocs passent ; surtout, **aucune des deux critiques de bord ne mord sur un choix de conception corrigeable** — le critère central « critiquable par les deux camps » est satisfait de façon symétrique. La mécanique est saine par construction : découplage physique de `/v1/price`, aucun champ d'écart, fourchette partout, aucun tri ni lexique évaluatif, base et périmètre explicités, provenance à critère uniforme.

Les **licences** ont été confirmées (recherche du 2026-06-20, §licences) : pré-condition **levée sous conditions** (valeurs uniquement, attribution, lien externe ; confirmation écrite RTE recommandée pour un palier payant). Restent des **pré-conditions de gouvernance** non bloquantes pour la neutralité (sourçage fin des millésimes, multi-source par filière, demande écrite RTE avant tier payant sur sa donnée).

**Re-jeu obligatoire** de cette revue à toute modification des sources, valeurs, lexique, périmètre ou agrégation.

— Revue conduite par Claude Code (évaluation adversariale multi-agents) pour Morgan (Kovelt), 2026-06-20.

---

## Addendum — Re-jeu n°3 du GATE (2026-06-20) : passage au multi-sources

**Objet :** re-jouer le GATE après l'ajout de **2e sources** au catalogue (`cost.rs`), conformément à l'exigence de re-jeu à toute modification des sources.

**Changement :** le catalogue passe de **mono-source** à **multi-sources** sur 6 des 7 filières — **IRENA** (LCOE mondiaux, USD 2024 → EUR à 0,9243) en 2e source des 5 renouvelables ; **CRE** (France, parc complet) en 2e source du nucléaire existant ; le **nucléaire nouveau reste mono-source (RTE)**. Effet : la fourchette mêle désormais dispersion intra-source ET inter-sources. Effet secondaire mesuré : IRENA (mondial) abaisse les planchers renouvelables (PV 45→30,5 ; éolien terrestre 50→26,8 ; éolien-mer 90→51,8).

| Bloc | Verdict |
|---|---|
| Bloc 1 — Symétrie | **PASS** |
| Bloc 2 — Non-verdict | **PASS** |
| Bloc 3 — Provenance | **PASS** |
| Bloc 4 — Test adverse (pro-nucléaire) | **PASS** |
| Bloc 4 — Test adverse (anti-nucléaire) | **PASS** |
| Bloc 4 — Test aveugle | **PASS** |

**6/6 au vert ; aucune critique de bord ne mord sur un choix de conception.**

**Critique pro-nucléaire (la plus forte)** : « vous avez ajouté une 2e source *moins chère* (IRENA mondial) à TOUS les renouvelables, mais laissé le nucléaire neuf mono-source à RTE 100/120/150 — l'étape de sourçage a mécaniquement élargi l'écart planchers neuf↔EnR (~2,2×→~3,7× vs PV). » **Factuellement exacte sur l'effet**, mais ne mord pas : (a) le mono-source du neuf relève d'une **asymétrie de disponibilité de licence** uniforme et **content-blind** — IPCC/NEA/IEA tous écartés pour clause NC ; le « correctif » réclamé (ajouter une contre-source basse au neuf) serait lui-même un biais ; (b) la direction n'est **pas auto-servante** : les sources écartées publient un neuf souvent *plus bas* que RTE — les exclure laisse le neuf sur un chiffre *haut* ; (c) l'écart n'est matérialisé **nulle part** (pas de champ gap/tri), et le disclaimer nomme le mono-source du neuf, le caractère mondial d'IRENA et l'absence d'écart calculé.

**Critique anti-nucléaire (la plus forte)** : « l'existant amorti reçoit une 2e source iso-périmètre serrée (CRE ~60), les EnR une source hors-périmètre (IRENA mondial) ; le neuf reste non contredit. » **Interne-contradictoire** : le levier invoqué rend les **EnR moins chères**, pas le nucléaire — effet *pro*-renouvelable, inutilisable comme preuve d'un tilt pro-nucléaire.

**Verdict explicite sur le « nucléaire neuf mono-source » : asymétrie de licence justifiée, non biais idéologique.** Preuve par contre-exemple : **IRENA, la source la plus pro-EnR, est INCLUSE** ; **NEA/IEA, pro-pilotable, sont EXCLUS** — un tri idéologique aurait fait l'inverse. Le mono-source laisse de plus le neuf sur la valeur la *moins flatteuse*.

**Transparence renforcée (recommandation du re-jeu, implémentée).** Pour que l'asymétrie ne vive pas que dans la prose du disclaimer, chaque entrée `/v1/cost-reference` expose désormais `geography` (`france` | `monde`) et `technology_source_count` (≥ 2 = dispersion inter-sources ; 1 = mono-source) — l'asymétrie de couverture est **lisible par machine**.

**Pré-conditions de gouvernance restantes (non bloquantes)** : (1) trouver une 2e source **licence-compatible pour le nucléaire neuf** (lèverait la dernière asymétrie de couverture) ; (2) une **contre-source France** pour les EnR (réduirait l'asymétrie géographique FR/mondial) ; (3) sourçage fin des hypothèses des 2e sources (CRE + IRENA portent `None`) ; (4) re-millésimer les points uniques IRENA (hydro 52,7 ; biomasse 80,4).

VERDICT-GLOBAL (re-jeu n°3) : **GREEN**.

---

## Addendum — Re-jeu n°4 du GATE (2026-06-20) : 2e source FR renouvelables → **ROUGE → changement annulé**

**Objet :** tentative d'ajout d'une **2e source française** aux renouvelables (prix de soutien Cour des comptes « Le soutien aux EnR via les CSPE », mars 2026 + prix d'appels d'offres CRE, via une base dédiée `support-price`), pour réduire l'asymétrie géographique FR/monde héritée du re-jeu n°3. Sources licence-compatibles (CRPA, sans clause NC).

| Bloc | Verdict |
|---|---|
| Bloc 2 — Non-verdict | pass |
| Bloc 3 — Provenance | pass |
| Bloc 4 — Test adverse (pro-nucléaire) | pass |
| Bloc 4 — Test adverse (anti-nucléaire) | pass |
| **Bloc 1 — Commensurabilité** | **FAIL** |
| **Bloc 4 — Test aveugle** | **FAIL** |

**Bloc 1 (FAIL) :** co-lister la grande hydro amortie (ADEME 15) et la petite hydro sous soutien (Cour 125) sous une **seule** filière `Hydraulique` reproduit la non-commensurabilité que l'ADR a corrigée pour le nucléaire en *scindant* la techno (« jamais fusionnés », §2). Le `basis` + le disclaimer en prose sont un traitement **plus faible** que ce précédent maison → mord sur un choix de conception.

**Bloc 4 (FAIL) :** enrichir les **seuls** renouvelables porte leur `technology_source_count` à **3** (uniforme) vs **2** (nucléaire existant) / **1** (neuf) → un lecteur aveugle partitionne nucléaire/renouvelable au seul compte de sources, et le neuf (mono-source, le plus cher) devient un « phare » structurel. Le rééquilibrage (ajouter des sources au nucléaire) est **bloqué par les licences NC** (IPCC/NEA/IEA).

Les deux critiques adverses (pro **et** anti-nucléaire) **passent** et s'effondrent symétriquement (la base `support-price` étiquette honnêtement la nature ; les prix de soutien FR *relèvent* les bornes EnR, effet non auto-servant) — mais le **critère strict** du GATE exige que **tous** les blocs passent. Deux blocs mordent sur la conception.

**Décision (Principe 0) :** la neutralité ne pouvant être garantie, **le changement est annulé** (modifs `cost.rs` non livrées). L'état VERT (re-jeu n°3, **v0.3.2**) est conservé. L'asymétrie géographique FR/monde des renouvelables est **assumée** (déjà jugée *content-blind* au re-jeu n°3 — IRENA, la source la plus pro-EnR, est incluse). 

**Constat utile :** l'enrichissement FR des renouvelables, bien qu'intuitif, **dégrade** la neutralité au regard des critères du GATE tant que le nucléaire n'est pas multi-sourçable. La contre-source FR redeviendra envisageable **une fois** une 2e source nucléaire licence-compatible disponible (pour garder les comptes de sources symétriques).

VERDICT-GLOBAL (re-jeu n°4) : **RED → changement non livré**.
