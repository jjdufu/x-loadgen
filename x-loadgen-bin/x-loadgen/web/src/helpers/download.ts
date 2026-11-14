/**
 * Trigger a browser file download from an in-memory Blob.
 *
 * Revoking the object URL synchronously right after `a.click()` races with
 * navigation/download initiation in Safari and Firefox, cancelling the
 * download; deferring revocation avoids leaking memory while letting the
 * browser start reading the blob stream.
 */
const REVOKE_DELAY_MS = 30_000;

export function downloadBlob(blob: Blob, filename: string): void {
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  a.rel = "noopener";
  document.body.appendChild(a);
  a.click();
  a.remove();
  window.setTimeout(() => {
    URL.revokeObjectURL(url);
  }, REVOKE_DELAY_MS);
}
