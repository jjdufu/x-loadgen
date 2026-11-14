/** Minimum byte length enforced by `auth::MIN_NEW_TOKEN_LEN` on the server. */
export const MIN_ADMIN_PASSWORD_BYTES = 24;

const RANDOM_TOKEN_ALPHABET =
  "ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789-_.~!*@#%^";

export interface PasswordValidationError {
  field: "old" | "new" | "confirm";
  message: string;
}

/**
 * Generate a cryptographically random ASCII token using rejection sampling so
 * every character in the alphabet has strictly uniform probability.
 */
export function generateStrongToken(length = 32): string {
  const n = RANDOM_TOKEN_ALPHABET.length;
  const maxUnbiased = Math.floor(256 / n) * n;
  const buf = new Uint8Array(length * 2);
  let out = "";
  while (out.length < length) {
    globalThis.crypto.getRandomValues(buf);
    for (const byte of buf) {
      if (byte < maxUnbiased) {
        out += RANDOM_TOKEN_ALPHABET[byte % n];
        if (out.length === length) break;
      }
    }
  }
  return out;
}

/** Allowed characters match `src/auth.rs`: ASCII graphic `0x21..=0x7E` excluding `"`, `;`, `,`, `\`. */
export function isAllowedAdminTokenChar(code: number): boolean {
  if (code < 0x21 || code > 0x7e) return false;
  return code !== 0x22 && code !== 0x3b && code !== 0x2c && code !== 0x5c;
}

export function validateNewAdminToken(
  newToken: string,
  oldToken: string,
  confirmToken: string,
): PasswordValidationError | null {
  if (!oldToken) {
    return { field: "old", message: "请输入当前口令" };
  }
  const byteLen = new TextEncoder().encode(newToken).length;
  if (byteLen < MIN_ADMIN_PASSWORD_BYTES) {
    return {
      field: "new",
      message: `新口令至少需要 ${MIN_ADMIN_PASSWORD_BYTES} 个字符`,
    };
  }
  for (let i = 0; i < newToken.length; i += 1) {
    if (!isAllowedAdminTokenChar(newToken.charCodeAt(i))) {
      return {
        field: "new",
        message:
          "新口令仅限可打印 ASCII 字符，且不能包含空格、双引号、分号、逗号或反斜杠",
      };
    }
  }
  if (newToken === oldToken) {
    return { field: "new", message: "新口令不能与当前口令相同" };
  }
  if (confirmToken !== newToken) {
    return { field: "confirm", message: "两次输入的新口令不一致" };
  }
  return null;
}
