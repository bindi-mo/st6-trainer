//! Preprocessing pipeline for ROI images
//!
//! Implements HSV color masking and Otsu thresholding per the video scanner spec.

use opencv::core::{Mat, Scalar};
use opencv::imgproc;
use opencv::prelude::*;
use crate::video::types::VideoScanError;

/// Preprocessing configuration
#[derive(Debug, Clone)]
pub struct PreprocessingConfig {
    /// HSV lower bound for white/text extraction
    pub hsv_lower: Scalar,
    /// HSV upper bound for white/text extraction
    pub hsv_upper: Scalar,
    /// Whether to apply Otsu thresholding
    pub use_otsu: bool,
}

impl Default for PreprocessingConfig {
    fn default() -> Self {
        Self {
            // White text extraction: low saturation, high value
            hsv_lower: Scalar::new(0.0, 0.0, 200.0, 0.0),
            hsv_upper: Scalar::new(180.0, 30.0, 255.0, 0.0),
            use_otsu: true,
        }
    }
}

/// Preprocess an ROI for text/icon recognition
///
/// Pipeline:
/// 1. Convert to HSV color space
/// 2. Apply HSV color mask to extract white/UI colors
/// 3. Apply Otsu thresholding for binarization
pub fn preprocess_roi(roi: &Mat, config: &PreprocessingConfig) -> Result<Mat, VideoScanError> {
    let mut hsv: Mat = Mat::default()?;
    imgproc::cvt_color(roi, &mut hsv, imgproc::COLOR_BGR2HSV, 0)?;

    let mut mask: Mat = Mat::default()?;
    opencv::core::in_range(&hsv, &config.hsv_lower, &config.hsv_upper, &mut mask)?;

    let mut result: Mat = Mat::default()?;
    if config.use_otsu {
        imgproc::threshold(&mask, &mut result, 0.0, 255.0,
            imgproc::THRESH_BINARY | imgproc::THRESH_OTSU)?;
    } else {
        mask.copy_to(&mut result)?;
    }

    Ok(result)
}

/// Preprocess key history ROI specifically
/// Optimized for directional arrows and button icons
pub fn preprocess_key_history(roi: &Mat) -> Result<Mat, VideoScanError> {
    let config = PreprocessingConfig {
        hsv_lower: Scalar::new(0.0, 0.0, 180.0, 0.0),
        hsv_upper: Scalar::new(180.0, 50.0, 255.0, 0.0),
        use_otsu: true,
    };
    preprocess_roi(roi, &config)
}

/// Preprocess system notification ROI
/// Optimized for "COUNTER", "PUNISH COUNTER" text
pub fn preprocess_system_notification(roi: &Mat) -> Result<Mat, VideoScanError> {
    let config = PreprocessingConfig {
        hsv_lower: Scalar::new(0.0, 0.0, 200.0, 0.0),
        hsv_upper: Scalar::new(180.0, 30.0, 255.0, 0.0),
        use_otsu: true,
    };
    preprocess_roi(roi, &config)
}

/// Preprocess frame meter ROI
/// Optimized for numeric digits
pub fn preprocess_frame_meter(roi: &Mat) -> Result<Mat, VideoScanError> {
    let config = PreprocessingConfig {
        hsv_lower: Scalar::new(0.0, 0.0, 150.0, 0.0),
        hsv_upper: Scalar::new(180.0, 60.0, 255.0, 0.0),
        use_otsu: true,
    };
    preprocess_roi(roi, &config)
}

/// Preprocess drive gauge ROI
/// Optimized for gauge block detection (color-based)
pub fn preprocess_drive_gauge(roi: &Mat) -> Result<Mat, VideoScanError> {
    // For drive gauge, we want to preserve color information
    // Just apply slight denoising
    let mut result: Mat = Mat::default()?;
    let blur_result: Result<(), opencv::Error> = imgproc::gaussian_blur(roi, &mut result, opencv::core::Size::new(3, 3), 0.0, 0.0, opencv::core::BORDER_DEFAULT);
    blur_result.map_err(|e| VideoScanError::OpenCV(e))?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use opencv::core::Mat;

    #[test]
    fn test_preprocessing_config_default() {
        let config = PreprocessingConfig::default();
        assert_eq!(config.hsv_lower[0], 0.0);
        assert_eq!(config.hsv_upper[0], 180.0);
        assert!(config.use_otsu);
    }
}