import { useState } from "react";
import type { FormEvent } from "react";
import { useNavigate } from "react-router-dom";
import { apiUrl } from "../lib/api";

export default function Header() {
  const [q, setQ] = useState("");
  const nav = useNavigate();

  async function onSubmit(e: FormEvent) {
    e.preventDefault();
    const target = apiUrl("/api/v1/search");
    const url = target.startsWith("http")
      ? new URL(target)
      : new URL(target, window.location.origin);
    url.searchParams.set("q", q.trim());
    try {
      const r = await fetch(url.toString());
      if (!r.ok) throw new Error("no match");
      const { kind, value } = await r.json();
      if (kind === "tx") nav(`/tx/${value}`);
      else if (kind === "block") nav(`/block/${value}`);
      else if (kind === "height") nav(`/block/${value}`);
      else if (kind === "key_image") nav(`/key_image/${value}`);
      else if (kind === "global_index") nav(`/tx/${value}`);
      else alert("No match");
    } catch {
      alert("No match");
    }
  }

  return (
    <header className="sticky top-0 z-10 border-b bg-white/80 backdrop-blur">
      <div className="mx-auto flex max-w-6xl items-center gap-4 px-4 py-3">
        <a href="/" className="text-lg font-semibold tracking-tight">
          Monero Explorer
        </a>
        <form onSubmit={onSubmit} className="flex-1">
          <input
            value={q}
            onChange={(e) => setQ(e.target.value)}
            placeholder="Search height / tx hash / key image / global index"
            className="w-full rounded-2xl border border-slate-300 px-4 py-2 font-mono focus:outline-none focus:ring-2 focus:ring-indigo-500"
            aria-label="Search"
          />
        </form>
        <a className="btn" href="/stats">
          Stats
        </a>
      </div>
    </header>
  );
}
