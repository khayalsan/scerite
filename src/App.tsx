import { useEffect, useState } from "react";
import type { FormEvent } from "react";
import { HardDrive, Network, Plus, X } from "lucide-react";
import { serverProfiles } from "./lib/serverProfiles";
import type { ServerProfile, ServerTarget } from "./lib/serverProfiles";
import "./App.css";

type ProfileDraft = {
  name: string;
  kind: "local" | "remote";
  directory: string;
  host: string;
  port: string;
  username: string;
};

const emptyDraft: ProfileDraft = {
  name: "",
  kind: "local",
  directory: "",
  host: "",
  port: "22",
  username: "",
};

function targetFor(draft: ProfileDraft): ServerTarget {
  if (draft.kind === "remote") {
    return {
      kind: "remote",
      host: draft.host.trim(),
      port: Number(draft.port),
      username: draft.username.trim(),
    };
  }

  return { kind: "local", directory: draft.directory };
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
    };
  }

  return { ...emptyDraft, name: profile.name, directory: profile.target.directory };
}

function errorText(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function App() {
  const [profiles, setProfiles] = useState<ServerProfile[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
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

      setProfiles((current) =>
        editingProfile
          ? current.map((profile) => (profile.id === saved.id ? saved : profile))
          : [...current, saved],
      );
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
                : `${target.username}@${target.host}:${target.port}`;

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
                      <button className="button button-quiet" onClick={() => openEditProfile(profile)}>Edit</button>
                      <button className="button button-quiet" onClick={() => setRemovingId(profile.id)}>Remove</button>
                    </div>
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
