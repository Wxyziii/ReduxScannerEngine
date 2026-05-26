cmake_minimum_required(VERSION 3.20)

project(ReduxScannerEngine LANGUAGES CXX)

add_executable(redux_rpf_scanner
    src/cpp/redux_rpf_scanner.cpp
)

target_compile_features(redux_rpf_scanner PRIVATE cxx_std_17)

if (MSVC)
    target_compile_options(redux_rpf_scanner PRIVATE /W4 /permissive-)
else()
    target_compile_options(redux_rpf_scanner PRIVATE -Wall -Wextra -Wpedantic)
endif()
