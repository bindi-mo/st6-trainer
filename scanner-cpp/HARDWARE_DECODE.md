# Hardware Decoding Enablement

## Purpose

Enable **hardware decoding via DXVA2 / D3D11VA** on Windows by switching the video loading in `scanner.cpp` to use the **MSMF (Media Foundation) backend**.

### Background

- The existing implementation used OpenCV's `VideoCapture` with the default backend (FFmpeg), which only supports software decoding.
- The OpenCV build has `WITH_FFMPEG=ON` but `WITH_CUDA=OFF`, so hardware decoding via NVDEC/CUVID is not available.
- The MSMF backend uses Windows' built-in Media Foundation and supports GPU-accelerated hardware decoding.

### Modified Files

| File | Change |
|------|--------|
| `scanner-cpp/scanner.cpp` | `open_video()` now tries `cv::CAP_MSMF` first, with fallback to the default backend |
| `scanner-cpp/test_scanner.cpp` | New test program with explicit template path |
| `scanner-cpp/CMakeLists.txt` | Added `test_scanner` target and DLL copy step |

---

## Build

### Prerequisites

- Windows 10/11
- Visual Studio 2022 Build Tools
- CMake 3.16+
- OpenCV 4.12.0 (static build, located at `opencv-build/install` in the project)

### Steps

```powershell
# 1. Navigate to the project directory
cd C:\Users\aquab\work\st6-trainer\scanner-cpp

# 2. Build
cmake --build build --config Release
```

### Output

| File | Path |
|------|------|
| Scanner DLL | `scanner-cpp/build/Release/st6_scanner.dll` |
| Test executable | `scanner-cpp/build/test_scanner.dir/Release/test_scanner.exe` |

### Notes

- If you get an error copying to `src-tauri/st6_scanner.dll`, the Tauri app may be using the DLL. Close the app and rebuild.
- Even if the copy step fails, the DLL is still generated and the test can be run.

---

## Run

### Test Program

```powershell
# 1. Navigate to the test executable directory
cd C:\Users\aquab\work\st6-trainer\scanner-cpp\build\test_scanner.dir\Release

# 2. Copy the DLL (first time only)
Copy-Item "..\..\Release\st6_scanner.dll" . -Force

# 3. Run with a video file
.\test_scanner.exe "E:\OBS\StreetFighter6\2026-11-27 00-43-45.mkv"
```

### Expected Output

```
[Test] Creating scanner for: E:\OBS\StreetFighter6\2026-11-27 00-43-45.mkv
[Scanner] Opened with MSMF backend (hardware decode via DXVA2/D3D11VA)
[Test] Scanner created successfully
...
[Test] Metadata:
  Resolution: 1920x1080
  FPS: 60.0002
  Total frames: 13488
  Character P1: jp
  Character P2: ed
[Test] Successfully read 4 frames
[Test] Done
```

### Verification

- `[Scanner] Opened with MSMF backend` confirms hardware decoding is active.
- Metadata (resolution, FPS, character names) is retrieved correctly.
- Frames are read without errors.

---

## Technical Details

### MSMF Backend Flow

```
VideoCapture::open(path, cv::CAP_MSMF)
    ↓
Media Foundation Source Reader
    ↓
DXVA2 / D3D11VA (GPU hardware decoding)
    ↓
Decoded frame (cv::Mat)
```

### Fallback Behavior

1. Try `cv::CAP_MSMF` → hardware decoding on success
2. Fall back to default backend (FFmpeg) → software decoding
3. Return error if both fail

### Log Output

Added to `open_video()` in `scanner.cpp`:
- MSMF success: `[Scanner] Opened with MSMF backend (hardware decode via DXVA2/D3D11VA)`
- FFmpeg fallback: `[Scanner] Opened with default backend (FFmpeg)`

---

## Troubleshooting

### DLL Copy Error

```
Error copying file ... st6_scanner.dll ... because it is being used by another process
```

**Fix**: Close any process using the DLL (e.g., the Tauri app) and rebuild.

### Template Not Found

```
[Scanner] Error: fight.png template not found or invalid size
```

**Fix**: Verify that `template_pack_path` in `test_scanner.cpp` points to the correct directory.

### MSMF Fails to Open

```
[Scanner] Opened with default backend (FFmpeg)
```

**Fix**: The video codec may not be supported by Media Foundation. The scanner will fall back to FFmpeg automatically.