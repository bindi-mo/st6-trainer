# ST6 Trainer

⚠️ **Still under development — not yet ready for training.**

Street Fighter 6 AI Trainer — a desktop application that analyzes your recorded SF6 match and replay videos (MP4/MKV), identifies the causes of getting hit, verifies guaranteed counterattacks and interrupts based on frame data, and presents concrete training mode practice menus tailored to your controller (pad, arcade stick, or leverless).

## Tech Stack

- **Frontend**: React 19, TypeScript, Vite
- **Backend**: Rust (Tauri 2)
- **Scanner**: C++ DLL + OpenCV 4.12.0 (static link)
- **Python Scripts**: Frame data scraping, template extraction, controller image download

## Requirements

- Windows 10 / 11 (64-bit)
- Visual Studio 2022 (C++ Desktop Development workload)
- CMake 3.16 or higher
- bun 1.4.4 or higher
- Rust toolchain (stable-msvc)

## Build Instructions

See [BUILD.md](./BUILD.md) for detailed build steps.

## Python Scripts (src-tauri/scripts)

- `extract_templates.py`: Extracts template images from videos. Uses OpenCV for frame navigation and rectangular selection, saving PNGs to `src-tauri/templates/`.
- `sf6_frame_data_processor.py`: CLI tool for scraping and converting SF6 frame data.
- `download_controller_images.py`: Downloads controller input images from the official SF6 website.

## License

[MIT](./LICENCE)
