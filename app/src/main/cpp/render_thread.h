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
 */
#pragma once

#include <GLES3/gl3.h>
#include <android/native_window.h>

#include <atomic>
#include <chrono>
#include <condition_variable>
#include <mutex>
#include <string>
#include <thread>

#include "egl_core.h"
#include "input_queue.h"

/**
 * A dedicated native render thread that owns the EGL context and draws to the
 * Android Surface handed down from Java.
 *
 * The tricky part of any Surface-based renderer is the lifecycle race: the
 * `ANativeWindow` must not be touched after `surfaceDestroyed` returns, or the
 * app crashes. RenderThread solves this with a mutex + condition variable and a
 * *synchronous* clearWindow(): the UI thread blocks inside surfaceDestroyed
 * until the render thread has unbound the EGLSurface and released the window.
 *
 * Render behavior is a minimal runtime test:
 *   - SOLID mode (default): clear the surface to a fixed solid color and swap
 *     buffers every frame. This is the easiest thing to verify via a screenshot
 *     or a glReadPixels center-pixel readback in logcat.
 *   - TRIANGLE mode: an OpenGL ES 3.0 spinning triangle (proves the shader
 *     pipeline). A touch-DOWN or key-DOWN toggles between the two modes, so the
 *     input path visibly drives rendering and neither branch is dead code.
 */
class RenderThread {
public:
    enum class RenderMode { Solid = 0, Triangle = 1 };

    RenderThread();
    ~RenderThread();

    RenderThread(const RenderThread&) = delete;
    RenderThread& operator=(const RenderThread&) = delete;

    void start();
    void requestStop();  // signals the thread to exit and joins it

    // --- Surface lifecycle (called from the JNI/UI thread) ---
    // Ownership of `window` (one reference obtained via ANativeWindow_fromSurface)
    // is transferred into the RenderThread.
    void setWindow(ANativeWindow* window);
    void clearWindow();  // blocks until the render thread released the window
    void setSize(int width, int height);
    void setPaused(bool paused);

    InputQueue& input() { return input_; }
    std::string rendererInfo();

private:
    void threadMain();
    void processWindowChangeLocked(std::unique_lock<std::mutex>& lock);
    void drainInput();
    void drawFrame(int width, int height);
    void logFrameStatsIfDue(int width, int height);
    bool initGl();
    void toggleMode();
    const char* modeName() const;

    std::thread thread_;
    std::mutex mutex_;
    std::condition_variable cond_;

    // Shared state guarded by mutex_.
    ANativeWindow* pendingWindow_ = nullptr;
    ANativeWindow* currentWindow_ = nullptr;
    bool windowChanged_ = false;
    bool clearRequested_ = false;
    bool clearAck_ = false;
    bool running_ = false;
    bool paused_ = false;
    int width_ = 0;
    int height_ = 0;

    EglCore egl_;
    InputQueue input_;

    std::mutex infoMutex_;
    std::string rendererInfo_;

    // Render mode, toggled by input (proves the touch/key path end-to-end).
    std::atomic<int> renderMode_{static_cast<int>(RenderMode::Solid)};

    // Frame timing / stats for logcat-based verification.
    std::chrono::steady_clock::time_point startTime_{};
    std::chrono::steady_clock::time_point lastStatsTime_{};
    long long frameCount_ = 0;
    int framesSinceStats_ = 0;

    // GLES 3.0 triangle resources (created once against the context).
    GLuint program_ = 0;
    GLuint vao_ = 0;
    GLuint vbo_ = 0;
    GLint uAngleLoc_ = -1;
    bool glTried_ = false;
    bool glOk_ = false;
};
