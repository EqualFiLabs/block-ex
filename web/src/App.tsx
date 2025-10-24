import { Outlet, Link, useRouteError } from "react-router-dom";
import Header from "./components/Header";

export default function App() {
  return (
    <div className="min-h-full">
      <Header />
      <main className="mx-auto max-w-6xl px-4 py-6">
        <Outlet />
      </main>
      <footer className="mx-auto max-w-6xl px-4 pb-8 text-sm text-slate-500">
        Built for research & education. Ring members are decoys; no ownership is implied.
      </footer>
    </div>
  );
}

export function ErrorBoundary() {
  const err = useRouteError();
  const message =
    err instanceof Error ? err.message : typeof err === "string" ? err : JSON.stringify(err, null, 2);

  return (
    <div className="p-6">
      <h1 className="text-xl font-semibold">Something went wrong</h1>
      <pre className="mt-2 rounded bg-slate-100 p-3">{message}</pre>
      <Link to="/" className="btn mt-4">
        Go Home
      </Link>
    </div>
  );
}
