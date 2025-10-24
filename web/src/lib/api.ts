export const API_BASE = import.meta.env.VITE_API_BASE || "";

export async function getJSON<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${API_BASE}${path}`, {
    headers: { Accept: "application/json" },
    ...init,
  });
  if (!res.ok) throw new Error(`${res.status} ${res.statusText}`);
  return res.json();
}

export function fmtHash(h?: string | null, n = 8) {
  if (!h) return "";
  return `${h.slice(0, n)}…${h.slice(-n)}`;
}
