import { invoke } from "@tauri-apps/api/core";

export type Info = {
  version: string;
  data_dir: string;
  port: number;
  pid: number;
  started_at: string;
};

let baseUrl: string | undefined;

/** The loopback URL of the server the app started; asked once from the shell. */
export async function serverUrl(): Promise<string> {
  baseUrl ??= await invoke<string>("server_url");
  return baseUrl;
}

export async function getInfo(): Promise<Info> {
  const response = await fetch(`${await serverUrl()}/api/v1/info`);
  if (!response.ok) throw new Error(`info: ${response.status}`);
  return response.json();
}
