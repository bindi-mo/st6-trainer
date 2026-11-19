#include "calibration.h"
#include <opencv2/imgproc.hpp>
#include <opencv2/imgcodecs.hpp>
#include <algorithm>
#include <cmath>
#include <filesystem>
#include <iostream>

namespace fs = std::filesystem;

Calibration::Calibration(const std::string& template_pack_path, const CalibrationConfig& config)
    : template_pack_path_(template_pack_path), config_(config) {
    
    // Define anchor templates per §2.3
    upper_anchors_ = {
        "anchor/upper_timer_top_edge.png",
        "anchor/upper_p1_hp_corner.png",
        "anchor/upper_p2_hp_corner.png",
        "anchor/upper_p1_drive_corner.png",
        "anchor/upper_p2_drive_corner.png"
    };
    
    lower_anchors_ = {
        "anchor/lower_p1_sa_bracket.png",
        "anchor/lower_p2_sa_bracket.png",
        "anchor/lower_frame_meter_top_edge.png"
    };
}

bool Calibration::load_templates() {
    upper_templates_.clear();
    lower_templates_.clear();
    
    // Load upper anchor templates
    for (const auto& rel_path : upper_anchors_) {
        std::string full_path = template_pack_path_ + "/" + rel_path;
        cv::Mat img = cv::imread(full_path, cv::IMREAD_GRAYSCALE);
        if (img.empty()) {
            std::cerr << "[Calibration] Failed to load upper anchor: " << full_path << std::endl;
            continue;
        }
        // Ensure template is CV_8UC1 (grayscale)
        if (img.type() != CV_8UC1) {
            img.convertTo(img, CV_8UC1);
        }
        upper_templates_.push_back(img);
        std::cout << "[Calibration] Loaded upper anchor: " << rel_path 
                  << " (" << img.cols << "x" << img.rows << ") type=" << img.type() << std::endl;
    }
    
    // Load lower anchor templates
    for (const auto& rel_path : lower_anchors_) {
        std::string full_path = template_pack_path_ + "/" + rel_path;
        cv::Mat img = cv::imread(full_path, cv::IMREAD_GRAYSCALE);
        if (img.empty()) {
            std::cerr << "[Calibration] Failed to load lower anchor: " << full_path << std::endl;
            continue;
        }
        // Ensure template is CV_8UC1 (grayscale)
        if (img.type() != CV_8UC1) {
            img.convertTo(img, CV_8UC1);
        }
        lower_templates_.push_back(img);
        std::cout << "[Calibration] Loaded lower anchor: " << rel_path 
                  << " (" << img.cols << "x" << img.rows << ") type=" << img.type() << std::endl;
    }
    
    if (upper_templates_.empty() && lower_templates_.empty()) {
        std::cerr << "[Calibration] No anchor templates loaded" << std::endl;
        return false;
    }
    
    std::cout << "[Calibration] Loaded " << upper_templates_.size() << " upper anchors, "
              << lower_templates_.size() << " lower anchors" << std::endl;
    return true;
}

AnchorMatch Calibration::match_anchor(const cv::Mat& frame, const cv::Mat& anchor_template,
                                      int band_y_start, int band_y_end) {
    AnchorMatch result;
    result.y_offset = 0;
    result.confidence = 0.0;
    result.prominence = 0.0;
    result.dx = 0;
    result.accepted = false;
    
    // Ensure frame is grayscale
    cv::Mat frame_gray;
    if (frame.channels() == 3) {
        cv::cvtColor(frame, frame_gray, cv::COLOR_BGR2GRAY);
    } else {
        frame_gray = frame.clone();
    }
    
    // Ensure frame_gray is CV_8UC1
    if (frame_gray.type() != CV_8UC1) {
        frame_gray.convertTo(frame_gray, CV_8UC1);
    }
    
    // Clamp band to frame bounds
    int y_start = std::max(0, std::min(band_y_start, frame_gray.rows - 1));
    int y_end = std::max(y_start + 1, std::min(band_y_end, frame_gray.rows));
    
    // Extract band
    cv::Mat band = frame_gray.rowRange(y_start, y_end).clone();
    
    // Check if template fits in band
    if (anchor_template.rows > band.rows || anchor_template.cols > band.cols) {
        std::cerr << "[Calibration] Anchor template (" << anchor_template.cols << "x" << anchor_template.rows 
                  << ") larger than search band (" << band.cols << "x" << band.rows << ")" << std::endl;
        return result;
    }
    
    // Perform 2D template matching (TM_CCOEFF_NORMED)
    cv::Mat result_map;
    try {
        cv::matchTemplate(band, anchor_template, result_map, cv::TM_CCOEFF_NORMED);
        
        // Debug: check result_map statistics
        double min_val, max_val;
        cv::Point min_loc, max_loc;
        cv::minMaxLoc(result_map, &min_val, &max_val, &min_loc, &max_loc);
        
        if (max_val < 0.01) {
            std::cerr << "[Calibration] Warning: Very low max correlation (" << max_val 
                      << ") for template " << anchor_template.cols << "x" << anchor_template.rows 
                      << " in band " << band.cols << "x" << band.rows << std::endl;
        }
    } catch (const cv::Exception& e) {
        std::cerr << "[Calibration] matchTemplate failed: " << e.what() << std::endl;
        return result;
    }
    
    // Find global maximum
    double min_val, max_val;
    cv::Point min_loc, max_loc;
    cv::minMaxLoc(result_map, &min_val, &max_val, &min_loc, &max_loc);
    
    result.confidence = max_val;
    
    // Compute prominence: v_max - max(R_i where |i - i_max| >= 3)
    // For simplicity, compute prominence as difference from median
    cv::Mat abs_diff;
    cv::absdiff(result_map, cv::Scalar(max_val), abs_diff);
    
    // Mask out peak region (±3 pixels in both x and y)
    cv::Mat mask = cv::Mat::zeros(result_map.size(), CV_8UC1);
    int peak_y = max_loc.y;
    int peak_x = max_loc.x;
    int y_start_mask = std::max(0, peak_y - 3);
    int y_end_mask = std::min(result_map.rows, peak_y + 4);
    int x_start_mask = std::max(0, peak_x - 3);
    int x_end_mask = std::min(result_map.cols, peak_x + 4);
    mask(cv::Rect(x_start_mask, y_start_mask, x_end_mask - x_start_mask, y_end_mask - y_start_mask)).setTo(255);
    
    cv::Mat non_peak;
    result_map.copyTo(non_peak, mask == 0);
    
    double non_peak_max = 0.0;
    if (countNonZero(non_peak) > 0) {
        cv::minMaxLoc(non_peak, nullptr, &non_peak_max);
    }
    
    result.prominence = max_val - non_peak_max;
    
    // Compute displacement
    int matched_y = y_start + max_loc.y;
    int baseline_y = y_start; // Baseline is band start
    result.y_offset = matched_y - baseline_y;
    
    // Horizontal displacement (should be near 0)
    int matched_x = max_loc.x;
    int baseline_x = (band.cols - anchor_template.cols) / 2; // Centered
    result.dx = matched_x - baseline_x;
    
    // Check acceptance gates (§2.5)
    bool conf_ok = result.confidence >= config_.confidence_threshold;
    bool prom_ok = result.prominence >= config_.prominence_threshold;
    bool dx_ok = std::abs(result.dx) <= config_.max_dx_px;
    
    result.accepted = conf_ok && prom_ok && dx_ok;
    
    std::cout << "[Calibration] Anchor match: conf=" << result.confidence 
              << " prom=" << result.prominence 
              << " dx=" << result.dx 
              << " y_offset=" << result.y_offset
              << " accepted=" << (result.accepted ? "YES" : "NO")
              << " (conf>=" << config_.confidence_threshold << ": " << (conf_ok ? "Y" : "N")
              << ", prom>=" << config_.prominence_threshold << ": " << (prom_ok ? "Y" : "N")
              << ", |dx|<=" << config_.max_dx_px << ": " << (dx_ok ? "Y" : "N") << ")" << std::endl;
    
    return result;
}

bool Calibration::compute_consensus(const std::vector<AnchorMatch>& matches,
                                   int& consensus_offset, double& consensus_confidence) {
    std::vector<int> accepted_offsets;
    double total_confidence = 0.0;
    
    for (const auto& m : matches) {
        if (m.accepted) {
            accepted_offsets.push_back(m.y_offset);
            total_confidence += m.confidence;
        }
    }
    
    if (accepted_offsets.empty()) {
        std::cerr << "[Calibration] No accepted anchor matches for consensus" << std::endl;
        return false;
    }
    
    // Compute median
    std::sort(accepted_offsets.begin(), accepted_offsets.end());
    int median = accepted_offsets[accepted_offsets.size() / 2];
    
    // Filter outliers (>2px from median)
    std::vector<int> filtered_offsets;
    for (int offset : accepted_offsets) {
        if (std::abs(offset - median) <= config_.consensus_vote_window_px) {
            filtered_offsets.push_back(offset);
        }
    }
    
    // Check survival rate
    double survival_rate = static_cast<double>(filtered_offsets.size()) / accepted_offsets.size();
    if (survival_rate < config_.consensus_survival_min) {
        std::cerr << "[Calibration] Consensus survival rate " << survival_rate 
                  << " < " << config_.consensus_survival_min << std::endl;
        return false;
    }
    
    // Recompute median from filtered set
    std::sort(filtered_offsets.begin(), filtered_offsets.end());
    consensus_offset = filtered_offsets[filtered_offsets.size() / 2];
    consensus_confidence = total_confidence / matches.size();
    
    std::cout << "[Calibration] Consensus: offset=" << consensus_offset 
              << " confidence=" << consensus_confidence
              << " survival=" << survival_rate << std::endl;
    
    return true;
}

CalibrationResult Calibration::calibrate_upper(const cv::Mat& frame) {
    CalibrationResult result;
    result.upper_success = false;
    result.lower_success = false;
    result.top_y_offset_px = 0;
    result.bottom_y_offset_px = 0;
    result.upper_confidence = 0.0;
    result.lower_confidence = 0.0;
    
    if (upper_templates_.empty()) {
        result.error_code = "E_CALIB_LOW_CONF";
        result.error_message = "No upper anchor templates loaded";
        return result;
    }
    
    // Compute band from config (upper: 0.00-0.25 of canonical height)
    // Add margin for anchor template height
    int canonical_height = 1080;
    int band_y_start = static_cast<int>(config_.upper_band_y_start * canonical_height);
    int band_y_end = static_cast<int>(config_.upper_band_y_end * canonical_height);
    
    // Ensure band is tall enough for largest anchor template
    int max_template_height = 0;
    for (const auto& tmpl : upper_templates_) {
        max_template_height = std::max(max_template_height, tmpl.rows);
    }
    band_y_end = std::max(band_y_end, band_y_start + max_template_height + 10);
    
    std::vector<AnchorMatch> matches;
    for (const auto& tmpl : upper_templates_) {
        AnchorMatch m = match_anchor(frame, tmpl, band_y_start, band_y_end);
        matches.push_back(m);
    }
    
    int consensus_offset = 0;
    double consensus_conf = 0.0;
    
    if (!compute_consensus(matches, consensus_offset, consensus_conf)) {
        result.error_code = "E_CALIB_AMBIGUOUS";
        result.error_message = "Failed to compute consensus for upper HUD";
        return result;
    }
    
    // Check sign: top_y_offset_px >= 0
    if (consensus_offset < 0) {
        result.error_code = "E_CALIB_AMBIGUOUS";
        result.error_message = "Upper HUD offset is negative (wrong anchor?)";
        return result;
    }
    
    result.upper_success = true;
    result.top_y_offset_px = consensus_offset;
    result.upper_confidence = consensus_conf;
    
    std::cout << "[Calibration] Upper HUD calibrated: top_y_offset=" << result.top_y_offset_px 
              << " confidence=" << result.upper_confidence << std::endl;
    
    return result;
}

CalibrationResult Calibration::calibrate_lower(const cv::Mat& frame) {
    CalibrationResult result;
    result.upper_success = false;
    result.lower_success = false;
    result.top_y_offset_px = 0;
    result.bottom_y_offset_px = 0;
    result.upper_confidence = 0.0;
    result.lower_confidence = 0.0;
    
    if (lower_templates_.empty()) {
        result.error_code = "E_CALIB_LOW_CONF";
        result.error_message = "No lower anchor templates loaded";
        return result;
    }
    
    // Compute band from config (lower: 0.70-1.00 of canonical height)
    // Add margin for anchor template height
    int canonical_height = 1080;
    int band_y_start = static_cast<int>(config_.lower_band_y_start * canonical_height);
    int band_y_end = static_cast<int>(config_.lower_band_y_end * canonical_height);
    
    // Ensure band is tall enough for largest anchor template
    int max_template_height = 0;
    for (const auto& tmpl : lower_templates_) {
        max_template_height = std::max(max_template_height, tmpl.rows);
    }
    band_y_start = std::max(0, band_y_start - max_template_height - 10);
    
    std::vector<AnchorMatch> matches;
    for (const auto& tmpl : lower_templates_) {
        AnchorMatch m = match_anchor(frame, tmpl, band_y_start, band_y_end);
        matches.push_back(m);
    }
    
    int consensus_offset = 0;
    double consensus_conf = 0.0;
    
    if (!compute_consensus(matches, consensus_offset, consensus_conf)) {
        result.error_code = "E_CALIB_AMBIGUOUS";
        result.error_message = "Failed to compute consensus for lower HUD";
        return result;
    }
    
    // Check sign: bottom_y_offset_px <= 0
    if (consensus_offset > 0) {
        result.error_code = "E_CALIB_AMBIGUOUS";
        result.error_message = "Lower HUD offset is positive (wrong anchor?)";
        return result;
    }
    
    result.lower_success = true;
    result.bottom_y_offset_px = consensus_offset;
    result.lower_confidence = consensus_conf;
    
    std::cout << "[Calibration] Lower HUD calibrated: bottom_y_offset=" << result.bottom_y_offset_px 
              << " confidence=" << result.lower_confidence << std::endl;
    
    return result;
}