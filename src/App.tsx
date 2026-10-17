import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { FormEvent } from "react";
import { HardDrive, Network, Plus, X } from "lucide-react";
import { serverProfiles } from "./lib/serverProfiles";
import type { ServerProfile, ServerTarget } from "./lib/serverProfiles";
import { serverSessions } from "./lib/serverSessions";
import type { LocalServerStatus, RemoteServerStatus } from "./lib/serverSessions";
import { LocalFilesPanel } from "./components/LocalFilesPanel";
import "./App.css";

type ProfileDraft = {
  name: string;
  kind: "local" | "remote";
  directory: string;
  executable: string;
  arguments: string;
  host: string;
  port: string;
  username: string;
  identityFile: string;
};

const emptyDraft: ProfileDraft = {
  name: "",
  kind: "local",
  directory: "",
  executable: "",
  arguments: "",
  host: "",
  port: "22",
  username: "",
  identityFile: "",
};

function targetFor(draft: ProfileDraft): ServerTarget {
  if (draft.kind === "remote") {
    return {
      kind: "remote",
      host: draft.host.trim(),
      port: Number(draft.port),
      username: draft.username.trim(),
      identityFile: draft.identityFile.trim() || null,
      directory: draft.directory.trim(),
      launch: {
        executable: draft.executable.trim(),
        arguments: draft.arguments.split(/\r?\n/).filter((argument) => argument.trim().length > 0),
      },
    };
  }

  return {
    kind: "local",
    directory: draft.directory,
    launch: {
      executable: draft.executable.trim(),
      arguments: draft.arguments.split(/\r?\n/).filter((argument) => argument.trim().length > 0),
    },
  };
}

function draftFor(profile: ServerProfile): ProfileDraft {
  if (profile.target.kind === "remote") {
    return {
      ...emptyDraft,
      name: profile.name,
      kind: "remote",
      host: profile.target.host,
      port: String(profile.target.port),
      username: profile.target.username,
      identityFile: profile.target.identityFile ?? "",
      directory: profile.target.directory,
      executable: profile.target.launch.executable,
      arguments: profile.target.launch.arguments.join("\n"),
    };
  }

  return {
    ...emptyDraft,
    name: profile.name,
    directory: profile.target.directory,
    executable: profile.target.launch.executable,
    arguments: profile.target.launch.arguments.join("\n"),
  };
}

function errorText(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function App() {
  const [profiles, setProfiles] = useState<ServerProfile[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [sessionError, setSessionError] = useState<string | null>(null);
  const [statuses, setStatuses] = useState<Record<string, LocalServerStatus>>({});
  const [remoteStatuses, setRemoteStatuses] = useState<Record<string, RemoteServerStatus | { state: "unreachable" }>>({});
  const [busySessionId, setBusySessionId] = useState<string | null>(null);
  const [connectedRemotes, setConnectedRemotes] = useState<Record<string, boolean>>({});
  const [consoleProfileId, setConsoleProfileId] = useState<string | null>(null);
  const [filesProfileId, setFilesProfileId] = useState<string | null>(null);
  const [consoleOutput, setConsoleOutput] = useState("");
  const [consoleInput, setConsoleInput] = useState("");
  const consoleCursor = useRef(0);
  const consoleOutputRef = useRef<HTMLDivElement>(null);
  const [formError, setFormError] = useState<string | null>(null);
  const [isFormOpen, setIsFormOpen] = useState(false);
  const [editingProfile, setEditingProfile] = useState<ServerProfile | null>(null);
  const [draft, setDraft] = useState<ProfileDraft>(emptyDraft);
  const [isSaving, setIsSaving] = useState(false);
  const [removingId, setRemovingId] = useState<string | null>(null);
  const [isRemoving, setIsRemoving] = useState(false);

  async function loadProfiles() {
    setIsLoading(true);
    setLoadError(null);
    try {
      setProfiles(await serverProfiles.list());
    } catch (error) {
      setLoadError(errorText(error));
    } finally {
      setIsLoading(false);
    }
  }

  useEffect(() => {
    void loadProfiles();
  }, []);

  useEffect(() => {
    const localProfiles = profiles.filter((profile) => profile.target.kind === "local");
    if (localProfiles.length === 0) return;

    let active = true;
    const refresh = async () => {
      const results = await Promise.allSettled(
        localProfiles.map(async (profile) => [profile.id, await serverSessions.status(profile.id)] as const),
      );
      if (!active) return;
      setStatuses((current) => {
        const next = { ...current };
        results.forEach((result) => {
          if (result.status === "fulfilled") next[result.value[0]] = result.value[1];
        });
        return next;
      });
      const failed = results.find((result) => result.status === "rejected");
      if (failed?.status === "rejected") setSessionError(errorText(failed.reason));
    };

    void refresh();
    const timer = window.setInterval(() => void refresh(), 2000);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, [profiles]);

  async function controlServer(profileId: string, action: "start" | "stop") {
    setBusySessionId(profileId);
    setSessionError(null);
    try {
      if (action === "start" && consoleProfileId === profileId) {
        consoleCursor.current = 0;
        setConsoleOutput("");
      }
      const status = await serverSessions[action](profileId);
      setStatuses((current) => ({ ...current, [profileId]: status }));
    } catch (error) {
      setSessionError(errorText(error));
    } finally {
      setBusySessionId(null);
    }
  }

  async function testRemoteConnection(profileId: string) {
    setBusySessionId(profileId);
    setSessionError(null);
    setConnectedRemotes((current) => ({ ...current, [profileId]: false }));
    try {
      await invoke<void>("test_remote_connection", { profileId });
      setConnectedRemotes((current) => ({ ...current, [profileId]: true }));
      await refreshRemoteStatus(profileId);
    } catch (error) {
      setSessionError(errorText(error));
      setRemoteStatuses((current) => ({ ...current, [profileId]: { state: "unreachable" } }));
    } finally {
      setBusySessionId(null);
    }
  }

  async function refreshRemoteStatus(profileId: string) {
    try {
      const status = await serverSessions.remoteStatus(profileId);
      setRemoteStatuses((current) => ({ ...current, [profileId]: status }));
      setConnectedRemotes((current) => ({ ...current, [profileId]: true }));
      return status;
    } catch (error) {
      setRemoteStatuses((current) => ({ ...current, [profileId]: { state: "unreachable" } }));
      setConnectedRemotes((current) => ({ ...current, [profileId]: false }));
      throw error;
    }
  }

  function toggleConsole(profile: ServerProfile) {
    if (consoleProfileId === profile.id) {
      setConsoleProfileId(null);
      return;
    }
    setConsoleInput("");
    setConsoleProfileId(profile.id);
    if (profile.target.kind === "remote") {
      void refreshRemoteStatus(profile.id).catch((error) => setSessionError(errorText(error)));
    }
  }

  function toggleFiles(profile: ServerProfile) {
    if (filesProfileId === profile.id) {
      setFilesProfileId(null);
      return;
    }
    setFilesProfileId(profile.id);
    if (profile.target.kind === "remote") {
      void refreshRemoteStatus(profile.id).catch((error) => setSessionError(errorText(error)));
    }
  }

  async function controlRemoteServer(profileId: string, action: "start" | "stop") {
    setBusySessionId(profileId);
    setSessionError(null);
    try {
      if (action === "start" && consoleProfileId === profileId) setConsoleOutput("");
      const status = await serverSessions[action === "start" ? "startRemote" : "stopRemote"](profileId);
      setRemoteStatuses((current) => ({ ...current, [profileId]: status }));
      setConnectedRemotes((current) => ({ ...current, [profileId]: true }));
    } catch (error) {
      setSessionError(errorText(error));
      setRemoteStatuses((current) => ({ ...current, [profileId]: { state: "unreachable" } }));
    } finally {
      setBusySessionId(null);
    }
  }

  const remoteMonitorIds = profiles
    .filter((profile) => profile.target.kind === "remote" && ["running", "stopping"].includes(remoteStatuses[profile.id]?.state ?? ""))
    .map((profile) => profile.id);
  const remoteMonitorKey = remoteMonitorIds.join("|");
  const consoleRemoteState = consoleProfileId ? remoteStatuses[consoleProfileId]?.state : undefined;

  useEffect(() => {
    if (!remoteMonitorKey) return;
    let active = true;
    const refresh = async () => {
      const results = await Promise.allSettled(remoteMonitorIds.map(async (id) => [id, await serverSessions.remoteStatus(id)] as const));
      if (!active) return;
      setRemoteStatuses((current) => {
        let changed = false;
        const next = { ...current };
        for (const result of results) {
          if (result.status === "fulfilled") {
            const [id, status] = result.value;
            if (next[id]?.state !== status.state) { next[id] = status; changed = true; }
          } else {
            const id = remoteMonitorIds[results.indexOf(result)];
            if (id && next[id]?.state !== "unreachable") { next[id] = { state: "unreachable" }; changed = true; }
          }
        }
        return changed ? next : current;
      });
      results.forEach((result, index) => {
        if (result.status === "rejected") {
          const id = remoteMonitorIds[index];
          if (id) setConnectedRemotes((current) => current[id] ? { ...current, [id]: false } : current);
        }
      });
    };
    void refresh();
    const timer = window.setInterval(() => void refresh(), 4000);
    return () => { active = false; window.clearInterval(timer); };
  }, [remoteMonitorKey]);

  useEffect(() => {
    if (!consoleProfileId) return;
    const profile = profiles.find((item) => item.id === consoleProfileId);
    if (profile?.target.kind === "remote") {
      if (consoleRemoteState !== "running" && consoleRemoteState !== "stopping") {
        setConsoleOutput(consoleRemoteState === "stopped" ? "Server is stopped." : "SSH status is unavailable.");
        return;
      }
      let active = true;
      let timer: number | undefined;
      const refresh = async () => {
        try {
          const snapshot = await serverSessions.remoteConsole(consoleProfileId);
          if (active) setConsoleOutput(snapshot.lines.slice(-1000).join("\n"));
        } catch (error) {
          if (active) {
            setSessionError(errorText(error));
            setRemoteStatuses((current) => ({ ...current, [consoleProfileId]: { state: "unreachable" } }));
            setConnectedRemotes((current) => ({ ...current, [consoleProfileId]: false }));
          }
        } finally {
          if (active) timer = window.setTimeout(() => void refresh(), 2000);
        }
      };
      setConsoleOutput("");
      void refresh();
      return () => { active = false; if (timer !== undefined) window.clearTimeout(timer); };
    }
    let active = true;
    let timer: number | undefined;

    const refresh = async () => {
      try {
        const snapshot = await serverSessions.console(consoleProfileId, consoleCursor.current);
        if (!active) return;
        consoleCursor.current = snapshot.nextSequence - 1;
        if (snapshot.lines.length) {
          setConsoleOutput((current) => [...current.split("\n"), ...snapshot.lines.map((line) => line.text)].slice(-1000).join("\n"));
        }
      } catch (error) {
        if (active) setSessionError(errorText(error));
      } finally {
        if (active) timer = window.setTimeout(() => void refresh(), 500);
      }
    };

    consoleCursor.current = 0;
    setConsoleOutput("");
    void refresh();
    return () => {
      active = false;
      if (timer !== undefined) window.clearTimeout(timer);
    };
  }, [consoleProfileId, profiles, consoleRemoteState]);

  useEffect(() => {
    if (consoleOutputRef.current) consoleOutputRef.current.scrollTop = consoleOutputRef.current.scrollHeight;
  }, [consoleOutput]);

  async function sendConsoleInput(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!consoleProfileId || !consoleInput.trim()) return;
    setSessionError(null);
    try {
      const profile = profiles.find((item) => item.id === consoleProfileId);
      if (profile?.target.kind === "remote") await serverSessions.sendRemoteInput(consoleProfileId, consoleInput.trim());
      else await serverSessions.sendInput(consoleProfileId, consoleInput.trim());
      setConsoleInput("");
    } catch (error) {
      setSessionError(errorText(error));
    }
  }

  function openNewProfile() {
    setEditingProfile(null);
    setDraft(emptyDraft);
    setFormError(null);
    setIsFormOpen(true);
  }

  function openEditProfile(profile: ServerProfile) {
    setEditingProfile(profile);
    setDraft(draftFor(profile));
    setFormError(null);
    setIsFormOpen(true);
    setRemovingId(null);
  }

  function closeForm() {
    setIsFormOpen(false);
    setEditingProfile(null);
    setFormError(null);
  }

  async function saveProfile(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setFormError(null);
    setIsSaving(true);

    try {
      const saved = editingProfile
        ? await serverProfiles.update(editingProfile.id, draft.name, targetFor(draft))
        : await serverProfiles.create(draft.name, targetFor(draft));

      setConnectedRemotes((current) => {
        const next = { ...current };
        delete next[saved.id];
        return next;
      });

      setProfiles((current) =>
        editingProfile
          ? current.map((profile) => (profile.id === saved.id ? saved : profile))
          : [...current, saved],
      );
      setRemoteStatuses((current) => {
        const next = { ...current };
        delete next[saved.id];
        return next;
      });
      closeForm();
    } catch (error) {
      setFormError(errorText(error));
    } finally {
      setIsSaving(false);
    }
  }

  async function removeProfile(id: string) {
    setIsRemoving(true);
    setActionError(null);
    try {
      await serverProfiles.delete(id);
      setProfiles((current) => current.filter((profile) => profile.id !== id));
      setRemoteStatuses((current) => {
        const next = { ...current };
        delete next[id];
        return next;
      });
      setRemovingId(null);
    } catch (error) {
      setActionError(errorText(error));
    } finally {
      setIsRemoving(false);
    }
  }

  return (
    <main className="app-shell">
      <aside className="sidebar" aria-label="Main navigation">
        <a className="brand" href="#servers" aria-label="ServerCrafter home">
          <span className="brand-mark" aria-hidden="true">S</span>
          <span>ServerCrafter</span>
        </a>
        <nav className="primary-nav">
          <a className="nav-link nav-link-active" href="#servers" aria-current="page">
            <span className="nav-indicator" aria-hidden="true" />
            Servers
            <span className="nav-count">{profiles.length}</span>
          </a>
        </nav>
        <div className="sidebar-footer">
          <span className="connection-dot" aria-hidden="true" />
          <span>Desktop app</span>
        </div>
      </aside>

      <section className="workspace" id="servers" aria-labelledby="page-title">
        <header className="page-header">
          <div>
            <h1 id="page-title">Servers</h1>
            <p>Keep your local folders and remote machines organized.</p>
          </div>
          {!isFormOpen && (
            <button className="button button-primary" onClick={openNewProfile}>
              <Plus size={16} aria-hidden="true" /> Add server
            </button>
          )}
        </header>

        {isFormOpen && (
          <form className="profile-form" onSubmit={saveProfile} aria-labelledby="form-title">
            <div className="form-heading">
              <div>
                <h2 id="form-title">{editingProfile ? "Edit server" : "Add a server"}</h2>
                <p>Save a folder on this computer or an SSH connection.</p>
              </div>
              <button className="icon-button" type="button" onClick={closeForm} aria-label="Close form">
                <X size={18} aria-hidden="true" />
              </button>
            </div>

            {formError && <p className="form-error" role="alert">{formError}</p>}

            <div className="form-grid">
              <label className="field field-wide">
                <span>Server name</span>
                <input
                  autoFocus
                  maxLength={80}
                  value={draft.name}
                  onChange={(event) => setDraft({ ...draft, name: event.currentTarget.value })}
                  placeholder="Survival world"
                  required
                />
              </label>

              <fieldset className="field field-wide target-choice">
                <legend>Where is the server?</legend>
                <label className={draft.kind === "local" ? "choice choice-selected" : "choice"}>
                  <input
                    type="radio"
                    name="target-kind"
                    value="local"
                    checked={draft.kind === "local"}
                    onChange={() => setDraft({ ...draft, kind: "local" })}
                  />
                  <span>On this computer</span>
                </label>
                <label className={draft.kind === "remote" ? "choice choice-selected" : "choice"}>
                  <input
                    type="radio"
                    name="target-kind"
                    value="remote"
                    checked={draft.kind === "remote"}
                    onChange={() => setDraft({ ...draft, kind: "remote" })}
                  />
                  <span>Remote with SSH</span>
                </label>
              </fieldset>

              {draft.kind === "local" ? (
                <>
                  <label className="field field-wide">
                    <span>Server folder</span>
                    <input
                      value={draft.directory}
                      onChange={(event) => setDraft({ ...draft, directory: event.currentTarget.value })}
                      placeholder="C:\\Minecraft\\Survival"
                      required
                    />
                    <small>Enter the folder that contains the server files.</small>
                  </label>
                  <label className="field field-wide">
                    <span>Launch executable</span>
                    <input
                      value={draft.executable}
                      onChange={(event) => setDraft({ ...draft, executable: event.currentTarget.value })}
                      placeholder="java"
                      required
                    />
                  </label>
                  <label className="field field-wide">
                    <span>Arguments</span>
                    <textarea
                      value={draft.arguments}
                      onChange={(event) => setDraft({ ...draft, arguments: event.currentTarget.value })}
                      rows={4}
                      placeholder={'-Xms2G\n-jar\nserver.jar\nnogui'}
                    />
                    <small>Enter one argument per line. Avoid passwords or tokens; arguments are saved with the profile.</small>
                  </label>
                </>
              ) : (
                <>
                  <label className="field field-wide">
                    <span>Host name or address</span>
                    <input
                      value={draft.host}
                      onChange={(event) => setDraft({ ...draft, host: event.currentTarget.value })}
                      placeholder="server.example.com"
                      required
                    />
                  </label>
                  <label className="field">
                    <span>SSH user</span>
                    <input
                      value={draft.username}
                      onChange={(event) => setDraft({ ...draft, username: event.currentTarget.value })}
                      placeholder="minecraft"
                      required
                    />
                  </label>
                  <label className="field">
                    <span>SSH port</span>
                    <input
                      type="number"
                      min={1}
                      max={65535}
                      value={draft.port}
                      onChange={(event) => setDraft({ ...draft, port: event.currentTarget.value })}
                      required
                    />
                  </label>
                  <label className="field field-wide">
                    <span>SSH identity file <small>(optional)</small></span>
                    <input
                      value={draft.identityFile}
                      onChange={(event) => setDraft({ ...draft, identityFile: event.currentTarget.value })}
                      placeholder="C:\\Users\\you\\.ssh\\id_ed25519"
                    />
                    <small>Leave blank to use your system SSH agent and default keys. Encrypted keys must be loaded into the agent. Host keys must already be trusted in known_hosts.</small>
                  </label>
                  <label className="field field-wide">
                    <span>Remote server folder <small>(needed for start and files)</small></span>
                    <input
                      value={draft.directory}
                      onChange={(event) => setDraft({ ...draft, directory: event.currentTarget.value })}
                      placeholder="/home/minecraft/survival"
                    />
                    <small>Use an absolute Linux path. File access stays inside this folder.</small>
                  </label>
                  <label className="field field-wide">
                    <span>Launch executable <small>(needed to start)</small></span>
                    <input
                      value={draft.executable}
                      onChange={(event) => setDraft({ ...draft, executable: event.currentTarget.value })}
                      placeholder="java"
                    />
                  </label>
                  <label className="field field-wide">
                    <span>Launch arguments</span>
                    <textarea
                      value={draft.arguments}
                      onChange={(event) => setDraft({ ...draft, arguments: event.currentTarget.value })}
                      rows={4}
                      placeholder={'-Xms2G\n-jar\nserver.jar\nnogui'}
                    />
                    <small>Enter one argument per line. Avoid passwords or tokens; arguments are saved with the profile.</small>
                  </label>
                </>
              )}
            </div>

            <div className="form-actions">
              <button className="button button-quiet" type="button" onClick={closeForm} disabled={isSaving}>
                Cancel
              </button>
              <button className="button button-primary" type="submit" disabled={isSaving}>
                {isSaving ? "Saving…" : editingProfile ? "Save changes" : "Save server"}
              </button>
            </div>
          </form>
        )}

        {loadError && profiles.length > 0 && (
          <div className="notice notice-error" role="alert">
            <div>
              <strong>Could not refresh saved servers.</strong>
              <span>{loadError}</span>
            </div>
            <button className="button button-quiet" onClick={() => void loadProfiles()}>Try again</button>
          </div>
        )}

        {actionError && (
          <div className="notice notice-error" role="alert">
            <div>
              <strong>Could not remove the saved profile.</strong>
              <span>{actionError}</span>
            </div>
            <button className="button button-quiet" onClick={() => setActionError(null)}>Dismiss</button>
          </div>
        )}

        {sessionError && (
          <div className="notice notice-error" role="alert">
            <div><strong>Could not complete the server action.</strong><span>{sessionError}</span></div>
            <button className="button button-quiet" onClick={() => setSessionError(null)}>Dismiss</button>
          </div>
        )}

        <div className="list-heading">
          <h2>Saved servers</h2>
          {!isLoading && <span>{profiles.length} {profiles.length === 1 ? "server" : "servers"}</span>}
        </div>

        {isLoading ? (
          <p className="state-message" role="status">Loading saved servers…</p>
        ) : loadError && profiles.length === 0 ? (
          <div className="empty-state">
            <h3>Saved servers could not be loaded</h3>
            <p>{loadError}</p>
            <button className="button button-secondary" onClick={() => void loadProfiles()}>Try again</button>
          </div>
        ) : profiles.length === 0 ? (
          <div className="empty-state">
            <div className="empty-mark" aria-hidden="true"><span /></div>
            <h3>No servers saved</h3>
            <p>Add a local or remote profile to get started.</p>
            {!isFormOpen && <button className="button button-secondary" onClick={openNewProfile}>Add your first server</button>}
          </div>
        ) : (
          <div className="server-list" aria-label="Saved servers">
            {profiles.map((profile) => {
              const target = profile.target;
              const isLocal = target.kind === "local";
              const location = target.kind === "local"
                ? target.directory
                : `${target.username}@${target.host}:${target.port}${target.directory ? ` · ${target.directory}` : ""}`;
              const status = isLocal ? statuses[profile.id] : undefined;
              const remoteStatus = !isLocal ? remoteStatuses[profile.id] : undefined;
              const state = status?.state ?? "checking";
              const remoteState = remoteStatus?.state;
              const serverRunning = isLocal ? state === "running" : remoteState === "running";
              const remoteStateLabel = remoteState === "unreachable"
                ? "SSH status unavailable"
                : remoteState === "unmanaged"
                  ? "Java process detected outside ServerCrafter"
                : remoteStatus && !remoteStatus.tmuxAvailable
                  ? "SSH reachable · tmux required"
                  : remoteState ? `SSH reachable · ${remoteState}` : "SSH reachable";
              const remoteStateTone = remoteState === "running" ? "running" : remoteState === "stopped" && remoteStatus?.tmuxAvailable ? "stopped" : "stopping";
              const isBusy = busySessionId === profile.id;

              return (
                <article className="server-row" key={profile.id}>
                  <span className={isLocal ? "server-icon server-icon-local" : "server-icon server-icon-remote"} aria-hidden="true">
                    {isLocal ? <HardDrive size={18} /> : <Network size={18} />}
                  </span>
                  <div className="server-main">
                    <div className="server-name-line">
                      <h3>{profile.name}</h3>
                      <span className={isLocal ? "target-badge target-badge-local" : "target-badge target-badge-remote"}>
                        {isLocal ? "Local" : "Remote"}
                      </span>
                    </div>
                    <p title={location}>{location}</p>
                    {isLocal && <span className={`session-state session-state-${state}`}>{status ? state : "Checking status…"}</span>}
                    {!isLocal && connectedRemotes[profile.id] && <span className={`session-state session-state-${remoteStateTone}`}>{remoteStateLabel}</span>}
                  </div>
                  {removingId === profile.id ? (
                    <div className="remove-confirm" role="group" aria-label={`Remove ${profile.name}`}>
                      <span>Remove profile? Server files stay untouched.</span>
                      <button className="button button-danger-quiet" onClick={() => void removeProfile(profile.id)} disabled={isRemoving}>
                        {isRemoving ? "Removing…" : "Remove"}
                      </button>
                      <button className="button button-quiet" onClick={() => setRemovingId(null)} disabled={isRemoving}>Cancel</button>
                    </div>
                  ) : (
                    <div className="row-actions">
                      {isLocal && (!status || state === "stopped") && <button className="button button-primary" onClick={() => void controlServer(profile.id, "start")} disabled={isBusy || !target.launch.executable.trim()}>{isBusy ? "Starting…" : "Start"}</button>}
                      {isLocal && state === "running" && <button className="button button-secondary" onClick={() => void controlServer(profile.id, "stop")} disabled={isBusy}>{isBusy ? "Stopping…" : "Stop"}</button>}
                      {isLocal && state === "stopping" && <button className="button button-secondary" disabled>Stopping…</button>}
                      {isLocal && <button className="button button-quiet" onClick={() => toggleConsole(profile)}>{consoleProfileId === profile.id ? "Close console" : "Console"}</button>}
                      {isLocal && <button className="button button-quiet" onClick={() => toggleFiles(profile)}>{filesProfileId === profile.id ? "Close files" : "Files"}</button>}
                      {!isLocal && <button className="button button-secondary" onClick={() => void testRemoteConnection(profile.id)} disabled={isBusy}>{isBusy ? "Connecting…" : "Test SSH"}</button>}
                      {!isLocal && (!remoteState || remoteState === "stopped" || remoteState === "unreachable") && <button className="button button-primary" onClick={() => void controlRemoteServer(profile.id, "start")} disabled={isBusy || !target.directory || !target.launch.executable.trim() || remoteState === "stopped" && !remoteStatus?.tmuxAvailable} title={!target.directory || !target.launch.executable.trim() ? "Set the remote folder and launch executable in the profile." : remoteState === "stopped" && !remoteStatus?.tmuxAvailable ? "Install tmux on the remote server to start it." : undefined}>{isBusy ? "Starting…" : "Start"}</button>}
                      {!isLocal && remoteState === "running" && <button className="button button-secondary" onClick={() => void controlRemoteServer(profile.id, "stop")} disabled={isBusy}>{isBusy ? "Stopping…" : "Stop"}</button>}
                      {!isLocal && remoteState === "stopping" && <button className="button button-secondary" disabled>Stopping…</button>}
                      {!isLocal && <button className="button button-quiet" onClick={() => toggleConsole(profile)}>{consoleProfileId === profile.id ? "Close console" : "Console"}</button>}
                      {!isLocal && <button className="button button-quiet" onClick={() => toggleFiles(profile)}>{filesProfileId === profile.id ? "Close files" : "Files"}</button>}
                      <button className="button button-quiet" onClick={() => openEditProfile(profile)} disabled={!isLocal && (remoteState === "running" || remoteState === "stopping" || remoteState === "unmanaged")} title={!isLocal && (remoteState === "running" || remoteState === "stopping" || remoteState === "unmanaged") ? "Stop the server before changing its connection or launch settings." : undefined}>Edit</button>
                      <button className="button button-quiet" onClick={() => setRemovingId(profile.id)} disabled={isLocal ? state !== "stopped" : remoteState === "running" || remoteState === "stopping" || remoteState === "unmanaged"} title={(isLocal && state !== "stopped") || remoteState === "running" || remoteState === "stopping" || remoteState === "unmanaged" ? "Stop the server before removing its profile." : undefined}>Remove</button>
                    </div>
                  )}
                  {consoleProfileId === profile.id && (
                    <section className="console-panel" aria-label={`${profile.name} console`}>
                      <div className="console-output" ref={consoleOutputRef} role="log" aria-live="polite">
                        <pre>{consoleOutput || "Console output will appear here."}</pre>
                      </div>
                      <form className="console-input-row" onSubmit={(event) => void sendConsoleInput(event)}>
                        <span aria-hidden="true">&gt;</span>
                        <input aria-label="Console command" value={consoleInput} onChange={(event) => setConsoleInput(event.currentTarget.value)} placeholder="Enter a server command" disabled={!serverRunning || isBusy} />
                        <button className="button button-primary" type="submit" disabled={!serverRunning || isBusy || !consoleInput.trim()}>Send</button>
                      </form>
                    </section>
                  )}
                  {filesProfileId === profile.id && (
                    <LocalFilesPanel profileId={profile.id} serverName={profile.name} canEdit={isLocal ? status?.state === "stopped" : remoteState === "stopped" && !!remoteStatus?.tmuxAvailable} remote={!isLocal} onClose={() => setFilesProfileId(null)} />
                  )}
                </article>
              );
            })}
          </div>
        )}
      </section>
    </main>
  );
}

export default App;
