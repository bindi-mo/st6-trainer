import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";

interface MoveEventRecord {
  frame_start: number;
  frame_end: number;
  round_index: number;
  damage_p1: number | null;
  damage_p2: number | null;
  attack_property_p1: string | null;
  attack_property_p2: string | null;
  conf: number | null;
}

interface InputEventRecord {
  frame_start: number;
  frame_end: number;
  player: string;
  round_index: number;
  directions: string[];
  buttons: string[];
}

interface StatsDashboardProps {
  matchId: string | null;
}

export function StatsDashboard(props: StatsDashboardProps) {
  const { matchId } = props;
  const [tab, setTab] = useState<"damage" | "punish" | "training" | "input">("damage");
  const [moveEvents, setMoveEvents] = useState<MoveEventRecord[]>([]);
  const [inputEvents, setInputEvents] = useState<InputEventRecord[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!matchId) {
      setMoveEvents([]);
      setInputEvents([]);
      return;
    }
    let mounted = true;
    async function loadData() {
      setLoading(true);
      setError(null);
      try {
        // Fetch move events and input events via timeline
        const timeline = await invoke<any[]>("get_match_timeline", { matchId });
        const moves = timeline.filter((t) => t.entry_type === "move");
        const inputs = timeline.filter((t) => t.entry_type === "input");
        if (mounted) {
          setMoveEvents(moves);
          setInputEvents(inputs);
        }
      } catch (e) {
        if (mounted) setError(e as string);
      } finally {
        if (mounted) setLoading(false);
      }
    }
    loadData();
    return () => { mounted = false; };
  }, [matchId]);

  // Damage breakdown stats
  const totalDamageP1 = moveEvents.reduce((sum, m) => sum + (m.damage_p1 || 0), 0);
  const totalDamageP2 = moveEvents.reduce((sum, m) => sum + (m.damage_p2 || 0), 0);
  const moveCount = moveEvents.length;

  // Punish success: moves with block_advantage < 0 followed by counter
  // Simplified: count moves with conf > 0.8 as "confirmed"
  const confirmedPunishes = moveEvents.filter((m) => (m.conf || 0) > 0.8).length;
  const punishRate = moveCount > 0 ? (confirmedPunishes / moveCount) * 100 : 0;

  // Input tendencies: most used buttons
  const buttonCounts: Record<string, number> = {};
  inputEvents.forEach((e) => {
    e.buttons.forEach((b) => {
      buttonCounts[b] = (buttonCounts[b] || 0) + 1;
    });
  });
  const topButtons = Object.entries(buttonCounts)
    .sort((a, b) => b[1] - a[1])
    .slice(0, 5);

  return (
    <section className="section stats-section">
      <h2>統計ダッシュボード</h2>
      <div className="stats-tabs">
        <button
          className={tab === "damage" ? "active" : ""}
          onClick={() => setTab("damage")}
        >
          被弾原因内訳
        </button>
        <button
          className={tab === "punish" ? "active" : ""}
          onClick={() => setTab("punish")}
        >
          反撃成功率
        </button>
        <button
          className={tab === "training" ? "active" : ""}
          onClick={() => setTab("training")}
        >
          トレメニュー達成率
        </button>
        <button
          className={tab === "input" ? "active" : ""}
          onClick={() => setTab("input")}
        >
          入力傾向
        </button>
      </div>

      {loading && <div className="stats-loading">読み込み中…</div>}
      {error && <div className="error">統計読み込みエラー: {error}</div>}

      {!loading && !error && (
        <div className="stats-content">
          {tab === "damage" && (
            <div className="stat-card">
              <h3>ダメージ内訳</h3>
              <div className="stat-row">
                <span>P1 総ダメージ:</span>
                <span className="stat-value">{totalDamageP1}</span>
              </div>
              <div className="stat-row">
                <span>P2 総ダメージ:</span>
                <span className="stat-value">{totalDamageP2}</span>
              </div>
              <div className="stat-row">
                <span>検出技数:</span>
                <span className="stat-value">{moveCount}</span>
              </div>
            </div>
          )}
          {tab === "punish" && (
            <div className="stat-card">
              <h3>反撃成功率</h3>
              <div className="stat-row">
                <span>確定反撃数:</span>
                <span className="stat-value">{confirmedPunishes}</span>
              </div>
              <div className="stat-row">
                <span>反撃成功率:</span>
                <span className="stat-value">{punishRate.toFixed(1)}%</span>
              </div>
              <div className="stat-row">
                <span>検出技数:</span>
                <span className="stat-value">{moveCount}</span>
              </div>
            </div>
          )}
          {tab === "training" && (
            <div className="stat-card">
              <h3>トレーニングメニュー達成率</h3>
              <div className="stat-row">
                <span>推奨メニュー:</span>
                <span className="stat-value">-</span>
              </div>
              <div className="stat-row">
                <span>達成率:</span>
                <span className="stat-value">-</span>
              </div>
              <p className="stat-hint">レポート生成後にトレーニングメニューが表示されます。</p>
            </div>
          )}
          {tab === "input" && (
            <div className="stat-card">
              <h3>入力傾向</h3>
              <div className="stat-row">
                <span>総入力イベント:</span>
                <span className="stat-value">{inputEvents.length}</span>
              </div>
              <ul className="top-buttons">
                {topButtons.map(([btn, count]) => (
                  <li key={btn}>
                    {btn}: {count}回
                  </li>
                ))}
                {topButtons.length === 0 && <li>データなし</li>}
              </ul>
            </div>
          )}
        </div>
      )}
    </section>
  );
}
