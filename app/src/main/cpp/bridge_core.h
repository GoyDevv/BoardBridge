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

#include <android/native_window.h>

#include <cstdint>

// The passive bridge core. BoardBridge no longer runs its own render loop:
// the Android UI thread (via JNI) is the *producer* that supplies the
// ANativeWindow, size, input, and lifecycle flags; a separate *consumer* (the
// game thread, driving the GLFW shim) owns the EGL context and does the actual
// drawing. This is the ownership model a real LWJGL/Minecraft launcher needs.
namespace bridge {

enum class EventType { Touch, Key };

struct Event {
    EventType type;
    int32_t code;     // Touch: pointerId ; Key: keyCode
    int32_t action;   // MotionEvent / KeyEvent action
    int32_t unicode;  // Key: unicode char
    float x;          // Touch x
    float y;          // Touch y
    int64_t timeMs;
};

// ---- Producer side (Android UI / JNI thread) ----
void setWindow(ANativeWindow* window);  // takes ownership of one reference
void clearWindow();                     // blocks (bounded) until the consumer frees its surface
void setSize(int width, int height);
void setPaused(bool paused);
void requestClose();
void beginSession();  // reset closing/paused/destroy flags for a fresh game-thread session
void pushTouch(int32_t pointerId, int32_t action, float x, float y, int64_t timeMs);
void pushKey(int32_t keyCode, int32_t action, int32_t unicode, int64_t timeMs);
void setRendererInfo(const char* info);
void getRendererInfo(char* out, int cap);

// ---- Consumer side (game thread / GLFW shim) ----
// Blocks until a usable window exists and rendering is not paused, or until
// close is requested. Returns the window (NOT owned; valid until the next
// generation change) or nullptr if closing.
ANativeWindow* consumerWaitWindow();
void consumerSurfaceAlive(bool alive);  // consumer created (true) / destroyed (false) its EGLSurface
int64_t windowGeneration();
bool consumerShouldRelease(int64_t boundGeneration);
bool shouldClose();
bool isPaused();
bool pollEvent(Event* out);
void getSize(int* width, int* height);

}  // namespace bridge
