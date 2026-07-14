/*
 * Copyright 2026 The BoardBridge Authors
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 *
 * The native game-thread client. It makes the exact same GLFW + GL-loader calls
 * that an LWJGL program (org.lwjgl.glfw.GLFW + org.lwjgl.opengles.GLES) makes:
 *   glfwInit -> glfwCreateWindow -> glfwMakeContextCurrent ->
 *   load GL via glfwGetProcAddress -> loop { draw; glfwSwapBuffers; glfwPollEvents }
 * It draws through libglfw.so (our shim), proving the passive bridge works
 * end-to-end. When the JRE step (#3) lands, this native loop is replaced by the
 * JVM running Minecraft/LWJGL against the same libglfw.so unchanged.
 */
#define GLFW_INCLUDE_NONE
#include <GLFW/glfw3.h>

#include <chrono>
#include <cstdio>

#include "bridge_core.h"
#include "game_client.h"
#include "log.h"

namespace {

// GL enums / types loaded via glfwGetProcAddress (no GLES headers linked here,
// to prove the LWJGL-style dynamic GL loading path works).
constexpr unsigned int GL_COLOR_BUFFER_BIT = 0x00004000;
constexpr unsigned int GL_VENDOR = 0x1F00;
constexpr unsigned int GL_RENDERER = 0x1F01;
constexpr unsigned int GL_VERSION = 0x1F02;
constexpr unsigned int GL_RGBA = 0x1908;
constexpr unsigned int GL_UNSIGNED_BYTE = 0x1401;

using PFN_glClearColor = void (*)(float, float, float, float);
using PFN_glClear = void (*)(unsigned int);
using PFN_glViewport = void (*)(int, int, int, int);
using PFN_glGetString = const unsigned char* (*)(unsigned int);
using PFN_glReadPixels = void (*)(int, int, int, int, unsigned int, unsigned int, void*);

PFN_glClearColor p_glClearColor = nullptr;
PFN_glClear p_glClear = nullptr;
PFN_glViewport p_glViewport = nullptr;
PFN_glGetString p_glGetString = nullptr;
PFN_glReadPixels p_glReadPixels = nullptr;

int g_palette = 0;  // toggled by tap / mouse button

void errorCallback(int code, const char* desc) {
    LOGE("game_client GLFW error 0x%x: %s", code, desc ? desc : "");
}

void mouseButtonCallback(GLFWwindow* /*w*/, int button, int action, int /*mods*/) {
    if (button == GLFW_MOUSE_BUTTON_LEFT && action == GLFW_PRESS) {
        g_palette ^= 1;
        LOGI("game_client: tap -> palette %d", g_palette);
    }
}

void keyCallback(GLFWwindow* /*w*/, int key, int /*sc*/, int action, int /*mods*/) {
    if (action == GLFW_PRESS) LOGI("game_client: key %d down", key);
}

void framebufferSizeCallback(GLFWwindow* /*w*/, int width, int height) {
    LOGI("game_client: framebuffer resized %dx%d", width, height);
    if (p_glViewport) p_glViewport(0, 0, width, height);
}

template <typename T>
T loadGl(const char* name) {
    return reinterpret_cast<T>(glfwGetProcAddress(name));
}

}  // namespace

void game_client_run() {
    LOGI("game_client: starting (LWJGL-style GLFW client)");
    if (!glfwInit()) {
        LOGE("game_client: glfwInit failed");
        return;
    }
    glfwSetErrorCallback(errorCallback);
    glfwWindowHint(GLFW_CLIENT_API, GLFW_OPENGL_ES_API);
    glfwWindowHint(GLFW_CONTEXT_VERSION_MAJOR, 3);
    glfwWindowHint(GLFW_CONTEXT_VERSION_MINOR, 0);

    GLFWwindow* window = glfwCreateWindow(0, 0, "BoardBridge LWJGL-style test", nullptr, nullptr);
    if (window == nullptr) {
        LOGE("game_client: glfwCreateWindow returned null (closing?)");
        glfwTerminate();
        return;
    }
    glfwMakeContextCurrent(window);
    glfwSwapInterval(1);

    glfwSetMouseButtonCallback(window, mouseButtonCallback);
    glfwSetKeyCallback(window, keyCallback);
    glfwSetFramebufferSizeCallback(window, framebufferSizeCallback);

    // Load GL entry points via glfwGetProcAddress, exactly as LWJGL's
    // GLES.createCapabilities() does under the hood.
    p_glClearColor = loadGl<PFN_glClearColor>("glClearColor");
    p_glClear = loadGl<PFN_glClear>("glClear");
    p_glViewport = loadGl<PFN_glViewport>("glViewport");
    p_glGetString = loadGl<PFN_glGetString>("glGetString");
    p_glReadPixels = loadGl<PFN_glReadPixels>("glReadPixels");
    LOGI("game_client: GL loaded clearColor=%p clear=%p getString=%p readPixels=%p",
         (void*)p_glClearColor, (void*)p_glClear, (void*)p_glGetString, (void*)p_glReadPixels);

    if (p_glGetString) {
        const unsigned char* vendor = p_glGetString(GL_VENDOR);
        const unsigned char* renderer = p_glGetString(GL_RENDERER);
        const unsigned char* version = p_glGetString(GL_VERSION);
        char info[512];
        std::snprintf(info, sizeof(info), "GL_VENDOR=%s | GL_RENDERER=%s | GL_VERSION=%s",
                      vendor ? reinterpret_cast<const char*>(vendor) : "?",
                      renderer ? reinterpret_cast<const char*>(renderer) : "?",
                      version ? reinterpret_cast<const char*>(version) : "?");
        bridge::setRendererInfo(info);
        LOGI("game_client: %s", info);
    }

    auto startTime = std::chrono::steady_clock::now();
    auto lastStats = startTime;
    long long frameCount = 0;
    int framesSinceStats = 0;

    while (!glfwWindowShouldClose(window)) {
        // Solid palette color (deterministic, so glReadPixels is verifiable).
        float r, g, b;
        if (g_palette == 0) {
            r = 0.00f; g = 0.62f; b = 0.65f;  // teal ~ (0,158,166)
        } else {
            r = 0.95f; g = 0.45f; b = 0.10f;  // orange
        }
        if (p_glClearColor) p_glClearColor(r, g, b, 1.0f);
        if (p_glClear) p_glClear(GL_COLOR_BUFFER_BIT);

        glfwSwapBuffers(window);
        glfwPollEvents();

        ++frameCount;
        ++framesSinceStats;
        if (frameCount == 1) {
            int fw = 0, fh = 0;
            glfwGetFramebufferSize(window, &fw, &fh);
            LOGI("First frame rendered (%dx%d) via GLFW shim", fw, fh);
        }
        auto now = std::chrono::steady_clock::now();
        float dt = std::chrono::duration<float>(now - lastStats).count();
        if (dt >= 1.0f) {
            int fw = 0, fh = 0;
            glfwGetFramebufferSize(window, &fw, &fh);
            unsigned char px[4] = {0, 0, 0, 0};
            if (p_glReadPixels && fw > 0 && fh > 0) {
                p_glReadPixels(fw / 2, fh / 2, 1, 1, GL_RGBA, GL_UNSIGNED_BYTE, px);
            }
            LOGI("frames=%lld fps=%.1f palette=%d center_pixel_RGBA=(%d,%d,%d,%d)", frameCount,
                 static_cast<float>(framesSinceStats) / dt, g_palette, px[0], px[1], px[2], px[3]);
            lastStats = now;
            framesSinceStats = 0;
        }
    }

    LOGI("game_client: loop ended, tearing down");
    glfwDestroyWindow(window);
    glfwTerminate();
}
