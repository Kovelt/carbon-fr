# ADR-0023 — Affichage du prix de l'électricité : décomposition ancrée sur le TRV

- **Statut :** Accepté (ratifié le 2026-06-20 — forme d'exposition retenue : endpoint `/price`)
- **Date :** 2026-06-20
- **Décideur :** Morgan (Kovelt / carbon-fr)
- **Supersède / lié à :** ADR-0005 (méthodologie carbone), ADR-0006 (cycle de vie des révisions de données), ADR-0014 (primitives de scheduling carbon-aware)

> **Note de numérotation (2026-06-20)** : rédigé initialement sous le numéro provisoire « ADR-0015 », réattribué **ADR-0023** à l'intégration (0015 et 0016 étant déjà pris par le tier hébergé et les webhooks). La couche LCOE associée est l'**ADR-0024**.

---

## Contexte

Un retour de communauté (forum énergie) a fait émerger une demande récurrente : **afficher le « prix réel » de l'électricité** en regard de ce que paie le consommateur. La formulation spontanée des utilisateurs (« le coût de production du nucléaire vs le prix sur la facture ») mélange en réalité trois notions distinctes :

1. **Le coût de production par source (LCOE)** — donnée *analytique*, estimée, dépendante de la méthode et de la source (ADEME, AIE, Cour des comptes…), exprimée en coût amorti sur 20–40 ans. Pas de source canonique unique ; chiffres débattus.
2. **Le prix de gros spot (marché, coût marginal / merit order)** — donnée *factuelle*, horaire, temps réel. La dernière centrale appelée (souvent le gaz en pointe) fixe le prix unique que toute la production touche. Source canonique : ENTSO-E Transparency Platform (déjà intégrée pour les flux transfrontaliers).
3. **Le prix payé (TRV / offres de marché)** — ce que débourse le client : composante énergie + acheminement (TURPE) + taxes + commercialisation/marge.

### Enjeu de positionnement

carbon-fr est un **instrument de mesure neutre** (« les données, ouvertes et vérifiables ; les conclusions, c'est à l'utilisateur de les tirer »). La demande du forum est, dans sa version naïve, une comparaison « prix réel vs prix payé » qui recoupe mot pour mot un discours politique (« on paie le nucléaire au prix du gaz / le marché marginal est cassé »). Ce terrain est d'autant plus inflammable depuis la **fin de l'ARENH (31/12/2025)** et son remplacement par le **VNU (Versement Nucléaire Universel) au 01/01/2026**.

**Principe directeur retenu :** la donnée brute est neutre, mais le biais s'infiltre par (a) le *choix de la paire* mise en regard, (b) l'*asymétrie des sources* (mesure vs estimation présentées à égalité), et (c) l'*omission* de ce qui sépare les deux bornes. La neutralité ne se déclare pas, elle se construit dans la structure de donnée.

---

## Décision

### 1. Modèle de donnée : décomposition complète, pas de paire de chiffres

On n'affiche **pas** deux chiffres choisis (« coût de production » vs « facture »). On affiche **la chaîne complète du prix payé**, décomposée par composante, chacune sourcée. Le « prix réel de l'énergie » n'est **pas un chiffre asséné** : il **émerge** comme la *composante énergie* de cette chaîne.

### 2. Ancrage sur le Tarif Réglementé de Vente (TRV)

La décomposition s'ancre sur le **TRV**, dont la **CRE publie officiellement la construction par empilement**. Raison : c'est le seul périmètre où **les quatre composantes sont sourçables de bout en bout**, ce qui élimine le maillon non vérifiable. Les offres de marché pourront se superposer ultérieurement (couche optionnelle), mais le socle vérifiable est le TRV.

Composantes et sources :

| Composante | Source | Nature |
|---|---|---|
| Énergie (« prix réel de l'énergie ») | Prix spot day-ahead **ENTSO-E** | Factuelle, horaire, temps réel |
| Acheminement | **TURPE** (fixé par la CRE) | Factuelle, réglementaire |
| Taxes | Accise sur l'électricité (ex-TICFE/CSPE) + TVA | Factuelle, réglementaire |
| Commercialisation / résidu | Résidu structurel du TRV (construction CRE) | Dérivée, **pas un chiffre inventé** |

> La « marge » n'est pas posée comme un chiffre libre (non sourçable, et précisément le maillon le plus suspecté) : sur le TRV elle devient le **résidu de la construction réglementée publiée par la CRE**.

### 3. Le « prix réel de l'énergie » = composante énergie spot

On retient l'**option 1** : ancrer le « prix réel de l'énergie » sur la **composante énergie de la facture (spot ENTSO-E)**, et non sur un LCOE. Justification :

- **Source unique et canonique** (ENTSO-E), déjà intégrée — aucun nouveau problème de confiance.
- **Cohérence d'unité et de temps** : €/kWh, horaire, temps réel — aligné avec la décomposition. Un LCOE (coût amorti pluriannuel) glissé dans une vue temps réel casserait la cohérence et trahirait par comparaison bancale.
- **Le « réel » émerge** de la donnée au lieu d'être affirmé — conforme au principe directeur.
- **Actionnable** : seule la composante énergie varie heure par heure ; c'est elle qui alimente la primitive « cheapest + greenest window » (cf. ADR-0014). Le LCOE est statique et inexploitable en FinOps.

Le LCOE n'est **pas abandonné** : il pourra constituer une **couche pédagogique séparée et bornée** (comparatif « coût de production vs prix de marché »), sous son **propre ADR (ADR-0024)**, explicitement étiquetée « estimation », et **jamais présentée comme pair de la donnée live**.

### 4. Contexte explicatif (sans verdict)

À chaque point horaire, on expose en contexte : le **mix** (% par source) et la **technologie marginale** qui fixe le prix spot. L'utilisateur lit *pourquoi* le prix est ce qu'il est (ex. « 92 % nucléaire + renouvelable, prix fixé par le gaz ») et tire sa conclusion — ou pas. **carbon-fr ne formule aucun jugement.**

### 5. Garde-fous de neutralité (contraintes de conception)

- Aucun libellé évaluatif (« tu surpaies », « écart anormal », « prix juste »…).
- Aucune mise en regard de deux chiffres isolés : la décomposition complète est **obligatoire** dès qu'un prix est affiché.
- Mesure et estimation ne sont **jamais** présentées au même niveau de statut ; toute estimation est étiquetée.
- Chaque composante porte sa source et son horodatage.

### 6. Forme d'exposition — **nouvel endpoint `/price`** *(ratifié 2026-06-20)*

Création d'un endpoint dédié `/price` plutôt qu'enrichissement de `/mix`. Justification :

- **Séparation des domaines** : coût ≠ composition/carbone. `/mix` reste centré sur la composition et l'intensité.
- **Stabilité de surface OpenAPI** : on ne mute pas `/mix`, qui approche du verrouillage de surface (cohérent avec la prudence appliquée au contrat forecast).
- **Versionnement indépendant** et structure de réponse dédiée (objet de décomposition imbriqué) sans distordre `/mix`.
- **Composition propre** : « cheapest + greenest window » devient une jointure `/intensity` (ou forecast) + `/price`.
- **Nuance** : la *technologie marginale* est pertinente dans les deux contextes ; candidate à figurer dans `/mix` (fait dérivé de la composition) et à être *référencée* par `/price`. À confirmer à l'implémentation.

---

## Conséquences

### Positives
- Réponse complète et vérifiable à la demande communautaire **sans** sortir du rôle d'instrument de mesure.
- Déblocage de la primitive **« cheapest + greenest window »** (FinOps), différenciant fort pour l'audience cible.
- Valeur pédagogique élevée (le merit order et la construction du TRV rendus lisibles et vérifiables).
- Documente la **refonte de la formule TRV 2025→2026**, peu visible du grand public.

### Coûts / charges
- **Nouveau domaine de donnée** → nouveau(x) port(s) hexagonal(aux) : un port « prix de gros » (ENTSO-E day-ahead) distinct de l'usage flux transfrontaliers ; une source de **référence réglementaire** (TURPE, taxes, construction TRV — CRE) probablement chargée en données de référence versionnées plutôt qu'en flux live.
- Les **paramètres réglementaires changent dans le temps** (TURPE, taux de taxes, formule TRV). Appliquer une **stratégie de millésime/vintage** analogue à celle des révisions RTE (clé incluant la période de validité) pour garantir la reproductibilité historique.
- Nouvelle surface OpenAPI (`/price`) à spécifier, documenter (Swagger `/docs`) et couvrir (SDK `@carbon-fr/sdk`).

### Risques / points ouverts (actions de gouvernance avant implémentation)
1. **Licence de réutilisation ENTSO-E** : vérifier les conditions de réutilisation des données day-ahead de la Transparency Platform (cohérence avec l'exigence de pureté de licence). Un ADR ou une clause dédiée peut être requis, comme pour Open-Meteo.
2. ~~**Sources CRE (TURPE, TRV, taxes)** : confirmer disponibilité et figer les références exactes de la formule TRV post-2026.~~ **Sourcé (2026-06-20).** Valeurs millésime 2026 figées dans `TrvReference::trv_2026` à partir de sources primaires : **TURPE 7** (CRE délib. n°2025-78 du 13/03/2025, grille au 1/8/2025 → ≈ 78 €/MWh pour 6 kVA / 2 400 kWh) ; **accise** 30,85 €/MWh (CRE délib. TRVE 2026 n°2026-06 du 14/01/2026 + BOFiP `BOI-RES-EAT-000240`) ; **commercialisation** 18,11 €/MWh HT (même délib. n°2026-06) ; **TVA** 20 % unique (BOFiP `ACTU-2025-00057`, le taux réduit 5,5 % a été supprimé par la LF 2025). *Caveats* : l'acheminement en €/MWh est une conversion dépendant du profil (6 kVA / 2 400 kWh retenus) ; au 2e semestre 2026 le TURPE est revalorisé (+3,04 % au 1/8/2026) et l'accise peut être réindexée → re-millésimer le cas échéant.
3. ~~**Forme d'exposition** : ratifier `/price` vs enrichissement `/mix`.~~ **Tranché (2026-06-20) : `/price`.**
4. ~~**Emplacement de la technologie marginale** : `/mix` (dérivé composition) référencé par `/price`, à trancher à l'implémentation.~~ **Tranché (2026-06-20) : exposée dans `/v1/price`** (objet `PriceContext`, technologie marginale estimée par ordre de mérite, marquée `estimated: true`), et non via `/mix`.

---

## Alternatives considérées

- **A — LCOE asséné comme « prix réel ».** Rejetée : pas de source canonique, chiffres débattus, unité/temporalité incohérentes avec une vue temps réel ; présente une estimation avec l'autorité d'une mesure.
- **B — Deux chiffres « réel vs payé » face à face.** Rejetée : la juxtaposition *est* un verdict implicite ; l'omission de l'empilement intermédiaire (TURPE + taxes) impute l'écart au marché/à la marge → parti pris par omission.
- **C — Enrichissement de `/mix` plutôt que `/price` dédié.** Écartée (recommandation) : couple coût et composition, mute une surface en voie de verrouillage. Conservée comme repli si la jointure inter-domaines s'avère trop coûteuse côté client.
- **D — Ne rien faire (hors périmètre).** Écartée : la composante énergie spot est factuelle, sourcée, et sert directement la mission carbon-aware. Le risque ne vient pas de la donnée mais de son cadrage, qui est ici maîtrisé par la structure.

---

## Suite

Une fois cet ADR ratifié (et le point 3 tranché), préparation d'un **brief d'implémentation pour Claude Code** : définition du/des port(s), schéma de la réponse `/price`, stratégie de millésime des références réglementaires, spécification OpenAPI et couverture SDK.
