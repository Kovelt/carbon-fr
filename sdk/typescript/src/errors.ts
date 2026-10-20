import type { ProblemDetails } from "./types.js";

/**
 * Erreur API : porte le code HTTP et le corps **Problem Details** (RFC 9457).
 * `code` est l'identifiant court stable (`no_data`, `bad_request`, …) ; le
 * message reprend `detail` (sinon `title`).
 */
export class CarbonFrError extends Error {
  readonly status: number;
  readonly code: string;
  /** Corps Problem Details brut, si disponible. */
  readonly problem: Partial<ProblemDetails>;
  constructor(status: number, body: Partial<ProblemDetails>) {
    super(body.detail ?? body.title ?? `HTTP ${status}`);
    this.name = "CarbonFrError";
    this.status = status;
    this.code = body.code ?? "error";
    this.problem = body;
  }
}

/**
 * Convertit une réponse non 2xx en {@link CarbonFrError} en lisant son corps
 * Problem Details (corps absent ou non-JSON : erreur sans détail).
 *
 * @internal Partagé par les appels REST (`client.ts`) et le flux SSE
 * (`stream.ts`) ; non exporté par le paquet.
 */
export async function errorFromResponse(res: Response): Promise<CarbonFrError> {
  let body: Partial<ProblemDetails> = {};
  try {
    body = (await res.json()) as Partial<ProblemDetails>;
  } catch {
    // Corps non-JSON.
  }
  return new CarbonFrError(res.status, body);
}
