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

#include <cstdint>
#include <deque>
#include <mutex>

enum class InputType {
    Touch,
    Key,
};

// A single input sample marshaled from the Java input thread to the renderer.
struct InputEvent {
    InputType type;
    int32_t   code;         // Touch: pointerId ; Key: keyCode
    int32_t   action;       // MotionEvent / KeyEvent action
    int32_t   unicodeChar;  // Key only
    float     x;            // Touch only
    float     y;            // Touch only
    int64_t   timeMs;
};

/**
 * A bounded, mutex-protected producer/consumer queue. Producers are Android
 * input threads (via JNI); the single consumer is the native render thread.
 */
class InputQueue {
public:
    void pushTouch(int32_t pointerId, int32_t action, float x, float y, int64_t timeMs);
    void pushKey(int32_t keyCode, int32_t action, int32_t unicodeChar, int64_t timeMs);

    // Pops the oldest event into `out`; returns false if the queue is empty.
    bool pop(InputEvent& out);

    void clear();

private:
    void pushLocked(const InputEvent& event);

    std::mutex mutex_;
    std::deque<InputEvent> queue_;
    static constexpr size_t kMaxEvents = 4096;  // drop oldest beyond this cap
};
