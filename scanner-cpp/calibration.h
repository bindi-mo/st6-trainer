#ifndef CALIBRATION_H
#define CALIBRATION_H

#include <opencv2/core.hpp>
#include <string>
#include <vector>

struct CalibrationConfig {
    double upper_band_y_start;  // 0.00
    double upper_band_y_end;    // 0.25
    double lower_band_y_start;  // 0.70
    double lower_band_y_end;    // 1.00
    int search_x_tolerance_px;  // 32
    double confidence_threshold; // 0.90
    double prominence_threshold; // 0.10
    int max_dx_px;              // 2
    double consensus_survival_min; // 0.80
    int consensus_vote_window_px;  // 2
};

struct AnchorMatch {
    int y_offset;
    double confidence;
    double prominence;
    int dx;
    bool accepted;
};

struct CalibrationResult {
    bool upper_success;
    bool lower_success;
    int top_y_offset_px;
    int bottom_y_offset_px;
    double upper_confidence;
    double lower_confidence;
    std::string error_code;
    std::string error_message;
};

class Calibration {
public:
    Calibration(const std::string& template_pack_path, const CalibrationConfig& config);
    
    // Load anchor templates from template pack
    bool load_templates();
    
    // Calibrate upper HUD group (HP/Drive/Names)
    CalibrationResult calibrate_upper(const cv::Mat& frame);
    
    // Calibrate lower HUD group (SA/FrameMeter/ColorBars)
    CalibrationResult calibrate_lower(const cv::Mat& frame);
    
    // Get loaded anchor template paths
    const std::vector<std::string>& get_upper_anchors() const { return upper_anchors_; }
    const std::vector<std::string>& get_lower_anchors() const { return lower_anchors_; }
    
    // Check if templates are loaded
    bool is_ready() const { return !upper_templates_.empty() || !lower_templates_.empty(); }

private:
    std::string template_pack_path_;
    CalibrationConfig config_;
    
    std::vector<std::string> upper_anchors_;
    std::vector<std::string> lower_anchors_;
    std::vector<cv::Mat> upper_templates_;
    std::vector<cv::Mat> lower_templates_;
    
    // Match a single anchor template in the search band
    AnchorMatch match_anchor(const cv::Mat& frame, const cv::Mat& anchor_template, 
                            int band_y_start, int band_y_end);
    
    // Compute consensus from multiple anchor matches
    bool compute_consensus(const std::vector<AnchorMatch>& matches, 
                          int& consensus_offset, double& consensus_confidence);
};

#endif // CALIBRATION_H