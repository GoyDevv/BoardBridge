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

// Entry point for the native "game thread" that drives the GLFW shim exactly
// as an LWJGL (org.lwjgl.glfw + org.lwjgl.opengles) program would: create a
// window, make the context current, load GL via glfwGetProcAddress, and run a
// draw/swap/poll loop. Runs until glfwWindowShouldClose() becomes true.
void game_client_run();
