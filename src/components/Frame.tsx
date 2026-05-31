import { useState, useEffect, useRef } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { useNavigation } from "../hooks/useNavigation";
import { Button, ProgressBar } from "@heroui/react";
import {
  IconChevronLeft,
  IconChevronRight,
  IconDownload,
  IconMinus,
  IconSquare,
  IconSquares,
  IconX,
} from "@tabler/icons-react";

interface DownloadItem {
  id: string;
  name: string;
  title?: string;
  instanceId?: string;
  progress: number;
  status: string;
}

function DownloadsPopup() {
  const [downloads, setDownloads] = useState<DownloadItem[]>([]);
  const [open, setOpen] = useState(false);
  const [, setUserClosed] = useState(false);
  const popupRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const unlistenProgress = listen<string>("install-progress", (event) => {
      const [current, total] = event.payload.split("/").map(Number);
      const progress = Math.floor((current / total) * 100);
      setDownloads((prev) =>
        prev.map((d) => (d.id === "install" ? { ...d, progress } : d))
      );
    });

    const unlistenStatus = listen<string>("install-status", (event) => {
      console.log("install-status raw:", event.payload);
      const raw = event.payload;
      let status = "";
      let instanceId: string | undefined;
      let title: string | undefined;
      try {
        const parsed = JSON.parse(raw);
        status = parsed.status ?? raw;
        instanceId = parsed.instanceId;
        title = parsed.title;
      } catch {
        status = raw;
      }
      const safeStatus = status || "Downloading...";
      setDownloads((prev) => {
        const exists = prev.find((d) => d.id === "install");
        if (!exists) {
          return [
            ...prev,
            { id: "install", name: title ?? instanceId ?? "Modstack", instanceId, progress: 0, status: safeStatus },
          ];
        }
        return prev.map((d) =>
          d.id === "install"
            ? { ...d, status: safeStatus, name: title ?? instanceId ?? d.name, instanceId: instanceId ?? d.instanceId }
            : d
        );
      });
      setUserClosed((closed) => {
        if (!closed) setOpen(true);
        return closed;
      });
    });

    const unlistenDone = listen("install-done", () => {
      setDownloads((prev) =>
        prev.map((d) => (d.id === "install" ? { ...d, progress: 100 } : d))
      );
      setTimeout(() => {
        setDownloads((prev) => prev.filter((d) => d.id !== "install"));
        setUserClosed(false);
      }, 1500);
    });

    const unlistenAsset = listen<string>("asset-progress", (event) => {
      const [current, total] = event.payload.split("/").map(Number);
      const progress = Math.floor((current / total) * 100);
      setDownloads((prev) => {
        const exists = prev.find((d) => d.id === "assets");
        if (!exists) {
          return [
            ...prev,
            { id: "assets", name: "Assets", progress, status: "Downloading assets" },
          ];
        }
        return prev.map((d) =>
          d.id === "assets" ? { ...d, progress } : d
        );
      });
      setUserClosed((closed) => {
        if (!closed) setOpen(true);
        return closed;
      });
      if (current === total) {
        setTimeout(() => {
          setDownloads((prev) => prev.filter((d) => d.id !== "assets"));
          setUserClosed(false);
        }, 1500);
      }
    });

    const unlistenAssetStatus = listen<string>("asset-status", (event) => {
      console.log("asset-status raw:", event.payload); // 👈
      const raw = event.payload;
      let status = "";
      let instanceId: string | undefined;
      let title: string | undefined;
      try {
        const parsed = JSON.parse(raw);
        status = parsed.status ?? raw;
        instanceId = parsed.instanceId;
        title = parsed.title;
      } catch {
        status = raw;
      }
      const safeStatus = status || "Downloading assets";
      setDownloads((prev) => {
        const exists = prev.find((d) => d.id === "assets");
        if (!exists) {
          return [
            ...prev,
            {
              id: "assets",
              name: title ?? instanceId ?? "Assets",
              title, 
              instanceId,
              progress: 0,
              status: safeStatus,
            },
          ];
        }
        return prev.map((d) =>
          d.id === "assets"
            ? { ...d, status: safeStatus, instanceId, title: title ?? d.title, name: title ?? instanceId ?? d.name }
            : d
        );
      });
      setUserClosed((closed) => {
        if (!closed) setOpen(true);
        return closed;
      });
    });

    const unlistenJavaStart = listen<{ version: number }>(
      "java-download-start",
      (event) => {
        const { version } = event.payload;
        const id = `java-${version}`;
        setDownloads((prev) => {
          const exists = prev.find((d) => d.id === id);
          if (!exists) {
            return [
              ...prev,
              { id, name: `Java ${version}`, progress: 0, status: "Starting download..." },
            ];
          }
          return prev;
        });
        setUserClosed((closed) => {
          if (!closed) setOpen(true);
          return closed;
        });
      }
    );

    const unlistenJavaProgress = listen<{
      version: number;
      percent: number;
      status: string;
    }>("java-download-progress", (event) => {
      const { version, percent, status } = event.payload;
      const id = `java-${version}`;
      setDownloads((prev) =>
        prev.map((d) => (d.id === id ? { ...d, progress: percent, status } : d))
      );
    });

    const unlistenJavaLog = listen<{ version: number; message: string }>(
      "java-log",
      (event) => {
        const { version, message } = event.payload;
        const id = `java-${version}`;
        setDownloads((prev) =>
          prev.map((d) => (d.id === id ? { ...d, status: message } : d))
        );
      }
    );

    const unlistenJavaDone = listen<{ version: number }>(
      "java-download-done",
      (event) => {
        const { version } = event.payload;
        const id = `java-${version}`;
        setDownloads((prev) =>
          prev.map((d) =>
            d.id === id ? { ...d, progress: 100, status: "Installed" } : d
          )
        );
        setTimeout(() => {
          setDownloads((prev) => prev.filter((d) => d.id !== id));
          setUserClosed(false);
        }, 1500);
      }
    );

    return () => {
      unlistenProgress.then((f) => f());
      unlistenStatus.then((f) => f());
      unlistenDone.then((f) => f());
      unlistenAsset.then((f) => f());
      unlistenAssetStatus.then((f) => f());
      unlistenJavaStart.then((f) => f());
      unlistenJavaProgress.then((f) => f());
      unlistenJavaLog.then((f) => f());
      unlistenJavaDone.then((f) => f());
    };
  }, []);

  useEffect(() => {
    if (!open) return;
    const timer = setTimeout(() => {
      const handler = (e: MouseEvent) => {
        if (popupRef.current && !popupRef.current.contains(e.target as Node)) {
          setOpen(false);
          setUserClosed(true);
        }
      };
      document.addEventListener("mousedown", handler);
      return () => document.removeEventListener("mousedown", handler);
    }, 0);
    return () => clearTimeout(timer);
  }, [open]);

  if (downloads.length === 0) return null;

  return (
    <div className="relative flex items-center" ref={popupRef}>
      <Button
        variant="ghost"
        size="lg"
        isIconOnly
        onPress={() => {
          setOpen((v) => !v);
          setUserClosed(false);
        }}
        className="relative rounded-none ring-inset"
        aria-label="View active downloads"
      >
        <IconDownload size={16} />
        <span className="absolute top-1.5 right-1.5 size-1.5 rounded-full bg-success" />
      </Button>

      {open && (
        <div className="absolute right-0 top-full mt-1 min-w-[316px] max-w-sm w-max z-50 rounded-lg border border-white/10 bg-surface shadow-xl">
          <div className="flex items-center justify-between px-3 py-2 border-b border-white/10">
            <div className="flex items-center gap-2 text-sm font-semibold">
              <IconDownload size={14} className="text-success" />
              Downloads
            </div>
            <Button
              variant="ghost"
              isIconOnly
              onPress={() => {
                setOpen(false);
                setUserClosed(true);
              }}
              className="size-5 rounded"
            >
              <IconX size={12} />
            </Button>
          </div>

          <div className="flex flex-col gap-3 px-3 py-3">
            {downloads.map((item) => (
              <div key={item.id} className="flex flex-col gap-1">
                <span className="text-sm font-semibold text-foreground">
                  {item.title ?? item.name}
                </span>

                <ProgressBar value={item.progress}>
                  <ProgressBar.Track>
                    <ProgressBar.Fill />
                  </ProgressBar.Track>
                </ProgressBar>

                <div className="flex items-center justify-start gap-1.5 text-xs text-muted w-full">
                  <span className="shrink-0">{item.progress}%</span>
                  <span>{item.status}</span>
                </div>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

export default () => {
  const back = useNavigation((state) => state.back);
  const forward = useNavigation((state) => state.forward);
  const canGoBack = useNavigation((state) => state.canGoBack);
  const canGoForward = useNavigation((state) => state.canGoForward);

  const [isMaximized, setIsMaximized] = useState(false);

  const closeApp = () => getCurrentWindow().close();
  const minimizeApp = () =>
    getCurrentWindow()
      .minimize()
      .then(() => setIsMaximized(false));
  const maximizeApp = () =>
    getCurrentWindow()
      .toggleMaximize()
      .then(() => setIsMaximized(true));

  const checkIsMaximized = async () => {
    setIsMaximized(await getCurrentWindow().isMaximized());
  };

  useEffect(() => {
    checkIsMaximized();
    getCurrentWindow().onResized(checkIsMaximized);
  }, []);

  return (
    <div
      data-tauri-drag-region
      className="w-full flex justify-between bg-surface"
    >
      <div data-tauri-drag-region className="px-4 flex items-center gap-x-2">
        <img
          data-tauri-drag-region
          src="./icon.png"
          alt="Logo"
          className="w-6 h-6"
        />
        <img
          data-tauri-drag-region
          src="./modstack-title.png"
          alt="Logo"
          className="h-4 w-auto"
        />
        <div className="flex items-center gap-x-1">
          <Button
            variant="tertiary"
            isIconOnly
            onPress={() => back()}
            isDisabled={!canGoBack}
            className="size-6 rounded-full"
          >
            <IconChevronLeft />
          </Button>
          <Button
            variant="tertiary"
            isIconOnly
            onPress={() => forward()}
            isDisabled={!canGoForward}
            className="size-6 rounded-full"
          >
            <IconChevronRight />
          </Button>
        </div>
      </div>

      <div className="flex items-center">
        <DownloadsPopup />
        <Button
          variant="ghost"
          size="lg"
          isIconOnly
          onPress={minimizeApp}
          className="rounded-none ring-inset"
        >
          <IconMinus />
        </Button>
        <Button
          variant="ghost"
          size="lg"
          isIconOnly
          onPress={maximizeApp}
          className="rounded-none ring-inset"
        >
          {isMaximized ? (
            <IconSquares className="-rotate-90" />
          ) : (
            <IconSquare />
          )}
        </Button>
        <Button
          variant="ghost"
          size="lg"
          isIconOnly
          onPress={closeApp}
          className="rounded-none ring-inset hover:bg-danger-soft-hover hover:text-danger-soft-foreground"
        >
          <IconX />
        </Button>
      </div>
    </div>
  );
};