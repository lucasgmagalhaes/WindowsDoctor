// Ponto de restauração antes de alterar o sistema (opção do usuário).

import type { Api } from "./api";
import type { Notify } from "./App";

const KEY = "restorePointBeforeChanges";

export function restorePointEnabled(): boolean {
  try {
    return localStorage.getItem(KEY) !== "0";
  } catch {
    return true;
  }
}

export function setRestorePointEnabled(on: boolean) {
  try {
    localStorage.setItem(KEY, on ? "1" : "0");
  } catch {
    // Sem armazenamento: vale só nesta sessão.
  }
}

/**
 * Chame antes de qualquer alteração. Cria o ponto só na primeira vez da sessão;
 * uma falha é avisada mas não impede a alteração.
 */
export async function beforeChange(api: Api, reason: string, notify?: Notify) {
  if (!restorePointEnabled()) return;
  try {
    const r = await api.ensureRestorePoint(reason);
    if (r?.kind === "created") notify?.("Ponto de restauração criado antes da primeira alteração.");
    if (r?.kind === "failed") notify?.(`Não foi possível criar o ponto de restauração: ${r.message}`, "err");
  } catch (e) {
    notify?.(`Não foi possível criar o ponto de restauração: ${e}`, "err");
  }
}

// ---------------------------------------------------------------- monitor de tela preta

const MONITOR_KEY = "blackScreenMonitor";

export function blackScreenMonitorEnabled(): boolean {
  try {
    return localStorage.getItem(MONITOR_KEY) === "1";
  } catch {
    return false;
  }
}

export function setBlackScreenMonitorEnabled(on: boolean) {
  try {
    localStorage.setItem(MONITOR_KEY, on ? "1" : "0");
  } catch {
    // Vale só nesta sessão.
  }
}
