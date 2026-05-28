import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "@heroui/react";
// @ts-ignore
import { SkinViewerGLTF } from "../../src-tauri/src/skin";
import type { ArmStyle } from "../utils/skinsStore";

type CapeEntry = {
  id: string;
  alias: string;
  url: string;
};

function CapeThumbnail({
  url,
  size = 56,
  selected,
}: {
  url: string;
  size?: number;
  selected: boolean;
}) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    if (!url) return;
    const img = new Image();
    img.crossOrigin = "anonymous";
    img.onload = () => {
      const canvas = canvasRef.current;
      if (!canvas) return;
      const ctx = canvas.getContext("2d");
      if (!ctx) return;
      const scaleX = img.width / 64;
      const scaleY = img.height / 32;
      canvas.width = size;
      canvas.height = Math.round(size * 1.6);
      ctx.imageSmoothingEnabled = false;
      ctx.clearRect(0, 0, canvas.width, canvas.height);
      ctx.drawImage(
        img,
        1 * scaleX, 1 * scaleY,
        10 * scaleX, 16 * scaleY,
        0, 0,
        canvas.width, canvas.height,
      );
    };
    img.src = url;
  }, [url, size]);

  return (
    <canvas
      ref={canvasRef}
      style={{
        imageRendering: "pixelated",
        borderRadius: 4,
        outline: selected ? "2px solid #4ade80" : "2px solid transparent",
        boxShadow: selected ? "0 0 8px #4ade8055" : "none",
        transition: "outline 0.15s, box-shadow 0.15s",
      }}
    />
  );
}

export function CapeViewer({  
  skinUrl,
  capeUrl,
  armStyle,
  initialRotation = Math.PI * 1.18,
}: {
  skinUrl: string;
  capeUrl: string | null;
  armStyle: ArmStyle;
  initialRotation?: number;
}) {
  const containerRef = useRef<HTMLDivElement>(null);
  const viewerRef = useRef<any>(null);

  useEffect(() => {
    const el = containerRef.current;
    if (!el || !skinUrl) return;

    viewerRef.current?.dispose?.();
    viewerRef.current = null;

    el.innerHTML = "";
    const canvas = document.createElement("canvas");
    el.appendChild(canvas);

    (async () => {
      try {
        const viewer = new SkinViewerGLTF({
          canvas,
          autoRotate: false,
          initialRotation,
          cape: capeUrl ?? undefined,
        });
        await viewer.loadSkin(skinUrl, armStyle);
        if (capeUrl) await viewer.loadCape(capeUrl);
        viewerRef.current = viewer;
      } catch {}
    })();

    return () => { viewerRef.current?.dispose?.(); viewerRef.current = null; };
  }, [skinUrl, capeUrl, armStyle, initialRotation]);

  return (
    <div ref={containerRef} style={{ width: "100%", height: "100%", overflow: "hidden" }} />
  );
}

export function ChangeCapeModal({
  skinUrl,
  armStyle,
  activeCapeId,
  onClose,
  onSelect,
  accessToken,
}: {
  skinUrl: string;
  armStyle: ArmStyle;
  activeCapeId: string | null;
  onClose: () => void;
  onSelect: (capeId: string | null, capeUrl: string | null) => void;
  accessToken: string;
}) {
  const [capes, setCapes] = useState<CapeEntry[]>([]);
  const [selected, setSelected] = useState<string | null>(activeCapeId);
  const [loading, setLoading] = useState(true);
  const [applying, setApplying] = useState(false);

  const previewCape = capes.find((c) => c.id === selected)?.url ?? null;

  useEffect(() => {
    (async () => {
      try {
        const result = await invoke<CapeEntry[]>("get_player_capes", { accessToken });
        setCapes(result);
      } catch {
        toast.danger("Error", { description: "Could not load capes." });
      } finally {
        setLoading(false);
      }
    })();
  }, [accessToken]);

  const handleConfirm = async () => {
    if (applying) return;
    setApplying(true);
    try {
      await invoke("set_active_cape", { capeId: selected ?? "", accessToken });
      toast.success("Cape applied!", {
        description: selected ? "Cape activated ✓" : "Cape removed ✓",
      });
      const selectedUrl = capes.find((c) => c.id === selected)?.url ?? null;
      onSelect(selected, selectedUrl);
      onClose();
    } catch (e: any) {
      toast.danger("Error", { description: e?.message ?? "Could not apply cape." });
    } finally {
      setApplying(false);
    }
  };

  return (
    <>
      <div
        onClick={onClose}
        style={{
          position: "fixed", inset: 0,
          background: "rgba(0,0,0,0.78)",
          backdropFilter: "blur(5px)",
          zIndex: 100,
        }}
      />

      <div style={{
        position: "fixed",
        top: "50%", left: "50%",
        transform: "translate(-50%,-50%)",
        width: 540,
        background: "#141414",
        border: "1px solid #272727",
        borderRadius: 16,
        zIndex: 101,
        overflow: "hidden",
        boxShadow: "0 32px 80px rgba(0,0,0,0.9)",
      }}>
        <div style={{
          display: "flex", alignItems: "center", justifyContent: "space-between",
          padding: "18px 24px", borderBottom: "1px solid #1f1f1f",
        }}>
          <span style={{ fontWeight: 600, fontSize: 16, color: "#fff" }}>Change cape</span>
          <button
            onClick={onClose}
            style={{
              width: 28, height: 28, borderRadius: 8,
              background: "#1f1f1f", border: "1px solid #2a2a2a",
              color: "#888", cursor: "pointer", fontSize: 16,
              display: "flex", alignItems: "center", justifyContent: "center",
            }}
          >×</button>
        </div>

        <div style={{ display: "flex", minHeight: 360 }}>
          <div style={{
            width: 190, flexShrink: 0,
            background: "#0d0d0d",
            borderRight: "1px solid #1f1f1f",
            display: "flex", flexDirection: "column",
            alignItems: "center", justifyContent: "flex-end",
            paddingBottom: 32,
            position: "relative",
          }}>
            <div style={{ position: "absolute", inset: 0, top: -20, left: -50, right: 0 }}>
              <CapeViewer skinUrl={skinUrl} capeUrl={previewCape} armStyle={armStyle} />
            </div>
            <span style={{
              position: "absolute", bottom: 10,
              color: "#ffffff22", fontSize: 11, pointerEvents: "none",
            }}>Drag to rotate</span>
          </div>

          <div style={{ flex: 1, padding: 20, overflowY: "auto", maxHeight: 380 }}>
            {loading ? (
              <div style={{
                display: "flex", alignItems: "center", justifyContent: "center", height: 200,
              }}>
                <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 8, color: "#555", fontSize: 13 }}>
                  <div style={{
                    width: 20, height: 20,
                    border: "2px solid #4ade8033",
                    borderTop: "2px solid #4ade80",
                    borderRadius: "50%",
                    animation: "spin 0.7s linear infinite",
                  }} />
                  Loading capes...
                </div>
              </div>
            ) : (
              <div style={{
                display: "grid",
                gridTemplateColumns: "repeat(auto-fill, minmax(72px, 1fr))",
                gap: 12,
              }}>
                <div
                  onClick={() => setSelected(null)}
                  style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 6, cursor: "pointer" }}
                >
                  <div style={{
                    width: 56, height: 90,
                    border: selected === null ? "2px solid #4ade80" : "2px solid #2a2a2a",
                    borderRadius: 8, background: "#1a1a1a",
                    display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center",
                    gap: 4,
                    boxShadow: selected === null ? "0 0 8px #4ade8055" : "none",
                    transition: "border-color 0.15s, box-shadow 0.15s",
                    fontSize: 11,
                    color: selected === null ? "#4ade80" : "#444",
                  }}>
                    <span style={{ fontSize: 20 }}>✕</span>
                    <span>None</span>
                  </div>
                </div>

                {capes.map((cape) => (
                  <div
                    key={cape.id}
                    onClick={() => setSelected(cape.id)}
                    style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 6, cursor: "pointer" }}
                  >
                    <CapeThumbnail url={cape.url} size={56} selected={selected === cape.id} />
                    <span style={{
                      fontSize: 10,
                      color: selected === cape.id ? "#4ade80" : "#555",
                      textAlign: "center", maxWidth: 70,
                      overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
                      transition: "color 0.15s",
                    }}>
                      {cape.alias}
                    </span>
                  </div>
                ))}

                {capes.length === 0 && (
                  <div style={{
                    gridColumn: "1/-1", color: "#444",
                    fontSize: 12, textAlign: "center", paddingTop: 40,
                  }}>
                    No capes found on your account.
                  </div>
                )}
              </div>
            )}
          </div>
        </div>

        <div style={{
          display: "flex", justifyContent: "flex-end", gap: 10,
          padding: "14px 24px", borderTop: "1px solid #1f1f1f",
        }}>
          <button
            onClick={onClose}
            style={{
              padding: "8px 18px",
              background: "transparent", border: "1px solid #2a2a2a",
              borderRadius: 8, color: "#888", fontSize: 13, cursor: "pointer",
            }}
            onMouseEnter={(e) => (e.currentTarget.style.borderColor = "#3a3a3a")}
            onMouseLeave={(e) => (e.currentTarget.style.borderColor = "#2a2a2a")}
          >
            Cancel
          </button>
          <button
            onClick={handleConfirm}
            disabled={applying}
            style={{
              padding: "8px 20px",
              background: "#4ade80",
              border: "none", borderRadius: 8,
              color: "#000", fontSize: 13, fontWeight: 600,
              cursor: applying ? "not-allowed" : "pointer",
              opacity: applying ? 0.7 : 1,
              transition: "background 0.15s, opacity 0.15s",
            }}
            onMouseEnter={(e) => { if (!applying) e.currentTarget.style.background = "#6ee7a0"; }}
            onMouseLeave={(e) => { if (!applying) e.currentTarget.style.background = "#4ade80"; }}
          >
            {applying ? "Applying..." : "✓ Select"}
          </button>
        </div>
      </div>

      <style>{`@keyframes spin { to { transform: rotate(360deg); } }`}</style>
    </>
  );
}