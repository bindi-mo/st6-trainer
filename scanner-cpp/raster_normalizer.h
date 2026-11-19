#ifndef RASTER_NORMALIZER_H
#define RASTER_NORMALIZER_H

#include <opencv2/core.hpp>
#include <string>

struct NormalizationResult {
    bool success;
    cv::Mat normalized_frame;
    int content_x_offset;  // Offset within frame after cropping
    int content_y_offset;
    int content_width;
    int content_height;
    std::string error_code;
    std::string error_message;
};

class RasterNormalizer {
public:
    RasterNormalizer(int canonical_width = 1920, int canonical_height = 1080);
    
    // Normalize frame to canonical raster (1920x1080)
    // - Crop letterbox/pillarbox
    // - Check aspect ratio (16:9 ± 0.010)
    // - Check isotropy (± 0.005)
    // - Scale to canonical size
    NormalizationResult normalize(const cv::Mat& input_frame);
    
    // Get canonical dimensions
    int get_canonical_width() const { return canonical_width_; }
    int get_canonical_height() const { return canonical_height_; }

private:
    int canonical_width_;
    int canonical_height_;
    double target_aspect_ratio_;  // 16:9 = 1.777...
    
    // Detect content region (non-black borders)
    bool detect_content_region(const cv::Mat& frame, 
                              int& x_offset, int& y_offset,
                              int& width, int& height);
    
    // Check aspect ratio
    bool check_aspect_ratio(int width, int height, std::string& error_msg);
    
    // Check isotropy (scale factors should be equal)
    bool check_isotropy(int src_width, int src_height, std::string& error_msg);
};

#endif // RASTER_NORMALIZER_H