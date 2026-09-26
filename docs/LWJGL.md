# LWJGL glue — not started

Status: **not started**. There is no code for this yet, and nothing in the
repository claims otherwise. This document is the plan, written down so the gap is
explicit and the work can be picked up without re-deriving it.

## What LWJGL needs on Android

LWJGL is not one library: the game talks to a Java API whose native backing is a
set of `.so` files it loads at runtime. On Android the loading rules are
different from the desktop, and that is the whole problem:

| LWJGL module | Native library | What Android needs |
|---|---|---|
| `lwjgl` (core) | `liblwjgl.so` | build for the Android ABI; its `Pointer`/`MemoryUtil` code is portable, but the VM assumptions (`GetDirectBufferAddress`, `Unsafe` access) must be verified against the launcher's JVM (ART vs. a bundled HotSpot/OpenJDK build) |
| `lwjgl-glfw` | `liblwjgl_glfw.so` + GLFW itself | the GLFW shim from [GLFW_COMPAT.md](GLFW_COMPAT.md), plus LWJGL's `glfw*` binding layer |
| `lwjgl-sdl` | `liblwjgl_sdl.so` + SDL3 | the SDL3 build from [SDL3.md](SDL3.md), plus LWJGL's SDL binding layer |
| `lwjgl-opengl` / `lwjgl-opengles` | `liblwjgl_opengl.so`, `liblwjgl_opengles.so` | the game's GL calls must resolve to `libGLESv2.so`; LWJGL's function-pointer lookup goes through `glfwGetProcAddress`/`SDL_GL_GetProcAddress`, so the shim must return the real driver pointers |
| `lwjgl-stb`, `lwjgl-tinyfd`, `lwjgl-jemalloc`, `lwjgl-openal` | per module | build per ABI; `lwjgl-jemalloc` must not fight Android's allocator, and audio needs the game to use Android's audio path or a bundled backend |

LWJGL resolves each module's native library by name through the standard
`System.loadLibrary` mechanism, overridable with system properties such as
`org.lwjgl.glfw.libname`, `org.lwjgl.sdl.libname`, `org.lwjgl.librarypath` and
`org.lwjgl.system.allocator`. **Verify the exact property names and the loading
path against the LWJGL version actually shipped** — this is version-specific
behaviour, and a wrong assumption here produces an `UnsatisfiedLinkError` at game
startup, not at build time. That verification is part of the work below, not an
afterthought.

## What exists here that the glue needs

| Need | Already in this repository |
|---|---|
| A window and an EGLSurface that survive rotation | **yes** — the lifecycle machine and the GLES backend |
| A context the game can make current on its own thread | **yes** — `attachGameThread` / `detachGameThread` / `swapBuffers` |
| Input in GLFW/SDL shape | **yes** — `InputEvent` and the generated SDL tables |
| Surface loss delivered to the game without interruption | **yes** — `LifecycleNotice::SurfaceRevoked` in the queue |
| GL function pointers | **partly** — the GLES backend loads `libGLESv2.so` itself; a shim must expose `glfwGetProcAddress`/`SDL_GL_GetProcAddress` equivalents so LWJGL's `GL.createCapabilities()` gets the driver's pointers, not a second copy |
| The JVM itself (`libjvm.so`, `java.home`, classpath) | **no — out of scope by design.** That is the launcher's job; this repository stops at the surface/EGL/input boundary |

## Remaining work, in order

1. **Decide the JVM.** LWJGL needs a JVM whose JNI behaviour it supports; a
   launcher typically ships its own (for example a bundled OpenJDK build) rather
   than using ART's. Nothing in this repository can decide that, and the LWJGL
   glue cannot be tested without it.
2. **Build LWJGL's natives for `arm64-v8a` (and the other ABIs).** LWJGL's
   `build.xml`/Gradle build is desktop-oriented; Android needs the NDK toolchain,
   `libc++_shared` or `c++_static` decisions, and a per-module list of what the
   game really loads. Ship the results in `jniLibs`.
3. **Point LWJGL's load path at those libraries.** Set the library-name and
   library-path properties (or pre-load with `System.loadLibrary`, keeping the
   `System.loadLibrary`-visible name LWJGL expects, for example `glfw`).
   Verify against the shipped LWJGL version, and document the result here.
4. **Implement the GLFW and/or SDL3 shim** — those are the modules LWJGL binds
   *against*, so the shim is what makes `glfwCreateWindow`/`SDL_CreateWindow` mean
   "the surface the bridge owns". See [GLFW_COMPAT.md](GLFW_COMPAT.md) and
   [SDL3.md](SDL3.md).
5. **Expose GL function pointers.** `glfwGetProcAddress`/`SDL_GL_GetProcAddress`
   must return the Android driver's `eglGetProcAddress` results so LWJGL's GL
   capabilities match the context the bridge created.
6. **Prove it with a minimal program.** A Java program that inits the shim,
   creates a window, attaches the game thread, clears the screen, polls input and
   prints the scancode of `KEYCODE_A` — run through the same emulator workflow as
   the demo app. That is the smallest honest end-to-end test of the glue, and it
   reuses the assertions the CI render test already makes.
7. **Then, and only then**, a real Minecraft launch: asset/log/version handling,
   the launcher's own auth and instance management, and the game's own GL
   capability checks (`GL_VERSION` must be ≥ what the version requires; on
   Android that means the ES 3.x context the GLES backend already creates).

Until steps 1–6 are done, `runtime::VERSION`-style reporting should say so; the
launcher UI's backend page is the natural place to show "LWJGL glue: not started"
next to "Vulkan: interface only" and "SDL3: interface only".
