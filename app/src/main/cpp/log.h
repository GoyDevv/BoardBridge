/*
 * Copyright 2026 The BoardBridge Authors
 * Licensed under the Apache License, Version 2.0.
 */
#pragma once

#include <android/log.h>

#define BB_TAG "BoardBridge"
#define LOGI(...) __android_log_print(ANDROID_LOG_INFO, BB_TAG, __VA_ARGS__)
#define LOGW(...) __android_log_print(ANDROID_LOG_WARN, BB_TAG, __VA_ARGS__)
#define LOGE(...) __android_log_print(ANDROID_LOG_ERROR, BB_TAG, __VA_ARGS__)
#define LOGD(...) __android_log_print(ANDROID_LOG_DEBUG, BB_TAG, __VA_ARGS__)
