import type { KeyValueRow, RequestDraft } from "../api/types";
import { createDefaultRequestDraft } from "./format";

function tokenizeShell(input: string): string[] {
  const cleaned = input.replace(/\\\r?\n/g, " ").trim();
  const tokens: string[] = [];
  let current = "";
  let quote: "'" | '"' | null = null;
  let escaped = false;

  for (let i = 0; i < cleaned.length; i++) {
    const ch = cleaned[i]!;
    if (escaped) {
      current += ch;
      escaped = false;
      continue;
    }
    if (ch === "\\" && quote !== "'") {
      escaped = true;
      continue;
    }
    if (quote) {
      if (ch === quote) {
        quote = null;
      } else {
        current += ch;
      }
      continue;
    }
    if (ch === "'" || ch === '"') {
      quote = ch;
      continue;
    }
    if (/\s/.test(ch)) {
      if (current.length > 0) {
        tokens.push(current);
        current = "";
      }
      continue;
    }
    current += ch;
  }
  if (current.length > 0) {
    tokens.push(current);
  }
  return tokens;
}

export function parseCurlToDraft(raw: string): RequestDraft {
  const draft = createDefaultRequestDraft("http");
  draft.name = "cURL 导入请求";
  const tokens = tokenizeShell(raw);
  if (tokens.length === 0) return draft;

  let idx = tokens[0]?.toLowerCase() === "curl" ? 1 : 0;
  let explicitMethod = "";
  const headers: KeyValueRow[] = [];
  const dataParts: string[] = [];
  let basicUser = "";
  let insecure = false;
  let targetUrl = "";

  while (idx < tokens.length) {
    const tok = tokens[idx]!;
    if ((tok === "-X" || tok === "--request") && idx + 1 < tokens.length) {
      explicitMethod = tokens[++idx]!.toUpperCase();
    } else if (
      (tok === "-H" || tok === "--header") &&
      idx + 1 < tokens.length
    ) {
      const h = tokens[++idx]!;
      const colon = h.indexOf(":");
      if (colon > 0) {
        headers.push({
          key: h.slice(0, colon).trim(),
          value: h.slice(colon + 1).trim(),
          active: true,
        });
      }
    } else if (
      (tok === "-d" ||
        tok === "--data" ||
        tok === "--data-raw" ||
        tok === "--data-binary") &&
      idx + 1 < tokens.length
    ) {
      dataParts.push(tokens[++idx]!);
    } else if ((tok === "-u" || tok === "--user") && idx + 1 < tokens.length) {
      basicUser = tokens[++idx]!;
    } else if (tok === "-k" || tok === "--insecure") {
      insecure = true;
    } else if (tok === "--url" && idx + 1 < tokens.length) {
      targetUrl = tokens[++idx]!;
    } else if (!tok.startsWith("-") && !targetUrl) {
      targetUrl = tok;
    }
    idx++;
  }

  if (targetUrl) {
    try {
      const parsed = new URL(targetUrl);
      const params: KeyValueRow[] = [];
      parsed.searchParams.forEach((value, key) => {
        params.push({ key, value, active: true });
      });
      if (params.length > 0) {
        draft.params = params;
        parsed.search = "";
        draft.endpoint = parsed.toString();
      } else {
        draft.endpoint = targetUrl;
      }
    } catch {
      draft.endpoint = targetUrl;
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
  const filteredHeaders: KeyValueRow[] = [];
  for (const h of headers) {
    if (h.key.toLowerCase() === "content-type") {
      contentType = h.value.split(";")[0]?.trim() ?? "";
    } else if (
      h.key.toLowerCase() === "authorization" &&
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
  draft.headers = filteredHeaders;

  if (dataParts.length > 0) {
    const joined = dataParts.join("&");
    draft.body = {
      content_type: contentType || "application/json",
      body: joined,
      form: [],
    };
    if (!explicitMethod) {
      explicitMethod = "POST";
    }
  }

  draft.method = explicitMethod || "GET";
  draft.tls.insecure_skip_verify = insecure;
  return draft;
}
