/** The company's model provider as `/admin/llm` describes it. */

export interface LlmProviderKind {
  kind: string;
  requiresKey: boolean;
  /** Served at an endpoint the operator lists. */
  listedEndpoint: boolean;
  defaultModel: string | null;
}

export interface LlmSettingsView {
  provider: string;
  model: string;
  baseUrl: string | null;
  keyStored: boolean;
  /** Present in the price table: costs are estimated. */
  priced: boolean;
  /** The stored configuration can serve runs (key readable, provider built). */
  usable: boolean;
  updatedAt: string;
}

export interface LlmView {
  /** The server can seal keys (`MIMORYA_MASTER_KEY`). */
  secretsAvailable: boolean;
  platform: { provider: string; model: string; priced: boolean } | null;
  settings: LlmSettingsView | null;
  providers: LlmProviderKind[];
  endpoints: string[];
}

export interface LlmTestResult {
  ok: boolean;
  provider: string;
  model: string;
  error: string | null;
}
