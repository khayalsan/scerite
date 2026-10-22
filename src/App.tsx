import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { Link, useLocation, useNavigate } from "react-router-dom";
import { invoke } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import type { FormEvent } from "react";
import { ArrowLeft, Download, FileKey, FolderOpen, Globe2, HardDrive, Network, Package, Palette, Plus, Puzzle, ScrollText, Server, Settings, SlidersHorizontal, Terminal, Users, X } from "lucide-react";
import { appSettings, localInstallRoot, serverEnvironments, serverProfiles } from "./lib/serverProfiles";
import type { ServerEnvironment, ServerProfile, ServerTarget } from "./lib/serverProfiles";
import { serverSessions } from "./lib/serverSessions";
import { serverWorlds } from "./lib/serverWorlds";
import type { LocalServerStatus, RemoteServerStatus } from "./lib/serverSessions";
import { LocalFilesPanel } from "./components/LocalFilesPanel";
import { WorldsPanel } from "./components/WorldsPanel";
import { ServerPropertiesPanel } from "./components/ServerPropertiesPanel";
import { ServerInstallPanel } from "./components/ServerInstallPanel";
import { PackagesPanel } from "./components/PackagesPanel";
import { ServerOverviewPanel } from "./components/ServerOverviewPanel";
import { LogsPanel } from "./components/LogsPanel";
import { SearchField } from "./components/SearchField";
import { PlayersPanel } from "./components/PlayersPanel";
import { ConsoleCommandInput } from "./components/ConsoleCommandInput";
import { ConsoleLogLine, isChatConsoleLine, plainConsoleLine } from "./components/ConsoleLogLine";
import { serverFiles } from "./lib/serverFiles";
import { packageSupportForSoftware, packageSupportFromFiles } from "./lib/serverSoftware";
import type { PackageSupport } from "./lib/serverSoftware";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "./components/ui/select";
import sceriteLogo from "../src-tauri/icons/logo.png";
import "./App.css";

type ProfileDraft = {
  name: string;
  kind: "local" | "remote";
  directory: string;
  executable: string;
  arguments: string;
  ramMb: string;
  environmentId: string;
  manualDirectory: boolean;
};

type EnvironmentDraft = {
  name: string;
  rootDirectory: string;
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

type ServerTab = "server" | "console" | "players" | "logs" | "files" | "worlds" | "properties" | "mods" | "plugins" | "install" | "settings";

const emptyDraft: ProfileDraft = {
  name: "",
  kind: "local",
  directory: "",
  executable: "",
  arguments: "",
  ramMb: "",
  environmentId: "",
  manualDirectory: false,
};

const emptyEnvironmentDraft: EnvironmentDraft = {
  name: "",
  rootDirectory: "",
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

function isJavaExecutable(executable: string): boolean {
  return /(?:^|[\\/])java(?:\.exe)?$/i.test(executable.trim());
}

function maximumRam(arguments_: string[]): string {
  const values = arguments_.flatMap((argument) => {
    const match = /^-Xmx(\d+)([mMgG])$/.exec(argument);
    return match ? [Number(match[1]) * (match[2].toLowerCase() === "g" ? 1024 : 1)] : [];
  });
  return values.length ? String(values[values.length - 1]) : "";
}

function withoutMaximumRam(arguments_: string[]): string[] {
  return arguments_.filter((argument) => !/^-Xmx\d+[mMgG]$/.test(argument));
}

function serverFolderName(name: string): string {
  return Array.from(name.trim())
    .map((character) => /[\p{L}\p{N}]/u.test(character) ? character.toLowerCase() : "-")
    .join("")
    .replace(/-+/g, "-")
    .replace(/^-|-$/g, "");
}

function targetFor(draft: ProfileDraft, previous?: ServerProfile): ServerTarget {
  const launchArguments = parseLaunchArguments(draft.arguments);
  if (isJavaExecutable(draft.executable)) {
    const withoutRam = withoutMaximumRam(launchArguments);
    launchArguments.splice(0, launchArguments.length, ...(draft.ramMb ? [`-Xmx${draft.ramMb}M`] : []), ...withoutRam);
  }
  const managedJava = previous?.target.kind === draft.kind &&
    (draft.kind === "local" || previous.target.kind === "remote" && previous.target.environmentId === draft.environmentId)
    ? previous.target.launch.managedJava : undefined;

  if (draft.kind === "remote") {
    return {
      kind: "remote",
      environmentId: draft.environmentId,
      directory: draft.manualDirectory ? draft.directory.trim() : "",
      launch: {
        executable: draft.executable.trim(),
        arguments: launchArguments,
        managedJava,
      },
    };
  }

  return {
    kind: "local",
    directory: draft.directory,
    launch: {
      executable: draft.executable.trim(),
      arguments: launchArguments,
      managedJava,
    },
  };
}

function draftFor(profile: ServerProfile): ProfileDraft {
  const arguments_ = profile.target.launch.arguments;
  const javaLaunch = isJavaExecutable(profile.target.launch.executable);
  if (profile.target.kind === "remote") {
    return {
      ...emptyDraft,
      name: profile.name,
      kind: "remote",
      environmentId: profile.target.environmentId,
      directory: profile.target.directory,
      manualDirectory: true,
      executable: profile.target.launch.executable,
      arguments: (javaLaunch ? withoutMaximumRam(arguments_) : arguments_).join("\n"),
      ramMb: javaLaunch ? maximumRam(arguments_) : "",
    };
  }

  return {
    ...emptyDraft,
    name: profile.name,
    directory: profile.target.directory,
    executable: profile.target.launch.executable,
    arguments: (javaLaunch ? withoutMaximumRam(arguments_) : arguments_).join("\n"),
    ramMb: javaLaunch ? maximumRam(arguments_) : "",
  };
}

function errorText(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function App() {
  const [theme, setTheme] = useState<ThemeId>(readSavedTheme);
  const navigate = useNavigate();
  const { pathname } = useLocation();
  const isAppSettings = pathname === "/settings";
  const selectedProfileId = pathname.startsWith("/servers/") ? pathname.slice("/servers/".length) : null;
  const [profiles, setProfiles] = useState<ServerProfile[]>([]);
  const [environments, setEnvironments] = useState<ServerEnvironment[]>([]);
  const [localRoot, setLocalRoot] = useState<string | null>(null);
  const [autoAcceptEula, setAutoAcceptEula] = useState<boolean | null>(null);
  const [eulaSettingError, setEulaSettingError] = useState<string | null>(null);
  const [isSavingEulaSetting, setIsSavingEulaSetting] = useState(false);
  const [localRootDraft, setLocalRootDraft] = useState("");
  const [localRootError, setLocalRootError] = useState<string | null>(null);
  const [isSavingLocalRoot, setIsSavingLocalRoot] = useState(false);
  const [environmentFilterId, setEnvironmentFilterId] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [sessionError, setSessionError] = useState<string | null>(null);
  const [autoBackupBusyIds, setAutoBackupBusyIds] = useState<Record<string, boolean>>({});
  const [backupRevisions, setBackupRevisions] = useState<Record<string, number>>({});
  const observedServerStates = useRef<Record<string, string>>({});
  const [statuses, setStatuses] = useState<Record<string, LocalServerStatus>>({});
  const [remoteStatuses, setRemoteStatuses] = useState<Record<string, RemoteServerStatus | { state: "unreachable" }>>({});
  const [busySessionId, setBusySessionId] = useState<string | null>(null);
  const [connectedRemotes, setConnectedRemotes] = useState<Record<string, boolean>>({});
  const [consoleProfileId, setConsoleProfileId] = useState<string | null>(null);
  const [filesProfileId, setFilesProfileId] = useState<string | null>(null);
  const [propertiesProfileId, setPropertiesProfileId] = useState<string | null>(null);
  const [installProfileId, setInstallProfileId] = useState<string | null>(null);
  const [consoleOutput, setConsoleOutput] = useState("");
  const [consoleSearch, setConsoleSearch] = useState("");
  const [consoleChatOnly, setConsoleChatOnly] = useState(false);
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
  const [activeServerTab, setActiveServerTab] = useState<ServerTab>("server");
  const selectedProfile = profiles.find((profile) => profile.id === selectedProfileId);
  const [legacyPackageSupport, setLegacyPackageSupport] = useState<{ profileId: string; support: PackageSupport } | null>(null);
  const packageSupport = packageSupportForSoftware(selectedProfile?.installedSoftware)
    ?? (legacyPackageSupport?.profileId === selectedProfileId ? legacyPackageSupport.support : null);
  const selectedRemoteConnected = selectedProfile ? connectedRemotes[selectedProfile.id] : false;

  useEffect(() => {
    if (!selectedProfile || selectedProfile.installedSoftware) return;
    if (selectedProfile.target.kind === "remote" && !selectedRemoteConnected) {
      setLegacyPackageSupport({ profileId: selectedProfile.id, support: packageSupportFromFiles(selectedProfile, []) });
      return;
    }
    let active = true;
    void serverFiles.list(selectedProfile.target.kind === "remote", selectedProfile.id, "")
      .then((entries) => {
        if (active) setLegacyPackageSupport({ profileId: selectedProfile.id, support: packageSupportFromFiles(selectedProfile, entries) });
      })
      .catch(() => {
        if (active) setLegacyPackageSupport({ profileId: selectedProfile.id, support: { mods: false, plugins: false } });
      });
    return () => { active = false; };
  }, [selectedProfile?.id, selectedProfile?.installedSoftware, selectedProfile?.target.directory, selectedRemoteConnected]);

  useEffect(() => {
    if (packageSupport && ((activeServerTab === "mods" && !packageSupport.mods) || (activeServerTab === "plugins" && !packageSupport.plugins))) {
      setActiveServerTab("server");
    }
  }, [activeServerTab, packageSupport?.mods, packageSupport?.plugins]);
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
      const [savedProfiles, savedEnvironments, savedLocalRoot, savedAutoAcceptEula] = await Promise.all([
        serverProfiles.list(),
        serverEnvironments.list(),
        localInstallRoot.get(),
        appSettings.getAutoAcceptEula(),
      ]);
      setProfiles(savedProfiles);
      setEnvironments(savedEnvironments);
      setLocalRoot(savedLocalRoot);
      setAutoAcceptEula(savedAutoAcceptEula);
      setLocalRootDraft(savedLocalRoot ?? "");
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

  async function selectLocalRoot() {
    try {
      const selected = await openDialog({ directory: true, multiple: false, title: "Choose local Minecraft root" });
      if (typeof selected === "string") setLocalRootDraft(selected);
    } catch (error) {
      setLocalRootError(errorText(error));
    }
  }

  async function saveLocalRoot(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setLocalRootError(null);
    setIsSavingLocalRoot(true);
    try {
      const saved = await localInstallRoot.set(localRootDraft);
      setLocalRoot(saved);
      setLocalRootDraft(saved);
    } catch (error) {
      setLocalRootError(errorText(error));
    } finally {
      setIsSavingLocalRoot(false);
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
    if (action === "start") {
      setActiveServerTab("console");
      setConsoleProfileId(profileId);
      setConsoleOutput("Starting server…");
    }
    try {
      const status = await serverSessions[action === "start" ? "startRemote" : "stopRemote"](profileId);
      setRemoteStatuses((current) => ({ ...current, [profileId]: status }));
      setConnectedRemotes((current) => ({ ...current, [profileId]: true }));
    } catch (error) {
      setSessionError(errorText(error));
      try {
        const status = await serverSessions.remoteStatus(profileId);
        setRemoteStatuses((current) => ({ ...current, [profileId]: status }));
        setConnectedRemotes((current) => ({ ...current, [profileId]: true }));
      } catch {
        setRemoteStatuses((current) => ({ ...current, [profileId]: { state: "unreachable" } }));
        setConnectedRemotes((current) => ({ ...current, [profileId]: false }));
      }
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
    for (const profile of profiles) {
      const remote = profile.target.kind === "remote";
      const state = remote ? remoteStatuses[profile.id]?.state : statuses[profile.id]?.state;
      if (!state || state === "unreachable" || state === "unmanaged") continue;
      const previous = observedServerStates.current[profile.id];
      observedServerStates.current[profile.id] = state;
      if (!(["running", "stopping"].includes(previous) && state === "stopped")) continue;
      setAutoBackupBusyIds((current) => ({ ...current, [profile.id]: true }));
      void serverWorlds.autoBackup(remote, profile.id)
        .then((path) => {
          if (path) setBackupRevisions((current) => ({ ...current, [profile.id]: (current[profile.id] ?? 0) + 1 }));
        })
        .catch((error: unknown) => setSessionError(`Automatic world backup failed: ${errorText(error)}`))
        .finally(() => setAutoBackupBusyIds((current) => ({ ...current, [profile.id]: false })));
    }
  }, [profiles, statuses, remoteStatuses]);

  useEffect(() => {
    if (!consoleProfileId) return;
    const profile = profiles.find((item) => item.id === consoleProfileId);
    if (profile?.target.kind === "remote") {
      if (consoleRemoteState === "unreachable") {
        setConsoleOutput("SSH status is unavailable.");
        return;
      }
      let active = true;
      let timer: number | undefined;
      const refresh = async () => {
        try {
          const snapshot = await serverSessions.remoteConsole(consoleProfileId);
          if (active) setConsoleOutput(snapshot.lines.slice(-1000).join("\n") || (consoleRemoteState === "stopped" ? "Server is stopped. No console output is available." : "Console output will appear here."));
        } catch (error) {
          if (active) {
            setSessionError(errorText(error));
          }
        } finally {
          if (active && consoleRemoteState !== "stopped") timer = window.setTimeout(() => void refresh(), 2000);
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
    if (consoleOutputRef.current && !consoleSearch.trim()) consoleOutputRef.current.scrollTop = consoleOutputRef.current.scrollHeight;
  }, [consoleOutput, consoleSearch, consoleChatOnly]);

  useEffect(() => {
    if (consoleOutputRef.current && consoleSearch.trim()) consoleOutputRef.current.scrollTop = 0;
  }, [consoleSearch, consoleChatOnly]);

  useEffect(() => { setConsoleSearch(""); }, [consoleProfileId]);

  async function sendConsoleInput(command: string): Promise<boolean> {
    if (!consoleProfileId) return false;
    setSessionError(null);
    try {
      const profile = profiles.find((item) => item.id === consoleProfileId);
      if (profile?.target.kind === "remote") await serverSessions.sendRemoteInput(consoleProfileId, command);
      else await serverSessions.sendInput(consoleProfileId, command);
      return true;
    } catch (error) {
      setSessionError(errorText(error));
      return false;
    }
  }

  function openNewEnvironment(environment?: ServerEnvironment) {
    setEditingEnvironment(environment ?? null);
    setEnvironmentDraft(environment ? {
      name: environment.name,
      rootDirectory: environment.rootDirectory,
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
          environmentDraft.rootDirectory,
          environmentDraft.host,
          Number(environmentDraft.port),
          environmentDraft.username,
          environmentDraft.identityFile.trim() || null,
        )
        : await serverEnvironments.create(
          environmentDraft.name,
          environmentDraft.rootDirectory,
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
    if (isJavaExecutable(draft.executable) && parseLaunchArguments(draft.arguments).some((argument) => argument.startsWith("-Xmx"))) {
      setFormError("Set maximum RAM in the field below and remove -Xmx from launch arguments.");
      return;
    }
    if (isJavaExecutable(draft.executable) && draft.ramMb &&
        (!/^\d+$/.test(draft.ramMb) || Number(draft.ramMb) < 256 || Number(draft.ramMb) > 1048576)) {
      setFormError("Choose a maximum RAM amount between 256 and 1048576 MB.");
      return;
    }
    const initialHeaps = parseLaunchArguments(draft.arguments).flatMap((argument) => {
      const match = /^-Xms(\d+)([mMgG])$/.exec(argument);
      return match ? [Number(match[1]) * (match[2].toLowerCase() === "g" ? 1024 : 1)] : [];
    });
    const initialHeap = initialHeaps[initialHeaps.length - 1];
    if (isJavaExecutable(draft.executable) && draft.ramMb && initialHeap && Number(draft.ramMb) < initialHeap) {
      setFormError("Maximum RAM must be at least the -Xms amount in launch arguments.");
      return;
    }
    if (draft.kind === "remote" && !draft.environmentId) {
      setFormError("Choose a remote environment.");
      return;
    }
    setIsSaving(true);

    try {
      const saved = editingProfile
        ? await serverProfiles.update(editingProfile.id, draft.name, targetFor(draft, editingProfile))
        : await serverProfiles.create(draft.name, targetFor(draft), draft.kind === "remote" && !draft.manualDirectory);

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
            <span>Minecraft root folder</span>
            <input value={environmentDraft.rootDirectory} onChange={(event) => setEnvironmentDraft({ ...environmentDraft, rootDirectory: event.currentTarget.value })} placeholder="/home/ubuntu/minecraft" required />
            <small>Versions download to cache/minecraft under this folder. The folder must exist on the remote machine.</small>
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
                {editingProfile && <small>Renaming this server also renames its folder. Stop the server first. Change its location in a separate save.</small>}
              </label>

              <fieldset className="field field-wide target-choice">
                <legend>Where is the server?</legend>
                <label className={draft.kind === "local" ? "choice choice-selected" : "choice"}>
                  <input
                    type="radio"
                    name="target-kind"
                    value="local"
                    checked={draft.kind === "local"}
                    onChange={() => setDraft({ ...draft, kind: "local", manualDirectory: false, directory: "" })}
                  />
                  <span>On this computer</span>
                </label>
                <label className={draft.kind === "remote" ? "choice choice-selected" : "choice"}>
                  <input
                    type="radio"
                    name="target-kind"
                    value="remote"
                    checked={draft.kind === "remote"}
                    onChange={() => setDraft({ ...draft, kind: "remote", manualDirectory: false, directory: "" })}
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
                  {!editingProfile && <fieldset className="field field-wide target-choice">
                    <legend>Server folder</legend>
                    <label className={!draft.manualDirectory ? "choice choice-selected" : "choice"}>
                      <input type="radio" name="remote-folder-mode" checked={!draft.manualDirectory} onChange={() => setDraft({ ...draft, manualDirectory: false })} />
                      <span>Create or use a folder in this environment</span>
                    </label>
                    <label className={draft.manualDirectory ? "choice choice-selected" : "choice"}>
                      <input type="radio" name="remote-folder-mode" checked={draft.manualDirectory} onChange={() => setDraft({ ...draft, manualDirectory: true })} />
                      <span>Use an existing folder</span>
                    </label>
                  </fieldset>}
                  {draft.manualDirectory ? <label className="field field-wide">
                    <span>Remote server folder</span>
                    <input value={draft.directory} onChange={(event) => setDraft({ ...draft, directory: event.currentTarget.value })} placeholder="/home/minecraft/servers/survival-world" required />
                    <small>Enter the absolute Linux path of an existing server folder.</small>
                  </label> : <p className="field field-wide managed-folder-preview">
                    {(() => {
                      const root = environments.find((environment) => environment.id === draft.environmentId)?.rootDirectory;
                      const folder = serverFolderName(draft.name);
                      return root && folder ? `Server folder: ${root.replace(/\/$/, "")}/servers/${folder}` : "Enter a server name and choose an environment to see its folder.";
                    })()}
                  </p>}
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
              <label className="field field-wide">
                <span>Maximum RAM (MB)</span>
                <input
                  type="number"
                  min="256"
                  max="1048576"
                  step="1"
                  value={draft.ramMb}
                  onChange={(event) => setDraft({ ...draft, ramMb: event.currentTarget.value })}
                  placeholder="Use Java default"
                  disabled={!isJavaExecutable(draft.executable)}
                />
                <small>Sets Java's maximum heap (-Xmx) on the next start. Leave blank to use Java's default. Custom launch scripts manage their own memory.</small>
              </label>
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

  const normalizedConsoleSearch = consoleSearch.trim().toLowerCase();
  const visibleConsoleLines = (consoleOutput || "Console output will appear here.")
    .split(/\r?\n/)
    .filter((line) => !consoleChatOnly || isChatConsoleLine(line))
    .filter((line) => !normalizedConsoleSearch || plainConsoleLine(line).toLowerCase().includes(normalizedConsoleSearch));

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
        const isBusy = busySessionId === selectedProfile.id || !!autoBackupBusyIds[selectedProfile.id];
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
              <img className="brand-logo" src={sceriteLogo} alt="" />
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
              <button className={`server-tab${activeServerTab === "server" ? " server-tab-active" : ""}`} type="button" aria-current={activeServerTab === "server" ? "page" : undefined} onClick={() => setActiveServerTab("server")}>
                <Server size={16} aria-hidden="true" /> Server
              </button>
              <button className={`server-tab${activeServerTab === "console" ? " server-tab-active" : ""}`} type="button" aria-current={activeServerTab === "console" ? "page" : undefined} onClick={() => { setActiveServerTab("console"); if (consoleProfileId !== selectedProfile.id) toggleConsole(selectedProfile); }}>
                <Terminal size={16} aria-hidden="true" /> Console
              </button>
              <button className={`server-tab${activeServerTab === "players" ? " server-tab-active" : ""}`} type="button" aria-current={activeServerTab === "players" ? "page" : undefined} onClick={() => setActiveServerTab("players")}>
                <Users size={16} aria-hidden="true" /> Players
              </button>
              <button className={`server-tab${activeServerTab === "logs" ? " server-tab-active" : ""}`} type="button" aria-current={activeServerTab === "logs" ? "page" : undefined} onClick={() => setActiveServerTab("logs")}>
                <ScrollText size={16} aria-hidden="true" /> Logs
              </button>
              <button className={`server-tab${activeServerTab === "files" ? " server-tab-active" : ""}`} type="button" aria-current={activeServerTab === "files" ? "page" : undefined} onClick={() => { setActiveServerTab("files"); if (filesProfileId !== selectedProfile.id) toggleFiles(selectedProfile); }}>
                <FolderOpen size={16} aria-hidden="true" /> Files
              </button>
              <button className={`server-tab${activeServerTab === "worlds" ? " server-tab-active" : ""}`} type="button" aria-current={activeServerTab === "worlds" ? "page" : undefined} onClick={() => setActiveServerTab("worlds")}>
                <Globe2 size={16} aria-hidden="true" /> Worlds
              </button>
              {packageSupport?.mods && <button className={`server-tab${activeServerTab === "mods" ? " server-tab-active" : ""}`} type="button" aria-current={activeServerTab === "mods" ? "page" : undefined} onClick={() => setActiveServerTab("mods")}>
                <Package size={16} aria-hidden="true" /> Mods
              </button>}
              {packageSupport?.plugins && <button className={`server-tab${activeServerTab === "plugins" ? " server-tab-active" : ""}`} type="button" aria-current={activeServerTab === "plugins" ? "page" : undefined} onClick={() => setActiveServerTab("plugins")}>
                <Puzzle size={16} aria-hidden="true" /> Plugins
              </button>}
              <button className={`server-tab${activeServerTab === "properties" ? " server-tab-active" : ""}`} type="button" aria-current={activeServerTab === "properties" ? "page" : undefined} onClick={() => { setActiveServerTab("properties"); setPropertiesProfileId(selectedProfile.id); }}>
                <SlidersHorizontal size={16} aria-hidden="true" /> Properties
              </button>
              <button className={`server-tab${activeServerTab === "install" ? " server-tab-active" : ""}`} type="button" aria-current={activeServerTab === "install" ? "page" : undefined} onClick={() => { setActiveServerTab("install"); setInstallProfileId(selectedProfile.id); }}>
                <Download size={16} aria-hidden="true" /> Install
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
            <Link className="sidebar-settings-link server-page-app-settings" to="/settings" onClick={closeForm}>
              <Settings size={17} aria-hidden="true" /> <span>App settings</span>
            </Link>
          </aside>
        );
      })() : (
        <aside className="sidebar" aria-label="Main navigation">
          <Link className="brand" to="/" aria-label="scerite home">
            <img className="brand-logo" src={sceriteLogo} alt="" />
            <span>scerite</span>
          </Link>
          <nav className="primary-nav">
            <Link className={`nav-link${!isAppSettings ? " nav-link-active" : ""}`} to="/" aria-current={!isAppSettings ? "page" : undefined}>
              <span className="nav-indicator" aria-hidden="true" />
              <Server className="sidebar-servers-icon" size={17} aria-hidden="true" />
              Servers
              <span className="nav-count">{profiles.length}</span>
            </Link>
            {isAppSettings && profiles.length > 0 && <div className="sidebar-server-shortcuts" aria-label="Saved server shortcuts">
              <span className="sidebar-server-shortcuts-title">Saved servers</span>
              {profiles.map((profile) => <Link className="sidebar-server-link" to={`/servers/${profile.id}`} key={profile.id} title={profile.name} aria-label={`Open ${profile.name}`}>
                <Server size={16} aria-hidden="true" /><span>{profile.name}</span>
              </Link>)}
            </div>}
          </nav>
          <div className="sidebar-footer">
            <Link className={`sidebar-settings-link${isAppSettings ? " sidebar-settings-link-active" : ""}`} to="/settings" aria-current={isAppSettings ? "page" : undefined} onClick={closeForm}>
              <Settings size={17} aria-hidden="true" /> <span>App settings</span>
            </Link>
          </div>
        </aside>
      )}

      <section
        className="workspace"
        id="servers"
        aria-labelledby={!selectedProfileId ? "page-title" : undefined}
        aria-label={selectedProfileId ? `${selectedProfile?.name ?? "Server"} workspace` : undefined}
      >
        {!selectedProfileId && !isAppSettings && <header className="page-header">
          <div>
            <h1 id="page-title">Servers</h1>
            <p>Manage local servers and group remote servers by environment.</p>
          </div>
          <div className="page-actions">
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

        {!selectedProfileId && !isAppSettings && isEnvironmentFormOpen && renderEnvironmentForm()}
        {!selectedProfileId && !isAppSettings && isFormOpen && !isEnvironmentFormOpen && renderProfileForm()}

        {isAppSettings && <header className="page-header"><div><h1 id="page-title">App settings</h1><p>Preferences for this app and local server files.</p></div></header>}

        {isAppSettings && (
          <section className="app-settings-section" aria-labelledby="app-settings-title">
            <div className="environment-section-heading">
              <h2 id="app-settings-title">Preferences</h2>
            </div>
            <div className="app-setting-row app-theme-row">
              <span className="app-setting-copy"><strong>Theme</strong><small>Choose the app's appearance.</small></span>
              <label className="theme-picker">
                <Palette size={16} aria-hidden="true" />
                <span className="sr-only">Theme</span>
                <Select value={theme} onValueChange={(value) => {
                  const selectedTheme = themes.find((option) => option.id === value);
                  if (selectedTheme) setTheme(selectedTheme.id);
                }}>
                  <SelectTrigger aria-label="Color theme" className="theme-select-trigger">
                    <SelectValue>{(selected) => themes.find((option) => option.id === selected)?.label ?? "Violet Night"}</SelectValue>
                  </SelectTrigger>
                  <SelectContent>{themes.map((option) => <SelectItem key={option.id} value={option.id}>{option.label}</SelectItem>)}</SelectContent>
                </Select>
              </label>
            </div>
            <label className="app-setting-row">
              <span className="app-setting-copy">
                <strong>Auto enable EULA</strong>
                <small>Turn off if you really wanna read the EULA and accept before running the server</small>
              </span>
              <span className="property-switch">
                <input
                  type="checkbox"
                  checked={autoAcceptEula ?? true}
                  disabled={autoAcceptEula === null || isSavingEulaSetting}
                  onChange={(event) => {
                    const enabled = event.currentTarget.checked;
                    setIsSavingEulaSetting(true);
                    setEulaSettingError(null);
                    void appSettings.setAutoAcceptEula(enabled).then(setAutoAcceptEula)
                      .catch((error: unknown) => setEulaSettingError(errorText(error)))
                      .finally(() => setIsSavingEulaSetting(false));
                  }}
                />
                <span aria-hidden="true" />
              </span>
            </label>
            {eulaSettingError && <p className="form-error" role="alert">{eulaSettingError}</p>}
          </section>
        )}

        {isAppSettings && (
          <section className="local-root-section" aria-labelledby="local-root-title">
            <div className="environment-section-heading">
              <h2 id="local-root-title">This computer</h2>
            </div>
            <form className="local-root-form" onSubmit={(event) => void saveLocalRoot(event)}>
              <label className="field">
                <span>Minecraft root folder</span>
                <div className="path-control">
                  <input value={localRootDraft} onChange={(event) => setLocalRootDraft(event.currentTarget.value)} placeholder="C:\\Minecraft" required />
                  <button className="button button-secondary" type="button" onClick={() => void selectLocalRoot()} aria-label="Browse for local Minecraft root"><FolderOpen size={15} />Browse</button>
                </div>
                <small>Vanilla versions are cached in cache/minecraft beneath this folder.</small>
              </label>
              <button className="button button-secondary" type="submit" disabled={isSavingLocalRoot || !localRootDraft.trim() || localRootDraft === localRoot}>
                {isSavingLocalRoot ? "Saving…" : "Save root"}
              </button>
            </form>
            {localRootError && <p className="form-error" role="alert">{localRootError}</p>}
          </section>
        )}

        {!selectedProfileId && !isAppSettings && environments.length > 0 && (
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
                      <span className="environment-card-address">{environment.rootDirectory || "Set a Minecraft root to install versions"}</span>
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

        {!isAppSettings && loadError && profiles.length > 0 && (
          <div className="notice notice-error" role="alert">
            <div>
              <strong>Could not refresh saved servers.</strong>
              <span>{loadError}</span>
            </div>
            <button className="button button-quiet" onClick={() => void loadProfiles()}>Try again</button>
          </div>
        )}

        {!isAppSettings && actionError && (
          <div className="notice notice-error" role="alert">
            <div>
              <strong>Could not remove the saved profile.</strong>
              <span>{actionError}</span>
            </div>
            <button className="button button-quiet" onClick={() => setActionError(null)}>Dismiss</button>
          </div>
        )}

        {!isAppSettings && sessionError && (
          <div className="notice notice-error" role="alert">
            <div><strong>Could not complete the server action.</strong><span>{sessionError}</span></div>
            <button className="button button-quiet" onClick={() => setSessionError(null)}>Dismiss</button>
          </div>
        )}

        {!selectedProfileId && !isAppSettings && <div className="list-heading">
          <h2>{filteredEnvironment ? `${filteredEnvironment.name} servers` : "Saved servers"}</h2>
          {!isLoading && <span>{visibleProfiles.length} {visibleProfiles.length === 1 ? "server" : "servers"}</span>}
        </div>}

        {!isAppSettings && (isLoading ? (
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
              const overviewState = isLocal
                ? !status ? "checking" : state === "stopped" ? "offline" : "online"
                : !remoteState ? "checking" : ["running", "stopping", "unmanaged"].includes(remoteState) ? "online" : "offline";
              const remoteStateLabel = remoteState === "unreachable"
                ? "SSH status unavailable"
                : remoteState === "unmanaged"
                  ? "Java process detected outside scerite"
                : remoteStatus && !remoteStatus.tmuxAvailable
                  ? "SSH reachable · tmux required"
                  : remoteState ? `SSH reachable · ${remoteState}` : "SSH reachable";
              const remoteStateTone = remoteState === "running" ? "running" : remoteState === "stopped" && remoteStatus?.tmuxAvailable ? "stopped" : "stopping";
              const isBusy = busySessionId === profile.id || !!autoBackupBusyIds[profile.id];
              const isDetailPage = selectedProfileId === profile.id;
              return (
                <article className={`server-row${selectedProfileId === profile.id ? " server-row-detail" : ""}`} key={profile.id} hidden={!!selectedProfileId && selectedProfileId !== profile.id}>
                  {!isDetailPage && (
                    <Link className="server-row-open" to={`/servers/${profile.id}`} onClick={() => setActiveServerTab("server")} aria-label={`Open ${profile.name}`}>
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
                        <section className="server-tab-panel" hidden={activeServerTab !== "server"} aria-label="Server overview">
                          {activeServerTab === "server" && <ServerOverviewPanel profile={profile} environment={target.kind === "remote" ? environments.find((item) => item.id === target.environmentId) : undefined} state={overviewState} />}
                        </section>
                        <section className="server-tab-panel" hidden={activeServerTab !== "console"} aria-label="Server console">
                          {consoleProfileId === profile.id ? (
                            <section className="console-panel" aria-label={`${profile.name} console`}>
                              <div className="console-toolbar">
                                <strong>Live console</strong>
                                <div className="console-toolbar-actions">
                                  <label className="console-chat-toggle">Chat only <span className="property-switch"><input type="checkbox" role="switch" checked={consoleChatOnly} onChange={(event) => setConsoleChatOnly(event.currentTarget.checked)} /><span aria-hidden="true" /></span></label>
                                  <button className="button button-quiet" type="button" onClick={() => toggleConsole(profile)}>Disconnect</button>
                                </div>
                              </div>
                              <div className="console-search"><SearchField id="console-search" label="Search console output" placeholder="Find matching lines" value={consoleSearch} onChange={setConsoleSearch} />{normalizedConsoleSearch && <small>{visibleConsoleLines.length} matching lines</small>}</div>
                              <div className="console-output" ref={consoleOutputRef} role="log" aria-live="polite">
                                <pre>{visibleConsoleLines.length ? visibleConsoleLines.map((line, index) => <ConsoleLogLine key={index} line={line} trailingNewline={index < visibleConsoleLines.length - 1} />) : consoleChatOnly ? "No chat messages found." : "No matching console lines."}</pre>
                              </div>
                              <ConsoleCommandInput key={profile.id} disabled={!serverRunning || isBusy} onSend={sendConsoleInput} />
                            </section>
                          ) : (
                            <div className="server-tab-empty">
                              <h2>Console</h2>
                              <p>Connect to view server output and send commands.</p>
                              <button className="button button-primary" onClick={() => toggleConsole(profile)}>Connect console</button>
                            </div>
                          )}
                        </section>
                        <section className="server-tab-panel" hidden={activeServerTab !== "players"} aria-label="Server players">
                          {activeServerTab === "players" && <PlayersPanel key={profile.id} profileId={profile.id} remote={!isLocal} running={serverRunning} />}
                        </section>
                        <section className="server-tab-panel" hidden={activeServerTab !== "logs"} aria-label="Server logs">
                          {activeServerTab === "logs" && <LogsPanel key={profile.id} profileId={profile.id} remote={!isLocal} running={serverRunning} />}
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
                        <section className="server-tab-panel" hidden={activeServerTab !== "worlds"} aria-label="Server worlds">
                          {activeServerTab === "worlds" && <WorldsPanel key={profile.id} profileId={profile.id} remote={!isLocal} running={serverRunning} canEdit={!autoBackupBusyIds[profile.id] && (isLocal ? state === "stopped" : remoteState === "stopped" && !!remoteStatus?.tmuxAvailable)} backupRevision={backupRevisions[profile.id] ?? 0} />}
                        </section>
                        {packageSupport?.mods && <section className="server-tab-panel" hidden={activeServerTab !== "mods"} aria-label="Server mods">
                          {activeServerTab === "mods" && <PackagesPanel key={`${profile.id}-mods`} kind="mod" software={profile.installedSoftware} installedVersion={profile.installedVersion} profileId={profile.id} remote={!isLocal} canInstall={isLocal ? state === "stopped" : remoteState === "stopped"} />}
                        </section>}
                        {packageSupport?.plugins && <section className="server-tab-panel" hidden={activeServerTab !== "plugins"} aria-label="Server plugins">
                          {activeServerTab === "plugins" && <PackagesPanel key={`${profile.id}-plugins`} kind="plugin" software={profile.installedSoftware} installedVersion={profile.installedVersion} profileId={profile.id} remote={!isLocal} canInstall={isLocal ? state === "stopped" : remoteState === "stopped"} />}
                        </section>}
                        <section className="server-tab-panel server-properties-tab" hidden={activeServerTab !== "properties"} aria-label="Server properties">
                          {propertiesProfileId === profile.id && (
                            <ServerPropertiesPanel
                              profileId={profile.id}
                              remote={!isLocal}
                              canEdit={isLocal ? status?.state === "stopped" : remoteState === "stopped"}
                            />
                          )}
                        </section>
                        <section className="server-tab-panel" hidden={activeServerTab !== "install"} aria-label="Install Minecraft">
                          {installProfileId === profile.id && <ServerInstallPanel profileId={profile.id} remote={!isLocal} rootConfigured={isLocal ? !!localRoot : !!environments.find((environment) => environment.id === (target.kind === "remote" ? target.environmentId : ""))?.rootDirectory} canInstall={isLocal ? state === "stopped" : remoteState === "stopped"} onUpdated={async () => setProfiles(await serverProfiles.list())} />}
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
        ))}
      </section>
    </main>
  );
}

export default App;
