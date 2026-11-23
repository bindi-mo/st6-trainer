import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { useEffect, useRef, useState } from "react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import "./App.css";
import { ErrorBoundary } from "./components/ErrorBoundary";
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

function App() {
  // ===== Scanner state (existing) =====
  const [selectedVideo, setSelectedVideo] = useState<string | null>(null);
  const [scanResult, setScanResult] = useState<VideoScanResult | null>(null);
  const [isScanning, setIsScanning] = useState(false);
  const [scanError, setScanError] = useState<string | null>(null);
  const [scanProgress, setScanProgress] = useState(0);
  // Manual calibration state (Phase 6)
  const [manualTopY, setManualTopY] = useState<number | null>(null);
  const [manualBottomY, setManualBottomY] = useState<number | null>(null);
  const [manualVerified, setManualVerified] = useState<boolean | null>(null);
  const [calibError, setCalibError] = useState<string | null>(null);
  const [calibSaving, setCalibSaving] = useState(false);

  // ===== LLM state =====
  const [workerStatus, setWorkerStatus] = useState<WorkerStatus | null>(null);
  const [workerStatusError, setWorkerStatusError] = useState<string | null>(null);
  const [chatInput, setChatInput] = useState("");
  const [isStreaming, setIsStreaming] = useState(false);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [reportMarkdown, setReportMarkdown] = useState<string | null>(null);
  const [reportError, setReportError] = useState<string | null>(null);
  const [isGeneratingReport, setIsGeneratingReport] = useState(false);
  const [modelDownloadProgress, setModelDownloadProgress] = useState<DownloadProgress | null>(null);

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
    }
  }

  async function scanVideo() {
    if (!selectedVideo) return;

    console.log("[Frontend] Starting scan for:", selectedVideo);
    setIsScanning(true);
    setScanError(null);
    setScanProgress(0);
    setManualTopY(null);
    setManualBottomY(null);
    setManualVerified(null);
    setCalibError(null);

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

  // ===== LLM functions =====
  async function refreshWorkerStatus() {
    try {
      const status = await invoke<WorkerStatus>("llm_worker_status");
      setWorkerStatus(status);
      setWorkerStatusError(null);
    } catch (error) {
      setWorkerStatusError(error as string);
      setWorkerStatus(null);
    }
  }

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

  // ===== Render helpers =====
  const statusLabel = (s: WorkerStatus | null) => {
    if (!s) return "未初期化";
    switch (s.state) {
      case "Ready": return "準備完了";
      case "Busy": return "処理中";
      case "Error": return "エラー";
      case "LoadingModel": return "モデル読込中";
      case "Connecting": return "接続中";
      case "Starting": return "起動中";
      case "ShuttingDown": return "停止中";
      case "Uninitialized": return "未初期化";
      default: return s.state;
    }
  };

  return (
    <main className="container">
      <header className="app-header">
        <div className="header-title">
          <h1>ST6 Trainer</h1>
          <div className="header-subtitle">Street Fighter 6 対戦分析・トレーニング支援</div>
        </div>
        <div className="header-controls">
          <span className={`status-indicator ${workerStatus ? "ready" : "offline"}`}>
            {workerStatus ? `LLM: ${statusLabel(workerStatus)}` : "LLM: 未初期化"}
          </span>
          <button onClick={refreshWorkerStatus}>
            状態更新
          </button>
          <button onClick={selectVideo} disabled={isScanning}>
            🎬 動画読込
          </button>
        </div>
      </header>

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

              {scanError && (
                <div className="error">Error: {scanError}</div>
              )}

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
        </div>

        {/* Right pane: LLM Chat + Report */}
        <div className="pane right-pane">
          <ErrorBoundary
            fallback={({ error, reset }) => (
              <div className="section error">
                <h3>LLM エラー</h3>
                <p>{error.message}</p>
                <button onClick={reset}>復旧</button>
              </div>
            )}
          >
            <section className="section llm-section">
              <h2>LLM チャット</h2>

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
                  disabled={isStreaming}
                />
                <button
                  onClick={sendChatMessage}
                  disabled={isStreaming || !chatInput.trim()}
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

              {workerStatusError && (
                <div className="error">ワーカー状態エラー: {workerStatusError}</div>
              )}
            </section>

            <section className="section report-section">
              <h2>レポートビューア</h2>
              <div className="report-controls">
                <button
                  onClick={() => generateReport("analyze_damage")}
                  disabled={isGeneratingReport || !scanResult}
                  className="primary"
                >
                  {isGeneratingReport ? "生成中…" : "被弾原因分析レポート"}
                </button>
                <button
                  onClick={() => generateReport("verify_punish")}
                  disabled={isGeneratingReport || !scanResult}
                  className="secondary"
                >
                  {isGeneratingReport ? "生成中…" : "確定反撃検証レポート"}
                </button>
                <button
                  onClick={() => generateReport("training_menu")}
                  disabled={isGeneratingReport || !scanResult}
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
    </main>
  );
}

export default App;
