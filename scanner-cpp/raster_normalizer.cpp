#include "raster_normalizer.h"
#include <opencv2/imgproc.hpp>
#include <cmath>
#include <iostream>

RasterNormalizer::RasterNormalizer(int canonical_width, int canonical_height)
    : canonical_width_(canonical_width), canonical_height_(canonical_height),
      target_aspect_ratio_(16.0 / 9.0) {
    // target_aspect_ratio_ initialized in initializer list
}

bool RasterNormalizer::detect_content_region(const cv::Mat& frame,
                                            int& x_offset, int& y_offset,
                                            int& width, int& height) {
    // Convert to grayscale for analysis
    cv::Mat gray;
    if (frame.channels() == 3) {
        cv::cvtColor(frame, gray, cv::COLOR_BGR2GRAY);
    } else {
        gray = frame.clone();
    }
    
    // For now, assume the entire frame is content (no letterboxing)
    // This is a simplification - in production, we would detect actual borders
    x_offset = 0;
    y_offset = 0;
    width = frame.cols;
    height = frame.rows;
    
    return true;
}

bool RasterNormalizer::check_aspect_ratio(int width, int height, std::string& error_msg) {
    double aspect = static_cast<double>(width) / height;
    double diff = std::abs(aspect - target_aspect_ratio_);
    
    if (diff > 0.010) {
        error_msg = "E_ASPECT_UNSUPPORTED: Content aspect ratio " + 
                   std::to_string(aspect) + " differs from 16:9 by " + 
                   std::to_string(diff) + " (threshold: 0.010)";
        std::cerr << "[RasterNormalizer] " << error_msg << std::endl;
        return false;
    }
    
    return true;
}

bool RasterNormalizer::check_isotropy(int src_width, int src_height, std::string& error_msg) {
    double scale_x = static_cast<double>(canonical_width_) / src_width;
    double scale_y = static_cast<double>(canonical_height_) / src_height;
    double diff = std::abs(scale_x - scale_y);
    
    if (diff > 0.005) {
        error_msg = "E_ANISOTROPIC_SCALE: Scale factors differ by " + 
                   std::to_string(diff) + " (threshold: 0.005)";
        std::cerr << "[RasterNormalizer] " << error_msg << std::endl;
        return false;
    }
    
    return true;
}

NormalizationResult RasterNormalizer::normalize(const cv::Mat& input_frame) {
    NormalizationResult result;
    result.success = false;
    result.content_x_offset = 0;
    result.content_y_offset = 0;
    result.content_width = 0;
    result.content_height = 0;
    
    if (input_frame.empty()) {
        result.error_code = "E_INVALID_INPUT";
        result.error_message = "Input frame is empty";
        return result;
    }
    
    // Check input frame for black frame detection
    double input_min, input_max;
    cv::Point input_min_loc, input_max_loc;
    cv::Mat input_gray;
    if (input_frame.channels() == 3) {
        cv::cvtColor(input_frame, input_gray, cv::COLOR_BGR2GRAY);
    } else {
        input_gray = input_frame.clone();
    }
    cv::minMaxLoc(input_gray, &input_min, &input_max, &input_min_loc, &input_max_loc);
    
    if (input_max < 10.0) {
        result.error_code = "E_BLACK_FRAME";
        result.error_message = "Input frame is black (max_val=" + std::to_string(input_max) + ")";
        std::cerr << "[RasterNormalizer] " << result.error_message << std::endl;
        return result;
    }
    
    // Step 1: Detect content region (crop letterbox/pillarbox)
    int x_off, y_off, content_w, content_h;
    if (!detect_content_region(input_frame, x_off, y_off, content_w, content_h)) {
        result.error_code = "E_CONTENT_DETECTION_FAILED";
        result.error_message = "Failed to detect content region";
        return result;
    }
    
    // Step 2: Check aspect ratio
    if (!check_aspect_ratio(content_w, content_h, result.error_message)) {
        result.error_code = "E_ASPECT_UNSUPPORTED";
        return result;
    }
    
    // Step 3: Check isotropy
    if (!check_isotropy(content_w, content_h, result.error_message)) {
        result.error_code = "E_ANISOTROPIC_SCALE";
        return result;
    }
    
    // Step 4: Crop content region
    cv::Mat cropped = input_frame(cv::Rect(x_off, y_off, content_w, content_h)).clone();
    
    // Check cropped frame for black frame detection
    cv::Mat cropped_gray;
    if (cropped.channels() == 3) {
        cv::cvtColor(cropped, cropped_gray, cv::COLOR_BGR2GRAY);
    } else {
        cropped_gray = cropped.clone();
    }
    double cropped_min, cropped_max;
    cv::minMaxLoc(cropped_gray, &cropped_min, &cropped_max);
    
    if (cropped_max < 10.0) {
        result.error_code = "E_BLACK_FRAME";
        result.error_message = "Cropped frame is black (max_val=" + std::to_string(cropped_max) + ")";
        std::cerr << "[RasterNormalizer] " << result.error_message << std::endl;
        return result;
    }
    
    // Step 5: Scale to canonical raster
    cv::Mat normalized;
    cv::resize(cropped, normalized, cv::Size(canonical_width_, canonical_height_), 
               0, 0, cv::INTER_LINEAR);
    
    result.success = true;
    result.normalized_frame = normalized;
    result.content_x_offset = x_off;
    result.content_y_offset = y_off;
    result.content_width = content_w;
    result.content_height = content_h;
    
    return result;
}