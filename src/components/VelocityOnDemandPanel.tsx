import { useEffect, useState } from "react";
import { velocityOnDemand } from "../lib/velocityOnDemand";
import type { VelocityOnDemandStatus } from "../lib/velocityOnDemand";

export function VelocityOnDemandPanel({ profileId, canChange }: { profileId: string; canChange: boolean }) {
  const [status, setStatus] = useState<VelocityOnDemandStatus | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let active = true;
    void velocityOnDemand.status(profileId).then((value) => { if (active) setStatus(value); })
      .catch((reason) => { if (active) setError(String(reason)); });
    return () => { active = false; };
  }, [profileId]);

  const change = async () => {
    if (!status || !canChange) return;
    setBusy(true);
    setError("");
    try {
      setStatus(await (status.enabled ? velocityOnDemand.disable(profileId) : velocityOnDemand.enable(profileId)));
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  };

  return <section className="velocity-on-demand">
    <h3>Start when a player joins</h3>
    <p>Scerite installs a Velocity proxy on this remote host. Authenticated players can connect to the usual address; the Paper or Purpur server starts when they join and stops after 5 minutes with no players. The proxy stays online.</p>
    <label>On-demand startup <span className="property-switch"><input type="checkbox" role="switch" checked={status?.enabled ?? false} disabled={!status || !canChange || busy} onChange={() => void change()} /><span aria-hidden="true" /></span></label>
    {!canChange && <p>Stop the server before changing this setting.</p>}
    {status?.enabled && <p>Public port: {status.publicPort}. Backend port: {status.backendPort} (local access only).</p>}
    {status?.enabled && !status.proxyRunning && <p className="form-error" role="alert">The Velocity proxy is not running. Players cannot join until it is restarted.</p>}
    {busy && <p>Configuring Velocity…</p>}
    {error && <p className="form-error" role="alert">{error}</p>}
  </section>;
}
