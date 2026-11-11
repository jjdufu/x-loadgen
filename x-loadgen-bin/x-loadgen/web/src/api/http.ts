import axios, { AxiosError } from "axios";
import { ElMessage } from "element-plus";

export const http = axios.create({
  baseURL: "",
  withCredentials: true,
  timeout: 20000,
});

interface ErrorBody {
  error?: { code?: string; message?: string };
}

http.interceptors.response.use(
  (response) => response,
  async (error: AxiosError<ErrorBody>) => {
    const status = error.response?.status;
    const code = error.response?.data?.error?.code;
    const message = error.response?.data?.error?.message;
    const { default: router } = await import("../router");
    const current = router.currentRoute.value.name;
    if (status === 401 && current !== "login") {
      await router.push({ name: "login" });
    } else if (code === "password_change_required" && current !== "password") {
      await router.push({ name: "password" });
    } else if (message) {
      ElMessage.error(message);
    }
    return Promise.reject(error);
  },
);
