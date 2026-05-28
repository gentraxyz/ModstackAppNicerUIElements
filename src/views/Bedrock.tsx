import { useEffect, useState } from "react";
import { Button } from "@heroui/react";
import { listen } from "@tauri-apps/api/event";
import NewsCarousel from "../components/NewsCarousel";
import bedrockHero from "../assets/bedrock_hero.jpg";
import {
  bedrockGetStatus,
  bedrockInstall,
  bedrockLaunch,
  BedrockStatus,
} from "../utils/bedrock";
import { useAuth } from "../stores/authContext";

type PlayState =
  | "checking"
  | "not_installed"
  | "installing"
  | "ready"
  | "launching"
  | "playing"
  | "error";

export default function Bedrock() {
  const { refreshMicrosoftToken } = useAuth();

  const [status, setStatus] = useState<BedrockStatus | null>(null);
  const [playState, setPlayState] = useState<PlayState>("checking");
  const [errorMsg, setErrorMsg] = useState("");
  const [showAlreadyInstalled, setShowAlreadyInstalled] = useState(false);

  useEffect(() => {
    checkStatus();

    const unlisten: Array<() => void> = [];

    listen<BedrockStatus>("bedrock-already-installed", (e) => {
      setStatus(e.payload);
      setPlayState("ready");
      setShowAlreadyInstalled(true);
    }).then((u) => unlisten.push(u));

    listen("bedrock-installed", () => {
      setPlayState("ready");
      setShowAlreadyInstalled(false);
      checkStatus();
    }).then((u) => unlisten.push(u));

    listen("bedrock-launched", () => {
      setPlayState("playing");
      setShowAlreadyInstalled(false);
    }).then((u) => unlisten.push(u));

    listen("bedrock-closed", () => {
      setPlayState("ready");
    }).then((u) => unlisten.push(u));

    return () => unlisten.forEach((u) => u());
  }, []);

  async function checkStatus() {
    setPlayState("checking");
    try {
      const s = await bedrockGetStatus();
      setStatus(s);
      setPlayState(s.installed ? "ready" : "not_installed");
    } catch {
      setPlayState("error");
      setErrorMsg("Error checking Bedrock status.");
    }
  }

  async function getMsToken(): Promise<string> {
    const fresh = await refreshMicrosoftToken();
    if (!fresh) throw new Error("No active Microsoft session. Please sign in first.");
    return fresh;
  }

  async function handleInstall(force = false) {
    setPlayState("installing");
    setErrorMsg("");
    setShowAlreadyInstalled(false);
    try {
      const token = await getMsToken();
      await bedrockInstall(force, token);
    } catch (e: any) {
      setErrorMsg(e?.toString() ?? "Unknown error");
      setPlayState("error");
    }
  }

  async function handlePlay() {
    setPlayState("launching");
    setErrorMsg("");
    setShowAlreadyInstalled(false);
    try {
      await bedrockLaunch();
    } catch (e: any) {
      setErrorMsg(e?.toString() ?? "Error launching game");
      setPlayState("error");
      setTimeout(() => setPlayState("ready"), 4000);
    }
  }

  function buttonLabel(): string {
    switch (playState) {
      case "checking":      return "Checking...";
      case "not_installed": return "Install Bedrock";
      case "installing":    return "Installing...";
      case "ready":         return "Play Bedrock";
      case "launching":     return "Launching...";
      case "playing":       return "Playing";
      case "error":         return "Retry";
      default:              return "Play Bedrock";
    }
  }

  function isButtonDisabled(): boolean {
    return ["checking", "installing", "launching", "playing"].includes(playState);
  }

  function handleButtonPress() {
    if (isButtonDisabled()) return;
    if (playState === "not_installed" || playState === "error") handleInstall(false);
    else if (playState === "ready") handlePlay();
  }

  return (
    <div className="w-full h-full flex flex-col min-h-0">
      <div className="flex-1 overflow-y-auto min-h-0">

        <div className="w-full h-[50vh] overflow-hidden flex-shrink-0">
          <img
            src={bedrockHero}
            alt="Minecraft Bedrock"
            className="w-full h-full object-cover"
          />
        </div>

        <div className="h-14 grid grid-cols-3 bg-surface-secondary shadow flex-shrink-0 sticky top-0 z-10">
          <div className="flex items-center px-4 text-sm text-foreground/50 select-none">
            {status?.version && <span>v{status.version}</span>}
          </div>

          <Button
            isDisabled={isButtonDisabled()}
            onPress={handleButtonPress}
            className="justify-self-center relative -top-5 min-w-64 w-auto h-14 font-minecraft text-3xl text-shadow-[0_3px_#0000005e] text-foreground bg-transparent hover:saturate-80 disabled:opacity-100 disabled:hover:saturate-30"
          >
            <svg
              xmlns="http://www.w3.org/2000/svg"
              preserveAspectRatio="none"
              viewBox="0 0 496 108"
              fill="none"
              className="absolute -z-10 w-full h-full"
            >
              <path
                d="M2 10v88h8v8h476v-8h8V10h-8V2H10v8H2z"
                fill="color-mix(in srgb, var(--color-accent) 50%, black 50%)"
                stroke="#000"
                strokeWidth={4}
              />
              <path d="M12 10v88h472V10H12z" fill="var(--color-accent)" />
              <path
                d="M12 11h472V4H12v6z"
                fill="color-mix(in srgb, var(--color-accent) 80%, white 20%)"
              />
            </svg>
            <span className="relative z-10 text-center leading-tight px-4">
              {buttonLabel()}
            </span>
          </Button>

          <div />
        </div>

        {playState === "error" && errorMsg && (
          <div className="mx-4 mt-2 px-3 py-2 rounded bg-red-500/10 border border-red-500/30 text-red-400 text-xs">
            {errorMsg}
          </div>
        )}

        {showAlreadyInstalled && (
          <div className="mx-4 mt-3 p-4 rounded-lg bg-surface-secondary border border-accent/30 flex flex-col gap-3">
            <p className="text-sm text-foreground/80">
              Minecraft Bedrock{" "}
              {status?.store_installed
                ? "is already installed from the Microsoft Store"
                : "is already installed"}
              .{" "}
              {status?.version && (
                <span className="text-foreground/50">(v{status.version})</span>
              )}
            </p>
            <p className="text-xs text-foreground/50">What would you like to do?</p>
            <div className="flex gap-2">
              <Button
                size="sm"
                className="bg-accent text-foreground font-minecraft"
                onPress={handlePlay}
              >
                Launch
              </Button>
              <Button
                size="sm"
                className="border-foreground/20 text-foreground/60 font-minecraft"
                onPress={() => handleInstall(true)}
              >
                Reinstall
              </Button>
              <Button
                size="sm"
                className="text-foreground/40 font-minecraft ml-auto"
                onPress={() => setShowAlreadyInstalled(false)}
              >
                Dismiss
              </Button>
            </div>
          </div>
        )}

        <div className="p-4">
          <NewsCarousel />
        </div>

      </div>
    </div>
  );
}