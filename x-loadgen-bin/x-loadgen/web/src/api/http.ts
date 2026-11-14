import axios, { AxiosError } from "axios";
import { invalidateCachedMe, setCachedMe } from "../modules/auth";
import { toast } from "../modules/toast";

declare module "axios" {
  interface AxiosRequestConfig {
    /**
     * Background request (polling, live refresh): never toast on failure.
     * The caller reports the error itself, e.g. with an inline banner.
     * Auth redirects still apply.
     */
    silent?: boolean;
  }
}

export const http = axios.create({
  baseURL: "",
  withCredentials: true,
  timeout: 60000,
});

/** Per-request options accepted by the API helpers. */
export interface RequestOptions {
  silent?: boolean;
}

interface ErrorBody {
  error?: { code?: string; message?: string };
}

function requestUrl(error: AxiosError<ErrorBody>): string {
  const cfg = error.config;
  if (!cfg) {
    return "";
  }
  return `${cfg.baseURL ?? ""}${cfg.url ?? ""}`;
}

function localizeServerMessage(code: string | undefined, msg: string): string {
  const lower = msg.trim().toLowerCase();
  if (lower === "invalid token" || lower === "missing authentication token") {
    return "口令无效或登录已过期，请重新输入";
  }
  if (lower === "old token is incorrect") {
    return "当前口令不正确";
  }
  if (lower.startsWith("new token must be at least")) {
    return "新口令长度不足（至少需要 24 个字符）";
  }
  if (lower.startsWith("new token must be printable ascii")) {
    return "新口令仅限可打印 ASCII 字符，且不能包含空格、引号、分号、逗号或反斜杠";
  }
  if (lower === "new token must differ from the old token") {
    return "新口令不能与当前口令相同";
  }
  if (code === "too_many_requests" || lower.includes("too many")) {
    return "请求过于频繁，请稍后再试";
  }
  const runNotFound = /^run\s+(.+)\s+not found$/i.exec(msg.trim());
  if (runNotFound) {
    return `压测任务不存在（${runNotFound[1]}）`;
  }
  return msg;
}

/** Human-readable message for any failure, including ones without a JSON body. */
function describeError(error: AxiosError<ErrorBody>): string {
  const errObj = error.response?.data?.error;
  if (errObj?.message) {
    return localizeServerMessage(errObj.code, errObj.message);
  }
  if (!error.response) {
    if (error.code === "ECONNABORTED" || error.code === "ETIMEDOUT") {
      return "请求超时，请稍后重试";
    }
    if (error.code === "ERR_CANCELED") return "请求已取消";
    return "无法连接到服务器，请检查网络或服务状态";
  }
  return `请求失败（HTTP ${error.response.status}）`;
}

http.interceptors.response.use(
  (response) => response,
  async (error: AxiosError<ErrorBody>) => {
    const status = error.response?.status;
    const code = error.response?.data?.error?.code;
    const message = describeError(error);
    // Callers that show their own banner read `err.message`.
    error.message = message;
    const url = requestUrl(error);
    const isMeProbe = url.includes("/api/v1/auth/me");
    const silent = error.config?.silent === true || error.code === "ERR_CANCELED";
    const { default: router } = await import("../router");
    const current = router.currentRoute.value.name;

    if (status === 401) {
      invalidateCachedMe();
    } else if (code === "password_change_required") {
      setCachedMe({
        auth_required: true,
        must_change_password: true,
      });
    }

    if (status === 401 && current !== "login" && !isMeProbe) {
      await router.push({ name: "login" });
    } else if (code === "password_change_required" && current !== "password") {
      await router.push({ name: "password" });
    } else if (!silent && !isMeProbe) {
      toast.error(message);
    }
    return Promise.reject(error);
  },
);
