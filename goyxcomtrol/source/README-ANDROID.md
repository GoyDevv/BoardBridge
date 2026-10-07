# XTG Cloud Pad 1.1.0 - Android

An on-screen Xbox controller for Android with two completely different ways of reaching
the game, a full Material You interface, and your Warzone layout built in.

## What's in it

- **Material You.** On Android 12+ the palette is read straight from the system's
  wallpaper-derived colours (`system_accent1_*`, `system_neutral*`), so the settings
  panel, the pad outlines, the pressed states and the sliders all follow your wallpaper.
  Older versions get the same design in a fixed blue.
- **Six settings tabs:** Camera, Pad, Layout, Cloud, Controller, About.
- **Profiles:** Default, Warzone, Saved A, Saved B - with **portrait and landscape stored
  separately**, so rotating the phone swaps to the layout you built for that way round.
- **Your Warzone layout, carried over verbatim** from the browser extension: Shoot under
  the left thumb, scope on the right, run and Slide where the thumb already is, square
  buttons and custom labels and all.
- **Full layout editor:** drag to move, size, height for the zones, per-control fade,
  round or square, **rename to anything**, **remap what each control sends** (any of the
  17 pad inputs), hide/show per control, plus a list of every control with a switch.
- **Left stick anywhere:** the stick appears wherever your thumb lands inside its
  rectangle, so you never have to find it. Or pin it in place.
- **Analog triggers:** drag up LT or RT for partial pull. A real trigger is not a switch.
- **Mouse + keyboard mode:** the camera becomes a real mouse, the left stick becomes WASD,
  the buttons send keys. Pointer lock is answered so the client accepts it.
- **PC mode** - see below. This is the part a browser extension genuinely cannot do.
- **Fade when idle**, opacity, lite graphics, deadzone, invert vertical, debug HUD.
- **Pad on/off** by long-pressing the gear, so you can reach the page to press Play.
- **Backup:** copy a layout to the clipboard, paste one back.
- **No vibration anywhere** unless you switch it on. No analytics. No network of its own.

## PC mode - why this works here and not in an extension

The cloud client decides once, while it starts up, whether you are a touch device or a
mouse-and-keyboard device, and it decides from the device rather than from the events
that turn up later. On a phone it picks touch and never attaches a mouse or key listener
at all.

A browser extension can only lie in JavaScript. The HTTP request your browser already
sent still says Android, the client notices that its own story stopped matching itself,
and it refuses to start - which is exactly the "we ran into some error" the extension
produced when it tried.

**The app sets the user agent on the WebView itself**, so the request header and the
JavaScript say the same thing: desktop Chrome on Windows, no touchscreen, a fine hovering
pointer, no `ontouchstart`, `platform` `Win32`, non-mobile `userAgentData`. Nothing to
catch. Turning PC mode on reloads the page, because that decision happens at start-up.

## 1. Built-in browser mode (works immediately, no setup)

The app is a full-screen browser pointed at Xbox Cloud Gaming. The pad is drawn natively
on top of it and the page's `navigator.getGamepads()` is answered by the app itself.

Why this is better than the browser extension, even though it does the same job:

- The page's own `getGamepads()` call reaches the touch state **synchronously**, through
  a Java method, in the same JavaScript task. There is no event hop, no frame boundary,
  nothing queued.
- Touch is read natively, including every historical sample inside a batched
  `MotionEvent` with its own event time. Measured travel ratio is **1.000** - the camera
  delivers exactly as much turn as your thumb asked for. The extension sits at about
  1.07-1.15 because a browser content script cannot see the digitiser as directly.
- The WebView's "; wv" marker is stripped from the user agent, because cloud clients use
  it to switch themselves off.

## 2. Shizuku mode - a real controller (experimental)

With Shizuku running, the app creates a **genuine virtual Xbox controller inside the
kernel**. Not an API spoof: a real input device that every app on the phone can see -
this browser, Quetta, Chrome, the official Xbox app - with analog triggers.

### Why this is possible

`/dev/uhid` is a write-only protocol: you write one packed `uhid_event` to create a HID
device and another for every input report. **No ioctl is involved**, which is the only
reason this can be done in pure Java with no native code at all. (`/dev/uinput` is driven
entirely by ioctl and would need JNI.)

And the shell user is specifically allowed to use it:

- `system/sepolicy/private/shell.te`: `allow shell uhid_device:chr_file rw_file_perms;`
- `file_contexts`: both `/dev/uinput` and `/dev/uhid` are labelled `uhid_device`
- `ueventd.rc`: `/dev/uhid 0660 uhid uhid`
- `packages/modules/adb/daemon/main.cpp` adds `AID_UHID` to the shell process's
  supplementary groups, with the comment *"for using 'hid' command to read/write to
  /dev/uhid"*

A Shizuku user service is forked from that shell, so it inherits exactly those rights.
The app's own process has none of them, which is why the privileged half is a separate
service and the app only makes binder calls into it.

### The controller it creates

It reports itself as Microsoft `045E:02EA`, so Android loads its own built-in
`Vendor_045e_Product_02ea.kl` keylayout. The HID report descriptor is written to agree
with that file exactly:

| In the report | evdev | Android | Pad |
|---|---|---|---|
| X, Y | ABS_X/Y | AXIS_X/Y | left stick |
| Rx, Ry | ABS_RX/RY | AXIS_Z/RZ | right stick |
| Z | ABS_Z | LTRIGGER | LT, analog |
| Rz | ABS_RZ | RTRIGGER | RT, analog |
| Hat switch | ABS_HAT0X/Y | HAT_X/Y | d-pad |
| Buttons 1,2,4,5,7,8,11,12,13,14,15 | BTN_SOUTH, EAST, NORTH, WEST, TL, TR, SELECT, START, MODE, THUMBL, THUMBR | BUTTON_A/B/X/Y/L1/R1/SELECT/START/MODE/THUMBL/THUMBR | A B X Y LB RB View Menu Xbox L3 R3 |

Buttons 3, 6, 9 and 10 are deliberately skipped - that is where a real Xbox Bluetooth
pad leaves gaps, and the kernel's gamepad button mapping is positional.

Reports are pushed at up to 250Hz and only when a byte actually changed, so an idle pad
costs nothing.

### Setting it up

1. Install Shizuku (Play Store or GitHub) and start it. On Android 11 and newer,
   wireless debugging is enough - **no PC needed**. With root it is even simpler.
2. Open XTG Cloud Pad, press the gear, turn on *Real controller through Shizuku*.
3. Grant the permission when Shizuku asks.
4. The status line tells you exactly what happened, including the uid it got and the
   errno if `/dev/uhid` refused.

Shizuku has to be restarted after every reboot unless you are rooted - that is a Shizuku
limitation, not this app's.

### Using it over other apps

Gear -> *Show the pad over other apps*. You will be sent to the Android permission screen
once. After that a small round button sits in the top-right corner of every app; tap it
to put the pad up or take it down.

The pad is two windows on purpose. A full-screen overlay swallows every touch inside its
bounds - Android gives you no way to hand an unclaimed one back to the app underneath -
so only the thumb-sized button stays on screen permanently.

## Notes and limits

- **Honest caveat:** the Shizuku path is written from the kernel and AOSP sources listed
  above, but it has not been run on a physical phone. If it fails, the status line is the
  diagnosis - send it over.
- Long-press the gear to turn the pad off. You need this to press Play on the page,
  because the camera zone covers the right half of the screen.
- There is no vibration anywhere in this app, by design.
- A virtual HID device is visible to everything on the phone while it exists, including
  anti-cheat that enumerates input devices. Cloud gaming streams your input to a server
  and does not care, but be aware of it.

## Building from source

No Gradle, no Android Studio. `build.sh` drives the SDK tools directly:
aapt2 -> aidl -> javac -> d8 -> zipalign -> apksigner.

```sh
ANDROID_HOME=/path/to/sdk ./build.sh
```

You need `build-tools;34.0.0`, `platforms;android-34`, a JDK 17, and the three Shizuku
client jars (`dev.rikka.shizuku:api`, `:provider`, `:aidl`, version 13.1.5) in
`../android/libs/`. Edit the `LIBS` and `JAVAC` lines in `build.sh` if your paths differ.

## Source map

| File | What it does |
|---|---|
| `PadState.java` | the whole controller state, the camera estimator, the 15-byte HID report |
| `PadView.java` | drawing, multi-touch, the layout editor |
| `Ctrl.java` | one control; the default layout |
| `UHid.java` | the HID descriptor and the `/dev/uhid` protocol, in pure Java |
| `VPadService.java` | runs inside Shizuku as shell and owns the device |
| `IVPad.aidl` | the binder interface between the two halves |
| `ShizukuLink.java` | binding, permissions, and a readable reason for every failure |
| `Core.java` | the shared state and the 250Hz report pump |
| `WebBridge.java` + `assets/bridge.js` | the synchronous pull used by browser mode |
| `OverlayService.java` | the pad on top of other apps |
| `MainActivity.java` | browser, overlay, profiles, edit bar, orientation |
| `Sheet.java` | the Material You settings panel |
| `Mat.java` | the wallpaper-derived palette |
| `Layouts.java` | the shipped layouts, including the Warzone conversion |
| `Prefs.java` | per-profile, per-orientation layout storage |
