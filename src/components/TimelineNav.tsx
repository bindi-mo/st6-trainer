import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";

interface TimelineEntry {
  frame_start: number;
  frame_end: number;
  round_index: number;
  entry_type: string;
  player: string;
  description: string;
  move_name: string | null;
  damage_p1: number | null;
  damage_p2: number | null;
  conf: number | null;
}

interface TimelineNavProps {
  matchId: string | null;
  controlType: string;
  onJumpToFrame: (frame: number) => void;
  onOpenFrameData: (characterId: string, controlType: string, moveName: string) => void;
}

export function TimelineNav(props: TimelineNavProps) {
  const { matchId, controlType, onJumpToFrame, onOpenFrameData } = props;
  const [timeline, setTimeline] = useState<TimelineEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!matchId) {
      setTimeline([]);
      return;
    }
    let mounted = true;
    async function loadTimeline() {
      setLoading(true);
      setError(null);
      try {
        const data = await invoke<TimelineEntry[]>("get_match_timeline", {
          matchId,
        });
        if (mounted) {
          setTimeline(data);
        }
      } catch (e) {
        if (mounted) {
          setError(e as string);
        }
      } finally {
        if (mounted) setLoading(false);
      }
    }
    loadTimeline();
    return () => { mounted = false; };
  }, [matchId]);

  if (!matchId) {
    return (
      <section className="section">
        <h2>検出タイムライン</h2>
        <div className="timeline-empty">試合を選択してタイムラインを表示してください。</div>
      </section>
    );
  }

  return (
    <section className="section">
      <h2>検出タイムライン</h2>
      {loading && <div className="timeline-loading">読み込み中…</div>}
      {error && <div className="error">タイムライン読み込みエラー: {error}</div>}
      {!loading && !error && (
        <div className="timeline-list">
          {timeline.length === 0 && (
            <div className="timeline-empty">イベントが見つかりません。</div>
          )}
          {timeline.map((entry, idx) => (
            <div
              key={idx}
              className={`timeline-item ${entry.entry_type}`}
              onClick={() => onJumpToFrame(entry.frame_start)}
            >
              <div className="timeline-frame">
                F{entry.frame_start} - F{entry.frame_end} (R{entry.round_index})
              </div>
              <div className="timeline-body">
                <span className={`timeline-player ${entry.player}`}>{entry.player}</span>
                <span className="timeline-desc">{entry.description}</span>
                {entry.move_name && (
                  <span
                    className="timeline-move"
                    onClick={(e) => {
                      e.stopPropagation();
                      onOpenFrameData("luke", controlType, entry.move_name!);
                    }}
                  >
                    {entry.move_name}
                  </span>
                )}
                {(entry.damage_p1 !== null || entry.damage_p2 !== null) && (
                  <span className="timeline-damage">
                    P1:{entry.damage_p1 ?? "-"} / P2:{entry.damage_p2 ?? "-"}
                  </span>
                )}
                {entry.conf !== null && (
                  <span className="timeline-conf">conf:{entry.conf.toFixed(2)}</span>
                )}
              </div>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}
