import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";

interface MatchSummary {
  id: string;
  played_at: string | null;
  my_character: string;
  opponent_character: string;
  my_control_type: string;
  opponent_control_type: string;
  total_rounds: number;
  result: string;
  source_video_path: string | null;
  scan_config_version: string | null;
}

interface MatchHistoryProps {
  onLoadMatch: (match: MatchSummary) => void;
}

export function MatchHistory(props: MatchHistoryProps) {
  const { onLoadMatch } = props;
  const [matches, setMatches] = useState<MatchSummary[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let mounted = true;
    async function loadMatches() {
      setLoading(true);
      setError(null);
      try {
        const data = await invoke<MatchSummary[]>("rag_list_matches", { limit: 20 });
        if (mounted) setMatches(data);
      } catch (e) {
        if (mounted) setError(e as string);
      } finally {
        if (mounted) setLoading(false);
      }
    }
    loadMatches();
    return () => { mounted = false; };
  }, []);

  return (
    <section className="section match-history-section">
      <h2>試合履歴</h2>
      {loading && <div className="history-loading">読み込み中…</div>}
      {error && <div className="error">履歴読み込みエラー: {error}</div>}
      {!loading && !error && (
        <div className="history-list">
          {matches.length === 0 && (
            <div className="history-empty">試合履歴がありません。</div>
          )}
          {matches.map((m) => (
            <div
              key={m.id}
              className="history-item"
              onClick={() => onLoadMatch(m)}
            >
              <div className="history-match-id">{m.id}</div>
              <div className="history-detail">
                {m.my_character} ({m.my_control_type}) vs {m.opponent_character} ({m.opponent_control_type})
              </div>
              <div className="history-detail">
                結果: {m.result} / ラウンド: {m.total_rounds}
              </div>
              <div className="history-detail">
                {m.played_at ? new Date(m.played_at).toLocaleString() : "-"}
              </div>
              {m.source_video_path && (
                <div className="history-video" title={m.source_video_path}>
                  📹 {m.source_video_path.split(/[\\/]/).pop() ?? m.source_video_path}
                </div>
              )}
            </div>
          ))}
        </div>
      )}
    </section>
  );
}
