# ADR-0020 — Politique de dépréciation (préavis, en-têtes, fenêtre de retrait)

- **Statut** : Accepté
- **Date** : 2026-06-17
- **S'appuie sur** : ADR-0007 (API `/v1`), ADR-0011 (contrat de prévision), ADR-0019 (politique de versionnement — les 4 axes)
- **Complète** : ADR-0019 décrit *quand* on incrémente une version ; cet ADR décrit *comment on retire* l'ancienne sans casser les consommateurs.

## Contexte

`carbon-fr` est une **API publique**. Ses consommateurs (SDK, scripts carbon-aware, intégrations tierces) ne peuvent pas être prévenus individuellement : le seul canal fiable est la **réponse HTTP elle-même** et la documentation. ADR-0019 acte que `/v1` et `/v2` *coexistent* en cas de rupture, mais ne dit rien de :

- combien de temps une version d'API reste servie après l'arrivée de la suivante ;
- comment un consommateur **apprend** qu'un endpoint, un paramètre ou une méthodologie qu'il utilise est en fin de vie, **avant** la coupure ;
- ce qui distingue une dépréciation (ça marche encore, mais c'est annoncé condamné) d'un retrait (ça ne répond plus).

Sans politique écrite, le risque concret est la **rupture silencieuse** : un champ disparaît, une intégration casse en prod sans préavis, la confiance dans l'API s'effondre. Pour une première API publique, c'est le défaut de maintenabilité le plus coûteux.

## Décision

### Trois états de cycle de vie

Tout élément public (version d'API, endpoint, paramètre, champ de réponse, méthodologie/modèle, version de SDK) suit :

1. **Actif** — supporté, recommandé.
2. **Déprécié** — fonctionne **toujours à l'identique**, mais signalé condamné, avec une **date de retrait annoncée**. Aucune dégradation de comportement pendant cette phase.
3. **Retiré** (*sunset*) — l'élément ne répond plus (`410 Gone` pour un endpoint retiré dont le chemin est conservé ; `404` si le chemin disparaît). Jamais avant la date annoncée.

> **Règle d'or : on n'enlève jamais rien sans être passé par l'état « Déprécié » annoncé.** Pas de retrait surprise.

### Comment c'est annoncé (canal = la réponse HTTP)

Dès qu'un endpoint passe **Déprécié**, **chacune de ses réponses** porte :

- **`Deprecation`** (RFC 9745) — date à laquelle l'élément est devenu déprécié, en horodatage HTTP (IMF-fixdate).
- **`Sunset`** (RFC 8594) — date prévue de retrait, en horodatage HTTP.
- **`Link`** avec `rel="deprecation"` (et `rel="sunset"`) — URL pointant vers l'entrée de migration (CHANGELOG / docs).

Exemple :

```
HTTP/1.1 200 OK
Deprecation: Wed, 17 Jun 2026 00:00:00 GMT
Sunset: Sat, 19 Dec 2026 00:00:00 GMT
Link: <https://carbon-fr-api.kovelt.fr/docs#deprecations>; rel="deprecation"
```

En complément (pas en remplacement) :

- **OpenAPI** : `deprecated: true` sur l'opération ou le champ (`utoipa`), donc visible sur la Swagger UI `/docs`.
- **CHANGELOG.md** : section **Déprécié** (format Keep a Changelog) à l'annonce, section **Supprimé** au retrait.
- **`/v1/methodologies`** : une méthodologie/version dépréciée passe son statut `served` → `deprecated` dans le catalogue (déjà exposé, ADR-0010).

### Fenêtre minimale de retrait

- **Après la `1.0`** : au moins **6 mois** entre l'annonce de dépréciation (`Deprecation`/`Sunset` posés) et le retrait effectif d'une **version d'API** ou d'un endpoint. La version d'API précédente reste servie pendant toute la fenêtre (coexistence `/v1`+`/v2`, ADR-0019).
- **Avant la `1.0` (`0.y.z`, état actuel)** : les ruptures restent tolérées en *minor* (GOUVERNANCE §6), mais **même en pré-1.0** un retrait est annoncé via les en-têtes `Deprecation`/`Sunset` **et** le CHANGELOG, avec un préavis d'**au moins 30 jours**. La discipline d'annonce ne dépend pas du palier `1.0` ; seule la durée du préavis le fait.

### Par axe de versionnement (rappel ADR-0019)

- **Contrat d'API** (`/v1`) : politique ci-dessus, en-têtes inclus. C'est le cœur de cet ADR.
- **Méthodologies & modèles** : **immuables une fois publiées** (ADR-0005/0006) — on ne *modifie* jamais `acv-ademe@1`, on en publie une nouvelle version. Une version publiée n'est donc pas « cassée » : elle peut seulement être **dépréciée** (statut `deprecated` dans `/v1/methodologies`) puis retirée du service après la fenêtre, l'ADR d'origine passant en statut `Déprécié`.
- **SDK** (`@carbon-fr/sdk`) : dépréciations marquées `@deprecated` (JSDoc) dans une *minor*, retrait dans une *major* ; le SDK suit le **contrat d'API**, pas le code serveur.

### Mécanisme d'implémentation

Aucun élément n'est déprécié aujourd'hui : **aucun en-tête n'est donc émis pour l'instant** (pas de code spéculatif, cf. discipline du projet). À la **première dépréciation réelle**, on ajoutera dans `adapter-http` un utilitaire pur et testé `deprecation_headers(deprecated_at, sunset_at, link)` (construction des trois en-têtes ci-dessus), appliqué via une couche `tower` aux routes concernées — livré **dans la même PR** que la dépréciation qu'il sert. Cet ADR fige le **contrat** (noms d'en-têtes, RFC, fenêtre) ; le code viendra avec son premier usage.

## Conséquences

- **Pas de rupture silencieuse** : un consommateur voit la condamnation dans ses réponses HTTP bien avant la coupure (un client peut même *alerter* sur la présence d'un en-tête `Sunset`).
- **Engage** : tenir à jour la section **Déprécié/Supprimé** du CHANGELOG ; poser les en-têtes dès qu'on déprécie ; respecter la fenêtre minimale (ne pas retirer avant la date `Sunset` publiée) ; faire passer l'ADR d'une méthodologie retirée en statut `Déprécié`.
- **Ne nous engage pas** à du code mort : les en-têtes n'existent dans le code qu'à partir de la première dépréciation effective.
- **Cohérent self-hosting** : tout passe par des en-têtes standards et l'OpenAPI ; une instance auto-hébergée hérite des mêmes garanties sans configuration.

## Alternatives envisagées

- **En-têtes propriétaires** (`X-API-Deprecated: true`) — écarté : `Deprecation` (RFC 9745) et `Sunset` (RFC 8594) sont les standards IETF, compris par des outils tiers ; pas de raison de réinventer.
- **Annoncer uniquement dans le CHANGELOG / la doc** — écarté : un consommateur qui ne relit pas le CHANGELOG ne verra rien ; le canal fiable est la réponse HTTP qu'il reçoit déjà.
- **Retirer en pré-1.0 sans préavis (au nom du « 0.x tout est permis »)** — écarté : techniquement autorisé par SemVer, mais une API *publique et déployée* a des utilisateurs réels dès maintenant ; on s'impose un préavis minimal même avant la `1.0`.
- **Implémenter tout de suite l'utilitaire d'en-têtes** — écarté : rien n'est déprécié, ce serait du code mort (clippy `-D warnings`, discipline « pas de code mort ») ; on fige le contrat ici, le code arrive avec son premier usage.
