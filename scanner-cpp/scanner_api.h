#pragma once

#include <cstdint>

#ifdef _WIN32
#ifdef SCANNER_EXPORTS
#define SCANNER_API __declspec(dllexport)
#else
#define SCANNER_API __declspec(dllimport)
#endif
#else
#define SCANNER_API
#endif

#ifdef __cplusplus
extern "C"
{
#endif

    // Opaque handle type
    typedef struct ScannerImpl ScannerImpl;
    typedef ScannerImpl *ScannerHandle;

#ifdef __cplusplus
}
#endif

// Frame data structure matching Rust side (defined outside extern "C" for C++ visibility)
typedef struct
{
    int frame_index;
    double timestamp_ms;

    // Player 1 inputs (packed: direction in lower 4 bits, buttons in upper bits)
    uint8_t p1_inputs[64];
    int p1_input_count;

    // Player 2 inputs
    uint8_t p2_inputs[64];
    int p2_input_count;

    // Drive gauge (0-6 blocks, burnout flag)
    uint8_t p1_drive_blocks;
    uint8_t p1_drive_burnout;
    uint8_t p2_drive_blocks;
    uint8_t p2_drive_burnout;

    // System notification
    uint8_t has_notification;
    uint8_t notification_type; // 0=Counter, 1=PunishCounter, 2=Reversal, 3=FirstAttack, 4=WhiffPunish, 5=CrushCounter
    float notification_confidence;

    // Frame meter data
    uint8_t has_frame_meter;
    int startup;
    int active;
    int recovery;
    int hit_advantage;
    int block_advantage;
} FrameData;

// Scanner configuration
typedef struct
{
    int frame_stride;          // Process every Nth frame (1 = all)
    int hud_redetect_interval; // Re-detect HUD every N frames
    int max_frames;            // Max frames to process (0 = all)
} ScanConfig;

#ifdef __cplusplus
extern "C"
{
#endif

    // Create scanner from video file
    // Returns NULL on failure
    SCANNER_API ScannerHandle scanner_create(const char *video_path, const ScanConfig *config);

    // Get next frame data
    // Returns 1 if frame available, 0 if end of video, -1 on error
    SCANNER_API int scanner_next_frame(ScannerHandle handle, FrameData *out_frame);

    // Get video metadata
    SCANNER_API int scanner_get_total_frames(ScannerHandle handle);
    SCANNER_API double scanner_get_fps(ScannerHandle handle);
    SCANNER_API int scanner_get_width(ScannerHandle handle);
    SCANNER_API int scanner_get_height(ScannerHandle handle);

    // Destroy scanner and free resources
    SCANNER_API void scanner_destroy(ScannerHandle handle);

    // Get last error message (call after failed operation)
    SCANNER_API const char *scanner_get_last_error();

#ifdef __cplusplus
}
#endif