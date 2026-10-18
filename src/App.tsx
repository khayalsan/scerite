import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { Link, useLocation, useNavigate } from "react-router-dom";
import { invoke } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import type { FormEvent } from "react";
import { ArrowLeft, FileKey, FolderOpen, HardDrive, Network, Palette, Plus, Settings, SlidersHorizontal, Terminal, X } from "lucide-react";
import { serverEnvironments, serverProfiles } from "./lib/serverProfiles";
import type { ServerEnvironment, ServerProfile, ServerTarget } from "./lib/serverProfiles";
import { serverSessions } from "./lib/serverSessions";
import type { LocalServerStatus, RemoteServerStatus } from "./lib/serverSessions";
import { LocalFilesPanel } from "./components/LocalFilesPanel";
import { ServerPropertiesPanel } from "./components/ServerPropertiesPanel";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "./components/ui/select";
import "./App.css";

type ProfileDraft = {
  name: string;
  kind: "local" | "remote";
  directory: string;
  executable: string;
  arguments: string;
  environmentId: string;
};

type EnvironmentDraft = {
  name: string;
  host: string;
  port: string;
  username: string;
  identityFile: string;
};

const themes = [
  { id: "violet-night", label: "Violet Night" },
  { id: "catppuccin-mocha", label: "Catppuccin Mocha" },
  { id: "catppuccin-latte", label: "Catppuccin Latte" },
  { id: "nord", label: "Nord" },
  { id: "dracula", label: "Dracula" },
  { id: "rose-pine", label: "Rosé Pine" },
] as const;

type ThemeId = (typeof themes)[number]["id"];

const themeStorageKey = "scerite.theme";

function readSavedTheme(): ThemeId {
  try {
    const savedTheme = window.localStorage.getItem(themeStorageKey);
    return themes.find((theme) => theme.id === savedTheme)?.id ?? "violet-night";
  } catch {
    return "violet-night";
  }
}

type ServerTab = "console" | "files" | "properties" | "settings";

const emptyDraft: ProfileDraft = {
  name: "",
  kind: "local",
  directory: "",
  executable: "",
  arguments: "",
  environmentId: "",
};

const emptyEnvironmentDraft: EnvironmentDraft = {
  name: "",
  host: "",
  port: "22",
  username: "",
  identityFile: "",
};

function parseLaunchArguments(value: string): string[] {
  const launchArguments: string[] = [];
  let argument = "";
  let quote: "'" | '"' | null = null;
  let tokenStarted = false;

  for (const character of value) {
    if (quote) {
      if (character === quote) quote = null;
      else argument += character;
      tokenStarted = true;
    } else if (character === "'" || character === '"') {
      quote = character;
      tokenStarted = true;
    } else if (/\s/.test(character)) {
      if (tokenStarted) {
        launchArguments.push(argument);
        argument = "";
        tokenStarted = false;
      }
    } else {
      argument += character;
      tokenStarted = true;
    }
  }
  if (tokenStarted) launchArguments.push(argument);

  return launchArguments;
}

function targetFor(draft: ProfileDraft): ServerTarget {
  const launchArguments = parseLaunchArguments(draft.arguments);

  if (draft.kind === "remote") {
    return {
      kind: "remote",
      environmentId: draft.environmentId,
      directory: draft.directory.trim(),
      launch: {
        executable: draft.executable.trim(),
        arguments: launchArguments,
      },
    };
  }

  return {
    kind: "local",
    directory: draft.directory,
    launch: {
      executable: draft.executable.trim(),
      arguments: launchArguments,
    },
  };
}

function draftFor(profile: ServerProfile): ProfileDraft {
  if (profile.target.kind === "remote") {
    return {
      ...emptyDraft,
      name: profile.name,
      kind: "remote",
      environmentId: profile.target.environmentId,
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
  const [theme, setTheme] = useState<ThemeId>(readSavedTheme);
  const navigate = useNavigate();
  const { pathname } = useLocation();
  const selectedProfileId = pathname.startsWith("/servers/") ? pathname.slice("/servers/".length) : null;
  const [profiles, setProfiles] = useState<ServerProfile[]>([]);
  const [environments, setEnvironments] = useState<ServerEnvironment[]>([]);
  const [environmentFilterId, setEnvironmentFilterId] = useState<string | null>(null);
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
  const [propertiesProfileId, setPropertiesProfileId] = useState<string | null>(null);
  const [consoleOutput, setConsoleOutput] = useState("");
  const [consoleInput, setConsoleInput] = useState("");
  const consoleCursor = useRef(0);
  const consoleOutputRef = useRef<HTMLDivElement>(null);
  const remoteStatusRouteRef = useRef<string | null>(null);
  const [formError, setFormError] = useState<string | null>(null);
  const [isFormOpen, setIsFormOpen] = useState(false);
  const [editingProfile, setEditingProfile] = useState<ServerProfile | null>(null);
  const [draft, setDraft] = useState<ProfileDraft>(emptyDraft);
  const [isSaving, setIsSaving] = useState(false);
  const [isTestingConnection, setIsTestingConnection] = useState(false);
  const [formConnectionResult, setFormConnectionResult] = useState<string | null>(null);
  const [formConnectionSucceeded, setFormConnectionSucceeded] = useState(false);
  const [isEnvironmentFormOpen, setIsEnvironmentFormOpen] = useState(false);
  const [editingEnvironment, setEditingEnvironment] = useState<ServerEnvironment | null>(null);
  const [environmentDraft, setEnvironmentDraft] = useState<EnvironmentDraft>(emptyEnvironmentDraft);
  const [environmentError, setEnvironmentError] = useState<string | null>(null);
  const [isSavingEnvironment, setIsSavingEnvironment] = useState(false);
  const [removingEnvironmentId, setRemovingEnvironmentId] = useState<string | null>(null);
  const [removingId, setRemovingId] = useState<string | null>(null);
  const [isRemoving, setIsRemoving] = useState(false);
  const [activeServerTab, setActiveServerTab] = useState<ServerTab>("console");
  const selectedProfile = profiles.find((profile) => profile.id === selectedProfileId);
  const visibleProfiles = !selectedProfileId && environmentFilterId
    ? profiles.filter((profile) => profile.target.kind === "remote" && profile.target.environmentId === environmentFilterId)
    : profiles;
  const filteredEnvironment = environments.find((environment) => environment.id === environmentFilterId);
  const profileRemovalDisabled = editingProfile
    ? editingProfile.target.kind === "local"
      ? statuses[editingProfile.id]?.state !== "stopped"
      : ["running", "stopping", "unmanaged"].includes(remoteStatuses[editingProfile.id]?.state ?? "")
    : true;

  useLayoutEffect(() => {
    const root = document.documentElement;
    root.dataset.theme = theme;
    root.classList.toggle("dark", theme !== "catppuccin-latte");
    try {
      window.localStorage.setItem(themeStorageKey, theme);
    } catch {
      // The selected theme still applies for this session if storage is unavailable.
    }
  }, [theme]);

  async function loadProfiles() {
    setIsLoading(true);
    setLoadError(null);
    try {
      const [savedProfiles, savedEnvironments] = await Promise.all([
        serverProfiles.list(),
        serverEnvironments.list(),
      ]);
      setProfiles(savedProfiles);
      setEnvironments(savedEnvironments);
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
    if (!selectedProfileId) {
      remoteStatusRouteRef.current = null;
      return;
    }
    const profile = profiles.find((item) => item.id === selectedProfileId);
    if (!profile || profile.target.kind !== "remote" || remoteStatusRouteRef.current === profile.id) return;
    remoteStatusRouteRef.current = profile.id;
    void refreshRemoteStatus(profile.id).catch((error) => setSessionError(errorText(error)));
  }, [selectedProfileId, profiles]);

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

  async function testEnvironmentDraftConnection() {
    setIsTestingConnection(true);
    setEnvironmentError(null);
    setFormConnectionResult(null);
    setFormConnectionSucceeded(false);
    try {
      await invoke<void>("test_remote_connection_draft", {
        host: environmentDraft.host.trim(),
        port: Number(environmentDraft.port),
        username: environmentDraft.username.trim(),
        identityFile: environmentDraft.identityFile.trim() || null,
      });
      setFormConnectionResult("SSH connection verified. The server fingerprint is trusted and authentication succeeded.");
      setFormConnectionSucceeded(true);
    } catch (error) {
      setFormConnectionResult(errorText(error));
      setFormConnectionSucceeded(false);
    } finally {
      setIsTestingConnection(false);
    }
  }

  async function selectLocalDirectory() {
    try {
      const selected = await openDialog({ directory: true, multiple: false, title: "Choose server folder" });
      if (typeof selected === "string") setDraft((current) => ({ ...current, directory: selected }));
    } catch (error) {
      setFormError(errorText(error));
    }
  }

  async function selectIdentityFile() {
    try {
      const selected = await openDialog({ multiple: false, title: "Choose SSH identity file" });
      if (typeof selected === "string") setEnvironmentDraft((current) => ({ ...current, identityFile: selected }));
    } catch (error) {
      setEnvironmentError(errorText(error));
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
    if (profile.target.kind === "remote" && !connectedRemotes[profile.id]) {
      void refreshRemoteStatus(profile.id).catch((error) => setSessionError(errorText(error)));
    }
  }

  function toggleFiles(profile: ServerProfile) {
    if (filesProfileId === profile.id) {
      setFilesProfileId(null);
      return;
    }
    setFilesProfileId(profile.id);
    if (profile.target.kind === "remote" && !connectedRemotes[profile.id]) {
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

  function openNewEnvironment(environment?: ServerEnvironment) {
    setEditingEnvironment(environment ?? null);
    setEnvironmentDraft(environment ? {
      name: environment.name,
      host: environment.host,
      port: String(environment.port),
      username: environment.username,
      identityFile: environment.identityFile ?? "",
    } : emptyEnvironmentDraft);
    setEnvironmentError(null);
    setFormConnectionResult(null);
    setFormConnectionSucceeded(false);
    setIsEnvironmentFormOpen(true);
  }

  function closeEnvironmentForm() {
    setIsEnvironmentFormOpen(false);
    setEditingEnvironment(null);
    setEnvironmentError(null);
    setFormConnectionResult(null);
    setFormConnectionSucceeded(false);
  }

  async function saveEnvironment(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setEnvironmentError(null);
    setIsSavingEnvironment(true);
    try {
      const saved = editingEnvironment
        ? await serverEnvironments.update(
          editingEnvironment.id,
          environmentDraft.name,
          environmentDraft.host,
          Number(environmentDraft.port),
          environmentDraft.username,
          environmentDraft.identityFile.trim() || null,
        )
        : await serverEnvironments.create(
          environmentDraft.name,
          environmentDraft.host,
          Number(environmentDraft.port),
          environmentDraft.username,
          environmentDraft.identityFile.trim() || null,
        );
      setEnvironments((current) => editingEnvironment
        ? current.map((environment) => environment.id === saved.id ? saved : environment)
        : [...current, saved]);
      setEnvironmentFilterId(saved.id);
      if (!editingEnvironment) {
        setDraft((current) => ({ ...current, kind: "remote", environmentId: saved.id }));
      }
      closeEnvironmentForm();
    } catch (error) {
      setEnvironmentError(errorText(error));
    } finally {
      setIsSavingEnvironment(false);
    }
  }

  async function removeEnvironment(id: string) {
    setEnvironmentError(null);
    try {
      await serverEnvironments.delete(id);
      setEnvironments((current) => current.filter((environment) => environment.id !== id));
      setRemovingEnvironmentId(null);
      if (environmentFilterId === id) setEnvironmentFilterId(null);
    } catch (error) {
      setEnvironmentError(errorText(error));
    }
  }

  function openNewProfile(environmentId?: string) {
    setEditingProfile(null);
    setDraft({ ...emptyDraft, kind: environmentId ? "remote" : "local", environmentId: environmentId ?? "" });
    setFormError(null);
    setFormConnectionResult(null);
    setFormConnectionSucceeded(false);
    setIsFormOpen(true);
  }

  function openEditProfile(profile: ServerProfile) {
    setEditingProfile(profile);
    setDraft(draftFor(profile));
    setFormError(null);
    setFormConnectionResult(null);
    setFormConnectionSucceeded(false);
    setIsFormOpen(true);
    setRemovingId(null);
  }

  function closeForm() {
    setIsFormOpen(false);
    setEditingProfile(null);
    setRemovingId(null);
    setFormError(null);
    setFormConnectionResult(null);
    setFormConnectionSucceeded(false);
  }

  async function saveProfile(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setFormError(null);
    if (draft.kind === "remote" && !draft.environmentId) {
      setFormError("Choose a remote environment.");
      return;
    }
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
      if (selectedProfileId === saved.id && editingProfile) {
        setEditingProfile(saved);
        setDraft(draftFor(saved));
      } else {
        closeForm();
      }
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
      if (editingProfile?.id === id) closeForm();
      if (selectedProfileId === id) navigate("/");
    } catch (error) {
      setActionError(errorText(error));
    } finally {
      setIsRemoving(false);
    }
  }

  function renderEnvironmentForm() {
    return (
      <form className="profile-form environment-form" onSubmit={saveEnvironment} aria-labelledby="environment-form-title">
        <div className="form-heading">
          <div>
            <h2 id="environment-form-title">{editingEnvironment ? "Edit environment" : "Add remote environment"}</h2>
            <p>Save SSH connection settings once, then use them for multiple servers.</p>
          </div>
        </div>
        {environmentError && <p className="form-error" role="alert">{environmentError}</p>}
        <div className="form-grid">
          <label className="field field-wide">
            <span>Environment name</span>
            <input autoFocus maxLength={80} value={environmentDraft.name} onChange={(event) => setEnvironmentDraft({ ...environmentDraft, name: event.currentTarget.value })} placeholder="Production server" required />
          </label>
          <label className="field field-wide">
            <span>Host name or address</span>
            <input value={environmentDraft.host} onChange={(event) => setEnvironmentDraft({ ...environmentDraft, host: event.currentTarget.value })} placeholder="server.example.com" required />
          </label>
          <label className="field">
            <span>SSH user</span>
            <input value={environmentDraft.username} onChange={(event) => setEnvironmentDraft({ ...environmentDraft, username: event.currentTarget.value })} placeholder="minecraft" required />
          </label>
          <label className="field">
            <span>SSH port</span>
            <input type="number" min={1} max={65535} value={environmentDraft.port} onChange={(event) => setEnvironmentDraft({ ...environmentDraft, port: event.currentTarget.value })} required />
          </label>
          <label className="field field-wide">
            <span>SSH identity file <small>(optional)</small></span>
            <div className="path-control">
              <input value={environmentDraft.identityFile} onChange={(event) => setEnvironmentDraft({ ...environmentDraft, identityFile: event.currentTarget.value })} placeholder="C:\\Users\\you\\.ssh\\id_ed25519" />
              <button className="button button-secondary" type="button" onClick={() => void selectIdentityFile()} aria-label="Browse for SSH identity file"><FileKey size={15} />Browse</button>
            </div>
            <small>Leave blank to use your system SSH agent and default keys. Encrypted keys must be loaded into the agent. Host keys must already be trusted in known_hosts.</small>
          </label>
        </div>
        <div className="form-actions">
          <button className="button button-secondary" type="button" onClick={() => void testEnvironmentDraftConnection()} disabled={isTestingConnection || isSavingEnvironment || !environmentDraft.host.trim() || !environmentDraft.username.trim() || !Number(environmentDraft.port)}>
            {isTestingConnection ? "Testing SSH…" : "Test SSH"}
          </button>
          <button className="button button-quiet" type="button" onClick={closeEnvironmentForm} disabled={isSavingEnvironment}>Cancel</button>
          <button className="button button-primary" type="submit" disabled={isSavingEnvironment}>
            {isSavingEnvironment ? "Saving…" : editingEnvironment ? "Save environment" : "Create environment"}
          </button>
        </div>
        {formConnectionResult && <p className={formConnectionSucceeded ? "form-connection-result form-connection-success" : "form-connection-result form-connection-error"} role="status">{formConnectionResult}</p>}
      </form>
    );
  }

  function renderProfileForm() {
    return (
          <form className="profile-form server-profile-edit-form" onSubmit={saveProfile} aria-labelledby="form-title">
            <div className="form-heading">
              <div>
                <h2 id="form-title">{editingProfile ? "Edit server" : "Add a server"}</h2>
                <p>Save a folder on this computer or an SSH connection.</p>
              </div>
              {!selectedProfileId && (
                <button className="icon-button" type="button" onClick={closeForm} aria-label="Close form">
                  <X size={18} aria-hidden="true" />
                </button>
              )}
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
                    <div className="path-control">
                      <input
                        value={draft.directory}
                        onChange={(event) => setDraft({ ...draft, directory: event.currentTarget.value })}
                        placeholder="C:\\Minecraft\\Survival"
                        required
                      />
                      <button className="button button-secondary" type="button" onClick={() => void selectLocalDirectory()} aria-label="Browse for server folder"><FolderOpen size={15} />Browse</button>
                    </div>
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
                    <small>Separate arguments with spaces or new lines. Quote values that contain spaces. Avoid passwords or tokens; arguments are saved with the profile.</small>
                  </label>
                </>
              ) : (
                <>
                  <label className="field field-wide">
                    <span>Remote environment</span>
                    <Select
                      value={draft.environmentId || null}
                      onValueChange={(environmentId) => setDraft({ ...draft, environmentId: environmentId ?? "" })}
                    >
                      <SelectTrigger aria-label="Remote environment" aria-required="true">
                        <SelectValue>
                          {(selected) => environments.find((environment) => environment.id === selected)?.name ?? "Choose an environment"}
                        </SelectValue>
                      </SelectTrigger>
                      <SelectContent>
                        {environments.map((environment) => (
                          <SelectItem key={environment.id} value={environment.id}>
                            {environment.name} · {environment.username}@{environment.host}:{environment.port}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                    <small>SSH connection settings are saved once per environment and shared by its servers.</small>
                  </label>
                  <button className="button button-secondary" type="button" onClick={() => openNewEnvironment()}>
                    <Plus size={15} aria-hidden="true" /> Add remote environment
                  </button>
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
                    <small>Separate arguments with spaces or new lines. Quote values that contain spaces. Avoid passwords or tokens; arguments are saved with the profile.</small>
                  </label>
                </>
              )}
            </div>

            <div className="form-actions">
              {!selectedProfileId && (
                <button className="button button-quiet" type="button" onClick={closeForm} disabled={isSaving}>
                  Cancel
                </button>
              )}
              {editingProfile && (
                <button className="button button-danger-quiet" type="button" onClick={() => setRemovingId(editingProfile.id)} disabled={isSaving || isRemoving || profileRemovalDisabled} title={profileRemovalDisabled ? "Stop the server before removing its profile." : undefined}>
                  Delete profile
                </button>
              )}
              <button className="button button-primary" type="submit" disabled={isSaving}>
                {isSaving ? "Saving…" : editingProfile ? "Save changes" : "Save server"}
              </button>
            </div>
            {editingProfile && removingId === editingProfile.id && (
              <div className="remove-confirm profile-form-remove-confirm" role="group" aria-label={`Delete ${editingProfile.name}`}>
                <span>Delete this profile? Server files will stay untouched.</span>
                <button className="button button-danger-quiet" type="button" onClick={() => void removeProfile(editingProfile.id)} disabled={isRemoving}>
                  {isRemoving ? "Deleting…" : "Delete profile"}
                </button>
                <button className="button button-quiet" type="button" onClick={() => setRemovingId(null)} disabled={isRemoving}>Cancel</button>
              </div>
            )}
          </form>
    );
  }

  return (
    <main className={`app-shell${selectedProfile ? " server-detail-shell" : ""}`}>
      {selectedProfile ? (() => {
        const target = selectedProfile.target;
        const isLocal = target.kind === "local";
        const environment = target.kind === "remote"
          ? environments.find((item) => item.id === target.environmentId)
          : undefined;
        const status = isLocal ? statuses[selectedProfile.id] : undefined;
        const remoteStatus = !isLocal ? remoteStatuses[selectedProfile.id] : undefined;
        const state = status?.state ?? "checking";
        const remoteState = remoteStatus?.state;
        const isBusy = busySessionId === selectedProfile.id;
        const isConnected = !!connectedRemotes[selectedProfile.id];
        const statusLabel = isLocal
          ? status ? state : "Checking status…"
          : !remoteStatus
            ? "Not checked"
            : remoteState === "unreachable"
              ? "SSH unavailable"
              : remoteState === "unmanaged"
                ? "Java process detected"
                : !remoteStatus.tmuxAvailable
                  ? "tmux required"
                  : remoteState;
        const statusTone = isLocal ? state : remoteState ?? "unknown";

        return (
          <aside className="sidebar server-page-sidebar" aria-label={`${selectedProfile.name} navigation`}>
            <Link className="brand" to="/" aria-label="Back to scerite servers" onClick={closeForm}>
              <span className="brand-mark" aria-hidden="true">S</span>
              <span>scerite</span>
            </Link>
            <div className="server-page-context">
              <button className="back-link" onClick={() => { closeForm(); navigate("/"); }}>
                <ArrowLeft size={15} aria-hidden="true" /> All servers
              </button>
              <h1 id="page-title">{selectedProfile.name}</h1>
              <p>{selectedProfile.target.kind === "local"
                ? selectedProfile.target.directory
                : environment
                  ? `${environment.name} · ${environment.username}@${environment.host}:${environment.port}`
                  : "Remote environment unavailable"}</p>
            </div>
            <section className="server-status-card" aria-label="Server status">
              <span className="server-status-label">Server status</span>
              <span className={`server-status-value server-status-${statusTone}`}>
                <span className="server-status-dot" aria-hidden="true" />
                {statusLabel}
              </span>
              <div className="server-detail-actions">
                {isLocal && (!status || state === "stopped") && <button className="button button-primary" onClick={() => void controlServer(selectedProfile.id, "start")} disabled={isBusy || !target.launch.executable.trim()}>{isBusy ? "Starting…" : "Start server"}</button>}
                {isLocal && state === "running" && <button className="button button-secondary" onClick={() => void controlServer(selectedProfile.id, "stop")} disabled={isBusy}>{isBusy ? "Stopping…" : "Stop server"}</button>}
                {isLocal && state === "stopping" && <button className="button button-secondary" disabled>Stopping…</button>}
                {!isLocal && <button className={`button ${isConnected ? "button-connected" : "button-secondary"}`} onClick={() => void testRemoteConnection(selectedProfile.id)} disabled={isBusy}>{isConnected ? "Connected" : isBusy ? "Connecting…" : "Connect"}</button>}
                {!isLocal && (!remoteState || remoteState === "stopped" || remoteState === "unreachable") && <button className="button button-primary" onClick={() => void controlRemoteServer(selectedProfile.id, "start")} disabled={isBusy || !target.directory || !target.launch.executable.trim() || remoteState === "stopped" && !remoteStatus?.tmuxAvailable} title={!target.directory || !target.launch.executable.trim() ? "Set the remote folder and launch executable in the profile." : remoteState === "stopped" && !remoteStatus?.tmuxAvailable ? "Install tmux on the remote server to start it." : undefined}>{isBusy ? "Starting…" : "Start server"}</button>}
                {!isLocal && remoteState === "running" && <button className="button button-secondary" onClick={() => void controlRemoteServer(selectedProfile.id, "stop")} disabled={isBusy}>{isBusy ? "Stopping…" : "Stop server"}</button>}
                {!isLocal && remoteState === "stopping" && <button className="button button-secondary" disabled>Stopping…</button>}
              </div>
            </section>
            <nav className="server-tab-list" aria-label="Server sections">
              <button className={`server-tab${activeServerTab === "console" ? " server-tab-active" : ""}`} type="button" aria-current={activeServerTab === "console" ? "page" : undefined} onClick={() => { setActiveServerTab("console"); if (consoleProfileId !== selectedProfile.id) toggleConsole(selectedProfile); }}>
                <Terminal size={16} aria-hidden="true" /> Console
              </button>
              <button className={`server-tab${activeServerTab === "files" ? " server-tab-active" : ""}`} type="button" aria-current={activeServerTab === "files" ? "page" : undefined} onClick={() => { setActiveServerTab("files"); if (filesProfileId !== selectedProfile.id) toggleFiles(selectedProfile); }}>
                <FolderOpen size={16} aria-hidden="true" /> Files
              </button>
              <button className={`server-tab${activeServerTab === "properties" ? " server-tab-active" : ""}`} type="button" aria-current={activeServerTab === "properties" ? "page" : undefined} onClick={() => { setActiveServerTab("properties"); setPropertiesProfileId(selectedProfile.id); }}>
                <SlidersHorizontal size={16} aria-hidden="true" /> Properties
              </button>
              <button className={`server-tab${activeServerTab === "settings" ? " server-tab-active" : ""}`} type="button" aria-current={activeServerTab === "settings" ? "page" : undefined} onClick={() => {
                setActiveServerTab("settings");
                if (isFormOpen && editingProfile?.id === selectedProfile.id) return;
                if (isLocal || !["running", "stopping", "unmanaged"].includes(remoteState ?? "")) {
                  openEditProfile(selectedProfile);
                }
              }}>
                <Settings size={16} aria-hidden="true" /> Settings
              </button>
            </nav>
          </aside>
        );
      })() : (
        <aside className="sidebar" aria-label="Main navigation">
          <Link className="brand" to="/" aria-label="scerite home">
            <span className="brand-mark" aria-hidden="true">S</span>
            <span>scerite</span>
          </Link>
          <nav className="primary-nav">
            <Link className="nav-link nav-link-active" to="/" aria-current="page">
              <span className="nav-indicator" aria-hidden="true" />
              Servers
              <span className="nav-count">{profiles.length}</span>
            </Link>
          </nav>
          <div className="sidebar-footer">
            <span className="connection-dot" aria-hidden="true" />
            <span>Desktop app</span>
          </div>
        </aside>
      )}

      <section
        className="workspace"
        id="servers"
        aria-labelledby={!selectedProfileId ? "page-title" : undefined}
        aria-label={selectedProfileId ? `${selectedProfile?.name ?? "Server"} workspace` : undefined}
      >
        {!selectedProfileId && <header className="page-header">
          <div>
            <h1 id="page-title">Servers</h1>
            <p>Manage local servers and group remote servers by environment.</p>
          </div>
          <div className="page-actions">
            <label className="theme-picker">
              <Palette size={16} aria-hidden="true" />
              <span>Theme</span>
              <Select
                value={theme}
                onValueChange={(value) => {
                  const selectedTheme = themes.find((option) => option.id === value);
                  if (selectedTheme) setTheme(selectedTheme.id);
                }}
              >
                <SelectTrigger aria-label="Color theme" className="theme-select-trigger">
                  <SelectValue>
                    {(selected) => themes.find((option) => option.id === selected)?.label ?? "Violet Night"}
                  </SelectValue>
                </SelectTrigger>
                <SelectContent>
                  {themes.map((option) => (
                    <SelectItem key={option.id} value={option.id}>{option.label}</SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </label>
            {!isEnvironmentFormOpen && (
              <button className="button button-secondary" onClick={() => openNewEnvironment()}>
                <Plus size={16} aria-hidden="true" /> Add environment
              </button>
            )}
            {!isFormOpen && !isEnvironmentFormOpen && (
              <button className="button button-primary" onClick={() => openNewProfile(filteredEnvironment?.id)}>
                <Plus size={16} aria-hidden="true" /> Add server
              </button>
            )}
          </div>
        </header>}

        {!selectedProfileId && isEnvironmentFormOpen && renderEnvironmentForm()}
        {!selectedProfileId && isFormOpen && !isEnvironmentFormOpen && renderProfileForm()}

        {!selectedProfileId && environments.length > 0 && (
          <section className="environment-section" aria-label="Remote environments">
            <div className="environment-section-heading">
              <h2>Remote environments</h2>
              {environmentFilterId && <button className="button button-quiet" onClick={() => setEnvironmentFilterId(null)}>Show all servers</button>}
            </div>
            {environmentError && !isEnvironmentFormOpen && <p className="form-error" role="alert">{environmentError}</p>}
            <div className="environment-grid">
              {environments.map((environment) => {
                const serverCount = profiles.filter((profile) => profile.target.kind === "remote" && profile.target.environmentId === environment.id).length;
                const isSelected = environmentFilterId === environment.id;
                return (
                  <article className={`environment-card${isSelected ? " environment-card-selected" : ""}`} key={environment.id}>
                    <button className="environment-card-main" type="button" onClick={() => setEnvironmentFilterId(isSelected ? null : environment.id)}>
                      <span className="environment-card-title">{environment.name}</span>
                      <span className="environment-card-address">{environment.username}@{environment.host}:{environment.port}</span>
                      <span className="environment-card-count">{serverCount} {serverCount === 1 ? "server" : "servers"}</span>
                    </button>
                    <div className="environment-card-actions">
                      <button className="button button-primary" type="button" onClick={() => { setEnvironmentFilterId(environment.id); openNewProfile(environment.id); }}>Add server</button>
                      <button className="button button-quiet" type="button" onClick={() => openNewEnvironment(environment)}>Edit</button>
                      <button className="button button-quiet" type="button" onClick={() => setRemovingEnvironmentId(environment.id)} disabled={serverCount > 0} title={serverCount > 0 ? "Remove its servers before deleting this environment." : undefined}>Remove</button>
                    </div>
                    {removingEnvironmentId === environment.id && (
                      <div className="remove-confirm" role="group" aria-label={`Remove ${environment.name}`}>
                        <span>Remove this empty environment?</span>
                        <button className="button button-danger-quiet" type="button" onClick={() => void removeEnvironment(environment.id)}>Remove environment</button>
                        <button className="button button-quiet" type="button" onClick={() => setRemovingEnvironmentId(null)}>Cancel</button>
                      </div>
                    )}
                  </article>
                );
              })}
            </div>
          </section>
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

        {!selectedProfileId && <div className="list-heading">
          <h2>{filteredEnvironment ? `${filteredEnvironment.name} servers` : "Saved servers"}</h2>
          {!isLoading && <span>{visibleProfiles.length} {visibleProfiles.length === 1 ? "server" : "servers"}</span>}
        </div>}

        {isLoading ? (
          <p className="state-message" role="status">Loading saved servers…</p>
        ) : loadError && profiles.length === 0 ? (
          <div className="empty-state">
            <h3>Saved servers could not be loaded</h3>
            <p>{loadError}</p>
            <button className="button button-secondary" onClick={() => void loadProfiles()}>Try again</button>
          </div>
        ) : selectedProfileId && !selectedProfile ? (
          <div className="empty-state">
            <h3>Server not found</h3>
            <p>This server may have been removed.</p>
            <button className="button button-secondary" onClick={() => navigate("/")}>Back to servers</button>
          </div>
        ) : profiles.length === 0 ? (
          <div className="empty-state">
            <div className="empty-mark" aria-hidden="true"><span /></div>
            <h3>{filteredEnvironment ? `No servers in ${filteredEnvironment.name}` : "No servers saved"}</h3>
            <p>{filteredEnvironment ? "Add a server to this environment." : "Add a local server or create a remote environment to get started."}</p>
            {!isFormOpen && <button className="button button-secondary" onClick={() => openNewProfile(filteredEnvironment?.id)}>{filteredEnvironment ? "Add server" : "Add your first server"}</button>}
          </div>
        ) : visibleProfiles.length === 0 ? (
          <div className="empty-state">
            <h3>No servers in {filteredEnvironment?.name ?? "this environment"}</h3>
            <p>Add a server to this environment.</p>
            {!isFormOpen && <button className="button button-secondary" onClick={() => filteredEnvironment && openNewProfile(filteredEnvironment.id)}>Add server</button>}
          </div>
        ) : (
          <div className="server-list" aria-label="Saved servers">
            {visibleProfiles.map((profile) => {
              const target = profile.target;
              const isLocal = target.kind === "local";
              const location = target.kind === "local"
                ? target.directory
                : `${environments.find((environment) => environment.id === target.environmentId)?.name ?? "Remote environment"}${target.directory ? ` · ${target.directory}` : ""}`;
              const status = isLocal ? statuses[profile.id] : undefined;
              const remoteStatus = !isLocal ? remoteStatuses[profile.id] : undefined;
              const state = status?.state ?? "checking";
              const remoteState = remoteStatus?.state;
              const serverRunning = isLocal ? state === "running" : remoteState === "running";
              const remoteStateLabel = remoteState === "unreachable"
                ? "SSH status unavailable"
                : remoteState === "unmanaged"
                  ? "Java process detected outside scerite"
                : remoteStatus && !remoteStatus.tmuxAvailable
                  ? "SSH reachable · tmux required"
                  : remoteState ? `SSH reachable · ${remoteState}` : "SSH reachable";
              const remoteStateTone = remoteState === "running" ? "running" : remoteState === "stopped" && remoteStatus?.tmuxAvailable ? "stopped" : "stopping";
              const isBusy = busySessionId === profile.id;
              const isDetailPage = selectedProfileId === profile.id;
              return (
                <article className={`server-row${selectedProfileId === profile.id ? " server-row-detail" : ""}`} key={profile.id} hidden={!!selectedProfileId && selectedProfileId !== profile.id}>
                  {!isDetailPage && (
                    <Link className="server-row-open" to={`/servers/${profile.id}`} onClick={() => setActiveServerTab("console")} aria-label={`Open ${profile.name}`}>
                      <span className={isLocal ? "server-icon server-icon-local" : "server-icon server-icon-remote"} aria-hidden="true">
                        {isLocal ? <HardDrive size={18} /> : <Network size={18} />}
                      </span>
                      <span className="server-main">
                        <span className="server-name-line">
                          <h3>{profile.name}</h3>
                          <span className={isLocal ? "target-badge target-badge-local" : "target-badge target-badge-remote"}>
                            {isLocal ? "Local" : "Remote"}
                          </span>
                        </span>
                        <span className="server-location" title={location}>{location}</span>
                        {isLocal && <span className={`session-state session-state-${state}`}>{status ? state : "Checking status…"}</span>}
                        {!isLocal && connectedRemotes[profile.id] && <span className={`session-state session-state-${remoteStateTone}`}>{remoteStateLabel}</span>}
                      </span>
                    </Link>
                  )}
                  {isDetailPage ? (
                    <div className="server-detail-content">
                      <div className="server-tab-content">
                        <section className="server-tab-panel" hidden={activeServerTab !== "console"} aria-label="Server console">
                          {consoleProfileId === profile.id ? (
                            <section className="console-panel" aria-label={`${profile.name} console`}>
                              <div className="console-toolbar">
                                <strong>Live console</strong>
                                <button className="button button-quiet" type="button" onClick={() => toggleConsole(profile)}>Disconnect</button>
                              </div>
                              <div className="console-output" ref={consoleOutputRef} role="log" aria-live="polite">
                                <pre>{consoleOutput || "Console output will appear here."}</pre>
                              </div>
                              <form className="console-input-row" onSubmit={(event) => void sendConsoleInput(event)}>
                                <span aria-hidden="true">&gt;</span>
                                <input aria-label="Console command" value={consoleInput} onChange={(event) => setConsoleInput(event.currentTarget.value)} placeholder="Enter a server command" disabled={!serverRunning || isBusy} />
                                <button className="button button-primary" type="submit" disabled={!serverRunning || isBusy || !consoleInput.trim()}>Send</button>
                              </form>
                            </section>
                          ) : (
                            <div className="server-tab-empty">
                              <h2>Console</h2>
                              <p>Connect to view server output and send commands.</p>
                              <button className="button button-primary" onClick={() => toggleConsole(profile)}>Connect console</button>
                            </div>
                          )}
                        </section>
                        <section className="server-tab-panel" hidden={activeServerTab !== "files"} aria-label="Server files">
                          {filesProfileId === profile.id ? (
                            <LocalFilesPanel profileId={profile.id} serverName={profile.name} canEdit={isLocal ? status?.state === "stopped" : remoteState === "stopped" && !!remoteStatus?.tmuxAvailable} remote={!isLocal} onClose={() => setFilesProfileId(null)} />
                          ) : (
                            <div className="server-tab-empty">
                              <h2>Files</h2>
                              <p>Browse and edit files in this server’s folder.</p>
                              <button className="button button-primary" onClick={() => toggleFiles(profile)}>Open files</button>
                            </div>
                          )}
                        </section>
                        <section className="server-tab-panel server-properties-tab" hidden={activeServerTab !== "properties"} aria-label="Server properties">
                          {propertiesProfileId === profile.id && (
                            <ServerPropertiesPanel
                              profileId={profile.id}
                              remote={!isLocal}
                              canEdit={isLocal ? status?.state === "stopped" : remoteState === "stopped"}
                            />
                          )}
                        </section>
                        <section className={`server-tab-panel server-settings-panel${isFormOpen && editingProfile?.id === profile.id ? " server-settings-editing" : ""}`} hidden={activeServerTab !== "settings"} aria-label="Server settings">
                          {isFormOpen && editingProfile?.id === profile.id ? renderProfileForm() : (
                            <div className="server-tab-empty">
                              <h2>Server settings</h2>
                              <p>Stop the server before editing its profile.</p>
                            </div>
                          )}
                        </section>
                      </div>
                    </div>
                  ) : (
                    <>
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
                          {!isLocal && <button className={`button ${connectedRemotes[profile.id] ? "button-connected" : "button-secondary"}`} onClick={() => void testRemoteConnection(profile.id)} disabled={isBusy}>{connectedRemotes[profile.id] ? "Connected" : isBusy ? "Connecting…" : "Connect"}</button>}
                          {!isLocal && (!remoteState || remoteState === "stopped" || remoteState === "unreachable") && <button className="button button-primary" onClick={() => void controlRemoteServer(profile.id, "start")} disabled={isBusy || !target.directory || !target.launch.executable.trim() || remoteState === "stopped" && !remoteStatus?.tmuxAvailable} title={!target.directory || !target.launch.executable.trim() ? "Set the remote folder and launch executable in the profile." : remoteState === "stopped" && !remoteStatus?.tmuxAvailable ? "Install tmux on the remote server to start it." : undefined}>{isBusy ? "Starting…" : "Start"}</button>}
                          {!isLocal && remoteState === "running" && <button className="button button-secondary" onClick={() => void controlRemoteServer(profile.id, "stop")} disabled={isBusy}>{isBusy ? "Stopping…" : "Stop"}</button>}
                          {!isLocal && remoteState === "stopping" && <button className="button button-secondary" disabled>Stopping…</button>}
                          <button className="button button-quiet" onClick={() => openEditProfile(profile)} disabled={!isLocal && (remoteState === "running" || remoteState === "stopping" || remoteState === "unmanaged")} title={!isLocal && (remoteState === "running" || remoteState === "stopping" || remoteState === "unmanaged") ? "Stop the server before changing its connection or launch settings." : undefined}>Edit</button>
                          <button className="button button-quiet" onClick={() => setRemovingId(profile.id)} disabled={isLocal ? state !== "stopped" : remoteState === "running" || remoteState === "stopping" || remoteState === "unmanaged"} title={(isLocal && state !== "stopped") || remoteState === "running" || remoteState === "stopping" || remoteState === "unmanaged" ? "Stop the server before removing its profile." : undefined}>Remove</button>
                        </div>
                      )}
                    </>
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
