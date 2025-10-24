import { Link } from "react-router-dom";
import { useFetch } from "../lib/useFetch";
import { fmtHash } from "../lib/api";

type BlockView = {
  height: number;
  hash: string;
  ts: number;
  tx_count: number;
  size_bytes: number;
  reward_nanos: number;
};
type MempoolView = { hash: string; first_seen: number; last_seen: number };
type SoftFacts = { block_height: number; median_fee_rate: string };

export default function Home() {
  const { data: blocks, loading: lb } = useFetch<BlockView[]>(
    "/api/v1/blocks?limit=20",
  );
  const { data: mempool, loading: lm } = useFetch<MempoolView[]>(
    "/api/v1/mempool",
  );
  const { data: sfacts } = useFetch<SoftFacts[]>(
    "/api/v1/soft_facts?limit=1",
  );

  return (
    <div className="grid grid-cols-1 lg:grid-cols-3 gap-4">
      <div className="lg:col-span-2 card">
        <div className="flex items-center justify-between mb-3">
          <h2 className="font-semibold">Recent Blocks</h2>
          <span className="text-sm text-slate-500">latest 20</span>
        </div>
        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead className="text-left text-slate-500">
              <tr>
                <th className="py-2">Height</th>
                <th>Hash</th>
                <th>Txs</th>
                <th>Size</th>
              </tr>
            </thead>
            <tbody>
              {lb &&
                Array.from({ length: 8 }).map((_, i) => (
                  <tr key={i}>
                    <td className="py-2">
                      <div className="skeleton h-5 w-16" />
                    </td>
                    <td>
                      <div className="skeleton h-5 w-40" />
                    </td>
                    <td>
                      <div className="skeleton h-5 w-8" />
                    </td>
                    <td>
                      <div className="skeleton h-5 w-12" />
                    </td>
                  </tr>
                ))}
              {blocks?.map((b) => (
                <tr key={b.height} className="border-t">
                  <td className="py-2">
                    <Link
                      className="text-indigo-600 hover:underline"
                      to={`/block/${b.height}`}
                    >
                      {b.height}
                    </Link>
                  </td>
                  <td className="font-mono">{fmtHash(b.hash)}</td>
                  <td>{b.tx_count}</td>
                  <td>{(b.size_bytes / 1024).toFixed(1)} KB</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>

      <div className="card space-y-4">
        <div>
          <div className="text-sm text-slate-500">Mempool Pressure</div>
          <div className="text-3xl font-semibold">
            {lm ? "…" : mempool?.length ?? 0}
          </div>
        </div>
        <div>
          <div className="text-sm text-slate-500">Median Fee (latest)</div>
          <div className="text-xl">{sfacts?.[0]?.median_fee_rate ?? "N/A"}</div>
        </div>
        <div className="text-xs text-slate-500">
          Tip: search accepts height, 64-hex tx hash or key image, or a global
          index.
        </div>
      </div>
    </div>
  );
}
