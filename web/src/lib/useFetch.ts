import { useEffect, useState } from "react";
import { apiUrl } from "./api";

export function useFetch<T>(path: string) {
  const [data, setData] = useState<T | null>(null);
  const [err, setErr] = useState<Error | null>(null);

  useEffect(() => {
    const ctrl = new AbortController();
    fetch(apiUrl(path), { signal: ctrl.signal })
      .then((r) => {
        if (!r.ok) throw new Error(`${r.status}`);
        return r.json();
      })
      .then(setData)
      .catch((e) => !ctrl.signal.aborted && setErr(e));
    return () => ctrl.abort();
  }, [path]);

  return { data, err, loading: !data && !err };
}
