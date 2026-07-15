// SPDX-License-Identifier: Apache-2.0

import { invoke } from "@tauri-apps/api/core";
import { useEffect } from "react";

const signal = (ready: boolean) => {
  invoke("set_frontend_ready", { ready }).catch(() => {
    // Non-Tauri context (browser dev) or teardown mid-flight: nothing to do.
  });
};

/**
 * Tells the backend when the webview is safe to emit into. The backend buffers
 * background events until this fires, avoiding the tauri-runtime-wry reentrancy
 * panic when an emit lands during startup or a reload (see src-tauri/src/emit_gate.rs).
 */
export function useFrontendReady(): void {
  useEffect(() => {
    signal(true);

    const onHide = () => signal(false);
    window.addEventListener("beforeunload", onHide);
    window.addEventListener("pagehide", onHide);
    return () => {
      window.removeEventListener("beforeunload", onHide);
      window.removeEventListener("pagehide", onHide);
    };
  }, []);
}
