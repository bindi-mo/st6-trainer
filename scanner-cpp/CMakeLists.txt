cmake_minimum_required(VERSION 3.16)
project(st6_scanner LANGUAGES CXX)

# Set C++ standard
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

# Windows DLL export
if(WIN32)
    add_compile_definitions(SCANNER_EXPORTS)
endif()

# Find OpenCV
find_package(OpenCV 4.1 REQUIRED COMPONENTS core imgproc videoio highgui imgcodecs)

# Create DLL
add_library(st6_scanner SHARED scanner.cpp)

# Link OpenCV
target_link_libraries(st6_scanner PRIVATE ${OpenCV_LIBS})
target_include_directories(st6_scanner PRIVATE ${OpenCV_INCLUDE_DIRS} ${CMAKE_CURRENT_SOURCE_DIR})

# Install headers
install(FILES scanner_api.h DESTINATION include)

# Install DLL
install(TARGETS st6_scanner
    RUNTIME DESTINATION bin
    LIBRARY DESTINATION lib
    ARCHIVE DESTINATION lib
)

# Set output name
set_target_properties(st6_scanner PROPERTIES
    OUTPUT_NAME "st6_scanner"
    VERSION ${PROJECT_VERSION}
    SOVERSION ${PROJECT_VERSION_MAJOR}
)

# Windows: copy DLL to build directory for easy testing
if(WIN32)
    add_custom_command(TARGET st6_scanner POST_BUILD
        COMMAND ${CMAKE_COMMAND} -E copy_if_different
        $<TARGET_FILE:st6_scanner>
        $<TARGET_FILE_DIR:st6_scanner>/../../st6_scanner.dll
        COMMENT "Copying scanner DLL to project root"
    )
endif()