const ABSOLUTE_URL = /^https?:\/\//i;
const rawBase = import.meta.env.VITE_API_BASE?.trim() ?? "";
export const API_BASE = rawBase;

function normalizePath(path: string) {
  return path.startsWith("/") ? path : `/${path}`;
}

export function apiUrl(path: string) {
  const normalized = normalizePath(path);

  if (!API_BASE) {
    return normalized;
  }

  if (ABSOLUTE_URL.test(API_BASE)) {
    return new URL(normalized, API_BASE).toString();
  }

  const base = API_BASE.endsWith("/") ? API_BASE.slice(0, -1) : API_BASE;
  if (normalized.startsWith(base)) {
    return normalized;
  }

  return `${base}${normalized}`;
}

export async function getJSON<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(apiUrl(path), {
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
