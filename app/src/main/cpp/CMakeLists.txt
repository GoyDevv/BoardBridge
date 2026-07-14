# Copyright 2026 The BoardBridge Authors
# Licensed under the Apache License, Version 2.0.
cmake_minimum_required(VERSION 3.22.1)
project(boardbridge LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_library(log-lib log)
find_library(android-lib android)
find_library(egl-lib EGL)
find_library(glesv3-lib GLESv3)

# libglfw.so — a real GLFW 3.x ABI shim over the passive BoardBridge core + EGL.
# This is the library LWJGL's org.lwjgl.glfw binding loads by name.
add_library(glfw SHARED
        glfw_shim.cpp
        egl_core.cpp
        bridge_core.cpp)
target_include_directories(glfw PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/glfw)
target_link_libraries(glfw ${log-lib} ${android-lib} ${egl-lib} ${glesv3-lib})
target_compile_options(glfw PRIVATE -Wall -Wextra)

# libboardbridge.so — JNI glue (passive provider) + the native game-thread
# client that drives the GLFW shim exactly as an LWJGL program would.
add_library(boardbridge SHARED
        native_bridge.cpp
        game_client.cpp
        jvm_launcher.cpp)
target_include_directories(boardbridge PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/glfw)
target_link_libraries(boardbridge glfw ${log-lib} ${android-lib})
target_compile_options(boardbridge PRIVATE -Wall -Wextra)
