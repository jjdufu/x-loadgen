import axios, { AxiosError } from "axios";
import { toast } from "../modules/toast";

export const http = axios.create({
  baseURL: "",
  withCredentials: true,
  timeout: 60000,
});

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

http.interceptors.response.use(
  (response) => response,
  async (error: AxiosError<ErrorBody>) => {
    const status = error.response?.status;
    const code = error.response?.data?.error?.code;
    const message = error.response?.data?.error?.message;
    const url = requestUrl(error);
    const isMeProbe = url.includes("/api/v1/auth/me");
    const { default: router } = await import("../router");
    const current = router.currentRoute.value.name;

    if (status === 401 && current !== "login" && !isMeProbe) {
      await router.push({ name: "login" });
    } else if (code === "password_change_required" && current !== "password") {
      await router.push({ name: "password" });
    } else if (message && !isMeProbe) {
      toast.error(message);
    }
    return Promise.reject(error);
  },
);
