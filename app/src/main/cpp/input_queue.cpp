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
#include "input_queue.h"

void InputQueue::pushLocked(const InputEvent& event) {
    if (queue_.size() >= kMaxEvents) {
        queue_.pop_front();  // drop the oldest to bound memory
    }
    queue_.push_back(event);
}

void InputQueue::pushTouch(int32_t pointerId, int32_t action, float x, float y, int64_t timeMs) {
    InputEvent e{};
    e.type = InputType::Touch;
    e.code = pointerId;
    e.action = action;
    e.unicodeChar = 0;
    e.x = x;
    e.y = y;
    e.timeMs = timeMs;
    std::lock_guard<std::mutex> lock(mutex_);
    pushLocked(e);
}

void InputQueue::pushKey(int32_t keyCode, int32_t action, int32_t unicodeChar, int64_t timeMs) {
    InputEvent e{};
    e.type = InputType::Key;
    e.code = keyCode;
    e.action = action;
    e.unicodeChar = unicodeChar;
    e.x = 0.0f;
    e.y = 0.0f;
    e.timeMs = timeMs;
    std::lock_guard<std::mutex> lock(mutex_);
    pushLocked(e);
}

bool InputQueue::pop(InputEvent& out) {
    std::lock_guard<std::mutex> lock(mutex_);
    if (queue_.empty()) {
        return false;
    }
    out = queue_.front();
    queue_.pop_front();
    return true;
}

void InputQueue::clear() {
    std::lock_guard<std::mutex> lock(mutex_);
    queue_.clear();
}
