# Copyright 2026 The BoardBridge Authors
# Licensed under the Apache License, Version 2.0.
cmake_minimum_required(VERSION 3.22.1)
project(boardbridge LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

add_library(boardbridge SHARED
        native_bridge.cpp
        egl_core.cpp
        render_thread.cpp
        input_queue.cpp)

find_library(log-lib log)
find_library(android-lib android)
find_library(egl-lib EGL)
find_library(glesv3-lib GLESv3)

target_link_libraries(boardbridge
        ${log-lib}
        ${android-lib}
        ${egl-lib}
        ${glesv3-lib})

target_compile_options(boardbridge PRIVATE -Wall -Wextra)
