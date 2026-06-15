# Collection Bruno — carbon-fr

Collection [Bruno](https://www.usebruno.com/) couvrant l'API `/v1` : cas
nominaux (national + régional, `rte-direct` + `acv-ademe`) et cas d'erreur
(400/404). Les fichiers `.bru` sont versionnés (lisibles en revue, diffables).

## Utilisation

1. Ouvrir le dossier `bruno/` dans Bruno (*Open Collection*).
2. Sélectionner l'environnement **Local** (variable `baseUrl`, défaut
   `http://localhost:8080`).
3. Lancer l'API : `DATABASE_URL=… cargo run -p server` (voir le README racine).

En ligne de commande (Bruno CLI) :

```bash
npm install -g @usebruno/cli
cd bruno
bru run --env Local
```

> Certaines requêtes (`Intensité date`, `stats`, `Prévision`) supposent que
> l'historique a été backfillé (`carbonfr-server backfill`) — la prévision
> s'appuie sur ~8 semaines en amont du `from`. Les requêtes régionales
> supposent que le poller a tourné au moins une fois.

## Contenu

| Requête | Vérifie |
|---|---|
| Health | `200`, corps `ok` |
| Intensité now — national (rte-direct / acv-ademe) | `200`, méthodologie |
| Intensité now — régional Bretagne (acv-ademe) | `200`, région |
| Mix — national / régional | `200`, unité `MW` |
| Méthodologies — catalogue | `200`, défaut `rte-direct` |
| Facteurs — acv-ademe | `200`, unité, filières |
| Intensité date — historique | `200` |
| Intensité stats — résumé / série journalière | `200`, `intervals` |
| Prévision — série (climatology@1) | `200`, `model`, unité |
| Prévision — créneau le plus bas-carbone | `200`, `average_intensity` |
| OpenAPI — spec | `200`, `openapi: 3.1.0` |
| Erreur — région en rte-direct | `404 no_data` |
| Erreur — région inconnue | `400 bad_request` |
| Erreur — date sans `from` | `400 bad_request` |
| Erreur — facteurs en rte-direct | `400 bad_request` |

> La collection n'est **pas** dans la CI : elle exige une API live (+ base,
> + données ODRÉ). C'est un outil de dev/QA manuel.
