package com.goydevv.inputbridge;

import android.content.ComponentName;
import android.content.Context;
import android.content.ServiceConnection;
import android.content.pm.PackageManager;
import android.os.IBinder;
import android.os.RemoteException;

import rikka.shizuku.Shizuku;

public final class BridgeClient {
    private static final int REQUEST_PERMISSION = 4107;
    private static final int SERVICE_VERSION = 1;
    private static BridgeClient instance;

    private final Context context;
    private final Shizuku.UserServiceArgs serviceArgs;
    private final ServiceConnection connection;
    private volatile IInputBridge bridge;

    private BridgeClient(Context context) {
        this.context = context.getApplicationContext();
        serviceArgs = new Shizuku.UserServiceArgs(
                new ComponentName(this.context, InjectorUserService.class))
                .processNameSuffix("injector")
                .version(SERVICE_VERSION)
                .tag("goy-inputbridge")
                .daemon(true)
                .debuggable(BuildConfig.DEBUG);

        connection = new ServiceConnection() {
            @Override public void onServiceConnected(ComponentName name, IBinder binder) {
                bridge = IInputBridge.Stub.asInterface(binder);
            }
            @Override public void onServiceDisconnected(ComponentName name) {
                bridge = null;
            }
        };
    }

    public static synchronized BridgeClient get(Context c) {
        if (instance == null) instance = new BridgeClient(c);
        return instance;
    }

    public boolean isShizukuRunning() {
        try { return Shizuku.pingBinder(); } catch (Throwable t) { return false; }
    }

    public boolean hasPermission() {
        try { return Shizuku.checkSelfPermission() == PackageManager.PERMISSION_GRANTED; }
        catch (Throwable t) { return false; }
    }

    public void requestPermission() {
        try { Shizuku.requestPermission(REQUEST_PERMISSION); } catch (Throwable ignored) {}
    }

    public boolean startService() {
        if (!isShizukuRunning()) return false;
        if (!hasPermission()) { requestPermission(); return false; }
        try {
            Shizuku.bindUserService(serviceArgs, connection);
            return true;
        } catch (Throwable t) {
            return false;
        }
    }

    public void stopService() {
        try {
            IInputBridge b = bridge;
            bridge = null;
            if (b != null) b.releaseAll();
        } catch (Throwable ignored) {}
        try { Shizuku.unbindUserService(serviceArgs, connection, true); } catch (Throwable ignored) {}
    }

    public void mouseMove(float dx, float dy, long time) {
        IInputBridge b = bridge; if (b == null) return;
        try { b.mouseMove(dx, dy, time); } catch (RemoteException e) { bridge = null; }
    }

    public void mouseButton(int button, boolean down, long time) {
        IInputBridge b = bridge; if (b == null) return;
        try { b.mouseButton(button, down, time); } catch (RemoteException e) { bridge = null; }
    }

    public void keyEvent(int code, boolean down, int meta, long time) {
        IInputBridge b = bridge; if (b == null) return;
        try { b.keyEvent(code, down, meta, time); } catch (RemoteException e) { bridge = null; }
    }

    public void mouseScroll(float h, float v, long time) {
        IInputBridge b = bridge; if (b == null) return;
        try { b.mouseScroll(h, v, time); } catch (RemoteException e) { bridge = null; }
    }

    public void releaseAll() {
        IInputBridge b = bridge; if (b == null) return;
        try { b.releaseAll(); } catch (RemoteException e) { bridge = null; }
    }

    public String status() {
        IInputBridge b = bridge; if (b == null) return "not connected";
        try { return b.getStatus(); } catch (RemoteException e) { bridge = null; return "disconnected"; }
    }

    public int uid() {
        IInputBridge b = bridge; if (b == null) return -1;
        try { return b.getUid(); } catch (RemoteException e) { bridge = null; return -1; }
    }
}
