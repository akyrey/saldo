const KIB = 1024;
const MIB = KIB * 1024;

/** A file size for people: bytes, KB or MB (binary units, one decimal). */
export function formatSize(bytes: number): string {
  if (bytes < KIB) return `${bytes} B`;
  if (bytes < MIB) return `${(bytes / KIB).toFixed(1)} KB`;
  return `${(bytes / MIB).toFixed(1)} MB`;
}
