import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";

interface MoveRecord {
  move_id: string;
  character_id: string;
  character_name: string;
  control_type: string;
  version: string;
  move_name: string;
  category: string;
  command: string;
  startup: string;
  active: string;
  recovery: string;
  hit_advantage: string;
  block_advantage: string;
  cancel: string;
  damage: string;
  combo_scaling: string | null;
  drive_gauge_gain_hit: number | null;
  drive_gauge_lose_dguard: number | null;
  drive_gauge_lose_punish: number | null;
  sa_gauge_gain: number | null;
  attribute: string | null;
  note: string | null;
}

interface FrameDataModalProps {
  open: boolean;
  onClose: () => void;
  characterId: string;
  controlType: string;
  moveName: string;
}

export function FrameDataModal(props: FrameDataModalProps) {
  const { open, onClose, characterId, controlType, moveName } = props;
  const [move, setMove] = useState<MoveRecord | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open || !moveName) {
      setMove(null);
      setError(null);
      return;
    }
    let mounted = true;
    async function loadMove() {
      setLoading(true);
      setError(null);
      try {
        const moves = await invoke<MoveRecord[]>("rag_get_moves", {
          characterId,
          controlType,
        });
        if (mounted) {
          const found = moves.find((m) => m.move_name === moveName);
          setMove(found ?? null);
          if (!found) setError(`フレームデータが見つかりません: ${moveName}`);
        }
      } catch (e) {
        if (mounted) setError(e as string);
      } finally {
        if (mounted) setLoading(false);
      }
    }
    loadMove();
    return () => { mounted = false; };
  }, [open, characterId, controlType, moveName]);

  if (!open) return null;

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <h3>フレームデータ詳細</h3>
          <button onClick={onClose} className="modal-close">×</button>
        </div>
        <div className="modal-body">
          {loading && <div>読み込み中…</div>}
          {error && <div className="error">{error}</div>}
          {move && !loading && (
            <div className="frame-data-detail">
              <div className="fdd-row">
                <strong>技名:</strong> {move.move_name}
              </div>
              <div className="fdd-row">
                <strong>キャラクター:</strong> {move.character_name} ({move.character_id})
              </div>
              <div className="fdd-row">
                <strong>操作系:</strong> {move.control_type}
              </div>
              <div className="fdd-row">
                <strong>コマンド:</strong> {move.command || "-"}
              </div>
              <div className="fdd-row">
                <strong>カテゴリ:</strong> {move.category}
              </div>
              <div className="fdd-row">
                <strong>発生:</strong> {move.startup}
              </div>
              <div className="fdd-row">
                <strong>持続:</strong> {move.active}
              </div>
              <div className="fdd-row">
                <strong>硬直:</strong> {move.recovery}
              </div>
              <div className="fdd-row">
                <strong>ヒット有利:</strong> {move.hit_advantage}
              </div>
              <div className="fdd-row">
                <strong>ガード有利:</strong> {move.block_advantage}
              </div>
              <div className="fdd-row">
                <strong>キャンセル:</strong> {move.cancel}
              </div>
              <div className="fdd-row">
                <strong>ダメージ:</strong> {move.damage}
              </div>
              {move.combo_scaling && (
                <div className="fdd-row">
                  <strong>コンボ補正:</strong> {move.combo_scaling}
                </div>
              )}
              {move.attribute && (
                <div className="fdd-row">
                  <strong>属性:</strong> {move.attribute}
                </div>
              )}
              {move.note && (
                <div className="fdd-row">
                  <strong>備考:</strong> {move.note}
                </div>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
