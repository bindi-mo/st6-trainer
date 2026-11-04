//! ROI (Region of Interest) extraction for specific HUD elements

use opencv::core::{Mat, Rect, Scalar};
use opencv::imgproc;
use opencv::prelude::*;
use crate::video::types::{PixelRect, RawFrameLog, InputSignal, Direction, Button, DriveGaugeState, SystemNotification, NotificationType, FrameMeterData, VideoScanError};
use crate::video::preprocessing::{preprocess_key_history, preprocess_system_notification, preprocess_frame_meter, preprocess_drive_gauge};

/// Extract all ROIs from a normalized frame
pub fn extract_all_rois(
    frame: &Mat,
    frame_index: usize,
    fps: f64,
    hud_layout: &crate::video::types::HudLayout,
) -> Result<RawFrameLog, VideoScanError> {
    let timestamp_ms = (frame_index as f64 / fps) * 1000.0;

    // Extract key input history (1P and 2P)
    let p1_inputs = extract_key_history(frame, &hud_layout.p1_drive_gauge, true)?;
    let p2_inputs = extract_key_history(frame, &hud_layout.p2_drive_gauge, false)?;

    // Extract drive gauges
    let p1_drive_gauge = extract_drive_gauge(frame, &hud_layout.p1_drive_gauge)?;
    let p2_drive_gauge = extract_drive_gauge(frame, &hud_layout.p2_drive_gauge)?;

    // Extract system notification
    let system_notification = extract_system_notification(frame, &hud_layout.system_notification)?;

    // Extract frame meter
    let frame_meter = extract_frame_meter(frame, &hud_layout.frame_meter)?;

    Ok(RawFrameLog {
        frame_index,
        timestamp_ms,
        p1_inputs,
        p2_inputs,
        p1_drive_gauge,
        p2_drive_gauge,
        system_notification,
        frame_meter,
    })
}

/// Extract key input history from the command history area
///
/// ROI: Left/right vertical strips showing directional arrows and button icons
fn extract_key_history(frame: &Mat, drive_gauge_rect: &PixelRect, is_p1: bool) -> Result<Vec<InputSignal>, VideoScanError> {
    // Key history is to the left of 1P drive gauge, right of 2P drive gauge
    let (x_min, x_max) = if is_p1 {
        (0, drive_gauge_rect.x_min)
    } else {
        (drive_gauge_rect.x_max, frame.cols()?)
    };

    let y_min = (frame.rows()? as f32 * 0.17) as i32;
    let y_max = (frame.rows()? as f32 * 0.82) as i32;

    if x_min >= x_max || y_min >= y_max {
        return Ok(Vec::new());
    }

    let roi_rect = Rect::new(x_min, y_min, x_max - x_min, y_max - y_min);
    let roi = Mat::roi(&frame, roi_rect)?;

    // Preprocess for icon detection
    let processed = preprocess_key_history(&roi)?;

    // TODO: Implement actual icon detection (template matching or ML)
    // For now, return empty - this would detect directional arrows and buttons
    Ok(Vec::new())
}

/// Extract drive gauge state (6 blocks, burnout detection)
fn extract_drive_gauge(frame: &Mat, rect: &PixelRect) -> Result<DriveGaugeState, VideoScanError> {
    let roi_rect = Rect::new(rect.x_min, rect.y_min, rect.width(), rect.height());
    let roi: Mat = Mat::roi(&frame, roi_rect)?;

    // Apply gaussian blur directly instead of using preprocess_drive_gauge
    let mut processed: Mat = Mat::default()?;
    imgproc::gaussian_blur(&roi, &mut processed, opencv::core::Size::new(3, 3), 0.0, 0.0, opencv::core::BORDER_DEFAULT)?;

    // Calculate average intensity
    let mut default_mat: Mat = Mat::default()?;
    let mean = opencv::core::mean(&processed, &default_mat)?;
    let pixel_intensity = mean[0] as f32;

    // Detect blocks by dividing into 6 horizontal segments
    let block_width = rect.width() / 6;
    let mut blocks = 0u8;
    let mut is_burnout = false;

    for i in 0..6 {
        let block_x = rect.x_min + i * block_width;
        let block_rect = Rect::new(block_x, rect.y_min, block_width, rect.height());
        let block_roi_result = Mat::roi(&frame, block_rect);
        let block_roi: Mat = match block_roi_result {
            Ok(m) => m,
            Err(e) => return Err(VideoScanError::OpenCV(e)),
        };

        let mut default_mat: Mat = Mat::default()?;
        let block_mean = opencv::core::mean(&block_roi, &default_mat)?;
        // Threshold for "filled" block (would need calibration)
        if block_mean[0] > 50.0 {
            blocks += 1;
        }

        // Check for burnout (flashing/color change)
        // Burnout typically shows as red/orange flashing
        let mut hsv: Mat = Mat::default()?;
        imgproc::cvt_color(&block_roi, &mut hsv, imgproc::COLOR_BGR2HSV, 0)?;
        let mut default_mat: Mat = Mat::default()?;
        let hsv_mean = opencv::core::mean(&hsv, &default_mat)?;
        // Hue around 0-15 (red) or 15-30 (orange) with high saturation
        if hsv_mean[0] < 30.0 && hsv_mean[1] > 100.0 {
            is_burnout = true;
        }
    }

    Ok(DriveGaugeState {
        blocks,
        is_burnout,
        pixel_intensity,
    })
}

/// Extract system notification (COUNTER, PUNISH COUNTER, etc.)
fn extract_system_notification(frame: &Mat, rect: &PixelRect) -> Result<Option<SystemNotification>, VideoScanError> {
    let roi_rect = Rect::new(rect.x_min, rect.y_min, rect.width(), rect.height());
    let roi = Mat::roi(&frame, roi_rect)?;

    // Check if there's any significant content in this area
    let mut default_mat: Mat = Mat::default()?;
    let mean = opencv::core::mean(&roi, &default_mat)?;
    if mean[0] < 30.0 {
        return Ok(None); // Likely empty
    }

    let processed = preprocess_system_notification(&roi)?;

    // TODO: OCR to read text
    // For now, detect based on color/pattern
    let notification_type = detect_notification_type(&roi)?;

    Ok(Some(SystemNotification {
        text: format!("{:?}", notification_type),
        notification_type,
        confidence: 0.8,
    }))
}

/// Detect notification type from color/pattern
fn detect_notification_type(roi: &Mat) -> Result<NotificationType, VideoScanError> {
    let mut hsv: Mat = Mat::default()?;
    imgproc::cvt_color(roi, &mut hsv, imgproc::COLOR_BGR2HSV, 0)?;
    let mut default_mat: Mat = Mat::default()?;
    let mean = opencv::core::mean(&hsv, &default_mat)?;

    let hue = mean[0];
    let sat = mean[1];

    // Color-based classification (would need calibration)
    if hue < 15.0 && sat > 150.0 {
        Ok(NotificationType::Counter) // Red
    } else if hue >= 15.0 && hue < 35.0 && sat > 150.0 {
        Ok(NotificationType::PunishCounter) // Orange
    } else if hue >= 100.0 && hue < 140.0 && sat > 100.0 {
        Ok(NotificationType::Reversal) // Blue
    } else {
        Ok(NotificationType::Counter) // Default
    }
}

/// Extract frame meter data (startup, active, recovery, advantage)
fn extract_frame_meter(frame: &Mat, rect: &PixelRect) -> Result<Option<FrameMeterData>, VideoScanError> {
    let roi_rect = Rect::new(rect.x_min, rect.y_min, rect.width(), rect.height());
    let roi = Mat::roi(&frame, roi_rect)?;

    let mut default_mat: Mat = Mat::default()?;
    let mean = opencv::core::mean(&roi, &default_mat)?;
    if mean[0] < 20.0 {
        return Ok(None); // Likely empty
    }

    let processed = preprocess_frame_meter(&roi)?;

    // TODO: OCR to read numeric values
    // Frame meter shows: 発生 (startup), 全体 (total), 硬直差 (advantage)

    Ok(Some(FrameMeterData {
        startup: None,
        active: None,
        recovery: None,
        hit_advantage: None,
        block_advantage: None,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::video::types::PixelRect;

    #[test]
    fn test_drive_gauge_block_calculation() {
        let rect = PixelRect {
            x_min: 100,
            y_min: 100,
            x_max: 220,
            y_max: 120,
        };
        assert_eq!(rect.width(), 120);
        assert_eq!(rect.width() / 6, 20);
    }
}