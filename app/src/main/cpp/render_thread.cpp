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
#include "render_thread.h"

#include <cmath>

#include "log.h"

namespace {
constexpr float kTwoPi = 6.28318530718f;

GLuint compileShader(GLenum type, const char* source) {
    GLuint shader = glCreateShader(type);
    glShaderSource(shader, 1, &source, nullptr);
    glCompileShader(shader);
    GLint ok = GL_FALSE;
    glGetShaderiv(shader, GL_COMPILE_STATUS, &ok);
    if (ok != GL_TRUE) {
        char buffer[512];
        glGetShaderInfoLog(shader, sizeof(buffer), nullptr, buffer);
        LOGE("Shader compile failed: %s", buffer);
        glDeleteShader(shader);
        return 0;
    }
    return shader;
}
}  // namespace

RenderThread::RenderThread() = default;

RenderThread::~RenderThread() {
    requestStop();
}

void RenderThread::start() {
    std::lock_guard<std::mutex> lock(mutex_);
    if (running_) {
        return;
    }
    running_ = true;
    thread_ = std::thread(&RenderThread::threadMain, this);
}

void RenderThread::requestStop() {
    {
        std::lock_guard<std::mutex> lock(mutex_);
        if (!running_ && !thread_.joinable()) {
            return;
        }
        running_ = false;
        cond_.notify_all();
    }
    if (thread_.joinable()) {
        thread_.join();
    }
}

void RenderThread::setWindow(ANativeWindow* window) {
    std::lock_guard<std::mutex> lock(mutex_);
    if (!running_) {
        // No render thread to take ownership; avoid leaking the reference.
        if (window != nullptr) {
            ANativeWindow_release(window);
        }
        return;
    }
    if (pendingWindow_ != nullptr && pendingWindow_ != window) {
        ANativeWindow_release(pendingWindow_);
    }
    pendingWindow_ = window;  // ownership transferred in
    windowChanged_ = true;
    paused_ = false;
    cond_.notify_all();
}

void RenderThread::clearWindow() {
    std::unique_lock<std::mutex> lock(mutex_);
    if (!running_) {
        if (currentWindow_ != nullptr) {
            ANativeWindow_release(currentWindow_);
            currentWindow_ = nullptr;
        }
        if (pendingWindow_ != nullptr) {
            ANativeWindow_release(pendingWindow_);
            pendingWindow_ = nullptr;
        }
        return;
    }
    // The Surface is gone: drop any not-yet-bound pending window as well.
    if (pendingWindow_ != nullptr) {
        ANativeWindow_release(pendingWindow_);
        pendingWindow_ = nullptr;
        windowChanged_ = false;
    }
    clearRequested_ = true;
    clearAck_ = false;
    cond_.notify_all();
    // Block until the render thread has released the window (or is stopping).
    cond_.wait(lock, [this] { return clearAck_ || !running_; });
}

void RenderThread::setSize(int width, int height) {
    std::lock_guard<std::mutex> lock(mutex_);
    width_ = width;
    height_ = height;
    cond_.notify_all();
}

void RenderThread::setPaused(bool paused) {
    std::lock_guard<std::mutex> lock(mutex_);
    paused_ = paused;
    cond_.notify_all();
}

std::string RenderThread::rendererInfo() {
    std::lock_guard<std::mutex> lock(infoMutex_);
    return rendererInfo_;
}

void RenderThread::processWindowChangeLocked(std::unique_lock<std::mutex>& lock) {
    if (clearRequested_) {
        egl_.destroyWindowSurface();
        if (currentWindow_ != nullptr) {
            ANativeWindow_release(currentWindow_);
            currentWindow_ = nullptr;
        }
        clearRequested_ = false;
        clearAck_ = true;
        cond_.notify_all();  // wake the clearWindow() waiter
    }

    if (windowChanged_) {
        windowChanged_ = false;
        egl_.destroyWindowSurface();
        if (currentWindow_ != nullptr) {
            ANativeWindow_release(currentWindow_);
            currentWindow_ = nullptr;
        }
        ANativeWindow* window = pendingWindow_;
        pendingWindow_ = nullptr;
        currentWindow_ = window;  // take ownership

        if (window != nullptr) {
            lock.unlock();
            bool created = egl_.createWindowSurface(window);
            if (created && egl_.makeCurrent()) {
                std::string info = egl_.queryGlInfo();
                {
                    std::lock_guard<std::mutex> infoLock(infoMutex_);
                    rendererInfo_ = info;
                }
                LOGI("Surface bound. %s", info.c_str());
            } else {
                LOGE("Failed to bind window surface");
            }
            lock.lock();
        }
    }
}

void RenderThread::threadMain() {
    startTime_ = std::chrono::steady_clock::now();

    if (!egl_.initialize()) {
        LOGE("EGL initialization failed; render thread exiting");
        std::lock_guard<std::mutex> lock(mutex_);
        running_ = false;
        if (pendingWindow_ != nullptr) {
            ANativeWindow_release(pendingWindow_);
            pendingWindow_ = nullptr;
        }
        if (currentWindow_ != nullptr) {
            ANativeWindow_release(currentWindow_);
            currentWindow_ = nullptr;
        }
        return;
    }

    std::unique_lock<std::mutex> lock(mutex_);
    while (running_) {
        if (windowChanged_ || clearRequested_) {
            processWindowChangeLocked(lock);
        }
        if (!running_) {
            break;
        }
        const bool canRender = !paused_ && egl_.hasSurface();
        if (!canRender) {
            cond_.wait(lock);
            continue;
        }
        const int width = width_;
        const int height = height_;
        lock.unlock();

        drainInput(width, height);
        egl_.makeCurrent();
        drawFrame(width, height);
        egl_.swapBuffers();  // paces to the display refresh (vsync)

        lock.lock();
    }

    // Teardown while still holding the lock for the shared window pointers.
    egl_.destroyWindowSurface();
    egl_.makeNothingCurrent();
    if (currentWindow_ != nullptr) {
        ANativeWindow_release(currentWindow_);
        currentWindow_ = nullptr;
    }
    if (pendingWindow_ != nullptr) {
        ANativeWindow_release(pendingWindow_);
        pendingWindow_ = nullptr;
    }
    lock.unlock();
    egl_.release();
}

void RenderThread::drainInput(int width, int height) {
    InputEvent event;
    while (input_.pop(event)) {
        if (event.type == InputType::Touch) {
            if (width > 0 && height > 0) {
                focusX_.store(event.x / static_cast<float>(width));
                focusY_.store(event.y / static_cast<float>(height));
            }
        } else {  // Key
            // KeyEvent.ACTION_DOWN == 0; shift the palette on each key-down.
            if (event.action == 0) {
                hueShift_.store(hueShift_.load() + 0.15f);
            }
        }
    }
}

bool RenderThread::initGl() {
    static const char* kVertexSrc =
            "#version 300 es\n"
            "layout(location = 0) in vec2 aPos;\n"
            "layout(location = 1) in vec3 aColor;\n"
            "uniform float uAngle;\n"
            "out vec3 vColor;\n"
            "void main() {\n"
            "  float c = cos(uAngle);\n"
            "  float s = sin(uAngle);\n"
            "  mat2 rot = mat2(c, -s, s, c);\n"
            "  gl_Position = vec4(rot * aPos, 0.0, 1.0);\n"
            "  vColor = aColor;\n"
            "}\n";
    static const char* kFragmentSrc =
            "#version 300 es\n"
            "precision mediump float;\n"
            "in vec3 vColor;\n"
            "out vec4 fragColor;\n"
            "void main() {\n"
            "  fragColor = vec4(vColor, 1.0);\n"
            "}\n";

    GLuint vs = compileShader(GL_VERTEX_SHADER, kVertexSrc);
    GLuint fs = compileShader(GL_FRAGMENT_SHADER, kFragmentSrc);
    if (vs == 0 || fs == 0) {
        return false;
    }
    program_ = glCreateProgram();
    glAttachShader(program_, vs);
    glAttachShader(program_, fs);
    glLinkProgram(program_);
    GLint linked = GL_FALSE;
    glGetProgramiv(program_, GL_LINK_STATUS, &linked);
    glDeleteShader(vs);
    glDeleteShader(fs);
    if (linked != GL_TRUE) {
        char buffer[512];
        glGetProgramInfoLog(program_, sizeof(buffer), nullptr, buffer);
        LOGE("Program link failed: %s", buffer);
        glDeleteProgram(program_);
        program_ = 0;
        return false;
    }
    uAngleLoc_ = glGetUniformLocation(program_, "uAngle");

    // Interleaved position(x, y) + color(r, g, b).
    static const GLfloat vertices[] = {
            0.0f,  0.6f, 1.0f, 0.25f, 0.25f,
            -0.6f, -0.5f, 0.25f, 1.0f, 0.25f,
            0.6f, -0.5f, 0.25f, 0.25f, 1.0f,
    };
    glGenVertexArrays(1, &vao_);
    glGenBuffers(1, &vbo_);
    glBindVertexArray(vao_);
    glBindBuffer(GL_ARRAY_BUFFER, vbo_);
    glBufferData(GL_ARRAY_BUFFER, sizeof(vertices), vertices, GL_STATIC_DRAW);
    glVertexAttribPointer(0, 2, GL_FLOAT, GL_FALSE, 5 * sizeof(GLfloat),
                          reinterpret_cast<void*>(0));
    glEnableVertexAttribArray(0);
    glVertexAttribPointer(1, 3, GL_FLOAT, GL_FALSE, 5 * sizeof(GLfloat),
                          reinterpret_cast<void*>(2 * sizeof(GLfloat)));
    glEnableVertexAttribArray(1);
    glBindVertexArray(0);
    return true;
}

void RenderThread::drawFrame(int width, int height) {
    if (!glTried_) {
        glOk_ = initGl();
        glTried_ = true;
    }

    const float fx = focusX_.load();
    const float fy = focusY_.load();
    const float hue = hueShift_.load();
    const float t =
            std::chrono::duration<float>(std::chrono::steady_clock::now() - startTime_).count();

    // Clear color animates over time and responds to the last touch + key hue.
    const float r = 0.5f + 0.5f * std::sin(t + fx * kTwoPi + hue);
    const float g = 0.5f + 0.5f * std::sin(t * 0.7f + fy * kTwoPi + hue + 2.094f);
    const float b = 0.5f + 0.5f * std::sin(t * 1.3f + hue + 4.188f);

    if (width > 0 && height > 0) {
        glViewport(0, 0, width, height);
    }
    glClearColor(r, g, b, 1.0f);
    glClear(GL_COLOR_BUFFER_BIT | GL_DEPTH_BUFFER_BIT);

    if (glOk_) {
        glUseProgram(program_);
        glUniform1f(uAngleLoc_, t);
        glBindVertexArray(vao_);
        glDrawArrays(GL_TRIANGLES, 0, 3);
        glBindVertexArray(0);
    }
}
