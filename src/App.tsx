import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { useState } from "react";
import "./App.css";

interface VideoScanResult {
  video_path: string;
  resolution: { width: number; height: number };
  fps: number;
  total_frames: number;
  duration_ms: number;
  frames: FrameData[];
  hud_layout: HudLayout;
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

function App() {
  const [selectedVideo, setSelectedVideo] = useState<string | null>(null);
  const [scanResult, setScanResult] = useState<VideoScanResult | null>(null);
  const [isScanning, setIsScanning] = useState(false);
  const [scanError, setScanError] = useState<string | null>(null);
  const [scanProgress, setScanProgress] = useState(0);

  async function selectVideo() {
    const filePath = await open({
      multiple: false,
      filters: [{ name: "Video", extensions: ["mp4", "mkv", "avi", "mov"] }],
    });
    if (filePath) {
      setSelectedVideo(filePath as string);
      setScanResult(null);
      setScanError(null);
    }
  }

  async function scanVideo() {
    if (!selectedVideo) return;

    console.log("[Frontend] Starting scan for:", selectedVideo);
    setIsScanning(true);
    setScanError(null);
    setScanProgress(0);

    try {
      console.log("[Frontend] Invoking scan_video command...");
      const result = await invoke("scan_video", { videoPath: selectedVideo });
      console.log("[Frontend] Scan completed, result:", result);
      setScanResult(result as VideoScanResult);
    } catch (error) {
      console.error("[Frontend] Scan error:", error);
      setScanError(error as string);
    } finally {
      setIsScanning(false);
      setScanProgress(100);
    }
  }

  return (
    <main className="container">
      <h1>ST6-Trainer - SF6 Video Scanner</h1>

      <div className="section">
        <h2>Video Scanner</h2>

        <div className="row">
          <button onClick={selectVideo} disabled={isScanning}>
            {selectedVideo ? "Change Video" : "Select Video"}
          </button>
          {selectedVideo && (
            <span className="file-path">{selectedVideo}</span>
          )}
        </div>

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
        )}
      </div>
    </main>
  );
}

export default App;
