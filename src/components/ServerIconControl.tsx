import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";

function errorText(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

async function iconFromFile(file: File): Promise<{ preview: string; bytes: number[] }> {
  const url = URL.createObjectURL(file);
  try {
    const image = new Image();
    image.src = url;
    await image.decode();
    const canvas = document.createElement("canvas");
    canvas.width = 64;
    canvas.height = 64;
    const context = canvas.getContext("2d");
    if (!context) throw new Error("Could not prepare the server icon.");
    const side = Math.min(image.naturalWidth, image.naturalHeight);
    context.drawImage(image, (image.naturalWidth - side) / 2, (image.naturalHeight - side) / 2, side, side, 0, 0, 64, 64);
    const preview = canvas.toDataURL("image/png");
    const decoded = atob(preview.split(",", 2)[1]);
    return { preview, bytes: Array.from(decoded, (character) => character.charCodeAt(0)) };
  } finally {
    URL.revokeObjectURL(url);
  }
}

export function ServerIconControl({ profileId, remote, running }: { profileId: string; remote: boolean; running: boolean }) {
  const [preview, setPreview] = useState<string | null>(null);
  const [bytes, setBytes] = useState<number[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  async function choose(file: File | undefined) {
    setError(null);
    setMessage(null);
    setPreview(null);
    setBytes(null);
    if (!file) return;
    try {
      const icon = await iconFromFile(file);
      setPreview(icon.preview);
      setBytes(icon.bytes);
    } catch (reason) {
      setError(`Could not open this image: ${errorText(reason)}`);
    }
  }

  async function upload() {
    if (!bytes || !window.confirm("Replace this server's multiplayer icon?")) return;
    setBusy(true);
    setError(null);
    setMessage(null);
    try {
      await invoke(remote ? "upload_remote_server_icon" : "upload_local_server_icon", { profileId, bytes });
      setMessage("Icon saved. Start the server to show it in Minecraft's multiplayer list.");
    } catch (reason) {
      setError(errorText(reason));
    } finally {
      setBusy(false);
    }
  }

  return <div className="server-icon-control">
    <div>
      <h3>Multiplayer icon</h3>
      <p>Choose an image. Scerite crops it to Minecraft's 64×64 PNG format.</p>
    </div>
    <div className="server-icon-actions">
      {preview && <img src={preview} alt="Selected server icon preview" width="64" height="64" />}
      <input type="file" accept="image/*" aria-label="Choose server icon image" onChange={(event) => void choose(event.currentTarget.files?.[0])} disabled={busy} />
      <button className="button button-primary" type="button" onClick={() => void upload()} disabled={!bytes || busy || running}>{busy ? "Uploading…" : "Save icon"}</button>
    </div>
    {running && <small>Stop the server before changing its icon.</small>}
    {error && <p className="form-error" role="alert">{error}</p>}
    {message && <p role="status">{message}</p>}
  </div>;
}
