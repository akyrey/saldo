import type { components } from "./schema";

export type ErrorDetail = components["schemas"]["ErrorDetail"];

/**
 * A failed API call. `code` is the server's error code (`validation_failed`,
 * `not_found`, ...) or `network` when the server could not be reached.
 */
export class ApiError extends Error {
  readonly status: number;
  readonly code: string;
  readonly details: readonly ErrorDetail[];

  constructor(status: number, code: string, message: string, details: readonly ErrorDetail[] = []) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.code = code;
    this.details = details;
  }
}

/** Builds an `ApiError` from whatever the server sent back. */
export function apiErrorFrom(status: number, body: unknown): ApiError {
  const envelope = body as Partial<components["schemas"]["ErrorEnvelope"]> | undefined;
  const error = envelope?.error;
  if (error && typeof error.code === "string") {
    return new ApiError(status, error.code, error.message, error.details ?? []);
  }
  return new ApiError(status, "unknown", `The server answered ${status}.`);
}

/** Field errors keyed by path, e.g. `{ "splits[1].amount": "must be positive" }`. */
export function fieldErrors(error: unknown): Record<string, string> {
  if (!(error instanceof ApiError)) return {};
  const result: Record<string, string> = {};
  for (const detail of error.details) {
    if (detail.field && !(detail.field in result)) result[detail.field] = detail.message;
  }
  return result;
}

/** A message for a person: the server's message, or a network hint. */
export function errorMessage(error: unknown): string {
  if (error instanceof ApiError) {
    if (error.code === "network") return "Cannot reach the server. Are you offline?";
    return error.message;
  }
  return "Something went wrong.";
}

/**
 * The fullest message for a person: the field messages when the server sent
 * some (they say what is wrong), else the general message; a plain `Error`'s
 * own message (a check made before calling the server).
 */
export function explain(error: unknown): string {
  if (error instanceof ApiError) {
    const details = error.details.map((d) => d.message).filter((m) => m.trim() !== "");
    return details.length > 0 ? details.join("; ") : errorMessage(error);
  }
  if (error instanceof Error) return error.message;
  return errorMessage(error);
}
