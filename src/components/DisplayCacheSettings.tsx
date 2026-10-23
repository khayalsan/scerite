import { useEffect, useState } from "react";
import { Trash2 } from "lucide-react";
import { displayCache } from "../lib/displayCache";

function formatSize(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function DisplayCacheSettings() {
  const [bytes, setBytes] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    void displayCache.size().then((size) => { if (active) setBytes(size); })
      .catch((reason: unknown) => { if (active) setError(String(reason)); });
    return () => { active = false; };
  }, []);

  async function clear() {
    setBusy(true);
    setError(null);
    try {
      await displayCache.clear();
      setBytes(await displayCache.size());
    } catch (reason) { setError(String(reason)); }
    finally { setBusy(false); }
  }

  return <section className="app-settings-section" aria-labelledby="display-cache-title">
    <div className="environment-section-heading"><h2 id="display-cache-title">Local display cache</h2></div>
    <div className="app-setting-row">
      <span className="app-setting-copy"><strong>{bytes === null ? "Calculating size…" : formatSize(bytes)}</strong>
        <small>Saved remote maps, recent logs, and server icons. Scerite checks the server for updates after showing them.</small></span>
      <button className="button button-secondary" type="button" onClick={() => void clear()} disabled={busy || bytes === null || bytes === 0}>
        <Trash2 size={15} aria-hidden="true" />{busy ? "Clearing…" : "Clear cache"}
      </button>
    </div>
    {error && <p className="form-error" role="alert">{error}</p>}
  </section>;
}
