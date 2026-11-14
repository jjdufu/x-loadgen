import type { KeyValueRow, RequestDraft } from "../api/types";
import { createDefaultRequestDraft } from "./format";

const DOUBLE_QUOTE_ESCAPABLE = new Set(['$', '`', '"', '\\']);

function decodeAnsiCChar(next: string): string {
  switch (next) {
    case "n":
      return "\n";
    case "r":
      return "\r";
    case "t":
      return "\t";
    case "\\":
      return "\\";
    case "'":
      return "'";
    case '"':
      return '"';
    default:
      return next;
  }
}

export function tokenizeShell(input: string): string[] {
  const cleaned = input.replace(/\\\r?\n/g, " ").trim();
  const tokens: string[] = [];
  let current = "";
  let inToken = false;
  let quote: "'" | '"' | "$'" | null = null;

  for (let i = 0; i < cleaned.length; i++) {
    const ch = cleaned[i]!;

    if (quote === "'") {
      if (ch === "'") {
        quote = null;
      } else {
        current += ch;
      }
      continue;
    }

    if (quote === "$'") {
      if (ch === "'") {
        quote = null;
      } else if (ch === "\\" && i + 1 < cleaned.length) {
        current += decodeAnsiCChar(cleaned[++i]!);
      } else {
        current += ch;
      }
      continue;
    }

    if (quote === '"') {
      if (ch === '"') {
        quote = null;
      } else if (ch === "\\" && i + 1 < cleaned.length) {
        const next = cleaned[i + 1]!;
        if (DOUBLE_QUOTE_ESCAPABLE.has(next)) {
          current += next;
          i++;
        } else {
          // POSIX shell preserves backslashes before non-special chars in "..."
          current += ch;
        }
      } else {
        current += ch;
      }
      continue;
    }

    if (ch === "\\" && i + 1 < cleaned.length) {
      inToken = true;
      current += cleaned[++i]!;
      continue;
    }
    if (ch === "$" && cleaned[i + 1] === "'") {
      inToken = true;
      quote = "$'";
      i++;
      continue;
    }
    if (ch === "'" || ch === '"') {
      inToken = true;
      quote = ch;
      continue;
    }
    if (/\s/.test(ch)) {
      if (inToken) {
        tokens.push(current);
        current = "";
        inToken = false;
      }
      continue;
    }
    inToken = true;
    current += ch;
  }

  if (inToken) {
    tokens.push(current);
  }
  return tokens;
}

/** Curl flags that consume one following argument (ignored if not mapped). */
const IGNORED_VALUED_FLAGS = new Set([
  "-o",
  "--output",
  "-c",
  "--cookie-jar",
  "-D",
  "--dump-header",
  "-w",
  "--write-out",
  "-m",
  "--max-time",
  "--connect-timeout",
  "-x",
  "--proxy",
  "-U",
  "--proxy-user",
  "-E",
  "--cert",
  "--key",
  "--cacert",
  "--capath",
  "-K",
  "--config",
  "-T",
  "--upload-file",
  "--resolve",
  "--connect-to",
  "--retry",
  "--retry-delay",
  "--retry-max-time",
  "--limit-rate",
  "--interface",
  "--proto",
]);

function encodeUrlencodePart(raw: string): string {
  const eq = raw.indexOf("=");
  if (eq >= 0) {
    const key = raw.slice(0, eq);
    const val = raw.slice(eq + 1);
    return key ? `${encodeURIComponent(key)}=${encodeURIComponent(val)}` : encodeURIComponent(val);
  }
  return encodeURIComponent(raw);
}

function looksLikeJsonBody(body: string): boolean {
  const trimmed = body.trim();
  if (
    !(trimmed.startsWith("{") && trimmed.endsWith("}")) &&
    !(trimmed.startsWith("[") && trimmed.endsWith("]"))
  ) {
    return false;
  }
  try {
    JSON.parse(trimmed);
    return true;
  } catch {
    return false;
  }
}

function pushHeaderLine(headers: KeyValueRow[], line: string): void {
  const colon = line.indexOf(":");
  if (colon > 0) {
    headers.push({
      key: line.slice(0, colon).trim(),
      value: line.slice(colon + 1).trim(),
      active: true,
    });
  }
}

export function parseCurlToDraft(raw: string): RequestDraft {
  const draft = createDefaultRequestDraft("http");
  draft.name = "cURL 导入请求";
  const tokens = tokenizeShell(raw);
  if (tokens.length === 0) return draft;

  let idx = tokens[0]?.toLowerCase() === "curl" ? 1 : 0;
  let explicitMethod = "";
  let forceGet = false;
  let forceJson = false;
  const headers: KeyValueRow[] = [];
  const dataParts: string[] = [];
  const formRows: KeyValueRow[] = [];
  let basicUser = "";
  let insecure = false;
  let targetUrl = "";
  let endOfFlags = false;

  const takeValue = (inlineVal: string | undefined): string | null => {
    if (inlineVal !== undefined) return inlineVal;
    if (idx + 1 < tokens.length) return tokens[++idx]!;
    return null;
  };

  while (idx < tokens.length) {
    const tok = tokens[idx]!;
    if (endOfFlags) {
      if (!targetUrl) targetUrl = tok;
      idx++;
      continue;
    }
    if (tok === "--") {
      endOfFlags = true;
      idx++;
      continue;
    }

    // Split `--flag=value` or attached short flags like `-XPOST`, `-HAccept: */*`, `-da=1`.
    let flag = tok;
    let inlineVal: string | undefined;
    if (tok.startsWith("--")) {
      const eq = tok.indexOf("=");
      if (eq > 2) {
        flag = tok.slice(0, eq);
        inlineVal = tok.slice(eq + 1);
      }
    } else if (
      tok.length > 2 &&
      /^-[XHduAbeFocDwmExUKT]/.test(tok)
    ) {
      flag = tok.slice(0, 2);
      inlineVal = tok.slice(2);
    }

    if (flag === "-X" || flag === "--request") {
      const val = takeValue(inlineVal);
      if (val) explicitMethod = val.toUpperCase();
    } else if (flag === "-H" || flag === "--header") {
      const val = takeValue(inlineVal);
      if (val) pushHeaderLine(headers, val);
    } else if (
      flag === "-d" ||
      flag === "--data" ||
      flag === "--data-raw" ||
      flag === "--data-binary" ||
      flag === "--data-ascii"
    ) {
      const val = takeValue(inlineVal);
      if (val !== null) dataParts.push(val);
    } else if (flag === "--data-urlencode") {
      const val = takeValue(inlineVal);
      if (val !== null) dataParts.push(encodeUrlencodePart(val));
    } else if (flag === "--json") {
      const val = takeValue(inlineVal);
      if (val !== null) {
        dataParts.push(val);
        forceJson = true;
      }
    } else if (flag === "-F" || flag === "--form" || flag === "--form-string") {
      const val = takeValue(inlineVal);
      if (val !== null) {
        const eq = val.indexOf("=");
        if (eq > 0) {
          formRows.push({
            key: val.slice(0, eq),
            value: val.slice(eq + 1),
            active: true,
          });
        }
      }
    } else if (flag === "-u" || flag === "--user") {
      const val = takeValue(inlineVal);
      if (val !== null) basicUser = val;
    } else if (flag === "-A" || flag === "--user-agent") {
      const val = takeValue(inlineVal);
      if (val !== null) {
        headers.push({ key: "User-Agent", value: val, active: true });
      }
    } else if (flag === "-e" || flag === "--referer") {
      const val = takeValue(inlineVal);
      if (val !== null) {
        headers.push({ key: "Referer", value: val, active: true });
      }
    } else if (flag === "-b" || flag === "--cookie") {
      const val = takeValue(inlineVal);
      if (val !== null && val.includes("=")) {
        headers.push({ key: "Cookie", value: val, active: true });
      }
    } else if (flag === "-G" || flag === "--get") {
      forceGet = true;
    } else if (flag === "-I" || flag === "--head") {
      if (!explicitMethod) explicitMethod = "HEAD";
    } else if (flag === "-k" || flag === "--insecure") {
      insecure = true;
    } else if (flag === "--url") {
      const val = takeValue(inlineVal);
      if (val) targetUrl = val;
    } else if (IGNORED_VALUED_FLAGS.has(flag)) {
      void takeValue(inlineVal);
    } else if (!tok.startsWith("-") && !targetUrl) {
      targetUrl = tok;
    }
    idx++;
  }

  const extraQueryParams: KeyValueRow[] = [];
  if (forceGet && dataParts.length > 0) {
    const qs = new URLSearchParams(dataParts.join("&"));
    qs.forEach((value, key) => {
      extraQueryParams.push({ key, value, active: true });
    });
    dataParts.length = 0;
  }

  if (targetUrl) {
    try {
      const parsed = new URL(targetUrl);
      const params: KeyValueRow[] = [];
      parsed.searchParams.forEach((value, key) => {
        params.push({ key, value, active: true });
      });
      params.push(...extraQueryParams);
      if (params.length > 0) {
        draft.params = params;
        parsed.search = "";
        draft.endpoint = parsed.toString();
      } else {
        draft.endpoint = targetUrl;
      }
    } catch {
      draft.endpoint = targetUrl;
      if (extraQueryParams.length > 0) {
        draft.params = extraQueryParams;
      }
    }
  }

  if (basicUser) {
    const sep = basicUser.indexOf(":");
    draft.auth = {
      auth_type: "basic",
      auth_active: true,
      username: sep >= 0 ? basicUser.slice(0, sep) : basicUser,
      password: sep >= 0 ? basicUser.slice(sep + 1) : "",
    };
  }

  let contentType = "";
  let hasAccept = false;
  const filteredHeaders: KeyValueRow[] = [];
  for (const h of headers) {
    const lower = h.key.toLowerCase();
    if (lower === "content-type") {
      contentType = h.value.split(";")[0]?.trim() ?? "";
    } else if (lower === "accept") {
      hasAccept = true;
      filteredHeaders.push(h);
    } else if (
      lower === "authorization" &&
      h.value.toLowerCase().startsWith("bearer ")
    ) {
      draft.auth = {
        auth_type: "bearer",
        auth_active: true,
        token: h.value.slice(7).trim(),
      };
    } else {
      filteredHeaders.push(h);
    }
  }
  if (forceJson && !hasAccept) {
    filteredHeaders.push({
      key: "Accept",
      value: "application/json",
      active: true,
    });
  }
  draft.headers = filteredHeaders;

  if (formRows.length > 0) {
    draft.body = {
      content_type: contentType || "multipart/form-data",
      body: null,
      form: formRows,
    };
    if (!explicitMethod) explicitMethod = "POST";
  } else if (dataParts.length > 0) {
    const joined = dataParts.join("&");
    const inferredType =
      contentType ||
      (forceJson || looksLikeJsonBody(joined)
        ? "application/json"
        : "application/x-www-form-urlencoded");
    draft.body = {
      content_type: inferredType,
      body: joined,
      form: [],
    };
    if (!explicitMethod && !forceGet) {
      explicitMethod = "POST";
    }
  }

  draft.method = explicitMethod || "GET";
  draft.tls.insecure_skip_verify = insecure;
  return draft;
}
