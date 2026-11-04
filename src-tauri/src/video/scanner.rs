//! Main video scanner - orchestrates the full scanning pipeline

use opencv::core::Mat;
use opencv::videoio::{VideoCapture, CAP_PROP_FRAME_COUNT, CAP_PROP_FPS, CAP_PROP_FRAME_WIDTH, CAP_PROP_FRAME_HEIGHT, CAP_ANY};
use opencv::prelude::*;
use rayon::prelude::*;
use std::path::Path;
use crate::video::types::{VideoScanResult, VideoScanError, Resolution};
use crate::video::hud::{HudDetector, normalize_frame};
use crate::video::roi::extract_all_rois;

/// Configuration for video scanning
#[derive(Debug, Clone)]
pub struct ScanConfig {
    /// Process every Nth frame (1 = all frames)
    pub frame_stride: usize,
    /// Re-detect HUD layout every N frames
    pub hud_redetect_interval: usize,
    /// Maximum frames to process (None = all)
    pub max_frames: Option<usize>,
    /// Number of threads for parallel processing
    pub num_threads: Option<usize>,
}

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            frame_stride: 1,
            hud_redetect_interval: 300,
            max_frames: None,
            num_threads: None,
        }
    }
}

/// Main video scanner
pub struct VideoScanner {
    config: ScanConfig,
    hud_detector: HudDetector,
}

impl VideoScanner {
    pub fn new(config: ScanConfig) -> Self {
        Self {
            config,
            hud_detector: HudDetector::new(),
        }
    }

    /// Scan a video file and extract all frame data
    pub fn scan_video(&mut self, video_path: &str) -> Result<VideoScanResult, VideoScanError> {
        let path = Path::new(video_path);
        if !path.exists() {
            return Err(VideoScanError::OpenVideo(format!("File not found: {}", video_path)));
        }

        // Open video capture
        let mut cap = VideoCapture::new_from_file_with_backend(video_path, CAP_ANY)?;
        if !cap.is_opened()? {
            return Err(VideoScanError::OpenVideo(format!("Failed to open: {}", video_path)));
        }

        // Get video properties
        let total_frames = cap.get(CAP_PROP_FRAME_COUNT)? as usize;
        let fps = cap.get(CAP_PROP_FPS)?;
        let src_width = cap.get(CAP_PROP_FRAME_WIDTH)? as u32;
        let src_height = cap.get(CAP_PROP_FRAME_HEIGHT)? as u32;
        let src_resolution = Resolution { width: src_width, height: src_height };

        let max_frames = self.config.max_frames.unwrap_or(total_frames);
        let frames_to_process = max_frames.min(total_frames);

        // Read first frame for HUD detection
        let mut first_frame: Mat = Mat::default()?;
        let read_ok = cap.read(&mut first_frame)?;
        if !read_ok || first_frame.empty()? {
            return Err(VideoScanError::ReadFrame("First frame is empty".into()));
        }

        // Normalize first frame and detect HUD layout
        let normalized_first = normalize_frame(&first_frame, src_resolution)?;
        let mut hud_layout = self.hud_detector.detect_layout(&normalized_first, src_resolution)?;

        // Prepare frame indices to process
        let frame_indices: Vec<usize> = (0..frames_to_process)
            .step_by(self.config.frame_stride)
            .collect();

        // Process frames (sequential for now due to OpenCV not being fully thread-safe)
        // TODO: Use frame buffer with parallel processing per spec
        let mut frames = Vec::with_capacity(frame_indices.len());
        let mut frame: Mat = Mat::default()?;
        let mut frame_index = 0;

        // Reset to beginning
        cap.set(opencv::videoio::CAP_PROP_POS_FRAMES, 0.0)?;

        while frame_index < frames_to_process {
            let read_ok = cap.read(&mut frame)?;
            if !read_ok || frame.empty()? {
                break;
            }

            // Only process frames at stride intervals
            if frame_index % self.config.frame_stride == 0 {
                // Re-detect HUD layout periodically
                if frame_index > 0 && frame_index % self.config.hud_redetect_interval == 0 {
                    let normalized = normalize_frame(&frame, src_resolution)?;
                    if let Ok(new_layout) = self.hud_detector.detect_layout(&normalized, src_resolution) {
                        hud_layout = new_layout;
                    }
                }

                // Normalize frame
                let normalized = normalize_frame(&frame, src_resolution)?;

                // Extract all ROIs
                let frame_log = extract_all_rois(&normalized, frame_index, fps, &hud_layout)?;
                frames.push(frame_log);
            }

            frame_index += 1;
        }

        let duration_ms = (frames_to_process as f64 / fps) * 1000.0;

        Ok(VideoScanResult {
            video_path: video_path.to_string(),
            resolution: Resolution::TARGET_1080P,
            fps,
            total_frames: frames_to_process,
            duration_ms,
            frames,
            hud_layout,
        })
    }

    /// Scan video with progress callback
    pub fn scan_video_with_progress<F>(
        &mut self,
        video_path: &str,
        mut progress: F
    ) -> Result<VideoScanResult, VideoScanError>
    where
        F: FnMut(usize, usize), // current, total
    {
        let path = Path::new(video_path);
        if !path.exists() {
            return Err(VideoScanError::OpenVideo(format!("File not found: {}", video_path)));
        }

        let mut cap = VideoCapture::new_from_file_with_backend(video_path, CAP_ANY)?;
        if !cap.is_opened()? {
            return Err(VideoScanError::OpenVideo(format!("Failed to open: {}", video_path)));
        }

        let total_frames = cap.get(CAP_PROP_FRAME_COUNT)? as usize;
        let fps = cap.get(CAP_PROP_FPS)?;
        let src_width = cap.get(CAP_PROP_FRAME_WIDTH)? as u32;
        let src_height = cap.get(CAP_PROP_FRAME_HEIGHT)? as u32;
        let src_resolution = Resolution { width: src_width, height: src_height };

        let max_frames = self.config.max_frames.unwrap_or(total_frames);
        let frames_to_process = max_frames.min(total_frames);

        // Read first frame for HUD detection
        let mut first_frame: Mat = Mat::default()?;
        let read_ok = cap.read(&mut first_frame);
        if read_ok.is_err() || !read_ok? || first_frame.empty()? {
            return Err(VideoScanError::ReadFrame("First frame is empty".into()));
        }

        let normalized_first = normalize_frame(&first_frame, src_resolution)?;
        let mut hud_layout = self.hud_detector.detect_layout(&normalized_first, src_resolution)?;

        let frame_indices: Vec<usize> = (0..frames_to_process)
            .step_by(self.config.frame_stride)
            .collect();
        let total_to_process = frame_indices.len();

        let mut frames = Vec::with_capacity(total_to_process);
        let mut frame: Mat = Mat::default()?;
        let mut frame_index = 0;
        let mut processed_count = 0;

        cap.set(opencv::videoio::CAP_PROP_POS_FRAMES, 0.0)?;

        while frame_index < frames_to_process {
            let read_ok = cap.read(&mut frame)?;
            if !read_ok || frame.empty()? {
                break;
            }

            if frame_index % self.config.frame_stride == 0 {
                if frame_index > 0 && frame_index % self.config.hud_redetect_interval == 0 {
                    let normalized = normalize_frame(&frame, src_resolution)?;
                    if let Ok(new_layout) = self.hud_detector.detect_layout(&normalized, src_resolution) {
                        hud_layout = new_layout;
                    }
                }

                let normalized = normalize_frame(&frame, src_resolution)?;
                let frame_log = extract_all_rois(&normalized, frame_index, fps, &hud_layout)?;
                frames.push(frame_log);

                processed_count += 1;
                progress(processed_count, total_to_process);
            }

            frame_index += 1;
        }

        let duration_ms = (frames_to_process as f64 / fps) * 1000.0;

        Ok(VideoScanResult {
            video_path: video_path.to_string(),
            resolution: Resolution::TARGET_1080P,
            fps,
            total_frames: frames_to_process,
            duration_ms,
            frames,
            hud_layout,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::video::types::Resolution;

    #[test]
    fn test_scan_config_default() {
        let config = ScanConfig::default();
        assert_eq!(config.frame_stride, 1);
        assert_eq!(config.hud_redetect_interval, 300);
        assert_eq!(config.max_frames, None);
    }

    #[test]
    fn test_resolution_target() {
        assert_eq!(Resolution::TARGET_1080P.width, 1920);
        assert_eq!(Resolution::TARGET_1080P.height, 1080);
    }
}