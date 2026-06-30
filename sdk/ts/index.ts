export type GhostTeamClientOptions = {
  baseUrl: string;
  apiKey?: string;
  fetchImpl?: typeof fetch;
  retries?: number;
  retryDelayMs?: number;
};

export type ApiResponse<T> = {
  ok: boolean;
  data: T;
};

export type ApiIdResponse<T> = {
  ok: boolean;
  id: T;
  note?: string;
  warning?: string;
};

export type GhostTeamErrorKind =
  | "network"
  | "http"
  | "parse"
  | "retry_exhausted";

export class GhostTeamError extends Error {
  kind: GhostTeamErrorKind;
  status?: number;
  body?: string;

  constructor(message: string, kind: GhostTeamErrorKind, status?: number, body?: string) {
    super(message);
    this.name = "GhostTeamError";
    this.kind = kind;
    this.status = status;
    this.body = body;
  }
}

export class GhostTeamClient {
  baseUrl: string;
  apiKey?: string;
  fetchImpl: typeof fetch;
  retries: number;
  retryDelayMs: number;

  constructor(options: GhostTeamClientOptions) {
    this.baseUrl = options.baseUrl.replace(/\/+$/, "");
    this.apiKey = options.apiKey;
    this.fetchImpl = options.fetchImpl ?? fetch;
    this.retries = options.retries ?? 2;
    this.retryDelayMs = options.retryDelayMs ?? 150;
  }

  setApiKey(apiKey: string): void {
    this.apiKey = apiKey;
  }

  clearApiKey(): void {
    this.apiKey = undefined;
  }

  async request<T>(path: string, init: RequestInit = {}): Promise<T> {
    const url = `${this.baseUrl}${path.startsWith("/") ? path : `/${path}`}`;
    const method = (init.method ?? "GET").toUpperCase();

    let lastError: unknown;

    for (let attempt = 0; attempt <= this.retries; attempt += 1) {
      const headers = new Headers(init.headers ?? {});
      if (this.apiKey) {
        headers.set("X-GhostTeam-Key", this.apiKey);
      }
      if (init.body && !headers.has("Content-Type")) {
        headers.set("Content-Type", "application/json");
      }

      try {
        const response = await this.fetchImpl(url, {
          ...init,
          method,
          headers,
        });

        if (response.status >= 500 && attempt < this.retries) {
          await sleep(this.retryDelayMs * 2 ** attempt);
          continue;
        }

        const text = await response.text();

        if (!response.ok) {
          throw new GhostTeamError(
            `GhostTeam API request failed with status ${response.status}`,
            "http",
            response.status,
            text,
          );
        }

        if (!text) {
          return undefined as T;
        }

        return JSON.parse(text) as T;
      } catch (error) {
        lastError = error;
        if (attempt < this.retries) {
          await sleep(this.retryDelayMs * 2 ** attempt);
          continue;
        }

        if (error instanceof GhostTeamError) {
          throw error;
        }

        throw new GhostTeamError(
          `GhostTeam request failed after ${this.retries + 1} attempts`,
          "retry_exhausted",
        );
      }
    }

    throw lastError instanceof Error
      ? lastError
      : new GhostTeamError("GhostTeam request failed", "retry_exhausted");
  }

  async get<T>(path: string): Promise<T> {
    return this.request<T>(path, { method: "GET" });
  }

  async post<T, B = unknown>(path: string, body: B): Promise<T> {
    return this.request<T>(path, {
      method: "POST",
      body: JSON.stringify(body),
    });
  }
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

export * from "./agents";
export * from "./tasks";
export * from "./messages";
export * from "./ghostos";
