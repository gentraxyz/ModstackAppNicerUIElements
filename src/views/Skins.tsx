import { useEffect, useRef, useState, useCallback } from "react";
// @ts-ignore
import { SkinViewerGLTF } from "../../src-tauri/src/skin";
import {
  type ArmStyle,
  type SavedSkin,
  loadAllSkins,
  addSkin,
  updateSkin,
  deleteSkin,
  getActiveId,
  setActiveId,
  uploadSkinToMojang,
  applySkinLocally,
} from "../utils/skinsStore";
import { useAuth } from "../stores/authContext";
import { toast } from "@heroui/react";
import { ChangeCapeModal, CapeViewer } from "../components/Capes";

const STEVE_SKIN_URL = "./steve.png";

function detectSlimFromImage(url: string): Promise<ArmStyle> {
  return new Promise((resolve) => {
    const img = new Image();
    img.crossOrigin = "anonymous";
    img.onload = () => {
      try {
        const canvas = document.createElement("canvas");
        canvas.width = 64; canvas.height = 64;
        const ctx = canvas.getContext("2d");
        if (!ctx) return resolve("wide");
        ctx.drawImage(img, 0, 0);
        const data = ctx.getImageData(54, 20, 1, 12).data;
        let transparent = 0;
        for (let i = 3; i < data.length; i += 4) {
          if (data[i] === 0) transparent++;
        }
        resolve(transparent > 6 ? "slim" : "wide");
      } catch { resolve("wide"); }
    };
    img.onerror = () => resolve("wide");
    img.src = url;
  });
}

type Props = {
  skinUrl: string;
  username: string;
  isPremium?: boolean;
  playerUuid?: string;
};

function SkinHead({ skinUrl, size = 64 }: { skinUrl: string; size?: number }) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  useEffect(() => {
    if (!skinUrl) return;
    const img = new Image();
    img.crossOrigin = "anonymous";
    img.onload = () => {
      const canvas = canvasRef.current;
      if (!canvas) return;
      const ctx = canvas.getContext("2d");
      if (!ctx) return;
      canvas.width = size; canvas.height = size;
      ctx.imageSmoothingEnabled = false;
      ctx.clearRect(0, 0, size, size);
      ctx.drawImage(img, 8, 8, 8, 8, 0, 0, size, size);
      ctx.drawImage(img, 40, 8, 8, 8, 0, 0, size, size);
    };
    img.src = skinUrl;
  }, [skinUrl, size]);
  return (
    <canvas
      ref={canvasRef}
      style={{ width: "100%", height: "100%", imageRendering: "pixelated" }}
    />
  );
}

function MiniViewer({ skinUrl, armStyle }: { skinUrl: string; armStyle: ArmStyle }) {
  const containerRef = useRef<HTMLDivElement | null>(null);
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
        const viewer = new SkinViewerGLTF({ canvas, autoRotate: false, autoRotateSpeed: 0.5 });
        await viewer.loadSkin(skinUrl, armStyle);
        viewerRef.current = viewer;
      } catch {}
    })();

    return () => { viewerRef.current?.dispose?.(); viewerRef.current = null; };
  }, [skinUrl, armStyle]);

  return (
    <div ref={containerRef} style={{ width: "100%", height: "100%", overflow: "hidden" }} />
  );
}

type ModalData =
  | { mode: "add"; dataUrl: string; name: string }
  | { mode: "edit"; skin: SavedSkin };

function SkinModal({
  data: initialData,
  onSave,
  onDelete,
  onClose,
  onReplaceTexture,
}: {
  data: ModalData;
  onSave: (result: { name: string; dataUrl: string; armStyle: ArmStyle }) => void;
  onDelete?: () => void;
  onClose: () => void;
  onReplaceTexture: () => void;
}) {
  const isEdit = initialData.mode === "edit";

  const [dataUrl, setDataUrl] = useState(
    isEdit ? initialData.skin.dataUrl : initialData.dataUrl
  );
  const [name, setName] = useState(
    isEdit ? initialData.skin.name : initialData.name
  );
  const [armStyle, setArmStyle] = useState<ArmStyle>(
    isEdit ? initialData.skin.armStyle : "wide"
  );
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!isEdit && dataUrl) {
      detectSlimFromImage(dataUrl).then(setArmStyle);
    }
  }, []);

  useEffect(() => {
    (window as any).__modalSetDataUrl = setDataUrl;
    (window as any).__modalSetName = setName;
    return () => {
      delete (window as any).__modalSetDataUrl;
      delete (window as any).__modalSetName;
    };
  }, []);

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
      <div
        style={{
          position: "fixed",
          top: "50%", left: "50%",
          transform: "translate(-50%, -50%)",
          width: 520,
          background: "#141414",
          border: "1px solid #272727",
          borderRadius: 16,
          zIndex: 101,
          overflow: "hidden",
          boxShadow: "0 32px 80px rgba(0,0,0,0.9)",
        }}
      >
        <div style={{
          display: "flex", alignItems: "center", justifyContent: "space-between",
          padding: "18px 24px", borderBottom: "1px solid #1f1f1f",
        }}>
          <span style={{ fontWeight: 600, fontSize: 16, color: "#fff" }}>
            {isEdit ? "Edit skin" : "Add skin"}
          </span>
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

        <div style={{ display: "flex", minHeight: 380 }}>
          <div style={{
            width: 200, flexShrink: 0,
            background: "#0d0d0d",
            borderRight: "1px solid #1f1f1f",
            position: "relative",
            display: "flex", flexDirection: "column",
            alignItems: "center", justifyContent: "flex-end",
            paddingBottom: 40,
          }}>
            <div style={{ position: "absolute", inset: 0, top: -20, left: -50, right: 0 }}>
              {dataUrl
                ? <MiniViewer skinUrl={dataUrl} armStyle={armStyle} />
                : (
                  <div style={{
                    width: "100%", height: "100%",
                    display: "flex", alignItems: "center", justifyContent: "center",
                    color: "#333", fontSize: 13,
                  }}>No skin</div>
                )
              }
            </div>
            <span style={{
              position: "absolute", bottom: 10, left: 0, right: 0,
              textAlign: "center", color: "#ffffff22", fontSize: 11,
              pointerEvents: "none",
            }}>Drag to rotate</span>
          </div>

          <div style={{
            flex: 1, padding: "20px 24px",
            display: "flex", flexDirection: "column", gap: 18,
          }}>
            <div>
              <label style={{ color: "#666", fontSize: 11, fontWeight: 600, display: "block", marginBottom: 6, textTransform: "uppercase", letterSpacing: "0.05em" }}>
                Name
              </label>
              <input
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="My skin..."
                style={{
                  width: "100%", padding: "8px 12px",
                  background: "#0d0d0d", border: "1px solid #2a2a2a",
                  borderRadius: 8, color: "#fff", fontSize: 13,
                  outline: "none", boxSizing: "border-box",
                  transition: "border-color 0.15s",
                }}
                onFocus={(e) => (e.currentTarget.style.borderColor = "#3a3a3a")}
                onBlur={(e) => (e.currentTarget.style.borderColor = "#2a2a2a")}
              />
            </div>

            <div>
              <label style={{ color: "#666", fontSize: 11, fontWeight: 600, display: "block", marginBottom: 6, textTransform: "uppercase", letterSpacing: "0.05em" }}>
                Texture
              </label>
              <button
                onClick={onReplaceTexture}
                style={{
                  display: "flex", alignItems: "center", gap: 8,
                  padding: "8px 14px",
                  background: "#1a1a1a", border: "1px solid #2a2a2a",
                  borderRadius: 8, color: "#ccc", fontSize: 13,
                  cursor: "pointer", transition: "border-color 0.15s",
                }}
                onMouseEnter={(e) => (e.currentTarget.style.borderColor = "#3a3a3a")}
                onMouseLeave={(e) => (e.currentTarget.style.borderColor = "#2a2a2a")}
              >
                <span style={{ fontSize: 15 }}>↑</span>
                Replace texture
              </button>
            </div>

            <div>
              <label style={{ color: "#666", fontSize: 11, fontWeight: 600, display: "block", marginBottom: 10, textTransform: "uppercase", letterSpacing: "0.05em" }}>
                Arm style
              </label>
              {(["wide", "slim"] as ArmStyle[]).map((style) => (
                <div
                  key={style}
                  onClick={() => setArmStyle(style)}
                  style={{
                    display: "flex", alignItems: "center", gap: 10,
                    cursor: "pointer", marginBottom: 10,
                    color: armStyle === style ? "#fff" : "#555",
                    fontSize: 13, userSelect: "none",
                    transition: "color 0.15s",
                  }}
                >
                  <div style={{
                    width: 16, height: 16, borderRadius: "50%",
                    border: armStyle === style ? "2px solid #4ade80" : "2px solid #333",
                    display: "flex", alignItems: "center", justifyContent: "center",
                    flexShrink: 0, transition: "border-color 0.15s",
                  }}>
                    {armStyle === style && (
                      <div style={{ width: 7, height: 7, borderRadius: "50%", background: "#4ade80" }} />
                    )}
                  </div>
                  {style === "wide" ? "Wide (classic)" : "Slim (thin)"}
                </div>
              ))}
            </div>

            {isEdit && onDelete && (
              <div style={{ marginTop: "auto", paddingTop: 8 }}>
                {!confirmDelete ? (
                  <button
                    onClick={() => setConfirmDelete(true)}
                    style={{
                      padding: "6px 12px",
                      background: "transparent", border: "1px solid #3a1a1a",
                      borderRadius: 8, color: "#ef4444aa", fontSize: 12, cursor: "pointer",
                      transition: "all 0.15s",
                    }}
                    onMouseEnter={(e) => { e.currentTarget.style.background = "#1a0808"; e.currentTarget.style.color = "#ef4444"; }}
                    onMouseLeave={(e) => { e.currentTarget.style.background = "transparent"; e.currentTarget.style.color = "#ef4444aa"; }}
                  >
                    Delete skin
                  </button>
                ) : (
                  <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
                    <span style={{ color: "#666", fontSize: 11 }}>Are you sure?</span>
                    <button
                      onClick={onDelete}
                      style={{ padding: "5px 10px", background: "#ef4444", border: "none", borderRadius: 6, color: "#fff", fontSize: 11, cursor: "pointer" }}
                    >Delete</button>
                    <button
                      onClick={() => setConfirmDelete(false)}
                      style={{ padding: "5px 10px", background: "#1f1f1f", border: "1px solid #2a2a2a", borderRadius: 6, color: "#888", fontSize: 11, cursor: "pointer" }}
                    >Cancel</button>
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
            onClick={async () => {
              if (!dataUrl || saving) return;
              setSaving(true);
              await onSave({ name: name.trim() || "Unnamed", dataUrl, armStyle });
              setSaving(false);
            }}
            disabled={!dataUrl || saving}
            style={{
              padding: "8px 20px",
              background: dataUrl ? "#4ade80" : "#1a2a1a",
              border: "none", borderRadius: 8,
              color: dataUrl ? "#000" : "#4ade8033",
              fontSize: 13, fontWeight: 600,
              cursor: dataUrl && !saving ? "pointer" : "not-allowed",
              transition: "background 0.15s, opacity 0.15s",
              opacity: saving ? 0.7 : 1,
            }}
            onMouseEnter={(e) => { if (dataUrl && !saving) e.currentTarget.style.background = "#6ee7a0"; }}
            onMouseLeave={(e) => { if (dataUrl && !saving) e.currentTarget.style.background = "#4ade80"; }}
          >
            {saving ? "Saving..." : isEdit ? "✓ Save" : "✓ Add skin"}
          </button>
        </div>
      </div>
    </>
  );
}

function SkinCard({
  skin, isActive, onSelect, onEdit, uploading,
}: {
  skin: SavedSkin; isActive: boolean; onSelect: () => void; onEdit: () => void; uploading: boolean;
}) {
  const [hover, setHover] = useState(false);
  return (
    <div
      style={{ width: 72, flexShrink: 0, position: "relative" }}
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
    >
      <div
        onClick={onSelect}
        style={{
          width: 72, height: 72, background: "#111", borderRadius: 10,
          border: isActive ? "2px solid #4ade80" : "2px solid #2a2a2a",
          boxShadow: isActive ? "0 0 10px #4ade8044" : "none",
          overflow: "hidden", cursor: uploading ? "wait" : "pointer",
          transition: "border-color 0.2s, box-shadow 0.2s",
          boxSizing: "border-box",
          opacity: uploading && !isActive ? 0.5 : 1,
        }}
      >
        <SkinHead skinUrl={skin.dataUrl} size={72} />
      </div>

      {isActive && uploading && (
        <div style={{
          position: "absolute", inset: 0,
          display: "flex", alignItems: "center", justifyContent: "center",
          background: "rgba(0,0,0,0.55)", borderRadius: 10,
        }}>
          <div style={{
            width: 18, height: 18,
            border: "2px solid #4ade8033",
            borderTop: "2px solid #4ade80",
            borderRadius: "50%",
            animation: "spin 0.7s linear infinite",
          }} />
        </div>
      )}

      {hover && !uploading && (
        <button
          onClick={(e) => { e.stopPropagation(); onEdit(); }}
          title="Edit"
          style={{
            position: "absolute", top: -7, right: -7,
            width: 22, height: 22, borderRadius: "50%",
            background: "#222", border: "1px solid #3a3a3a",
            color: "#aaa", cursor: "pointer", fontSize: 12,
            display: "flex", alignItems: "center", justifyContent: "center",
            zIndex: 10,
          }}
        >✎</button>
      )}

      <div style={{
        marginTop: 5, textAlign: "center", fontSize: 10,
        color: isActive ? "#4ade80" : "#555",
        whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis",
        maxWidth: 72, transition: "color 0.2s",
      }}>
        {skin.name}
      </div>
    </div>
  );
}

export default function Skins({ skinUrl, username, isPremium = true, playerUuid }: Props) {
  const { user, refreshMicrosoftToken } = useAuth();

  const [capeModalOpen, setCapeModalOpen] = useState(false);
  const [activeCapeId, setActiveCapeId] = useState<string | null>(null);
  const [activeCapeUrl, setActiveCapeUrl] = useState<string | null>(null);

  const fileInputRef = useRef<HTMLInputElement | null>(null);

  const [savedSkins, setSavedSkins] = useState<SavedSkin[]>([]);

  const [activeId, setActiveIdState] = useState<string | null>(null);
  const [activeSkinUrl, setActiveSkinUrl] = useState<string>("");
  const [activeArmStyle, setActiveArmStyle] = useState<ArmStyle>("wide");

  const [modal, setModal] = useState<ModalData | null>(null);
  const [uploading, setUploading] = useState(false);

  useEffect(() => {
    (async () => {
      const savedCapeId = localStorage.getItem("activeCapeId");
      const savedCapeUrl = localStorage.getItem("activeCapeUrl");
      if (savedCapeId) setActiveCapeId(savedCapeId);
      if (savedCapeUrl) setActiveCapeUrl(savedCapeUrl);
      
      const skins = await loadAllSkins();
      setSavedSkins(skins);
      const savedId = getActiveId();

      if (!isPremium) {
        const active = skins.find((s) => s.id === savedId) ?? skins[0] ?? null;
        if (active) {
          setActiveIdState(active.id);
          setActiveSkinUrl(active.dataUrl);
          setActiveArmStyle(active.armStyle);
        } else {
          setActiveSkinUrl(STEVE_SKIN_URL);
          setActiveArmStyle("wide");
        }
      } else {
        const active = skins.find((s) => s.id === savedId);
        if (active) {
          setActiveIdState(active.id);
          setActiveSkinUrl(active.dataUrl);
          setActiveArmStyle(active.armStyle);
        } else {
          setActiveIdState(null);
          setActiveSkinUrl(skinUrl || STEVE_SKIN_URL);
          detectSlimFromImage(skinUrl || STEVE_SKIN_URL).then(setActiveArmStyle);
        }
      }
    })();
  }, [isPremium, skinUrl]);

  const tryUploadToMojang = useCallback(async (dataUrl: string, armStyle: ArmStyle) => {
    if (!isPremium || !user?.minecraft?.access_token) return;

    let token = user.minecraft.access_token;

    const refreshed = await refreshMicrosoftToken();
    if (refreshed) {
      const stored = JSON.parse(localStorage.getItem("userAuth") || "null");
      token = stored?.minecraft?.access_token ?? token;
    }

    setUploading(true);
    const result = await uploadSkinToMojang(dataUrl, armStyle, token);
    setUploading(false);

    if (result.ok) {
      toast.success("Success!", { description: "Skin applied on Microsoft ✓" });
    } else {
      console.error("Upload error:", result.error);
      toast.danger("Error", { description: "Could not apply skin on Microsoft. Try signing out and back in." });
    }
  }, [isPremium, user, refreshMicrosoftToken]);

  const handleSelect = useCallback((skin: SavedSkin) => {
    setActiveIdState(skin.id);
    setActiveSkinUrl(skin.dataUrl);
    setActiveArmStyle(skin.armStyle);
    setActiveId(skin.id);
    if (isPremium) {
      tryUploadToMojang(skin.dataUrl, skin.armStyle);
    } else if (playerUuid) {
      applySkinLocally(skin.dataUrl, playerUuid).then((res) => {
        if (!res.ok) console.error("apply_skin_locally:", res.error);
      });
    }
  }, [isPremium, playerUuid, tryUploadToMojang]);

  const handleDelete = useCallback(async (id: string) => {
    await deleteSkin(id);
    setSavedSkins((prev) => {
      const updated = prev.filter((s) => s.id !== id);
      if (activeId === id) {
        if (isPremium && skinUrl) {
          setActiveIdState(null);
          setActiveSkinUrl(skinUrl);
          setActiveId(null);
          detectSlimFromImage(skinUrl).then(setActiveArmStyle);
        } else {
          const next = updated[0] ?? null;
          setActiveIdState(next?.id ?? null);
          setActiveSkinUrl(next?.dataUrl ?? STEVE_SKIN_URL);
          setActiveArmStyle(next?.armStyle ?? "wide");
          setActiveId(next?.id ?? null);
        }
      }
      return updated;
    });
    setModal(null);
  }, [activeId, isPremium, skinUrl]);

  const handleModalSave = useCallback(async (result: { name: string; dataUrl: string; armStyle: ArmStyle }) => {
    if (modal?.mode === "edit") {
      const id = modal.skin.id;
      await updateSkin(id, result);
      setSavedSkins((prev) =>
        prev.map((s) => s.id === id ? { ...s, ...result } : s)
      );
      if (activeId === id) {
        setActiveSkinUrl(result.dataUrl);
        setActiveArmStyle(result.armStyle);
        tryUploadToMojang(result.dataUrl, result.armStyle);
      }
    } else {
      const newSkin = await addSkin(result);
      setSavedSkins((prev) => [...prev, newSkin]);
      setActiveIdState(newSkin.id);
      setActiveSkinUrl(newSkin.dataUrl);
      setActiveArmStyle(newSkin.armStyle);
      setActiveId(newSkin.id);
      if (isPremium) {
        tryUploadToMojang(newSkin.dataUrl, newSkin.armStyle);
      } else if (playerUuid) {
        applySkinLocally(newSkin.dataUrl, playerUuid).then((res) => {
          if (!res.ok) console.error("apply_skin_locally:", res.error);
        });
      }
        }
        setModal(null);
      }, [modal, activeId, isPremium, playerUuid, tryUploadToMojang]);

  const handleFileChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    const inputEl = e.target;

    if (!file.type.includes("png")) {
      alert("Only PNG files are accepted");
      inputEl.value = "";
      return;
    }

    const reader = new FileReader();
    reader.onload = () => {
      const dataUrl = reader.result as string;
      const img = new Image();
      img.onload = () => {
        if (img.width < 64 || img.height < 64) {
          alert("Skin must be at least 64x64 pixels");
          inputEl.value = "";
          return;
        }
        const name = file.name.replace(/\.png$/i, "");
        if ((window as any).__modalSetDataUrl) {
          (window as any).__modalSetDataUrl(dataUrl);
          (window as any).__modalSetName?.(name);
        } else {
          setModal({ mode: "add", dataUrl, name });
        }
        inputEl.value = "";
      };
      img.onerror = () => { inputEl.value = ""; };
      img.src = dataUrl;
    };
    reader.onerror = () => { inputEl.value = ""; };
    reader.readAsDataURL(file);
  }, []);

  return (
    <div className="w-full h-full bg-[#020803] text-white relative overflow-hidden">

      <input
        ref={fileInputRef}
        type="file"
        accept=".png,image/png"
        style={{ display: "none" }}
        onChange={handleFileChange}
      />

      {modal && (
        <SkinModal
          data={modal}
          onSave={handleModalSave}
          onDelete={modal.mode === "edit" ? () => handleDelete(modal.skin.id) : undefined}
          onClose={() => setModal(null)}
          onReplaceTexture={() => fileInputRef.current?.click()}
        />
      )}

      {capeModalOpen && user?.minecraft?.access_token && (
        <ChangeCapeModal
          skinUrl={activeSkinUrl}
          armStyle={activeArmStyle}
          activeCapeId={activeCapeId}
          accessToken={user.minecraft.access_token}
          onClose={() => setCapeModalOpen(false)}
          onSelect={(id, url) => {
            setActiveCapeId(id);
            setActiveCapeUrl(url);
            if (id) localStorage.setItem("activeCapeId", id);
            else localStorage.removeItem("activeCapeId");
            if (url) localStorage.setItem("activeCapeUrl", url);
            else localStorage.removeItem("activeCapeUrl");
          }}
        />
      )}

      <div className="absolute top-3 left-4 text-2xl font-semibold flex items-center gap-3">
        Wardrobe
        <span className="text-green-400 text-sm px-2 py-0.5 rounded border border-green-400/30 bg-green-400/10">
          BETA
        </span>
        {!isPremium && (
          <span className="text-yellow-400 text-xs px-2 py-0.5 rounded border border-yellow-400/30 bg-yellow-400/10">
            NON-PREMIUM
          </span>
        )}
      </div>

      <div className="absolute left-[80px] top-[70px] flex flex-col items-center">
        <div className="inline-flex items-center bg-[#0b0b0b] px-4 h-[30px] rounded-[10px] border border-[#3a3a3a]">
          <span className="relative top-[4px] font-minecraftia text-[16px] text-white tracking-[1px] leading-none block">
            {username || "Player"}
          </span>
        </div>

        <div className="w-[300px] h-[380px] relative">
          {activeSkinUrl ? (
            <CapeViewer
              key={`${activeSkinUrl}-${activeCapeUrl}-${activeArmStyle}`}
              skinUrl={activeSkinUrl}
              capeUrl={activeCapeUrl}
              armStyle={activeArmStyle}
              initialRotation={Math.PI * 2.12}
            />
          ) : (
            <div className="absolute inset-0 flex items-center justify-center">
              <div className="flex flex-col items-center gap-2 text-center px-8">
                <span className="text-3xl"></span>
                <span className="text-white/40 text-sm">
                  {isPremium ? "No skin available" : "Upload a skin to get started"}
                </span>
              </div>
            </div>
          )}
        </div>

        <span className="text-white/40 text-sm mt-4">Drag to rotate</span>

        {isPremium && user?.minecraft?.access_token && (
          <button
            onClick={() => setCapeModalOpen(true)}
            style={{
              marginTop: 8,
              padding: "5px 14px",
              background: "#1a1a1a", border: "1px solid #2a2a2a",
              borderRadius: 8, color: "#ccc", fontSize: 12,
              cursor: "pointer",
              transition: "border-color 0.15s",
            }}
            onMouseEnter={(e) => (e.currentTarget.style.borderColor = "#3a3a3a")}
            onMouseLeave={(e) => (e.currentTarget.style.borderColor = "#2a2a2a")}
          >
            Change cape
          </button>
        )}
      </div>

      <div className="absolute left-[440px] top-[100px]">
        <div className="flex items-center gap-2 mb-5">
          <h2 className="text-white/60 text-sm">Saved skins</h2>
          {savedSkins.length > 0 && (
            <span className="text-white/30 text-xs bg-white/5 px-2 py-0.5 rounded-full">
              {savedSkins.length}
            </span>
          )}
          {uploading && (
            <span style={{
              fontSize: 11, color: "#4ade80aa",
              display: "flex", alignItems: "center", gap: 5,
            }}>
              <div style={{
                width: 10, height: 10,
                border: "1.5px solid #4ade8033",
                borderTop: "1.5px solid #4ade80",
                borderRadius: "50%",
                animation: "spin 0.7s linear infinite",
                display: "inline-block",
              }} />
              Applying on Microsoft...
            </span>
          )}
        </div>

        <div
          className="hide-scrollbar"
          style={{
            display: "flex", gap: 12, alignItems: "flex-start",
            overflowX: "auto", overflowY: "visible",
            paddingBottom: 20, paddingTop: 8,
            scrollbarWidth: "none",
          }}
        >
          <div
            onClick={() => fileInputRef.current?.click()}
            style={{
              width: 72, height: 72, flexShrink: 0,
              border: "2px dashed #4ade8066", borderRadius: 10,
              display: "flex", alignItems: "center", justifyContent: "center",
              cursor: "pointer", color: "#4ade80", fontSize: 28, fontWeight: 300,
              transition: "background 0.15s",
            }}
            onMouseEnter={(e) => (e.currentTarget.style.background = "rgba(74,222,128,0.08)")}
            onMouseLeave={(e) => (e.currentTarget.style.background = "transparent")}
            title="Add skin"
          >+</div>

          {savedSkins.map((skin) => (
            <SkinCard
              key={skin.id}
              skin={skin}
              isActive={activeId === skin.id}
              uploading={uploading && activeId === skin.id}
              onSelect={() => { if (!uploading) handleSelect(skin); }}
              onEdit={() => setModal({ mode: "edit", skin })}
            />
          ))}
        </div>

        {savedSkins.length === 0 && (
          <p className="text-white/25 text-xs mt-[-10px]"></p>
        )}

        {isPremium && user?.minecraft?.access_token && (
          <p className="text-white/20 text-xs mt-2">
            ✓ Connected to Microsoft — skins are applied automatically
          </p>
        )}

        {!isPremium && (
          <div style={{
            marginTop: 20, padding: "10px 14px",
            background: "#1a1500", border: "1px solid #fbbf2433",
            borderRadius: 10, maxWidth: 280,
          }}>
            <p style={{ color: "#fbbf24", fontSize: 12, marginBottom: 4, fontWeight: 600 }}>
              Non-premium mode
            </p>
            <p style={{ color: "#fbbf2499", fontSize: 11, lineHeight: 1.5 }}>
              La skin se aplica en singleplayer y servidores offline.<br />
              En servidores online con <em>online-mode=true</em> se requiere cuenta premium.
            </p>
          </div>
        )}
      </div>

      <style>{`
        @keyframes spin { to { transform: rotate(360deg); } }
        .hide-scrollbar::-webkit-scrollbar { display: none; }
      `}</style>
    </div>
  );
}