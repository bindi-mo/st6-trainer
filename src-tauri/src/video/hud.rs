//! HUD layout detection for dynamic anchor positioning
//!
//! Detects timer, health gauges, and calculates ROI positions dynamically.

use opencv::core::{Mat, Point, Rect, Scalar, Size};
use opencv::imgproc;
use opencv::prelude::*;
use crate::video::types::{HudLayout, NormalizedRect, PixelRect, Resolution, VideoScanError};

/// HUD detector for finding dynamic anchor points
pub struct HudDetector {
    /// Template for timer detection (would be loaded from assets)
    timer_template: Option<Mat>,
    /// Minimum match confidence for template matching
    match_threshold: f64,
}

impl HudDetector {
    pub fn new() -> Self {
        Self {
            timer_template: None,
            match_threshold: 0.7,
        }
    }

    /// Load timer template from file
    pub fn load_timer_template(&mut self, path: &str) -> Result<(), VideoScanError> {
        let template = opencv::imgcodecs::imread(path, opencv::imgcodecs::IMREAD_GRAYSCALE)?;
        if template.empty()? {
            return Err(VideoScanError::HudDetection("Failed to load timer template".into()));
        }
        self.timer_template = Some(template);
        Ok(())
    }

    /// Detect HUD layout from first frame
    ///
    /// Uses timer as primary anchor, calculates all other positions relative to it
    pub fn detect_layout(&self, frame: &Mat, resolution: Resolution) -> Result<HudLayout, VideoScanError> {
        // Normalize frame to 1080p first
        let normalized = normalize_frame(frame, resolution)?;

        // Detect timer position (primary anchor)
        let timer_center = self.detect_timer(&normalized)?;

        // Calculate all ROI positions relative to timer
        let layout = self.calculate_layout_from_timer(timer_center, resolution);

        Ok(layout)
    }

    /// Detect timer position using template matching or contour detection
    fn detect_timer(&self, frame: &Mat) -> Result<(f32, f32), VideoScanError> {
        // Convert to grayscale
        let mut gray: Mat = Mat::default()?;
        imgproc::cvt_color(frame, &mut gray, imgproc::COLOR_BGR2GRAY, 0)?;

        // Try template matching first if template available
        if let Some(template) = &self.timer_template {
            if let Ok(pos) = self.match_template(&gray, template) {
                return Ok(pos);
            }
        }

        // Fallback: contour detection for timer-like rectangle
        self.detect_timer_by_contours(&gray)
    }

    /// Template matching for timer
    fn match_template(&self, frame: &Mat, template: &Mat) -> Result<(f32, f32), VideoScanError> {
        let mut result = Mat::default()?;
        let mask = Mat::default()?;
        imgproc::match_template(frame, template, &mut result, imgproc::TM_CCOEFF_NORMED, &mask)?;

        let mut min_val = 0.0;
        let mut max_val = 0.0;
        let mut min_loc = Point::default();
        let mut max_loc = Point::default();
        let mask = Mat::default()?;
        opencv::core::min_max_loc(&result, &mut min_val, &mut max_val, &mut min_loc, &mut max_loc, &mask)?;

        if max_val >= self.match_threshold {
            let center_x = (max_loc.x + template.cols()? / 2) as f32 / frame.cols()? as f32;
            let center_y = (max_loc.y + template.rows()? / 2) as f32 / frame.rows()? as f32;
            Ok((center_x, center_y))
        } else {
            Err(VideoScanError::HudDetection("Timer template match below threshold".into()))
        }
    }

    /// Detect timer by finding rectangular contours in top-center area
    fn detect_timer_by_contours(&self, gray: &Mat) -> Result<(f32, f32), VideoScanError> {
        let mut binary: Mat = Mat::default()?;
        imgproc::threshold(gray, &mut binary, 200.0, 255.0, imgproc::THRESH_BINARY)?;

        let mut contours = opencv::types::VectorOfPoint::new();
        imgproc::find_contours(&binary, &mut contours, imgproc::RETR_EXTERNAL, imgproc::CHAIN_APPROX_SIMPLE, Point::default())?;

        let frame_center_x = gray.cols()? as f32 / 2.0;
        let search_region_top = (gray.rows()? as f32 * 0.05) as i32;
        let search_region_bottom = (gray.rows()? as f32 * 0.15) as i32;

        let mut best_match: Option<(f32, f32)> = None;
        let mut best_score = 0.0;

        let contours_len = contours.len();
        for i in 0..contours_len {
            let contour = contours.get(i)?;
            // Convert contour to Mat for contour_area
            let contour_mat = Mat::from_slice(&[contour])?;
            let area = imgproc::contour_area(&contour_mat, false)?;

            // Timer is a small-ish rectangle in top center
            if area < 100.0 || area > 5000.0 {
                continue;
            }

            let rect = imgproc::bounding_rect(&contour_mat)?;
            let center_x = rect.x + rect.width / 2;
            let center_y = rect.y + rect.height / 2;

            // Check if in top-center region
            if center_y < search_region_top || center_y > search_region_bottom {
                continue;
            }

            let distance_from_center = (center_x as f32 - frame_center_x).abs();
            let score = 1.0 / (1.0 + distance_from_center / 100.0);

            if score > best_score {
                best_score = score;
                best_match = Some((
                    center_x as f32 / gray.cols()? as f32,
                    center_y as f32 / gray.rows()? as f32,
                ));
            }
        }

        best_match.ok_or_else(|| VideoScanError::HudDetection("No timer contour found".into()))
    }

    /// Calculate all ROI positions from timer anchor
    ///
    /// Per spec: timer at (X0, Y0), all other positions are offsets
    fn calculate_layout_from_timer(&self, timer_center: (f32, f32), resolution: Resolution) -> HudLayout {
        let (tx, ty) = timer_center;

        // Offsets from spec (normalized coordinates)
        const OFFSET_LEFT: f32 = 0.27;      // Timer to left drive gauge
        const OFFSET_GAUGE_Y: f32 = 0.035;  // Timer Y to gauge Y
        const GAUGE_WIDTH: f32 = 0.21;      // Drive gauge width
        const GAUGE_HEIGHT: f32 = 0.02;     // Drive gauge height

        // 1P drive gauge (left of timer)
        let p1_drive_x_min = (tx - OFFSET_LEFT - GAUGE_WIDTH).max(0.0);
        let p1_drive_x_max = (tx - OFFSET_LEFT).min(1.0);
        let p1_drive_y_min = (ty + OFFSET_GAUGE_Y).max(0.0);
        let p1_drive_y_max = (ty + OFFSET_GAUGE_Y + GAUGE_HEIGHT).min(1.0);

        // 2P drive gauge (right of timer)
        let p2_drive_x_min = (tx + OFFSET_LEFT).max(0.0);
        let p2_drive_x_max = (tx + OFFSET_LEFT + GAUGE_WIDTH).min(1.0);
        let p2_drive_y_min = p1_drive_y_min;
        let p2_drive_y_max = p1_drive_y_max;

        // System notification (center area)
        let sys_x_min = 0.65;
        let sys_x_max = 0.85;
        let sys_y_min = 0.43;
        let sys_y_max = 0.48;

        // Frame meter (bottom area)
        let fm_x_min = 0.18;
        let fm_x_max = 0.82;
        let fm_y_min = 0.70;
        let fm_y_max = 0.82;

        HudLayout {
            timer_center,
            p1_health_start: (0.05, 0.08),  // Approximate
            p2_health_start: (0.95, 0.08),  // Approximate
            p1_drive_gauge: NormalizedRect {
                x_min: p1_drive_x_min,
                y_min: p1_drive_y_min,
                x_max: p1_drive_x_max,
                y_max: p1_drive_y_max,
            }.to_pixel_coords(resolution),
            p2_drive_gauge: NormalizedRect {
                x_min: p2_drive_x_min,
                y_min: p2_drive_y_min,
                x_max: p2_drive_x_max,
                y_max: p2_drive_y_max,
            }.to_pixel_coords(resolution),
            system_notification: NormalizedRect {
                x_min: sys_x_min,
                y_min: sys_y_min,
                x_max: sys_x_max,
                y_max: sys_y_max,
            }.to_pixel_coords(resolution),
            frame_meter: NormalizedRect {
                x_min: fm_x_min,
                y_min: fm_y_min,
                x_max: fm_x_max,
                y_max: fm_y_max,
            }.to_pixel_coords(resolution),
        }
    }
}

/// Normalize frame to 1080p with letterboxing
pub fn normalize_frame(frame: &Mat, src_resolution: Resolution) -> Result<Mat, VideoScanError> {
    let target = Resolution::TARGET_1080P;

    let scale = (target.width as f64 / src_resolution.width as f64)
        .min(target.height as f64 / src_resolution.height as f64);

    let new_width = (src_resolution.width as f64 * scale) as i32;
    let new_height = (src_resolution.height as f64 * scale) as i32;

    let mut resized: Mat = Mat::default()?;
    imgproc::resize(frame, &mut resized, Size::new(new_width, new_height), 0.0, 0.0, imgproc::INTER_LINEAR)?;

    // Create 1080p canvas and center the resized frame
    let mut canvas = Mat::new_rows_cols_with_default(
        target.height as i32,
        target.width as i32,
        frame.typ()?,
        Scalar::all(0.0),
    )?;

    let x_offset = (target.width as i32 - new_width) / 2;
    let y_offset = (target.height as i32 - new_height) / 2;

    let roi = Rect::new(x_offset, y_offset, new_width, new_height);
    let mut canvas_roi = Mat::roi(&canvas, roi)?;
    resized.copy_to(&mut canvas_roi)?;

    Ok(canvas)
}

impl Default for HudDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::video::types::Resolution;

    #[test]
    fn test_normalize_rect_to_pixel() {
        let rect = NormalizedRect {
            x_min: 0.1,
            y_min: 0.2,
            x_max: 0.3,
            y_max: 0.4,
        };
        let pixel = rect.to_pixel_coords(Resolution::TARGET_1080P);
        assert_eq!(pixel.x_min, 192);
        assert_eq!(pixel.y_min, 216);
        assert_eq!(pixel.x_max, 576);
        assert_eq!(pixel.y_max, 432);
    }

    #[test]
    fn test_hud_layout_calculation() {
        let detector = HudDetector::new();
        let layout = detector.calculate_layout_from_timer((0.5, 0.1), Resolution::TARGET_1080P);

        // Timer at center-top (0.5, 0.1)
        // 1P drive gauge should be left of timer
        assert!(layout.p1_drive_gauge.x_max < layout.p2_drive_gauge.x_min);
        // Both gauges at same Y level
        assert_eq!(layout.p1_drive_gauge.y_min, layout.p2_drive_gauge.y_min);
    }
}