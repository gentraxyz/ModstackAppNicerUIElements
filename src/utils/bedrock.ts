import { invoke } from "@tauri-apps/api/core";

export interface BedrockStatus {
  installed: boolean;
  version: string | null;
  install_path: string | null;
  platform: string;
  store_installed: boolean;
}

export interface LatestVersion {
  version: string;
  download_url: string;
}

export async function bedrockGetStatus(): Promise<BedrockStatus> {
  return invoke<BedrockStatus>("bedrock_get_status");
}

export async function bedrockGetLatestVersion(msAccessToken: string): Promise<LatestVersion> {
  return invoke<LatestVersion>("bedrock_get_latest_version", { msAccessToken });
}

export async function bedrockInstall(force = false, msAccessToken: string): Promise<void> {
  return invoke("bedrock_install", { force, msAccessToken });
}

export async function bedrockLaunch(): Promise<void> {
  return invoke("bedrock_launch");
}

export async function bedrockUninstall(): Promise<void> {
  return invoke("bedrock_uninstall");
}