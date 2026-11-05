# 🛠️ ST6 Trainer - Build & Environment Setup Guide

This document outlines the complete environment setup and build procedures for the **ST6 Trainer Tauri Application**, specifically covering the C++ scanner DLL (`st6_scanner`) with statically linked OpenCV and the overall Tauri project structure.

---

## 📋 Project Architecture Overview

```text
st6-trainer/
├── src/                      # Tauri Frontend (TypeScript)
├── src-tauri/                # Tauri backend (Rust)
│   ├── src/                  # Rust source code (loads st6_scanner.dll via FFI)
│   ├── tauri.conf.json       # configures app settings and window properties.
│   └── Cargo.toml            # manages Rust dependencies and package metadata.
├── scanner-cpp/              # C++ Scanner DLL Project
│   ├── opencv-4.12.0/        # OpenCV Source Code
│   ├── opencv-build/         # OpenCV Static Build Output (generated)
│   ├── build/                # st6_scanner Build Output (generated)
│   ├── CMakeLists.txt        # C++ build configuration
│   ├── scanner.cpp           # Scanner implementation
│   └── scanner_api.h         # FFI Header file
└── package.json              # Frontend & Tauri scripts
```

---

## 💻 Prerequisites

Ensure your development environment meets the following requirements:
* **OS**: Windows 10 / 11 (x64)
* **Compiler**: Visual Studio 2022 (MSVC) with C++ Desktop Development workload
* **Build Tools**: CMake (v3.16 or higher)
* **Node.js & npm**: For the Tauri frontend
* **Rust Toolchain**: Stable MSVC toolchain (`rustup default stable-msvc`)

---

## 🚀 Step 1: Download & Build OpenCV Statically (`/MT`)

To avoid runtime mismatches and external DLL dependencies in your Tauri production build, we build OpenCV as a static library with the static runtime (`/MT`, matching `MultiThreaded`) and disable unnecessary modules and IPP to prevent link errors.

### 1.1 Download and Extract OpenCV Source Code
Navigate to the `scanner-cpp` directory in PowerShell and download OpenCV 4.12.0 using `curl.exe`:

```powershell
cd scanner-cpp

# Download OpenCV 4.12.0 zip archive
curl.exe -L "https://github.com/opencv/opencv/archive/refs/tags/4.12.0.zip" -o "opencv-4.12.0.zip"

# Extract archive
Expand-Archive -Path "opencv-4.12.0.zip" -DestinationPath "." -Force

# Optional: Remove zip after extraction
Remove-Item -Force "opencv-4.12.0.zip"
```

### 1.2 Configure OpenCV
Create the build directory, run CMake configuration.
```powershell
mkdir opencv-build
cd opencv-build

# Configure CMake with static runtime (/MT) and disable unused modules
cmake -G "Visual Studio 17 2022" -A x64 `
    -DBUILD_SHARED_LIBS=OFF `
    -DBUILD_opencv_world=ON `
    -DBUILD_opencv_dnn=OFF `
    -DBUILD_opencv_gapi=OFF `
    -DBUILD_opencv_ts=OFF `
    -DBUILD_EXAMPLES=OFF `
    -DBUILD_TESTS=OFF `
    -DBUILD_PERF_TESTS=OFF `
    -DWITH_CUDA=OFF `
    -DWITH_IPP=OFF `
    -DCMAKE_MSVC_RUNTIME_LIBRARY="MultiThreaded" `
    -DCMAKE_INSTALL_PREFIX="./install" `
    ../opencv-4.12.0
```

### 1.3 Build and install OpenCV locally (./install):
   ```powershell
   cmake --build . --config Release --target ALL_BUILD /m
   cmake --build . --config Release --target INSTALL /m
   ```

---

## ⚙️ Step 2: Build the C++ Scanner DLL (`st6_scanner.dll`)

1. Return to the `scanner-cpp` directory:
   ```powershell
   cd ..
   ```

2. Create the build directory and generate project files:
   ```powershell
   if (Test-Path build) { Remove-Item -Recurse -Force build }
   mkdir build
   cd build

   cmake -G "Visual Studio 17 2022" -A x64 ..
   ```

3. Build the DLL in Release mode:
   ```powershell
   cmake --build . --config Release
   ```
   * **Output**: The compiled st6_scanner.dll is automatically copied to
    the src-tauri/ directory via the post-build CMake command. It is
    then included in the application bundle (both dev and release builds)
    via tauri.conf.json's bundle.resources configuration.

---

## 🦀 Step 3: Build & Run the Tauri Application

Once the C++ scanner DLL is successfully compiled, you can proceed with the standard Tauri workflow.

1. Navigate to the project root:
   ```powershell
   cd ../..
   ```

2. Install frontend dependencies (if not already done):
   ```powershell
   npm install
   ```

3. Run Tauri in development mode:
   ```powershell
   npm run tauri dev
   ```
   or

   ```powershell
   cargo tauri dev
   ```

4. Build the Tauri production application:
   ```powershell
   npm run tauri build
   ```
   *Note: Ensure `st6_scanner.dll` is bundled correctly by Tauri's external binaries configuration (`tauri.conf.json`) if required for production deployment.*

---

## 🔍 Troubleshooting

* **`LNK2038: mismatch detected for 'RuntimeLibrary'`**:
  Ensure both OpenCV and `scanner-cpp` are configured with `-DCMAKE_MSVC_RUNTIME_LIBRARY="MultiThreaded"` (`/MT`).
* **Unresolved External Symbols in IPP**:
  Verify `-DWITH_IPP=OFF` was passed during the OpenCV CMake configuration.