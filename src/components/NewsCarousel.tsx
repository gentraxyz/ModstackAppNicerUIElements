import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type NewsItem = {
  id: string;
  title: string;
  content: string;
  image: string;
  createdAt: string;
  published: boolean;
};

function NewsModal({ item, onClose }: { item: NewsItem; onClose: () => void }) {
  const date = new Date(item.createdAt).toLocaleDateString("en", {
    day: "numeric", month: "long", year: "numeric",
  });

  return (
    <>
      <div
        onClick={onClose}
        style={{
          position: "fixed", inset: 0,
          background: "rgba(0,0,0,0.85)",
          backdropFilter: "blur(6px)",
          zIndex: 200,
        }}
      />

      <div
        style={{
          position: "fixed",
          top: "50%", left: "50%",
          transform: "translate(-50%, -50%)",
          width: "min(680px, 90vw)",
          maxHeight: "90vh",
          background: "#111",
          border: "1px solid #222",
          borderRadius: 12,
          zIndex: 201,
          overflow: "hidden",
          display: "flex",
          flexDirection: "column",
        }}
      >
        <div style={{ width: "100%", aspectRatio: "16/7", position: "relative", flexShrink: 0 }}>
          <img
            src={item.image}
            alt={item.title}
            style={{ width: "100%", height: "100%", objectFit: "cover", display: "block" }}
            onError={(e) => { (e.currentTarget as HTMLImageElement).style.display = "none"; }}
          />

          <button
            onClick={onClose}
            style={{
              position: "absolute", top: 12, right: 12,
              width: 32, height: 32, borderRadius: "50%",
              background: "rgba(0,0,0,0.6)", border: "1px solid #ffffff22",
              color: "#fff", cursor: "pointer", fontSize: 16,
              display: "flex", alignItems: "center", justifyContent: "center",
            }}
          >×</button>

          <div style={{ position: "absolute", bottom: 12, left: 16 }}>
            <span style={{
              fontSize: 10, fontWeight: 600,
              background: "#4ade80", color: "#000",
              padding: "2px 8px", borderRadius: 3,
              letterSpacing: "0.03em",
            }}>News</span>
          </div>
        </div>

        <div style={{ padding: "20px 24px 28px", overflowY: "auto", flex: 1,}}>
          <p style={{ margin: "0 0 8px", fontSize: 11, color: "#4ade80", fontWeight: 600, letterSpacing: "0.05em", textTransform: "uppercase" }}>
            {date}
          </p>
          <h2 style={{ margin: "0 0 14px", fontSize: 20, fontWeight: 700, color: "#fff", lineHeight: 1.3 }}>
            {item.title}
          </h2>
          <p style={{ margin: 0, fontSize: 14, color: "#aaa", lineHeight: 1.7, whiteSpace: "pre-wrap", userSelect: "text" }}>
            {item.content}
          </p>
        </div>
      </div>
    </>
  );
}

export default function NewsCarousel() {
  const [news, setNews] = useState<NewsItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [selected, setSelected] = useState<NewsItem | null>(null);

  useEffect(() => {
    invoke<NewsItem[]>("get_news")
      .then((data) => {
        setNews(data.filter((n) => n.published));
        setLoading(false);
      })
      .catch(() => setLoading(false));
  }, []);

  if (loading) {
    return (
      <div style={{ display: "flex", alignItems: "center", justifyContent: "center", height: 200 }}>
        <div style={{
          width: 20, height: 20,
          border: "2px solid #ffffff11",
          borderTop: "2px solid #4ade80",
          borderRadius: "50%",
          animation: "spin 0.8s linear infinite",
        }} />
        <style>{`@keyframes spin { to { transform: rotate(360deg); } }`}</style>
      </div>
    );
  }

  if (!news.length) {
    return (
      <div style={{
        display: "flex", alignItems: "center", justifyContent: "center",
        height: 200, color: "#444", fontSize: 13,
      }}>
        No news available
      </div>
    );
  }

  return (
    <>
      {selected && (
        <NewsModal item={selected} onClose={() => setSelected(null)} />
      )}

      <div style={{
        display: "grid",
        gridTemplateColumns: "repeat(auto-fill, minmax(200px, 1fr))",
        gap: 12,
        padding: "4px 2px",
      }}>
        {news.map((item) => (
          <NewsCard key={item.id} item={item} onClick={() => setSelected(item)} />
        ))}
      </div>

      <style>{`
        @keyframes spin { to { transform: rotate(360deg); } }
        .news-card { transition: transform 0.15s ease, border-color 0.15s ease; }
        .news-card:hover { transform: translateY(-2px); border-color: #3a3a3a !important; }
        .news-card:hover .news-title { color: #fff !important; }
      `}</style>
    </>
  );
}

function NewsCard({ item, onClick }: { item: NewsItem; onClick: () => void }) {
  const date = new Date(item.createdAt).toLocaleDateString("en", {
    day: "numeric", month: "long", year: "numeric",
  });

  return (
    <div
      className="news-card"
      onClick={onClick}
      style={{
        background: "#111",
        border: "1px solid #222",
        borderRadius: 8,
        overflow: "hidden",
        cursor: "pointer",
        display: "flex",
        flexDirection: "column",
      }}
    >
      <div style={{ width: "100%", aspectRatio: "16/9", overflow: "hidden", position: "relative" }}>
        <img
          src={item.image}
          alt={item.title}
          style={{ width: "100%", height: "100%", objectFit: "cover", display: "block" }}
          onError={(e) => { (e.currentTarget as HTMLImageElement).style.display = "none"; }}
        />
        <div style={{ position: "absolute", bottom: 6, left: 6, display: "flex", gap: 4 }}>
          <span style={{
            fontSize: 10, fontWeight: 600,
            background: "#4ade80", color: "#000",
            padding: "2px 7px", borderRadius: 3,
            letterSpacing: "0.03em",
          }}>News</span>
        </div>
      </div>

      <div style={{
        padding: "10px 12px 12px",
        display: "flex", flexDirection: "column", gap: 4,
        flex: 1,
      }}>
        <h3
          className="news-title"
          style={{
            margin: 0, fontSize: 13, fontWeight: 600,
            color: "#ddd", lineHeight: 1.35,
            transition: "color 0.15s",
          }}
        >
          {item.title}
        </h3>

        {item.content && (
          <p style={{
            margin: 0, fontSize: 11, color: "#666",
            lineHeight: 1.5,
            display: "-webkit-box",
            WebkitLineClamp: 2,
            WebkitBoxOrient: "vertical",
            overflow: "hidden",
          }}>
            {item.content}
          </p>
        )}

        <span style={{ marginTop: "auto", paddingTop: 8, fontSize: 10, color: "#444" }}>
          {date}
        </span>
      </div>
    </div>
  );
}