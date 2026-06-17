# ADR-0021 — Format d'erreur : Problem Details (RFC 9457)

- **Statut** : Accepté
- **Date** : 2026-06-17
- **S'appuie sur** : ADR-0007 (API `/v1`), ADR-0019 (versionnement), ADR-0020 (dépréciation)

## Contexte

L'API renvoyait ses erreurs sous une forme **maison** : `{"error": "<code>", "message": "<texte>"}` en `application/json`. C'était cohérent en interne, mais :

- non standard → un client doit apprendre *notre* convention plutôt qu'une convention connue ;
- `application/json` ne distingue pas une erreur d'une réponse normale au niveau du type de média ;
- pas de champ `status` dans le corps (utile quand le code HTTP est perdu en chemin, ex. logs, agrégation).

[RFC 9457 — *Problem Details for HTTP APIs*](https://www.rfc-editor.org/rfc/rfc9457) (qui remplace la RFC 7807) standardise exactement ce besoin : un type de média `application/problem+json` et un objet `type`/`title`/`status`/`detail`/`instance`, extensible. C'est la convention attendue par les outils et les consommateurs d'API publiques.

## Décision

**Adopter Problem Details (RFC 9457) pour toutes les réponses d'erreur de l'API.**

- **Type de média** : `application/problem+json` (au lieu de `application/json`).
- **Corps** :

  ```json
  {
    "type": "about:blank",
    "title": "Donnée absente",
    "status": 404,
    "detail": "aucune donnée disponible pour la région bretagne",
    "code": "no_data"
  }
  ```

- **`type` = `about:blank`** : le couple `status` + `code` suffit à qualifier l'erreur ; on n'expose **pas** d'URI à déréférencer (RFC 9457 §4.2.1 autorise explicitement `about:blank`). Si un catalogue d'erreurs documenté apparaît un jour, `type` pourra pointer vers lui sans rupture (extension, pas changement).
- **`code` (extension carbon-fr)** : on **conserve** le code court, **stable et machine-lisible** (`no_data`, `bad_request`, `unauthorized`, `unavailable`, `internal`, `rate_limited`). C'est la valeur sur laquelle un client s'aligne (plus robuste qu'un parsing de `title`/`detail`, qui sont du texte humain susceptible d'évoluer). Le SDK lit `code`.
- **`title`** : libellé court **stable par code** ; **`detail`** : message spécifique à l'occurrence.
- **Uniformité** : une seule fonction `problem_response(status, code, title, detail)` produit le corps **et** le type de média, partagée par le mapping d'erreurs métier (`ApiError`) **et** le middleware d'authentification/quota (`auth.rs`). Aucune divergence possible entre les chemins.

### Confinement à l'adapter (règle hexagonale)

Tout vit dans `adapter-http` : le `core` ne connaît ni HTTP ni JSON. Le DTO `ProblemDetails` porte `ToSchema` (frontière de l'hexagone), apparaît dans l'OpenAPI et est figé par le **garde-fou de contrat** (instantané OpenAPI).

## Conséquences

- **Standard & dev-first** : un client peut traiter nos erreurs comme n'importe quelle API conforme RFC 9457 ; le `code` reste l'ancrage machine.
- **Rupture de contrat assumée (pré-1.0)** : le corps passe de `{error, message}` à Problem Details + nouveau type de média. C'est une **rupture** au sens d'ADR-0020. Elle est acceptable **avant la `1.0`** (GOUVERNANCE §6) et gérée proprement :
  - le **SDK TypeScript** est mis à jour **dans la même PR** (le seul consommateur connu : `CarbonFrError` lit désormais `code`/`detail`) ;
  - le **garde-fou OpenAPI** a forcé une régénération **volontaire** de l'instantané (le diff documente la rupture) ;
  - **CHANGELOG** la consigne en *Modifié*.
- **Engage** : toute nouvelle erreur passe par `problem_response` (jamais de corps d'erreur ad hoc) ; le `code` d'une erreur publiée est **stable** (le changer = rupture).

## Alternatives envisagées

- **Garder `{error, message}`** — écarté : non standard, pas de `status` dans le corps, pas de type de média distinctif.
- **`type` = URI déréférençable par code** (ex. `/problems/no_data`) — écarté **pour l'instant** : impose de maintenir des pages d'erreur ; `about:blank` + `code` couvre le besoin. Réversible plus tard sans rupture (on pourra remplir `type`).
- **Supprimer le code court, ne garder que `type`** — écarté : un code court explicite est plus simple à matcher pour un client qu'un suffixe d'URI, et préserve la continuité avec l'ancien champ `error`.
- **Doubler `error`/`message` pour rétrocompat** — écarté : pré-1.0, un seul consommateur (notre SDK) mis à jour en bloc ; le doublon serait une dette transitoire inutile.
