package com.goydevv.inputbridge;

import android.app.Service;
import android.content.Intent;
import android.content.res.Resources;
import android.os.IBinder;
import android.os.SystemClock;
import android.view.InputDevice;
import android.view.KeyEvent;
import android.view.MotionEvent;

import java.lang.reflect.Method;
import java.util.HashSet;
import java.util.Set;

public final class InjectorUserService extends Service {
    private Object inputManager;
    private Method injectMethod;
    private final Set<Integer> keys = new HashSet<>();
    private int buttons;

    private final IInputBridge.Stub binder = new IInputBridge.Stub() {
        @Override public void mouseMove(float dx, float dy, long eventTime) {
            injectMouseMove(dx, dy, eventTime);
        }
        @Override public void mouseButton(int button, boolean pressed, long eventTime) {
            injectMouseButton(button, pressed, eventTime);
        }
        @Override public void mouseScroll(float horizontal, float vertical, long eventTime) {
            injectScroll(horizontal, vertical, eventTime);
        }
        @Override public void keyEvent(int keyCode, boolean pressed, int metaState, long eventTime) {
            injectKey(keyCode, pressed, metaState, eventTime);
        }
        @Override public void releaseAll() {
            releaseAllInternal();
        }
        @Override public String getStatus() {
            return injectMethod != null && inputManager != null ? "injector ready" : "injector unavailable";
        }
        @Override public int getUid() {
            return android.os.Process.myUid();
        }
    };

    @Override public void onCreate() {
        super.onCreate();
        initInputManager();
    }

    @Override public IBinder onBind(Intent intent) {
        return binder;
    }

    @Override public boolean onUnbind(Intent intent) {
        releaseAllInternal();
        return super.onUnbind(intent);
    }

    @Override public void onDestroy() {
        releaseAllInternal();
        super.onDestroy();
    }

    private void initInputManager() {
        try {
            Class<?> c = Class.forName("android.hardware.input.InputManagerGlobal");
            Method get = c.getDeclaredMethod("getInstance");
            get.setAccessible(true);
            inputManager = get.invoke(null);
            injectMethod = c.getDeclaredMethod("injectInputEvent", android.view.InputEvent.class, int.class);
            injectMethod.setAccessible(true);
            return;
        } catch (Throwable ignored) {}

        try {
            Class<?> c = Class.forName("android.hardware.input.InputManager");
            Method get = c.getDeclaredMethod("getInstance");
            get.setAccessible(true);
            inputManager = get.invoke(null);
            injectMethod = c.getDeclaredMethod("injectInputEvent", android.view.InputEvent.class, int.class);
            injectMethod.setAccessible(true);
        } catch (Throwable ignored) {
            inputManager = null;
            injectMethod = null;
        }
    }

    private boolean inject(android.view.InputEvent event) {
        if (injectMethod == null || inputManager == null) {
            event.recycle();
            return false;
        }
        try {
            boolean ok = Boolean.TRUE.equals(injectMethod.invoke(inputManager, event, 0));
            event.recycle();
            return ok;
        } catch (Throwable t) {
            event.recycle();
            return false;
        }
    }

    private void injectMouseMove(float dx, float dy, long requestedTime) {
        if (dx == 0f && dy == 0f) return;
        long now = requestedTime > 0 ? requestedTime : SystemClock.uptimeMillis();
        MotionEvent.PointerProperties pp = new MotionEvent.PointerProperties();
        pp.id = 0;
        pp.toolType = MotionEvent.TOOL_TYPE_MOUSE;
        MotionEvent.PointerCoords pc = new MotionEvent.PointerCoords();
        pc.x = Resources.getSystem().getDisplayMetrics().widthPixels * 0.5f;
        pc.y = Resources.getSystem().getDisplayMetrics().heightPixels * 0.5f;
        pc.pressure = (buttons & MotionEvent.BUTTON_PRIMARY) != 0 ? 1f : 0f;
        pc.size = 1f;
        pc.setAxisValue(MotionEvent.AXIS_RELATIVE_X, dx);
        pc.setAxisValue(MotionEvent.AXIS_RELATIVE_Y, dy);

        MotionEvent ev = MotionEvent.obtain(
                now - 1, now, MotionEvent.ACTION_HOVER_MOVE,
                1,
                new MotionEvent.PointerProperties[]{pp},
                new MotionEvent.PointerCoords[]{pc},
                0, buttons, 1f, 1f, -1, 0,
                InputDevice.SOURCE_MOUSE, 0);
        inject(ev);
    }

    private void injectMouseButton(int button, boolean pressed, long requestedTime) {
        long now = requestedTime > 0 ? requestedTime : SystemClock.uptimeMillis();
        if (pressed) buttons |= button; else buttons &= ~button;

        MotionEvent.PointerProperties pp = new MotionEvent.PointerProperties();
        pp.id = 0;
        pp.toolType = MotionEvent.TOOL_TYPE_MOUSE;
        MotionEvent.PointerCoords pc = new MotionEvent.PointerCoords();
        pc.x = Resources.getSystem().getDisplayMetrics().widthPixels * 0.5f;
        pc.y = Resources.getSystem().getDisplayMetrics().heightPixels * 0.5f;
        pc.pressure = pressed ? 1f : 0f;
        pc.size = 1f;

        int action = pressed ? MotionEvent.ACTION_BUTTON_PRESS : MotionEvent.ACTION_BUTTON_RELEASE;
        MotionEvent ev = MotionEvent.obtain(
                now - 1, now, action, 1,
                new MotionEvent.PointerProperties[]{pp},
                new MotionEvent.PointerCoords[]{pc},
                0, buttons, 1f, 1f, -1, 0,
                InputDevice.SOURCE_MOUSE, 0);
        ev.setAction(action);
        inject(ev);
    }

    private void injectScroll(float horizontal, float vertical, long requestedTime) {
        if (horizontal == 0f && vertical == 0f) return;
        long now = requestedTime > 0 ? requestedTime : SystemClock.uptimeMillis();
        MotionEvent.PointerProperties pp = new MotionEvent.PointerProperties();
        pp.id = 0;
        pp.toolType = MotionEvent.TOOL_TYPE_MOUSE;
        MotionEvent.PointerCoords pc = new MotionEvent.PointerCoords();
        pc.x = Resources.getSystem().getDisplayMetrics().widthPixels * 0.5f;
        pc.y = Resources.getSystem().getDisplayMetrics().heightPixels * 0.5f;
        pc.setAxisValue(MotionEvent.AXIS_VSCROLL, vertical);
        pc.setAxisValue(MotionEvent.AXIS_HSCROLL, horizontal);

        MotionEvent ev = MotionEvent.obtain(
                now - 1, now, MotionEvent.ACTION_SCROLL, 1,
                new MotionEvent.PointerProperties[]{pp},
                new MotionEvent.PointerCoords[]{pc},
                0, buttons, 1f, 1f, -1, 0,
                InputDevice.SOURCE_MOUSE, 0);
        inject(ev);
    }

    private void injectKey(int keyCode, boolean pressed, int metaState, long requestedTime) {
        long now = requestedTime > 0 ? requestedTime : SystemClock.uptimeMillis();
        int action = pressed ? KeyEvent.ACTION_DOWN : KeyEvent.ACTION_UP;
        int repeat = pressed ? 0 : 0;
        KeyEvent ev = new KeyEvent(now, now, action, keyCode, repeat, metaState,
                KeyCharacterMap.VIRTUAL_KEYBOARD, 0, 0, InputDevice.SOURCE_KEYBOARD);
        if (pressed) keys.add(keyCode); else keys.remove(keyCode);
        inject(ev);
    }

    private void releaseAllInternal() {
        long now = SystemClock.uptimeMillis();
        for (Integer key : new HashSet<>(keys)) injectKey(key, false, 0, now);
        keys.clear();
        int[] mouse = {MotionEvent.BUTTON_PRIMARY, MotionEvent.BUTTON_SECONDARY, MotionEvent.BUTTON_TERTIARY};
        for (int b : mouse) if ((buttons & b) != 0) injectMouseButton(b, false, now);
        buttons = 0;
    }
}
