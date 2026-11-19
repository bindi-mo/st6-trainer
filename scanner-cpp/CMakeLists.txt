cmake_minimum_required(VERSION 3.16)

# Unify the runtime to static (/MT), the same as OpenCV
set(CMAKE_MSVC_RUNTIME_LIBRARY "MultiThreaded" CACHE STRING "MSVC runtime library" FORCE)

project(st6_scanner LANGUAGES CXX)

# C++ Standard Settings
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

# Macros for Exporting Windows DLLs
if(WIN32)
    add_compile_definitions(SCANNER_EXPORTS)
endif()

# Define the base path for the locally built static version of OpenCV
set(OpenCV_INSTALL_DIR "${CMAKE_CURRENT_SOURCE_DIR}/opencv-build/install")

# Creating a DLL
add_library(st6_scanner SHARED
    scanner.cpp
    calibration.cpp
    raster_normalizer.cpp
    input_tracker.cpp
)

# Configuring Include Directories
target_include_directories(st6_scanner PRIVATE
    ${OpenCV_INSTALL_DIR}/include
    ${CMAKE_CURRENT_SOURCE_DIR}
)

# Links to OpenCV and the necessary additional dependencies
# (Windows system libraries are also included here)
target_link_libraries(st6_scanner PRIVATE
    # OpenCV Main Static Library
    ${OpenCV_INSTALL_DIR}/x64/vc17/staticlib/opencv_world4120.lib

    # Set of built-in third-party libraries required for a static build of OpenCV (excluding IPP)
    ${OpenCV_INSTALL_DIR}/x64/vc17/staticlib/libjpeg-turbo.lib
    ${OpenCV_INSTALL_DIR}/x64/vc17/staticlib/libpng.lib
    ${OpenCV_INSTALL_DIR}/x64/vc17/staticlib/libtiff.lib
    ${OpenCV_INSTALL_DIR}/x64/vc17/staticlib/libwebp.lib
    ${OpenCV_INSTALL_DIR}/x64/vc17/staticlib/libopenjp2.lib
    ${OpenCV_INSTALL_DIR}/x64/vc17/staticlib/IlmImf.lib
    ${OpenCV_INSTALL_DIR}/x64/vc17/staticlib/zlib.lib
    ${OpenCV_INSTALL_DIR}/x64/vc17/staticlib/ittnotify.lib

    # Windows System-Dependent Libraries
    Vfw32.lib
    comctl32.lib
    gdi32.lib
    ole32.lib
    setupapi.lib
    wsock32.lib
    Advapi32.lib
    User32.lib
)

# Installing Header Files
install(FILES scanner_api.h DESTINATION include)

# Installing a DLL
install(TARGETS st6_scanner
    RUNTIME DESTINATION bin
    LIBRARY DESTINATION lib
    ARCHIVE DESTINATION lib
)

# Setting Target Name Properties
set_target_properties(st6_scanner PROPERTIES
    OUTPUT_NAME "st6_scanner"
    VERSION ${PROJECT_VERSION}
    SOVERSION ${PROJECT_VERSION_MAJOR}
)

# Windows: Automatically copy to the Tauri resources directory (bundled with app)
if(WIN32)
    add_custom_command(TARGET st6_scanner POST_BUILD
        COMMAND ${CMAKE_COMMAND} -E copy_if_different
        $<TARGET_FILE:st6_scanner>
        "${CMAKE_CURRENT_SOURCE_DIR}/../src-tauri/st6_scanner.dll"
        COMMENT "Copying scanner DLL to src-tauri for Tauri bundler"
    )
endif()
