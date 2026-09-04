export type ThemePreference = "system" | "dark" | "light";

export interface ChartViewportConfiguration {
  visible_candle_count: number | null;
  start_timestamp: number | null;
  follow_latest: boolean;
}

export interface ChartTabConfiguration {
  id: number;
  provider: string;
  symbol: string;
  interval: string;
  signals_visible: boolean;
  viewport: ChartViewportConfiguration;
}

export interface WorkspaceConfiguration {
  tabs: ChartTabConfiguration[];
  active_tab_id: number;
}

export interface UserConfiguration {
  theme: ThemePreference;
  locale: string | null;
  time_zone: string | null;
  workspace: WorkspaceConfiguration;
}

async function configurationResponse(response: Response): Promise<UserConfiguration> {
  if (!response.ok) {
    throw new Error(`Configuration request failed with status ${response.status}`);
  }
  return response.json() as Promise<UserConfiguration>;
}

export function loadUserConfiguration(
  endpoint: string,
  signal?: AbortSignal,
): Promise<UserConfiguration> {
  return fetch(new URL("/configuration", endpoint), { signal }).then(configurationResponse);
}

export function saveUserConfiguration(
  endpoint: string,
  configuration: UserConfiguration,
  signal?: AbortSignal,
): Promise<UserConfiguration> {
  return fetch(new URL("/configuration", endpoint), {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(configuration),
    signal,
  }).then(configurationResponse);
}
