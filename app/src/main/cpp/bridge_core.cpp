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
#include "bridge_core.h"

#include <chrono>
#include <condition_variable>
#include <cstring>
#include <deque>
#include <mutex>
#include <string>

#include "log.h"

namespace bridge {
namespace {

std::mutex g_mutex;
std::condition_variable g_cv;

ANativeWindow* g_window = nullptr;
int64_t g_generation = 0;
int g_width = 0;
int g_height = 0;
bool g_paused = false;
bool g_closing = false;
bool g_destroyPending = false;
bool g_consumerSurfaceAlive = false;

std::deque<Event> g_events;
constexpr size_t kMaxEvents = 4096;

std::mutex g_infoMutex;
std::string g_info;

}  // namespace

void setWindow(ANativeWindow* window) {
    std::lock_guard<std::mutex> lock(g_mutex);
    if (g_window != nullptr && g_window != window) {
        ANativeWindow_release(g_window);
    }
    g_window = window;
    g_destroyPending = false;
    ++g_generation;
    g_cv.notify_all();
    LOGI("bridge: window set (gen=%lld)", static_cast<long long>(g_generation));
}

void clearWindow() {
    std::unique_lock<std::mutex> lock(g_mutex);
    g_destroyPending = true;
    g_cv.notify_all();
    // Bounded wait for the consumer to destroy its EGLSurface (which references
    // this ANativeWindow) before we release the window — prevents use-after-free.
    g_cv.wait_for(lock, std::chrono::milliseconds(1500),
                  [] { return !g_consumerSurfaceAlive; });
    if (g_window != nullptr) {
        ANativeWindow_release(g_window);
        g_window = nullptr;
    }
    ++g_generation;
    g_destroyPending = false;
    g_cv.notify_all();
    LOGI("bridge: window cleared (gen=%lld)", static_cast<long long>(g_generation));
}

void setSize(int width, int height) {
    std::lock_guard<std::mutex> lock(g_mutex);
    g_width = width;
    g_height = height;
    g_cv.notify_all();
}

void setPaused(bool paused) {
    std::lock_guard<std::mutex> lock(g_mutex);
    g_paused = paused;
    g_cv.notify_all();
}

void requestClose() {
    std::lock_guard<std::mutex> lock(g_mutex);
    g_closing = true;
    g_cv.notify_all();
}

void beginSession() {
    std::lock_guard<std::mutex> lock(g_mutex);
    g_closing = false;
    g_destroyPending = false;
    g_paused = false;
    g_cv.notify_all();
    LOGI("bridge: begin session");
}

static void pushLocked(const Event& e) {
    if (g_events.size() >= kMaxEvents) {
        g_events.pop_front();
    }
    g_events.push_back(e);
}

void pushTouch(int32_t pointerId, int32_t action, float x, float y, int64_t timeMs) {
    std::lock_guard<std::mutex> lock(g_mutex);
    pushLocked(Event{EventType::Touch, pointerId, action, 0, x, y, timeMs});
}

void pushKey(int32_t keyCode, int32_t action, int32_t unicode, int64_t timeMs) {
    std::lock_guard<std::mutex> lock(g_mutex);
    pushLocked(Event{EventType::Key, keyCode, action, unicode, 0.0f, 0.0f, timeMs});
}

void setRendererInfo(const char* info) {
    std::lock_guard<std::mutex> lock(g_infoMutex);
    g_info = (info != nullptr) ? info : "";
}

void getRendererInfo(char* out, int cap) {
    std::lock_guard<std::mutex> lock(g_infoMutex);
    if (out != nullptr && cap > 0) {
        std::strncpy(out, g_info.c_str(), static_cast<size_t>(cap) - 1);
        out[cap - 1] = '\0';
    }
}

ANativeWindow* consumerWaitWindow() {
    std::unique_lock<std::mutex> lock(g_mutex);
    g_cv.wait(lock, [] {
        return g_closing || (g_window != nullptr && !g_paused && !g_destroyPending);
    });
    if (g_closing) {
        return nullptr;
    }
    return g_window;
}

void consumerSurfaceAlive(bool alive) {
    std::lock_guard<std::mutex> lock(g_mutex);
    g_consumerSurfaceAlive = alive;
    g_cv.notify_all();
}

int64_t windowGeneration() {
    std::lock_guard<std::mutex> lock(g_mutex);
    return g_generation;
}

bool consumerShouldRelease(int64_t boundGeneration) {
    std::lock_guard<std::mutex> lock(g_mutex);
    return g_closing || g_destroyPending || g_paused || g_generation != boundGeneration;
}

bool shouldClose() {
    std::lock_guard<std::mutex> lock(g_mutex);
    return g_closing;
}

bool isPaused() {
    std::lock_guard<std::mutex> lock(g_mutex);
    return g_paused;
}

bool pollEvent(Event* out) {
    std::lock_guard<std::mutex> lock(g_mutex);
    if (g_events.empty()) {
        return false;
    }
    *out = g_events.front();
    g_events.pop_front();
    return true;
}

void getSize(int* width, int* height) {
    std::lock_guard<std::mutex> lock(g_mutex);
    if (width != nullptr) *width = g_width;
    if (height != nullptr) *height = g_height;
}

}  // namespace bridge
