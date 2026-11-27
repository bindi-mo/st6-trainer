#include "scanner_api.h"
#include <iostream>
#include <string>

int main(int argc, char* argv[])
{
    if (argc < 2)
    {
        std::cerr << "Usage: test_scanner <video_path>" << std::endl;
        return 1;
    }

    const char* video_path = argv[1];

    std::cout << "[Test] Creating scanner for: " << video_path << std::endl;

    ScanConfig config = {};
    config.template_pack_path = "../../../../src-tauri/templates";
    config.on_calib_failure = CALIB_ASK;
    config.frame_stride = 1;
    config.hud_redetect_interval = 60;
    config.max_frames = 0;

    ScannerHandle handle = scanner_create(video_path, &config);
    if (!handle)
    {
        std::cerr << "[Test] Failed to create scanner: " << scanner_get_last_error() << std::endl;
        return 1;
    }

    std::cout << "[Test] Scanner created successfully" << std::endl;

    MatchMetadata metadata;
    if (scanner_get_metadata(handle, &metadata) != 1)
    {
        std::cerr << "[Test] Failed to get metadata: " << scanner_get_last_error() << std::endl;
        scanner_destroy(handle);
        return 1;
    }

    std::cout << "[Test] Metadata:" << std::endl;
    std::cout << "  Resolution: " << metadata.source_resolution << std::endl;
    std::cout << "  FPS: " << metadata.source_fps << std::endl;
    std::cout << "  Total frames: " << metadata.total_frames << std::endl;
    std::cout << "  Character P1: " << metadata.character_name_p1 << std::endl;
    std::cout << "  Character P2: " << metadata.character_name_p2 << std::endl;

    // Read a few frames to verify decoding works
    FrameTelemetry telemetry;
    InputEvent input_event;
    MoveEvent move_event;
    int frame_count = 0;

    std::cout << "[Test] Reading frames..." << std::endl;
    while (frame_count < 10)
    {
        int ret = scanner_next_all(handle, &telemetry, &input_event, &move_event);
        if (ret == 0)
        {
            std::cout << "[Test] End of stream at frame " << frame_count << std::endl;
            break;
        }
        else if (ret < 0)
        {
            std::cerr << "[Test] Error reading frame: " << scanner_get_last_error() << std::endl;
            scanner_destroy(handle);
            return 1;
        }

        if (frame_count == 0)
        {
            std::cout << "[Test] First frame: index=" << telemetry.frame_index
                      << " timestamp=" << telemetry.timestamp_ms << "ms" << std::endl;
        }
        frame_count++;
    }

    std::cout << "[Test] Successfully read " << frame_count << " frames" << std::endl;

    scanner_destroy(handle);
    std::cout << "[Test] Done" << std::endl;
    return 0;
}