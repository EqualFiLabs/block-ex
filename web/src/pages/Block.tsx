import { useParams, Link, useSearchParams } from "react-router-dom";
import { useEffect, type ReactNode } from "react";
import { useFetch } from "../lib/useFetch";
import { getJSON, fmtHash } from "../lib/api";

type BlockView = {
  height: number;
  hash: string;
  ts: number;
  tx_count: number;
  size_bytes: number;
  reward_nanos: number;
};

export default function Block() {
  const { id } = useParams();
  const { data: block, loading } = useFetch<BlockView>(`/api/v1/block/${id}`);
  const [sp] = useSearchParams();
  const page = Number(sp.get("page") ?? "1");
  const per = 20;

  useEffect(() => {
    window.scrollTo({ top: 0, behavior: "smooth" });
  }, [id, page]);

  async function copyJSON() {
    const data = await getJSON<Record<string, unknown>>(`/api/v1/block/${id}`);
    await navigator.clipboard.writeText(JSON.stringify(data, null, 2));
    alert("Block JSON copied");
  }

  return (
    <div className="space-y-4">
      <div className="card">
        {loading ? (
          <div className="skeleton h-6 w-1/3" />
        ) : (
          <>
            <div className="flex items-center justify-between">
              <h1 className="text-xl font-semibold">Block {block?.height}</h1>
              <div className="flex items-center gap-2">
                {typeof block?.height === "number" && (
                  <>
                    <Link className="btn" to={`/block/${block.height - 1}`}>
                      ← Prev
                    </Link>
                    <Link className="btn" to={`/block/${block.height + 1}`}>
                      Next →
                    </Link>
                  </>
                )}
                <button className="btn" onClick={copyJSON}>
                  Copy JSON
                </button>
              </div>
            </div>
            <dl className="grid grid-cols-2 md:grid-cols-3 gap-3 mt-3">
              <Info
                label="Hash"
                value={<span className="font-mono">{fmtHash(block?.hash, 12)}</span>}
              />
              <Info
                label="Timestamp"
                value={
                  block ? new Date(block.ts * 1000).toLocaleString() : <span>…</span>
                }
              />
              <Info label="Tx count" value={block?.tx_count ?? "…"} />
              <Info
                label="Size"
                value={block ? `${(block.size_bytes / 1024).toFixed(1)} KB` : "…"}
              />
              <Info
                label="Miner reward"
                value={
                  block ? `${(block.reward_nanos / 1e12).toFixed(12)} XMR` : "…"
                }
              />
            </dl>
          </>
        )}
      </div>

      <div className="card">
        <div className="flex items-center justify-between mb-2">
          <h2 className="font-semibold">Transactions</h2>
          <div className="text-sm text-slate-500">
            Page {page}, {per} per (client-side pagination)
          </div>
        </div>
        {/* Placeholder table; wire real tx list when API provides it */}
        <div className="text-sm text-slate-500">
          Tx listing will populate when /api provides tx hashes per block.
        </div>
      </div>
    </div>
  );
}

function Info({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div>
      <div className="text-xs text-slate-500">{label}</div>
      <div className="text-sm">{value}</div>
    </div>
  );
}
