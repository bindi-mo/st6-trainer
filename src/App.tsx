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

function App() {
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

      // Initialize manual calibration state from metadata
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
        // Update scan result metadata
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
      </div>
    </main>
  );
}

export default App;
