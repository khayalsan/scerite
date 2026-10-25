import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { velocityOnDemand } from "../lib/velocityOnDemand";
import type { VelocityOnDemandStatus, VelocityProxyUsage } from "../lib/velocityOnDemand";

export function VelocityOnDemandPanel({ profileId, canChange }: { profileId: string; canChange: boolean }) {
  const [status, setStatus] = useState<VelocityOnDemandStatus | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [usage, setUsage] = useState<VelocityProxyUsage | null>(null);
  const [step, setStep] = useState("");

  useEffect(() => {
    const listener = listen<{ profileId: string; step: string }>("velocity-setup-progress", (event) => {
      if (event.payload.profileId === profileId) setStep(event.payload.step);
    });
    const javaListener = listen<{ profileId: string; phase: string; downloaded: number; total: number }>("vanilla-install-progress", (event) => {
      if (event.payload.profileId !== profileId) return;
      const { phase, downloaded, total } = event.payload;
      if (phase === "java_installed") setStep("Java 25 ready");
      else if (phase === "downloading_remote_java") setStep("Downloading Java 25 on the remote host");
      else if (phase === "downloading_java") setStep(`Downloading Java 25${total ? ` (${Math.floor(downloaded / total * 100)}%)` : ""}`);
      else if (phase === "uploading_java") setStep("Uploading Java 25");
    });
    return () => {
      void listener.then((unlisten) => unlisten());
      void javaListener.then((unlisten) => unlisten());
    };
  }, [profileId]);

  useEffect(() => {
    let active = true;
    void velocityOnDemand.status(profileId).then((value) => { if (active) setStatus(value); })
      .catch((reason) => { if (active) setError(String(reason)); });
    return () => { active = false; };
  }, [profileId]);

  useEffect(() => {
    if (!status?.enabled) { setUsage(null); return; }
    let active = true;
    let timer: number | undefined;
    const refresh = async () => {
      try {
        const value = await velocityOnDemand.usage(profileId);
        if (active) {
          setUsage(value);
          setStatus((current) => current ? { ...current, proxyRunning: value.running } : current);
        }
      } catch {
        if (active) setUsage(null);
      } finally {
        if (active) timer = window.setTimeout(() => void refresh(), 5000);
      }
    };
    void refresh();
    return () => { active = false; if (timer !== undefined) window.clearTimeout(timer); };
  }, [profileId, status?.enabled]);

  const change = async () => {
    if (!status || !canChange) return;
    setBusy(true);
    setError("");
    setStep(status.enabled ? "Disabling Velocity" : "Checking the server");
    try {
      setStatus(await (status.enabled ? velocityOnDemand.disable(profileId) : velocityOnDemand.enable(profileId)));
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
      setStep("");
    }
  };

  return <section className="velocity-on-demand">
    <h3>Start when a player joins</h3>
    <p>Scerite installs a Velocity proxy on this remote host. Authenticated players can connect to the usual address; the Paper or Purpur server starts when they join and stops after 5 minutes with no players. The proxy stays online.</p>
    <label>On-demand startup <span className="property-switch"><input type="checkbox" role="switch" checked={status?.enabled ?? false} disabled={!status || !canChange || busy} onChange={() => void change()} /><span aria-hidden="true" /></span></label>
    {!canChange && <p>Stop the server before changing this setting.</p>}
    {status?.enabled && <p>Public port: {status.publicPort}. Backend port: {status.backendPort} (local access only).</p>}
    {status?.enabled && <p>Velocity usage · CPU {usage?.cpuPercent != null ? `${usage.cpuPercent.toFixed(1)}%` : "Unavailable"} · RAM {usage?.memoryBytes != null ? `${(usage.memoryBytes / 1024 / 1024).toFixed(0)} MB` : "Unavailable"}</p>}
    {status?.enabled && !status.proxyRunning && <p className="form-error" role="alert">The Velocity proxy is not running. Players cannot join until it is restarted.</p>}
    {busy && <p role="status">{step}…</p>}
    {error && <p className="form-error" role="alert">{error}</p>}
  </section>;
}
