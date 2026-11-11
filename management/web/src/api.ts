export type User = {
  id: string;
  username: string;
  display_name: string;
  role: "admin" | "operator" | "viewer";
  enabled: boolean;
  created_at: number;
};
export type Device = {
  id: string;
  name: string;
  platform: string;
  state: "pending" | "approved" | "disabled";
  verified: boolean;
  last_seen: number | null;
  created_at: number;
  note: string;
  public_key: string;
  can_connect: boolean;
};
export type Group = {
  id: string;
  name: string;
  user_ids: string[];
  device_ids: string[];
};
export type Grant = {
  id: string;
  subject_group: string;
  target_group: string;
  capability: "view" | "connect";
};
export type Audit = {
  id: number;
  time: number;
  actor: string;
  action: string;
  target: string;
  outcome: string;
  source: string;
  detail: string;
};
export type Overview = {
  devices: number;
  recently_seen: number;
  pending: number;
  enabled_users: number;
  server_time: number;
  connection_mode: string;
};
export type Listing<T> = { items: T[]; total: number };
let csrf = "";
export function setCsrf(value: string) {
  csrf = value;
}
export async function request<T>(
  path: string,
  method = "GET",
  body?: unknown,
): Promise<T> {
  const response = await fetch("/management/v1" + path, {
    method,
    credentials: "same-origin",
    headers: {
      "Content-Type": "application/json",
      ...(method === "GET" ? {} : { "X-CSRF-Token": csrf }),
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const data =
    response.status === 204
      ? null
      : await response
          .json()
          .catch(() => ({
            error: "The server returned an unexpected response.",
          }));
  if (!response.ok) {
    if (response.status === 401 && path != "/login")
      window.dispatchEvent(new Event("session-expired"));
    throw new Error(data?.error || `Request failed (${response.status})`);
  }
  return data as T;
}
export async function all<T>(resource: string): Promise<T[]> {
  const items: T[] = [];
  for (let page = 1; page <= 100; page++) {
    const result = await request<Listing<T>>(
      `/${resource}?limit=100&page=${page}`,
    );
    items.push(...result.items);
    if (items.length >= result.total) return items;
  }
  throw new Error(
    "Too many records to edit membership in this console. Narrow the fleet before editing.",
  );
}
