import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { useEffect, useRef, useState } from "react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import "./App.css";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { FrameDataModal } from "./components/FrameDataModal";
import { HeaderControls } from "./components/HeaderControls";
import { MatchHistory } from "./components/MatchHistory";
import { StatsDashboard } from "./components/StatsDashboard";
import { TimelineNav } from "./components/TimelineNav";
import { useToast } from "./hooks/useToast";

interface VideoScanResult {
  video_path: string;
  resolution: { width: number; height: number };
  fps: number;
  total_frames: number;
  duration_ms: number;
  frames: FrameData[];
  hud_layout: HudLayout;
  metadata: ScanMetadata;
}

interface ScanMetadata {
  match_id: string;
  character_name_p1: string;
  character_name_p2: string;
  control_type_p1: string;
  control_type_p2: string;
  detected_at_frame: number;
  source_resolution: string;
  source_fps: number;
  canonical_raster: [number, number];
  top_y_offset_px: number;
  bottom_y_offset_px: number;
  offset_snapped: boolean;
  manual_verified: boolean | null;
  input_history_available: boolean;
  attack_property_locale: string;
  extracted_at_utc: string;
  config_version: string;
  template_pack_version: string;
  extractor_version: string;
}

interface FrameData {
  frame_index: number;
  timestamp_ms: number;
  p1_inputs: number[];
  p2_inputs: number[];
  p1_drive_gauge: { blocks: number; is_burnout: boolean; pixel_intensity: number };
  p2_drive_gauge: { blocks: number; is_burnout: boolean; pixel_intensity: number };
  system_notification: { text: string; notification_type: string; confidence: number } | null;
  frame_meter: { startup: number; active: number; recovery: number; hit_advantage: number; block_advantage: number } | null;
}

interface HudLayout {
  timer_center: [number, number];
  p1_health_start: [number, number];
  p2_health_start: [number, number];
  p1_drive_gauge: { x_min: number; y_min: number; x_max: number; y_max: number };
  p2_drive_gauge: { x_min: number; y_min: number; x_max: number; y_max: number };
  system_notification: { x_min: number; y_min: number; x_max: number; y_max: number };
  frame_meter: { x_min: number; y_min: number; x_max: number; y_max: number };
}

// ===== LLM types =====

interface WorkerStatus {
  state: string;
  model_loaded: boolean;
  pipe_name: string;
}

interface ChatMessage {
  id: string;
  role: "user" | "assistant";
  content: string;
  timestamp: number;
}

interface DownloadProgress {
  model_file_name: string;
  bytes_downloaded: number;
  total_bytes: number | null;
}

interface CharacterSummary {
  character_id: string;
  character_name: string;
}

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

interface CacheEntryInfo {
  cache_key: string;
  video_path: string;
  video_sha256: string;
  match_id: string;
  character_p1: string;
  character_p2: string;
  total_frames: number;
  cached_at: string;
  file_size_bytes: number;
}

function App() {
  // ===== Scanner state (existing) =====
  const [selectedVideo, setSelectedVideo] = useState<string | null>(null);
  const [scanResult, setScanResult] = useState<VideoScanResult | null>(null);
  const [isScanning, setIsScanning] = useState(false);
  const [scanError, setScanError] = useState<string | null>(null);
  const [scanProgress, setScanProgress] = useState(0);
  const [scanProgressDetail, setScanProgressDetail] = useState<{
    frame_index: number;
    total_frames: number;
    progress_percent: number;
    elapsed_ms: number;
    estimated_remaining_ms: number;
    estimated_finish_at: number | null;
  } | null>(null);
  // Manual calibration state (Phase 6)
  const [manualTopY, setManualTopY] = useState<number | null>(null);
  const [manualBottomY, setManualBottomY] = useState<number | null>(null);
  const [manualVerified, setManualVerified] = useState<boolean | null>(null);
  const [calibError, setCalibError] = useState<string | null>(null);
  const [calibSaving, setCalibSaving] = useState(false);

  // ===== LLM state =====
  const [workerStatus, setWorkerStatus] = useState<WorkerStatus | null>(null);
  const [workerStatusError, setWorkerStatusError] = useState<string | null>(null);
  const [models, setModels] = useState<string[]>([]);
  const [modelsLoading, setModelsLoading] = useState(false);
  const [selectedModel, setSelectedModel] = useState("");
  const [workerStartError, setWorkerStartError] = useState<string | null>(null);
  const [isWorkerStarting, setIsWorkerStarting] = useState(false);
  const [chatInput, setChatInput] = useState("");
  const [isStreaming, setIsStreaming] = useState(false);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [reportMarkdown, setReportMarkdown] = useState<string | null>(null);
  const [reportError, setReportError] = useState<string | null>(null);
  const [isGeneratingReport, setIsGeneratingReport] = useState(false);
  const [modelDownloadProgress, setModelDownloadProgress] = useState<DownloadProgress | null>(null);

  // ===== Phase 14: Frontend feature state =====
  const [characters, setCharacters] = useState<CharacterSummary[]>([]);
  const [charactersLoading, setCharactersLoading] = useState(false);
  const [myCharacter, setMyCharacter] = useState("");
  const [opponentCharacter, setOpponentCharacter] = useState("");
  const [controlType, setControlType] = useState("classic");
  const [deviceType, setDeviceType] = useState("pad");
  const [activeMatchId, setActiveMatchId] = useState<string | null>(null);
  const [selectedMove, setSelectedMove] = useState<{ characterId: string; controlType: string; moveName: string } | null>(null);
  const [frameDataModalOpen, setFrameDataModalOpen] = useState(false);

  // ===== Scan cache state =====
  const [cacheEntries, setCacheEntries] = useState<CacheEntryInfo[]>([]);
  const [cacheLoading, setCacheLoading] = useState(false);
  const [cacheError, setCacheError] = useState<string | null>(null);

  const { toasts, toast, dismiss } = useToast();
  const messagesEndRef = useRef<HTMLDivElement>(null);

  // Scroll chat to bottom
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages, isStreaming]);

  // Listen for LLM stream tokens
  useEffect(() => {
    const unlisten = listen<{ token: string }>("llm-stream-token", (event) => {
      const token = event.payload.token;
      setMessages((prev) => {
        const last = prev[prev.length - 1];
        if (last && last.role === "assistant") {
          return [
            ...prev.slice(0, -1),
            { ...last, content: last.content + token },
          ];
        }
        return prev;
      });
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  // Listen for model download progress
  useEffect(() => {
    const unlisten = listen<DownloadProgress>("model-download-progress", (event) => {
      setModelDownloadProgress(event.payload);
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  // Listen for scan progress
  useEffect(() => {
    const unlisten = listen<{
      frame_index: number;
      total_frames: number;
      progress_percent: number;
      elapsed_ms: number;
      estimated_remaining_ms: number;
      estimated_finish_at: number | null;
    }>("scan-progress", (event) => {
      setScanProgressDetail(event.payload);
      setScanProgress(Math.round(event.payload.progress_percent));
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  // Load characters on startup
  useEffect(() => {
    let mounted = true;
    async function loadCharacters() {
      setCharactersLoading(true);
      try {
        const data = await invoke<CharacterSummary[]>("get_characters");
        if (mounted) setCharacters(data);
      } catch (e) {
        console.error("[App] Failed to load characters:", e);
      } finally {
        if (mounted) setCharactersLoading(false);
      }
    }
    loadCharacters();
    return () => { mounted = false; };
  }, []);

  // Load available models on startup
  useEffect(() => {
    let mounted = true;
    async function loadModels() {
      setModelsLoading(true);
      try {
        const data = await invoke<string[]>("list_models");
        if (mounted) setModels(data);
      } catch (e) {
        console.error("[App] Failed to load models:", e);
      } finally {
        if (mounted) setModelsLoading(false);
      }
    }
    loadModels();
    return () => { mounted = false; };
  }, []);

  // Load scan cache entries on startup
  useEffect(() => {
    loadCacheEntries();
  }, []);

  // ===== LLM worker management =====
  async function startWorker() {
    if (!selectedModel) {
      setWorkerStartError("モデルを選択してください");
      return;
    }
    setIsWorkerStarting(true);
    setWorkerStartError(null);
    try {
      const status = await invoke<WorkerStatus>("llm_worker_start", {
        modelPath: selectedModel,
        ctx_size: 4096,
        gpu_layers: 35,
      });
      setWorkerStatus(status);
      setWorkerStatusError(null);
      toast("LLM ワーカーを起動しました", "success");
    } catch (error) {
      setWorkerStartError(error as string);
      toast("LLM ワーカー起動に失敗しました", "error");
    } finally {
      setIsWorkerStarting(false);
    }
  }

  async function shutdownWorker() {
    try {
      await invoke("llm_worker_shutdown");
      setWorkerStatus(null);
      setWorkerStatusError(null);
      setMessages([]);
      toast("LLM ワーカーを停止しました", "info");
    } catch (error) {
      console.error("[App] Worker shutdown error:", error);
    }
  }

  // ===== Scanner functions (existing) =====
  async function selectVideo() {
    const filePath = await open({
      multiple: false,
      filters: [{ name: "Video", extensions: ["mp4", "mkv", "avi", "mov"] }],
    });
    if (filePath) {
      setSelectedVideo(filePath as string);
      setScanResult(null);
      setScanError(null);
      setManualTopY(null);
      setManualBottomY(null);
      setManualVerified(null);
      setCalibError(null);
      setActiveMatchId(null);
    }
  }

  async function scanVideo() {
    if (!selectedVideo) return;

    console.log("[Frontend] Starting scan for:", selectedVideo);
    setIsScanning(true);
    setScanError(null);
    setScanProgress(0);
    setScanProgressDetail(null);
    setManualTopY(null);
    setManualBottomY(null);
    setManualVerified(null);
    setCalibError(null);
    setActiveMatchId(null);

    try {
      console.log("[Frontend] Invoking scan_video command...");
      const result = await invoke("scan_video", { videoPath: selectedVideo });
      console.log("[Frontend] Scan completed, result:", result);
      setScanResult(result as VideoScanResult);

      const meta = (result as VideoScanResult).metadata;
      if (meta) {
        setManualTopY(meta.top_y_offset_px);
        setManualBottomY(meta.bottom_y_offset_px);
        setManualVerified(meta.manual_verified);
        setActiveMatchId(meta.match_id);
        setMyCharacter(meta.character_name_p1);
        setOpponentCharacter(meta.character_name_p2);
        setControlType(meta.control_type_p1);
      }
    } catch (error) {
      console.error("[Frontend] Scan error:", error);
      setScanError(error as string);
    } finally {
      setIsScanning(false);
      setScanProgress(100);
    }
  }

  async function adjustOffset(kind: "top" | "bottom", delta: number) {
    if (!selectedVideo) return;
    if (kind === "top") {
      setManualTopY((prev) => (prev === null ? null : prev + delta));
    } else {
      setManualBottomY((prev) => (prev === null ? null : prev + delta));
    }
  }

  async function saveManualOffsets() {
    if (!selectedVideo) return;
    if (manualTopY === null || manualBottomY === null) return;

    setCalibSaving(true);
    setCalibError(null);

    try {
      const success = await invoke("set_manual_offsets", {
        videoPath: selectedVideo,
        topYOffsetPx: manualTopY,
        bottomYOffsetPx: manualBottomY,
        manualVerified: true,
      });
      if (success === true) {
        setManualVerified(true);
        if (scanResult) {
          setScanResult({
            ...scanResult,
            metadata: {
              ...scanResult.metadata,
              top_y_offset_px: manualTopY,
              bottom_y_offset_px: manualBottomY,
              manual_verified: true,
            },
          });
        }
      } else {
        setCalibError("Failed to save manual offsets");
      }
    } catch (error) {
      console.error("[Frontend] Manual offset save error:", error);
      setCalibError(error as string);
    } finally {
      setCalibSaving(false);
    }
  }

  // ===== Scan cache functions =====
  async function loadCacheEntries() {
    setCacheLoading(true);
    setCacheError(null);
    try {
      const entries = await invoke<CacheEntryInfo[]>("get_scan_cache_info");
      setCacheEntries(entries);
    } catch (error) {
      console.error("[App] Failed to load cache entries:", error);
      setCacheError(error as string);
    } finally {
      setCacheLoading(false);
    }
  }

  async function clearCache() {
    setCacheLoading(true);
    setCacheError(null);
    try {
      const count = await invoke<number>("clear_scan_cache");
      setCacheEntries([]);
      toast(`キャッシュを${count}件削除しました`, "info");
    } catch (error) {
      console.error("[App] Failed to clear cache:", error);
      setCacheError(error as string);
      toast("キャッシュの削除に失敗しました", "error");
    } finally {
      setCacheLoading(false);
    }
  }

  // ===== Phase 14: Match history load =====
  async function loadMatchFromHistory(match: MatchSummary) {
    setActiveMatchId(match.id);
    setMyCharacter(match.my_character);
    setOpponentCharacter(match.opponent_character);
    setControlType(match.my_control_type);
    toast("試合を読み込みました: " + match.id, "info");
  }

  // ===== Phase 14: Frame data modal =====
  function openFrameDataModal(characterId: string, ctrlType: string, moveName: string) {
    setSelectedMove({ characterId, controlType: ctrlType, moveName });
    setFrameDataModalOpen(true);
  }

  // ===== LLM functions =====
  async function sendChatMessage() {
    const text = chatInput.trim();
    if (!text || isStreaming) return;

    const userMsg: ChatMessage = {
      id: Date.now().toString(),
      role: "user",
      content: text,
      timestamp: Date.now(),
    };
    setMessages((prev) => [...prev, userMsg]);
    setChatInput("");

    const assistantMsg: ChatMessage = {
      id: (Date.now() + 1).toString(),
      role: "assistant",
      content: "",
      timestamp: Date.now(),
    };
    setMessages((prev) => [...prev, assistantMsg]);
    setIsStreaming(true);

    try {
      await invoke("llm_stream", {
        prompt: text,
        temperature: 0.7,
        top_p: 0.9,
        max_tokens: 1024,
      });
    } catch (error) {
      console.error("[Frontend] LLM stream error:", error);
      setMessages((prev) => {
        const last = prev[prev.length - 1];
        if (last && last.role === "assistant" && last.id === assistantMsg.id) {
          return [
            ...prev.slice(0, -1),
            { ...last, content: "エラー: " + (error as string) },
          ];
        }
        return prev;
      });
      toast("LLM 推論に失敗しました", "error");
    } finally {
      setIsStreaming(false);
    }
  }

  async function cancelInference() {
    try {
      await invoke("llm_cancel");
      setIsStreaming(false);
      toast("推論をキャンセルしました", "info");
    } catch (error) {
      console.error("[Frontend] Cancel error:", error);
    }
  }

  async function generateReport(template: "analyze_damage" | "verify_punish" | "training_menu") {
    if (!scanResult) {
      toast("スキャン結果がありません。まず動画をスキャンしてください。", "error");
      return;
    }

    setIsGeneratingReport(true);
    setReportError(null);
    setReportMarkdown(null);

    try {
      let result: { prompt: string };
      if (template === "analyze_damage") {
        result = await invoke("rag_generate_analyze_damage", {
          match_id: scanResult.metadata.match_id,
        });
      } else if (template === "verify_punish") {
        result = await invoke("rag_generate_verify_punish", {
          match_id: scanResult.metadata.match_id,
          guarded_move: "",
          guard_frame: 0,
        });
      } else {
        result = await invoke("rag_generate_training_menu", {
          match_id: scanResult.metadata.match_id,
        });
      }

      // Send the generated prompt to the LLM for a response
      const assistantMsg: ChatMessage = {
        id: Date.now().toString(),
        role: "assistant",
        content: "",
        timestamp: Date.now(),
      };
      setMessages((prev) => [...prev, assistantMsg]);
      setIsStreaming(true);

      try {
        await invoke("llm_stream", {
          prompt: result.prompt,
          temperature: 0.7,
          top_p: 0.9,
          max_tokens: 2048,
        });
      } catch (error) {
        console.error("[Frontend] Report LLM error:", error);
        setReportError(error as string);
        toast("レポート生成に失敗しました", "error");
      } finally {
        setIsStreaming(false);
      }
    } catch (error) {
      setReportError(error as string);
      toast("プロンプト生成に失敗しました", "error");
    } finally {
      setIsGeneratingReport(false);
    }
  }

  return (
    <main className="container">
      <HeaderControls
        onSelectVideo={selectVideo}
        isScanning={isScanning}
        myCharacter={myCharacter}
        setMyCharacter={setMyCharacter}
        opponentCharacter={opponentCharacter}
        setOpponentCharacter={setOpponentCharacter}
        controlType={controlType}
        setControlType={setControlType}
        deviceType={deviceType}
        setDeviceType={setDeviceType}
        characters={characters}
        charactersLoading={charactersLoading}
      />

      {/* Toasts */}
      <div className="toast-container">
        {toasts.map((t) => (
          <div
            key={t.id}
            className={`toast toast-${t.type}`}
            onClick={() => dismiss(t.id)}
          >
            {t.message}
          </div>
        ))}
      </div>

      <div className="app-body">
        {/* Left pane: Scanner + Timeline */}
        <div className="pane left-pane">
          <ErrorBoundary
            fallback={({ error, reset }) => (
              <div className="section error">
                <h3>スキャナーエラー</h3>
                <p>{error.message}</p>
                <button onClick={reset}>復旧</button>
              </div>
            )}
          >
            <section className="section">
              <h2>Video Scanner</h2>

              {selectedVideo && (
                <span className="file-path">{selectedVideo}</span>
              )}

              <button
                onClick={scanVideo}
                disabled={isScanning || !selectedVideo}
                className="primary"
              >
                {isScanning ? `Scanning... ${scanProgress}%` : "Scan Video"}
              </button>

              {isScanning && scanProgressDetail && (
                <div className="scan-progress">
                  <div className="progress-label">
                    <span>{scanProgressDetail.frame_index} / {scanProgressDetail.total_frames} フレーム</span>
                    <span>{scanProgress}%</span>
                  </div>
                  <div className="progress-bar">
                    <div
                      className="progress-fill"
                      style={{ width: `${scanProgress}%` }}
                    ></div>
                  </div>
                  <div className="progress-info">
                    <span>
                      経過: {(scanProgressDetail.elapsed_ms / 1000).toFixed(1)}s
                    </span>
                    {scanProgressDetail.estimated_remaining_ms > 0 && (
                      <span>
                        残り: {(scanProgressDetail.estimated_remaining_ms / 1000).toFixed(1)}s
                      </span>
                    )}
                    {scanProgressDetail.estimated_finish_at && (
                      <span>
                        終了予定: {new Date(scanProgressDetail.estimated_finish_at).toLocaleTimeString()}
                      </span>
                    )}
                  </div>
                </div>
              )}

              {scanError && (
                <div className="error">Error: {scanError}</div>
              )}

              {/* Scan Cache Management */}
              <div className="section">
                <h3>スキャンキャッシュ</h3>
                <p>
                  スキャン結果をディスクにキャッシュし、再スキャンなしで
                  後処理をやり直せます。キャッシュは動画ファイルのハッシュと
                  スキャナー設定バージョンでキー化されます。
                </p>
                <div className="row">
                  <button
                    onClick={loadCacheEntries}
                    disabled={cacheLoading}
                    className="secondary"
                  >
                    {cacheLoading ? "読み込み中..." : "キャッシュ一覧を更新"}
                  </button>
                  <button
                    onClick={clearCache}
                    disabled={cacheLoading || cacheEntries.length === 0}
                    className="secondary"
                  >
                    キャッシュを全削除
                  </button>
                </div>
                {cacheError && <div className="error">キャッシュエラー: {cacheError}</div>}
                {cacheEntries.length > 0 && (
                  <div className="cache-list">
                    <table>
                      <thead>
                        <tr>
                          <th>Match ID</th>
                          <th>P1</th>
                          <th>P2</th>
                          <th>フレーム数</th>
                          <th>キャッシュ日時</th>
                          <th>サイズ</th>
                        </tr>
                      </thead>
                      <tbody>
                        {cacheEntries.map((entry) => (
                          <tr key={entry.cache_key}>
                            <td>{entry.match_id}</td>
                            <td>{entry.character_p1}</td>
                            <td>{entry.character_p2}</td>
                            <td>{entry.total_frames}</td>
                            <td>{new Date(entry.cached_at).toLocaleString()}</td>
                            <td>{(entry.file_size_bytes / 1024).toFixed(1)} KB</td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                )}
                {cacheEntries.length === 0 && !cacheLoading && (
                  <div className="hint">キャッシュエントリはありません。</div>
                )}
              </div>

              {scanResult && (
                <>
                  <div className="result">
                    <h3>Scan Results</h3>
                    <div className="info-grid">
                      <div><strong>Video:</strong> {scanResult.video_path}</div>
                      <div><strong>Resolution:</strong> {scanResult.resolution.width}x{scanResult.resolution.height}</div>
                      <div><strong>FPS:</strong> {scanResult.fps.toFixed(2)}</div>
                      <div><strong>Total Frames:</strong> {scanResult.total_frames}</div>
                      <div><strong>Duration:</strong> {(scanResult.duration_ms / 1000).toFixed(1)}s</div>
                      <div><strong>Frames Processed:</strong> {scanResult.frames.length}</div>
                    </div>

                    <h4>Metadata</h4>
                    <div className="info-grid">
                      <div><strong>Match ID:</strong> {scanResult.metadata.match_id}</div>
                      <div><strong>Character P1:</strong> {scanResult.metadata.character_name_p1}</div>
                      <div><strong>Character P2:</strong> {scanResult.metadata.character_name_p2}</div>
                      <div><strong>Control Type P1:</strong> {scanResult.metadata.control_type_p1}</div>
                      <div><strong>Control Type P2:</strong> {scanResult.metadata.control_type_p2}</div>
                      <div><strong>Top Y Offset:</strong> {scanResult.metadata.top_y_offset_px} px</div>
                      <div><strong>Bottom Y Offset:</strong> {scanResult.metadata.bottom_y_offset_px} px</div>
                      <div><strong>Manual Verified:</strong> {String(scanResult.metadata.manual_verified)}</div>
                    </div>

                    {/* Phase 6: Manual Calibration UI */}
                    <div className="section">
                      <h3>Manual Calibration (§2.7 / §7.1)</h3>
                      <p>
                        手動キャリブレーションオフセットを±1px単位で調整し、{" "}
                        <strong>manual_verified</strong> フラグを設定します。{" "}
                        オフセットを変更すると match_id が更新され、従来の誤レコードと混同されなくなります。
                      </p>

                      <div className="row">
                        <div className="calib-field">
                          <label>Top Y Offset (px)</label>
                          <div className="stepper">
                            <button onClick={() => adjustOffset("top", -1)} disabled={calibSaving || manualTopY === null}>
                              −
                            </button>
                            <span className="value">{manualTopY === null ? "—" : manualTopY}</span>
                            <button onClick={() => adjustOffset("top", 1)} disabled={calibSaving || manualTopY === null}>
                              +
                            </button>
                          </div>
                        </div>
                        <div className="calib-field">
                          <label>Bottom Y Offset (px)</label>
                          <div className="stepper">
                            <button onClick={() => adjustOffset("bottom", -1)} disabled={calibSaving || manualBottomY === null}>
                              −
                            </button>
                            <span className="value">{manualBottomY === null ? "—" : manualBottomY}</span>
                            <button onClick={() => adjustOffset("bottom", 1)} disabled={calibSaving || manualBottomY === null}>
                              +
                            </button>
                          </div>
                        </div>
                      </div>

                      <button
                        onClick={saveManualOffsets}
                        disabled={
                          calibSaving ||
                          manualTopY === null ||
                          manualBottomY === null ||
                          manualVerified === true
                        }
                        className="primary"
                      >
                        {calibSaving ? "Saving..." : "Save Manual Offsets (Verify)"}
                      </button>

                      {manualVerified === true && (
                        <div className="success">Manual offsets verified: match_id will reflect the adjusted offsets.</div>
                      )}
                      {calibError && <div className="error">Calibration error: {calibError}</div>}
                    </div>

                    <h4>Frame Data (first 10)</h4>
                    <div className="frame-list">
                      {scanResult.frames.slice(0, 10).map((frame, idx) => (
                        <div key={idx} className="frame-item">
                          <strong>Frame {frame.frame_index}</strong> ({frame.timestamp_ms.toFixed(1)}ms)
                          <br />
                          P1 Drive: {frame.p1_drive_gauge.blocks} blocks {frame.p1_drive_gauge.is_burnout ? "(BURNOUT)" : ""}
                          | P2 Drive: {frame.p2_drive_gauge.blocks} blocks {frame.p2_drive_gauge.is_burnout ? "(BURNOUT)" : ""}
                          {frame.system_notification && (
                            <span className="notification">
                              | Notification: {frame.system_notification.text} ({frame.system_notification.confidence.toFixed(2)})
                            </span>
                          )}
                          {frame.frame_meter && (
                            <span className="frame-meter">
                              | Meter: S:{frame.frame_meter.startup} A:{frame.frame_meter.active} R:{frame.frame_meter.recovery}
                            </span>
                          )}
                        </div>
                      ))}
                      {scanResult.frames.length > 10 && (
                        <div className="more-frames">
                          ... and {scanResult.frames.length - 10} more frames
                        </div>
                      )}
                    </div>
                  </div>
                </>
              )}
            </section>
          </ErrorBoundary>

          {/* Phase 14: Timeline navigation */}
          <TimelineNav
            matchId={activeMatchId}
            controlType={controlType}
            onJumpToFrame={(frame) => {
              // Scroll to or highlight the frame in the scanner results
              console.log("[App] Jump to frame:", frame);
              toast(`フレーム F${frame} へジャンプ`, "info");
            }}
            onOpenFrameData={(characterId, ctrlType, moveName) => {
              openFrameDataModal(characterId, ctrlType, moveName);
            }}
          />
        </div>

        {/* Right pane: Match history + LLM Chat + Report + Stats */}
        <div className="pane right-pane">
          {/* Phase 14: Match history sidebar */}
          <MatchHistory onLoadMatch={loadMatchFromHistory} />

          <ErrorBoundary
            fallback={({ error, reset }) => (
              <div className="section error">
                <h3>LLM エラー</h3>
                <p>{error.message}</p>
                <button onClick={reset}>復旧</button>
              </div>
            )}
          >
            {/* LLM モデル選択・ワーカー管理パネル */}
            <section className="section llm-model-panel">
              <h2>LLM モデル</h2>
              <div className="model-selection">
                <div className="model-list">
                  <label>使用モデル:</label>
                  {modelsLoading && <span className="loading">読み込み中…</span>}
                  <select
                    value={selectedModel}
                    onChange={(e) => setSelectedModel(e.target.value)}
                    disabled={modelsLoading || isWorkerStarting || (workerStatus?.state === "Ready" || workerStatus?.state === "Busy")}
                  >
                    <option value="">モデルを選択してください</option>
                    {models.map((m) => (
                      <option key={m} value={m}>
                        {m}
                      </option>
                    ))}
                  </select>
                  {models.length === 0 && !modelsLoading && (
                    <span className="model-empty">モデルが見つかりません。モデルをダウンロードしてください。</span>
                  )}
                </div>
                <div className="worker-actions">
                  {workerStatus ? (
                    <>
                      <span className={`worker-status ${workerStatus.state.toLowerCase()}`}>
                        {workerStatus.state === "Ready" ? "● 準備完了" : workerStatus.state === "Busy" ? "● 処理中" : workerStatus.state === "Error" ? "● エラー" : "○ " + workerStatus.state}
                      </span>
                      <button
                        onClick={shutdownWorker}
                        disabled={workerStatus.state !== "Ready" && workerStatus.state !== "Busy"}
                        className="secondary"
                      >
                        ワーカー停止
                      </button>
                    </>
                  ) : (
                    <>
                      <button
                        onClick={startWorker}
                        disabled={isWorkerStarting || !selectedModel || modelsLoading}
                        className="primary"
                      >
                        {isWorkerStarting ? "起動中…" : "ワーカー起動"}
                      </button>
                    </>
                  )}
                  {workerStartError && <div className="error">{workerStartError}</div>}
                </div>
              </div>
              {workerStatusError && (
                <div className="error">ワーカー状態エラー: {workerStatusError}</div>
              )}
            </section>

            {/* LLM チャット */}
            <section className="section llm-chat-section">
              <h2>LLM チャット</h2>
              <p className="section-hint">
                ワーカー起動後、メッセージを入力して LLM と対話できます。
              </p>
              <div className="chat-messages" ref={messagesEndRef}>
                {messages.map((m) => (
                  <div
                    key={m.id}
                    className={`chat-message ${m.role}`}
                  >
                    <strong>{m.role === "user" ? "あなた" : "アシスタント"}:</strong>
                    <div className="message-content">{m.content}</div>
                  </div>
                ))}
                {isStreaming && (
                  <div className="chat-message assistant">
                    <strong>アシスタント:</strong>
                    <div className="message-content streaming">生成中…</div>
                  </div>
                )}
              </div>

              <div className="chat-input-row">
                <input
                  type="text"
                  value={chatInput}
                  onChange={(e) => setChatInput(e.target.value)}
                  onKeyDown={(e) => e.key === "Enter" && !e.shiftKey && sendChatMessage()}
                  placeholder="メッセージを入力して Enter で送信"
                  disabled={isStreaming || !workerStatus || workerStatus.state !== "Ready"}
                />
                <button
                  onClick={sendChatMessage}
                  disabled={isStreaming || !chatInput.trim() || !workerStatus || workerStatus.state !== "Ready"}
                  className="primary"
                >
                  {isStreaming ? "生成中…" : "送信"}
                </button>
                {isStreaming && (
                  <button onClick={cancelInference} className="secondary">
                    キャンセル
                  </button>
                )}
              </div>
            </section>

            {/* レポート生成 */}
            <section className="section report-section">
              <h2>レポート生成</h2>
              <p className="section-hint">
                スキャン済みの試合データから、LLM を使ってレポートを生成します。
              </p>
              <div className="report-controls">
                <button
                  onClick={() => generateReport("analyze_damage")}
                  disabled={isGeneratingReport || !scanResult || !workerStatus || workerStatus.state !== "Ready"}
                  className="primary"
                >
                  {isGeneratingReport ? "生成中…" : "被弾原因分析レポート"}
                </button>
                <button
                  onClick={() => generateReport("verify_punish")}
                  disabled={isGeneratingReport || !scanResult || !workerStatus || workerStatus.state !== "Ready"}
                  className="secondary"
                >
                  {isGeneratingReport ? "生成中…" : "確定反撃検証レポート"}
                </button>
                <button
                  onClick={() => generateReport("training_menu")}
                  disabled={isGeneratingReport || !scanResult || !workerStatus || workerStatus.state !== "Ready"}
                  className="secondary"
                >
                  {isGeneratingReport ? "生成中…" : "練習メニューレポート"}
                </button>
              </div>

              {reportError && (
                <div className="error">レポートエラー: {reportError}</div>
              )}

              {reportMarkdown ? (
                <div className="report-content">
                  <Markdown components={{
                    h1: (props) => <h1 {...props} />,
                    h2: (props) => <h2 {...props} />,
                    h3: (props) => <h3 {...props} />,
                    p: (props) => <p {...props} />,
                    table: (props) => <table {...props} />,
                    tr: (props) => <tr {...props} />,
                    th: (props) => <th {...props} />,
                    td: (props) => <td {...props} />,
                    code: (props) => <code {...props} />,
                    pre: (props) => <pre {...props} />,
                    blockquote: (props) => <blockquote {...props} />,
                    ul: (props) => <ul {...props} />,
                    ol: (props) => <ol {...props} />,
                    li: (props) => <li {...props} />,
                    strong: (props) => <strong {...props} />,
                    em: (props) => <em {...props} />,
                  }} remarkPlugins={[remarkGfm]}>
                    {reportMarkdown}
                  </Markdown>
                </div>
              ) : !isGeneratingReport && !reportError && !scanResult ? (
                <div className="report-empty">動画をスキャンしてレポートを生成してください。</div>
              ) : null}
            </section>
          </ErrorBoundary>

          {/* Phase 14: Statistics dashboard */}
          <StatsDashboard matchId={activeMatchId} />
        </div>
      </div>

      {/* Model download progress bar */}
      {modelDownloadProgress && (
        <div className="download-progress">
          <div className="download-header">
            <span>モデルダウンロード: {modelDownloadProgress.model_file_name}</span>
            <button onClick={() => setModelDownloadProgress(null)}>×</button>
          </div>
          <div className="progress-bar">
            <div
              className="progress-fill"
              style={{
                width: modelDownloadProgress.total_bytes
                  ? (modelDownloadProgress.bytes_downloaded / modelDownloadProgress.total_bytes) * 100
                  : 0,
              }}
            />
          </div>
          <div className="download-stats">
            {modelDownloadProgress.bytes_downloaded} bytes /
            {modelDownloadProgress.total_bytes ?? "?"} bytes
          </div>
        </div>
      )}

      {/* Phase 14: Frame data reference modal */}
      <FrameDataModal
        open={frameDataModalOpen}
        onClose={() => setFrameDataModalOpen(false)}
        characterId={selectedMove?.characterId ?? ""}
        controlType={selectedMove?.controlType ?? ""}
        moveName={selectedMove?.moveName ?? ""}
      />

      {/* ステータスバー: LLM ワーカー状態などを表示 */}
      <footer className="status-bar">
        <div className="status-item">
          <span className="status-label">LLM ワーカー:</span>
          <span
            className={`status-value ${workerStatus ? workerStatus.state.toLowerCase() : "offline"}`}
          >
            {workerStatus ? (
              workerStatus.state === "Ready"
                ? "準備完了"
                : workerStatus.state === "Busy"
                  ? "処理中"
                  : workerStatus.state === "Error"
                    ? "エラー"
                    : workerStatus.state
            ) : (
              "未起動"
            )}
          </span>
        </div>
        {selectedModel && (
          <div className="status-item">
            <span className="status-label">モデル:</span>
            <span className="status-value">{selectedModel}</span>
          </div>
        )}
        {activeMatchId && (
          <div className="status-item">
            <span className="status-label">試合:</span>
            <span className="status-value">{activeMatchId}</span>
          </div>
        )}
      </footer>
    </main>
  );
}

export default App;
