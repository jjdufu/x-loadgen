const ids = new WeakMap<object, number>();
let seq = 0;

/**
 * Return a stable unique key for a mutable row object without adding extra
 * serializable properties to API payloads.
 */
export function rowObjectKey(row: object): number {
  const existing = ids.get(row);
  if (existing !== undefined) return existing;
  seq += 1;
  ids.set(row, seq);
  return seq;
}
